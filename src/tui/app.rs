/// Application state for the TUI — M5 edition.
///
/// Manages:
/// - Active theme + path
/// - Panel focus
/// - Sidebar widget selection and reordering
/// - Properties editor (field cursor + inline TextInput)
/// - Add-widget / Delete-confirm / Save / Open overlay popups
/// - Push-to-device (background worker + status updates)
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::{
    style::{Modifier, Style},
    widgets::HighlightSpacing,
};
use ratatui_explorer::{
    FileExplorer, FileExplorerBuilder, Input as ExplorerInput, Theme as ExplorerTheme,
};

use crate::theme::model::{MetricSource, Theme, ThemeMeta, TimeFormat, Widget, WidgetKind};
use crate::theme::toml::{load_theme_file, save_theme_file};

use super::event::MetricsSnapshot;
use super::fields::{Field, FieldType, apply_field, widget_fields};
use super::input::{InputResult, TextInput};
use super::palette;

pub const COLOR_PALETTE_COLUMNS: usize = 8;
pub const COLOR_PALETTE: &[&str] = &[
    "#CDD6F4", "#BAC2DE", "#A6ADC8", "#9399B2", "#7F849C", "#6C7086", "#585B70", "#45475A",
    "#313244", "#1E1E2E", "#181825", "#11111B", "#89B4FA", "#74C7EC", "#89DCEB", "#94E2D5",
    "#A6E3A1", "#F9E2AF", "#FAB387", "#F38BA8", "#EBA0AC", "#F5C2E7", "#CBA6F7", "#B4BEFE",
    "#8BD5CA", "#91D7E3", "#7DC4E4", "#8AADF4", "#B7BDF8", "#EE99A0", "#F5A97F", "#EED49F",
    "#A6DA95", "#F5BDE6", "#C6A0F6", "#7AA2F7", "#2AC3DE", "#73DACA", "#FF9E64", "#E0AF68",
    "#DDB6F2", "#89DDFF", "#ADD7FF", "#C3E88D", "#FFCB6B", "#F78C6C", "#FF5370", "#C792EA",
];

pub const LOG_VISIBLE_ROWS: usize = 5;
const LOG_HISTORY_CAPACITY: usize = 512;
const METRIC_HISTORY_CAPACITY: usize = 64;
const METRIC_KEYS: [&str; 5] = [
    "cpu_temp",
    "cpu_usage",
    "mem_usage",
    "gpu_temp",
    "gpu_usage",
];

const ROTATION_CODES: [crate::protocol::cmd::OrientationCode; 4] = [
    crate::protocol::cmd::OrientationCode::Raw0,
    crate::protocol::cmd::OrientationCode::Raw1,
    crate::protocol::cmd::OrientationCode::Raw2,
    crate::protocol::cmd::OrientationCode::Raw3,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationAction {
    RawCode(crate::protocol::cmd::OrientationCode),
    EnableAuto,
}

// ─── Focus ────────────────────────────────────────────────────────────────────

/// The panel that currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Canvas,
    Properties,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Self::Sidebar => Self::Canvas,
            Self::Canvas => Self::Properties,
            Self::Properties => Self::Sidebar,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Sidebar => Self::Properties,
            Self::Canvas => Self::Sidebar,
            Self::Properties => Self::Canvas,
        }
    }
}

// ─── Widget type selector ─────────────────────────────────────────────────────

/// Choices available when adding a new widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewWidgetKind {
    Metric,
    Clock,
    Text,
    Image,
    Video,
}

impl NewWidgetKind {
    pub const ALL: &'static [NewWidgetKind] = &[
        NewWidgetKind::Metric,
        NewWidgetKind::Clock,
        NewWidgetKind::Text,
        NewWidgetKind::Image,
        NewWidgetKind::Video,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NewWidgetKind::Metric => "Metric",
            NewWidgetKind::Clock => "Clock",
            NewWidgetKind::Text => "Text",
            NewWidgetKind::Image => "Image",
            NewWidgetKind::Video => "Video",
        }
    }
}

// ─── Overlay state ────────────────────────────────────────────────────────────

/// Which popup (if any) is currently visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    /// Add-widget type picker — cursor is index into `NewWidgetKind::ALL`
    AddWidget {
        cursor: usize,
    },
    /// Generic dropdown selector for a typed property field.
    FieldDropdown {
        field_name: &'static str,
        options: &'static [&'static str],
        cursor: usize,
    },
    /// Color picker for hex color fields.
    ColorPicker {
        field_name: &'static str,
        cursor: usize,
        input: TextInput,
        input_active: bool,
    },
    /// Delete confirmation — contains the widget index being deleted
    DeleteConfirm {
        idx: usize,
    },
    /// New theme dialog — filename + meta fields
    NewTheme {
        state: Box<NewThemeDialogState>,
    },
    /// Save-as dialog — holds the path input
    Save {
        state: Box<SaveDialogState>,
    },
    /// Open dialog — file explorer state.
    Open {
        state: Box<OpenDialogState>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDialogState {
    pub explorer: FileExplorer,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThemeDialogState {
    pub file_input: TextInput,
    pub name_input: TextInput,
    pub description_input: TextInput,
    pub active_field: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveDialogState {
    pub explorer: FileExplorer,
    pub path_input: TextInput,
    pub input_active: bool,
    pub error: Option<String>,
}

// ─── Push status ─────────────────────────────────────────────────────────────

/// Status line shown after push or file operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushStatus {
    None,
    PushInProgress,
    PushOk,
    RotateInProgress,
    RotateOk(String),
    SaveOk,
    OpenOk,
    Err(String),
}

// ─── App ─────────────────────────────────────────────────────────────────────

/// All application state.
pub struct App {
    /// The theme being edited (None if no theme loaded yet).
    pub theme: Option<Theme>,

    /// Path the theme was loaded from / will be saved to.
    pub theme_path: Option<PathBuf>,

    /// Currently focused panel.
    pub focus: Focus,

    /// Active overlay (if any).
    pub overlay: Overlay,

    /// Set to true when the main loop should exit.
    pub should_quit: bool,

    /// Index of the selected widget in the sidebar list.
    pub selected_widget: Option<usize>,

    /// Whether the theme has unsaved changes.
    pub dirty: bool,

    // ── Properties editor state ───────────────────────────────────────────────
    /// Index of the field row cursor within the Properties panel.
    pub prop_cursor: usize,

    /// Active inline text input for a field, if any.
    /// `None` = cursor is navigating; `Some` = a field is being edited.
    pub prop_input: Option<TextInput>,

    /// Validation error message from the last failed `apply_field`, cleared
    /// when the user begins editing again.
    pub prop_error: Option<String>,

    // ── Push status ───────────────────────────────────────────────────────────
    /// Result of the last push/save/open action.
    pub push_status: PushStatus,

    /// Activity log history shown in the bottom panel.
    pub log_lines: VecDeque<String>,

    /// Scroll offset from newest log line (0 = pinned to latest).
    pub log_scroll: usize,

    /// Push worker result channel while a push is running.
    push_result_rx: Option<Receiver<Result<(), String>>>,

    /// Cancellation flag for the active push worker.
    push_cancel: Option<Arc<AtomicBool>>,

    /// Join handle for the active push worker.
    push_worker: Option<JoinHandle<()>>,

    /// Rotation worker result channel while a rotation op is running.
    rotate_result_rx: Option<Receiver<Result<String, String>>>,

    /// Join handle for the active rotation worker.
    rotate_worker: Option<JoinHandle<()>>,

    /// Next raw orientation code index for `r` cycling.
    next_rotation_code_idx: usize,

    // ── Device connection ─────────────────────────────────────────────────────
    pub host: String,
    pub port: u16,
    pub recv_timeout_ms: u64,

    // ── Live metrics ──────────────────────────────────────────────────────────
    /// Most recent metrics snapshot from the background poller.
    pub metrics: MetricsSnapshot,

    /// Rolling history used by sparkline previews in the metrics panel.
    pub metric_history: HashMap<String, VecDeque<u64>>,
}

impl App {
    pub fn new(
        theme: Option<Theme>,
        theme_path: Option<PathBuf>,
        host: String,
        port: u16,
        recv_timeout_ms: u64,
    ) -> Self {
        let selected_widget = if theme.as_ref().map_or(0, |t| t.widgets.len()) > 0 {
            Some(0)
        } else {
            None
        };
        let mut app = Self {
            theme,
            theme_path,
            focus: Focus::Sidebar,
            overlay: Overlay::None,
            should_quit: false,
            selected_widget,
            dirty: false,
            prop_cursor: 0,
            prop_input: None,
            prop_error: None,
            push_status: PushStatus::None,
            log_lines: VecDeque::with_capacity(LOG_HISTORY_CAPACITY),
            log_scroll: 0,
            push_result_rx: None,
            push_cancel: None,
            push_worker: None,
            rotate_result_rx: None,
            rotate_worker: None,
            next_rotation_code_idx: 0,
            host,
            port,
            recv_timeout_ms,
            metrics: MetricsSnapshot {
                values: HashMap::new(),
                samples: HashMap::new(),
            },
            metric_history: HashMap::new(),
        };
        app.log_event("TUI started");
        app
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    pub fn selected_widget_ref(&self) -> Option<&Widget> {
        let idx = self.selected_widget?;
        self.theme.as_ref()?.widgets.get(idx)
    }

    pub fn selected_widget_mut(&mut self) -> Option<&mut Widget> {
        let idx = self.selected_widget?;
        self.theme.as_mut()?.widgets.get_mut(idx)
    }

    pub fn widget_count(&self) -> usize {
        self.theme.as_ref().map_or(0, |t| t.widgets.len())
    }

    pub fn theme_name(&self) -> &str {
        self.theme
            .as_ref()
            .map(|t| t.meta.name.as_str())
            .unwrap_or("(no theme)")
    }

    pub fn visible_log_lines(&self) -> Vec<String> {
        if self.log_lines.is_empty() {
            return Vec::new();
        }

        let max_scroll = self.max_log_scroll();
        let scroll = self.log_scroll.min(max_scroll);
        let end = self.log_lines.len().saturating_sub(scroll);
        let start = end.saturating_sub(LOG_VISIBLE_ROWS);

        self.log_lines
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect()
    }

    pub fn max_log_scroll(&self) -> usize {
        self.log_lines.len().saturating_sub(LOG_VISIBLE_ROWS)
    }

    pub fn scroll_log_page_up(&mut self) {
        self.log_scroll = self
            .log_scroll
            .saturating_add(LOG_VISIBLE_ROWS)
            .min(self.max_log_scroll());
    }

    pub fn scroll_log_page_down(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(LOG_VISIBLE_ROWS);
    }

    pub fn log_is_scrolled(&self) -> bool {
        self.log_scroll > 0
    }

    pub fn log_event(&mut self, message: impl Into<String>) {
        if self.log_lines.len() >= LOG_HISTORY_CAPACITY {
            self.log_lines.pop_front();
        }
        self.log_lines.push_back(message.into());
        if self.log_scroll > 0 {
            self.log_scroll = self.log_scroll.saturating_add(1).min(self.max_log_scroll());
        }
    }

    pub fn update_metrics(&mut self, snapshot: MetricsSnapshot) {
        for key in METRIC_KEYS {
            let sample = snapshot
                .samples
                .get(key)
                .copied()
                .map(|raw| metric_value_to_spark_sample(key, raw))
                .unwrap_or(0.0);
            let value = sample.round().clamp(0.0, 100.0) as u64;

            let history = self
                .metric_history
                .entry(key.to_string())
                .or_insert_with(|| VecDeque::with_capacity(METRIC_HISTORY_CAPACITY));
            if history.len() >= METRIC_HISTORY_CAPACITY {
                history.pop_front();
            }
            history.push_back(value);
        }

        self.metrics = snapshot;
    }

    /// Number of editable fields for the currently selected widget.
    pub fn field_count(&self) -> usize {
        self.selected_widget_ref()
            .map(|w| widget_fields(w).len())
            .unwrap_or(0)
    }

    // ── Top-level key handler ─────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Route to overlay handler first if one is active.
        match &self.overlay {
            Overlay::None => {}
            Overlay::Help => {
                match key.code {
                    KeyCode::Esc | KeyCode::F(1) => {
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Char('q') | KeyCode::Char('?') if no_ctrl_alt(&key) => {
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                }
                return;
            }
            Overlay::AddWidget { .. } => {
                self.handle_add_widget_key(key);
                return;
            }
            Overlay::FieldDropdown { .. } => {
                self.handle_field_dropdown_key(key);
                return;
            }
            Overlay::ColorPicker { .. } => {
                self.handle_color_picker_key(key);
                return;
            }
            Overlay::DeleteConfirm { .. } => {
                self.handle_delete_confirm_key(key);
                return;
            }
            Overlay::NewTheme { .. } => {
                self.handle_new_theme_key(key);
                return;
            }
            Overlay::Save { .. } => {
                self.handle_save_key(key);
                return;
            }
            Overlay::Open { .. } => {
                self.handle_open_key(key);
                return;
            }
        }

        // While an inline property editor is active, route every key to that
        // editor before global shortcuts so normal text (e.g. 'q') does not
        // accidentally trigger app-level actions.
        if self.prop_input.is_some() {
            self.handle_properties_key(key);
            return;
        }

        // Global bindings (no overlay).
        match key.code {
            KeyCode::Char('q') if no_ctrl_alt(&key) => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }
            KeyCode::F(1) => {
                self.overlay = Overlay::Help;
                return;
            }
            KeyCode::Char('?') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::Help;
                return;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_save();
                return;
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_open();
                return;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_new_theme();
                return;
            }
            KeyCode::Char('p') if no_ctrl_alt(&key) => {
                self.push_to_device();
                return;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.enable_auto_rotation();
                return;
            }
            KeyCode::Char('r') if no_ctrl_alt(&key) => {
                self.rotate_next_manual_orientation();
                return;
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                return;
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                return;
            }
            KeyCode::PageUp => {
                self.scroll_log_page_up();
                return;
            }
            KeyCode::PageDown => {
                self.scroll_log_page_down();
                return;
            }
            _ => {}
        }

        // Panel-specific keys.
        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Canvas => self.handle_canvas_key(key),
            Focus::Properties => self.handle_properties_key(key),
        }
    }

    pub fn handle_mouse(&mut self, _mouse: MouseEvent) {}

    pub fn handle_paste(&mut self, text: &str) {
        let pasted: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        if pasted.is_empty() {
            return;
        }

        if let Some(inp) = &mut self.prop_input {
            inp.insert_str(&pasted);
            self.prop_error = None;
            return;
        }

        match &mut self.overlay {
            Overlay::NewTheme { state } => {
                match state.active_field {
                    0 => state.file_input.insert_str(&pasted),
                    1 => state.name_input.insert_str(&pasted),
                    2 => state.description_input.insert_str(&pasted),
                    _ => {}
                }
                state.error = None;
            }
            Overlay::Save { state } => {
                state.path_input.insert_str(&pasted);
                state.input_active = true;
            }
            Overlay::ColorPicker {
                input,
                input_active,
                ..
            } => {
                *input_active = true;
                input.insert_str(&pasted);
            }
            _ => {}
        }
    }

    pub fn poll_push_result(&mut self) {
        if let Some(rx) = self.push_result_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.push_status = PushStatus::PushOk;
                    self.log_event("Push completed");
                    if let Some(path) = self.theme_path.as_deref()
                        && let Err(e) = crate::config::set_default_theme_path(path)
                    {
                        self.push_status =
                            PushStatus::Err(format!("pushed, but failed to update config: {}", e));
                        self.log_event(format!("Config update failed after push: {}", e));
                    }
                    self.cleanup_push_worker();
                }
                Ok(Err(e)) => {
                    self.log_event(format!("Push failed: {}", e));
                    self.push_status = PushStatus::Err(e);
                    self.cleanup_push_worker();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.push_status = PushStatus::Err("push worker disconnected".into());
                    self.log_event("Push worker disconnected");
                    self.cleanup_push_worker();
                }
            }
        }

        if let Some(rx) = self.rotate_result_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.log_event(format!("Rotation completed: {}", msg));
                    self.push_status = PushStatus::RotateOk(msg);
                    self.cleanup_rotate_worker();
                }
                Ok(Err(e)) => {
                    self.log_event(format!("Rotation failed: {}", e));
                    self.push_status = PushStatus::Err(e);
                    self.cleanup_rotate_worker();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.push_status = PushStatus::Err("rotation worker disconnected".into());
                    self.log_event("Rotation worker disconnected");
                    self.cleanup_rotate_worker();
                }
            }
        }
    }

    /// Tear down the push worker channel and join its thread.
    fn cleanup_push_worker(&mut self) {
        self.push_result_rx = None;
        self.push_cancel = None;
        if let Some(handle) = self.push_worker.take() {
            let _ = handle.join();
        }
    }

    /// Tear down the rotate worker channel and join its thread.
    fn cleanup_rotate_worker(&mut self) {
        self.rotate_result_rx = None;
        if let Some(handle) = self.rotate_worker.take() {
            let _ = handle.join();
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(cancel) = self.push_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cleanup_push_worker();
        self.cleanup_rotate_worker();
    }

    // ── Sidebar keys ──────────────────────────────────────────────────────────

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if ctrl => self.sidebar_move_up(),
            KeyCode::Down | KeyCode::Char('j') if ctrl => self.sidebar_move_down(),
            KeyCode::Up => self.sidebar_up(),
            KeyCode::Down => self.sidebar_down(),
            KeyCode::Char('k') if no_ctrl_alt(&key) => self.sidebar_up(),
            KeyCode::Char('j') if no_ctrl_alt(&key) => self.sidebar_down(),
            KeyCode::Enter => {
                if self.selected_widget.is_some() {
                    self.focus = Focus::Properties;
                    self.prop_cursor = 0;
                }
            }
            // Add widget
            KeyCode::Char('a') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::AddWidget { cursor: 0 };
            }
            // Delete widget
            KeyCode::Char('d') if no_ctrl_alt(&key) => {
                if let Some(idx) = self.selected_widget {
                    self.overlay = Overlay::DeleteConfirm { idx };
                }
            }
            _ => {}
        }
    }

    fn sidebar_up(&mut self) {
        if let Some(ref mut idx) = self.selected_widget
            && *idx > 0
        {
            *idx -= 1;
            self.prop_cursor = 0;
        }
    }

    fn sidebar_down(&mut self) {
        let count = self.widget_count();
        if let Some(ref mut idx) = self.selected_widget
            && *idx + 1 < count
        {
            *idx += 1;
            self.prop_cursor = 0;
        }
    }

    fn sidebar_move_up(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut())
            && idx > 0
        {
            theme.widgets.swap(idx, idx - 1);
            self.selected_widget = Some(idx - 1);
            self.dirty = true;
        }
    }

    fn sidebar_move_down(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut())
            && idx + 1 < theme.widgets.len()
        {
            theme.widgets.swap(idx, idx + 1);
            self.selected_widget = Some(idx + 1);
            self.dirty = true;
        }
    }

    // ── Canvas keys ───────────────────────────────────────────────────────────

    fn handle_canvas_key(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let step: u16 = if shift { 10 } else { 1 };

        match key.code {
            KeyCode::Up => self.move_widget_by(0, step, true),
            KeyCode::Down => self.move_widget_by(0, step, false),
            KeyCode::Left => self.move_widget_by(step, 0, true),
            KeyCode::Right => self.move_widget_by(step, 0, false),
            // j/k scroll widget selection in canvas too
            KeyCode::Char('k') if no_ctrl_alt(&key) => self.sidebar_up(),
            KeyCode::Char('j') if no_ctrl_alt(&key) => self.sidebar_down(),
            _ => {}
        }
    }

    fn move_widget_by(&mut self, dx: u16, dy: u16, subtract: bool) {
        if let Some(w) = self.selected_widget_mut() {
            if matches!(w.kind, WidgetKind::Video { .. }) {
                return;
            }

            let before_x = w.x;
            let before_y = w.y;

            if dx > 0 {
                if subtract {
                    w.x = w.x.saturating_sub(dx);
                } else {
                    w.x = w.x.saturating_add(dx).min(483);
                }
            }
            if dy > 0 {
                if subtract {
                    w.y = w.y.saturating_sub(dy);
                } else {
                    w.y = w.y.saturating_add(dy).min(479);
                }
            }
            if w.x != before_x || w.y != before_y {
                self.dirty = true;
            }
        }
    }

    // ── Properties keys ───────────────────────────────────────────────────────

    fn handle_properties_key(&mut self, key: KeyEvent) {
        // If inline editor is active, route to it.
        if self.prop_input.is_some() {
            self.handle_prop_input_key(key);
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Up => {
                if self.prop_cursor > 0 {
                    self.prop_cursor -= 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Char('k') if no_ctrl_alt(&key) => {
                if self.prop_cursor > 0 {
                    self.prop_cursor -= 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Down => {
                let max = self.field_count().saturating_sub(1);
                if self.prop_cursor < max {
                    self.prop_cursor += 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Char('j') if no_ctrl_alt(&key) => {
                let max = self.field_count().saturating_sub(1);
                if self.prop_cursor < max {
                    self.prop_cursor += 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Enter => {
                self.activate_property_editor();
            }
            KeyCode::Char(' ') if no_ctrl_alt(&key) => {
                if let Some(field) = self.current_field()
                    && field.kind == FieldType::Toggle
                {
                    self.toggle_field(&field);
                }
            }
            _ => {}
        }
    }

    fn handle_prop_input_key(&mut self, key: KeyEvent) {
        let result = if let Some(inp) = &mut self.prop_input {
            inp.handle_key(key)
        } else {
            return;
        };

        match result {
            InputResult::Pending => {}
            InputResult::Cancelled => {
                self.prop_input = None;
                self.prop_error = None;
            }
            InputResult::Confirmed => {
                // Take the input value and apply it.
                let new_value = self.prop_input.take().map(|i| i.value).unwrap_or_default();
                let field_name = self.current_field().map(|f| f.name);

                if let Some(name) = field_name {
                    self.apply_field_value(name, &new_value);
                }
            }
        }
    }

    fn current_field(&self) -> Option<Field> {
        self.selected_widget_ref()
            .and_then(|w| widget_fields(w).into_iter().nth(self.prop_cursor))
    }

    fn activate_property_editor(&mut self) {
        let Some(field) = self.current_field() else {
            return;
        };

        match field.kind {
            FieldType::Text => {
                self.prop_input = Some(TextInput::new(&field.value));
                self.prop_error = None;
            }
            FieldType::Toggle => {
                self.toggle_field(&field);
            }
            FieldType::Dropdown(options) => {
                self.begin_field_dropdown(&field, options);
            }
            FieldType::Color => {
                self.begin_color_picker(&field);
            }
        }
    }

    fn toggle_field(&mut self, field: &Field) {
        let next = if field.value.eq_ignore_ascii_case("true") {
            "false"
        } else {
            "true"
        };
        self.apply_field_value(field.name, next);
    }

    fn begin_field_dropdown(&mut self, field: &Field, options: &'static [&'static str]) {
        let cursor = options
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&field.value))
            .unwrap_or(0);
        self.overlay = Overlay::FieldDropdown {
            field_name: field.name,
            options,
            cursor,
        };
        self.prop_error = None;
    }

    fn begin_color_picker(&mut self, field: &Field) {
        let normalized = normalize_color_value(&field.value).unwrap_or_else(|| field.value.clone());
        let cursor = COLOR_PALETTE
            .iter()
            .position(|opt| opt.eq_ignore_ascii_case(&normalized))
            .unwrap_or(0);
        let mut input = TextInput::new(normalized);
        if input.value.is_empty() {
            sync_color_input_from_cursor(cursor, &mut input);
        }
        self.overlay = Overlay::ColorPicker {
            field_name: field.name,
            cursor,
            input,
            input_active: false,
        };
        self.prop_error = None;
    }

    fn apply_field_value(&mut self, field_name: &'static str, value: &str) {
        if let Some(w) = self.selected_widget_mut() {
            match apply_field(w, field_name, value) {
                Ok(()) => {
                    self.dirty = true;
                    self.prop_error = None;
                }
                Err(e) => {
                    self.prop_error = Some(e);
                }
            }
        }
    }

    fn handle_field_dropdown_key(&mut self, key: KeyEvent) {
        let mut selection: Option<(&'static str, &'static str)> = None;
        let mut close = false;

        if let Overlay::FieldDropdown {
            field_name,
            options,
            cursor,
        } = &mut self.overlay
        {
            match key.code {
                KeyCode::Esc => close = true,
                KeyCode::Up => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Char('k') if no_ctrl_alt(&key) => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Down => {
                    if *cursor + 1 < options.len() {
                        *cursor += 1;
                    }
                }
                KeyCode::Char('j') if no_ctrl_alt(&key) => {
                    if *cursor + 1 < options.len() {
                        *cursor += 1;
                    }
                }
                KeyCode::Home => {
                    *cursor = 0;
                }
                KeyCode::End => {
                    if !options.is_empty() {
                        *cursor = options.len() - 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(value) = options.get(*cursor) {
                        selection = Some((*field_name, *value));
                    }
                    close = true;
                }
                _ => {}
            }
        }

        if close {
            self.overlay = Overlay::None;
        }

        if let Some((field_name, value)) = selection {
            self.apply_field_value(field_name, value);
        }
    }

    fn handle_color_picker_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.overlay = Overlay::None;
            return;
        }

        let mut chosen: Option<(&'static str, String)> = None;
        let mut close = false;

        if let Overlay::ColorPicker {
            field_name,
            cursor,
            input,
            input_active,
        } = &mut self.overlay
        {
            if *input_active {
                match key.code {
                    KeyCode::Tab => {
                        *input_active = false;
                    }
                    _ => match input.handle_key(key) {
                        InputResult::Pending => {
                            if let Some(normalized) = normalize_color_value(&input.value)
                                && let Some(idx) = COLOR_PALETTE
                                    .iter()
                                    .position(|opt| opt.eq_ignore_ascii_case(&normalized))
                            {
                                *cursor = idx;
                            }
                        }
                        InputResult::Confirmed => {
                            chosen = Some((*field_name, input.value.clone()));
                            close = true;
                        }
                        InputResult::Cancelled => {
                            *input_active = false;
                        }
                    },
                }
            } else {
                let cols = COLOR_PALETTE_COLUMNS.max(1);
                let len = COLOR_PALETTE.len();
                match key.code {
                    KeyCode::Tab => {
                        *input_active = true;
                    }
                    KeyCode::Left => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('h') if no_ctrl_alt(&key) => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Right => {
                        if *cursor + 1 < len {
                            *cursor += 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('l') if no_ctrl_alt(&key) => {
                        if *cursor + 1 < len {
                            *cursor += 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Up => {
                        if *cursor >= cols {
                            *cursor -= cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('k') if no_ctrl_alt(&key) => {
                        if *cursor >= cols {
                            *cursor -= cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Down => {
                        if *cursor + cols < len {
                            *cursor += cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('j') if no_ctrl_alt(&key) => {
                        if *cursor + cols < len {
                            *cursor += cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Home => {
                        *cursor = 0;
                        sync_color_input_from_cursor(*cursor, input);
                    }
                    KeyCode::End => {
                        if len > 0 {
                            *cursor = len - 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(color) = COLOR_PALETTE.get(*cursor) {
                            chosen = Some((*field_name, (*color).to_string()));
                            close = true;
                        }
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT)
                            && (c == '#' || c.is_ascii_hexdigit()) =>
                    {
                        *input_active = true;
                        input.value.clear();
                        input.cursor = 0;
                        let _ = input.handle_key(key);
                    }
                    _ => {}
                }
            }
        }

        if close {
            self.overlay = Overlay::None;
        }

        if let Some((field_name, value)) = chosen {
            self.apply_field_value(field_name, &value);
        }
    }

    // ── Add widget overlay ────────────────────────────────────────────────────

    fn handle_add_widget_key(&mut self, key: KeyEvent) {
        let len = NewWidgetKind::ALL.len();
        if let Overlay::AddWidget { ref mut cursor } = self.overlay {
            match key.code {
                KeyCode::Esc => {
                    self.overlay = Overlay::None;
                }
                KeyCode::Up => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Char('k') if no_ctrl_alt(&key) => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Down => {
                    if *cursor + 1 < len {
                        *cursor += 1;
                    }
                }
                KeyCode::Char('j') if no_ctrl_alt(&key) => {
                    if *cursor + 1 < len {
                        *cursor += 1;
                    }
                }
                KeyCode::Enter => {
                    let kind_idx = *cursor;
                    self.overlay = Overlay::None;
                    self.add_widget(NewWidgetKind::ALL[kind_idx]);
                }
                _ => {}
            }
        }
    }

    fn add_widget(&mut self, kind: NewWidgetKind) {
        let added_kind_label = kind.label();
        let widget = Widget {
            kind: match kind {
                NewWidgetKind::Metric => WidgetKind::Metric {
                    source: MetricSource::CpuTemp,
                    unit: "°C".into(),
                    label: String::new(),
                    show_label: false,
                },
                NewWidgetKind::Clock => WidgetKind::Clock {
                    time_format: TimeFormat::HhMmSs,
                },
                NewWidgetKind::Text => WidgetKind::Text {
                    content: "Label".into(),
                },
                NewWidgetKind::Image => WidgetKind::Image {
                    path: String::new(),
                },
                NewWidgetKind::Video => WidgetKind::Video {
                    path: String::new(),
                },
            },
            x: 10,
            y: 10,
            width: 200,
            height: 100,
            text_size: 40,
            color: "FFFFFF".into(),
            alpha: 1.0,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            font: String::new(),
        };

        if let Some(theme) = &mut self.theme {
            theme.widgets.push(widget);
            self.selected_widget = Some(theme.widgets.len() - 1);
            self.dirty = true;
            self.log_event(format!("Added {} widget", added_kind_label));
        }
    }

    // ── Delete confirm overlay ────────────────────────────────────────────────

    fn handle_delete_confirm_key(&mut self, key: KeyEvent) {
        let idx = if let Overlay::DeleteConfirm { idx } = self.overlay {
            idx
        } else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::None;
            }
            KeyCode::Char('y') | KeyCode::Char('Y') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::None;
                self.delete_widget(idx);
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.delete_widget(idx);
            }
            _ => {}
        }
    }

    fn delete_widget(&mut self, idx: usize) {
        if let Some(theme) = &mut self.theme
            && idx < theme.widgets.len()
        {
            let label = widget_log_label(&theme.widgets[idx]);
            theme.widgets.remove(idx);
            self.dirty = true;
            let count = theme.widgets.len();
            self.selected_widget = if count == 0 {
                None
            } else {
                Some(idx.min(count - 1))
            };
            self.prop_cursor = 0;
            self.log_event(format!("Deleted widget {}", label));
        }
    }

    // ── New-theme overlay ─────────────────────────────────────────────────────

    fn begin_new_theme(&mut self) {
        let state = self.build_new_theme_dialog_state();
        self.overlay = Overlay::NewTheme {
            state: Box::new(state),
        };
        self.log_event("New theme dialog opened");
    }

    fn build_new_theme_dialog_state(&self) -> NewThemeDialogState {
        let file_path = default_new_theme_path(self.theme_path.as_deref());
        NewThemeDialogState {
            file_input: TextInput::new(file_path.display().to_string()),
            name_input: TextInput::new("New Theme"),
            description_input: TextInput::new(""),
            active_field: 0,
            error: None,
        }
    }

    fn handle_new_theme_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut cancelled = false;
        let mut create_request: Option<(PathBuf, String, String)> = None;

        if let Overlay::NewTheme { state } = &mut self.overlay {
            state.error = None;

            match key.code {
                KeyCode::Esc => {
                    close_overlay = true;
                    cancelled = true;
                }
                KeyCode::BackTab => {
                    state.active_field = state.active_field.saturating_add(2) % 3;
                }
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    state.active_field = state.active_field.saturating_add(2) % 3;
                }
                KeyCode::Tab => {
                    state.active_field = (state.active_field + 1) % 3;
                }
                KeyCode::Up => {
                    state.active_field = state.active_field.saturating_sub(1);
                }
                KeyCode::Down => {
                    if state.active_field < 2 {
                        state.active_field += 1;
                    }
                }
                _ => {
                    let input = match state.active_field {
                        0 => &mut state.file_input,
                        1 => &mut state.name_input,
                        _ => &mut state.description_input,
                    };

                    match input.handle_key(key) {
                        InputResult::Pending => {}
                        InputResult::Cancelled => {
                            close_overlay = true;
                            cancelled = true;
                        }
                        InputResult::Confirmed => {
                            if state.active_field < 2 {
                                state.active_field += 1;
                            } else {
                                let raw_file = state.file_input.value.trim();
                                if raw_file.is_empty() {
                                    state.error = Some("enter a theme filename".to_string());
                                } else {
                                    let file_path = normalize_theme_file_path(raw_file);
                                    let name = if state.name_input.value.trim().is_empty() {
                                        default_theme_name_from_path(&file_path)
                                    } else {
                                        state.name_input.value.trim().to_string()
                                    };
                                    let description =
                                        state.description_input.value.trim().to_string();
                                    create_request = Some((file_path, name, description));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            return;
        }

        if let Some((path, name, description)) = create_request {
            match self.create_new_theme(path.clone(), name, description) {
                Ok(()) => {
                    self.overlay = Overlay::None;
                }
                Err(e) => {
                    if let Overlay::NewTheme { state } = &mut self.overlay {
                        state.error = Some(e);
                    }
                }
            }
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("New theme dialog cancelled");
            }
        }
    }

    fn create_new_theme(
        &mut self,
        file_path: PathBuf,
        name: String,
        description: String,
    ) -> Result<(), String> {
        self.log_event(format!("Creating new theme {}", file_path.display()));

        let theme = Theme {
            meta: ThemeMeta { name, description },
            widgets: Vec::new(),
        };

        save_theme_file(&theme, &file_path).map_err(|e| format!("creating theme file {}", e))?;

        self.theme = Some(theme);
        self.theme_path = Some(file_path.clone());
        self.selected_widget = None;
        self.focus = Focus::Sidebar;
        self.prop_cursor = 0;
        self.prop_input = None;
        self.prop_error = None;
        self.dirty = false;
        self.push_status = PushStatus::SaveOk;

        self.log_event(format!("Created theme {}", file_path.display()));
        Ok(())
    }

    // ── Save overlay ──────────────────────────────────────────────────────────

    fn begin_save(&mut self) {
        match self.build_save_dialog_state() {
            Ok(state) => {
                let cwd = state.explorer.cwd().display().to_string();
                self.overlay = Overlay::Save {
                    state: Box::new(state),
                };
                self.log_event(format!("Save dialog started at {}", cwd));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("save dialog: {}", e));
                self.log_event(format!("Save dialog failed: {}", e));
            }
        }
    }

    fn handle_save_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut cancelled = false;
        let mut save_target: Option<PathBuf> = None;

        if let Overlay::Save { state } = &mut self.overlay {
            state.error = None;

            if key.code == KeyCode::Esc {
                close_overlay = true;
                cancelled = true;
            } else if key.code == KeyCode::Tab {
                state.input_active = !state.input_active;
            } else if state.input_active {
                match state.path_input.handle_key(key) {
                    InputResult::Pending => {}
                    InputResult::Cancelled => {
                        state.input_active = false;
                    }
                    InputResult::Confirmed => {
                        let raw = state.path_input.value.trim();
                        if raw.is_empty() {
                            state.error = Some("enter a file path".to_string());
                        } else {
                            save_target = Some(expand_tilde_path(raw));
                            close_overlay = true;
                        }
                    }
                }
            } else {
                match key.code {
                    KeyCode::Backspace => {
                        if let Err(e) = state.explorer.handle(ExplorerInput::Left) {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    }
                    KeyCode::Enter => {
                        let current = state.explorer.current().clone();
                        if current.is_dir {
                            if let Err(e) = state.explorer.handle(ExplorerInput::Right) {
                                state.error = Some(format!("navigate: {}", e));
                            }
                        } else {
                            state.path_input = TextInput::new(current.path.display().to_string());
                            state.input_active = true;
                        }
                    }
                    _ => {
                        if let Some(input) = explorer_input_from_key(key)
                            && let Err(e) = state.explorer.handle(input)
                        {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    }
                }
            }
        } else {
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("Save dialog cancelled");
            }
        }

        if let Some(path) = save_target {
            self.save_theme_to_path(path);
        }
    }

    fn build_save_dialog_state(&self) -> Result<SaveDialogState, String> {
        let mut builder = FileExplorerBuilder::default()
            .show_hidden(false)
            .theme(build_explorer_theme())
            .filter_map(|file| {
                if file.is_dir || is_toml_path(&file.path) {
                    Some(file)
                } else {
                    None
                }
            });

        let initial_path = self
            .theme_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("theme.toml"));

        if initial_path.exists() {
            builder = builder.working_file(initial_path.clone());
        } else if let Some(parent) = initial_path.parent()
            && !parent.as_os_str().is_empty()
        {
            builder = builder.working_dir(parent.to_path_buf());
        }

        let explorer = builder
            .build()
            .map_err(|e| format!("failed to initialize save explorer: {}", e))?;

        Ok(SaveDialogState {
            explorer,
            path_input: TextInput::new(initial_path.display().to_string()),
            input_active: false,
            error: None,
        })
    }

    fn save_theme_to_path(&mut self, path: PathBuf) {
        self.log_event(format!("Saving theme to {}", path.display()));
        if let Some(theme) = &self.theme {
            match save_theme_file(theme, &path) {
                Ok(()) => {
                    self.theme_path = Some(path);
                    self.dirty = false;
                    self.push_status = PushStatus::SaveOk;
                    if let Some(saved_path) = self.theme_path.as_deref() {
                        self.log_event(format!("Saved theme {}", saved_path.display()));
                    }
                }
                Err(e) => {
                    self.log_event(format!("Save failed: {}", e));
                    self.push_status = PushStatus::Err(format!("save: {}", e));
                }
            }
        }
    }

    // ── Open overlay ──────────────────────────────────────────────────────────

    fn begin_open(&mut self) {
        match self.build_open_dialog_state() {
            Ok(state) => {
                let cwd = state.explorer.cwd().display().to_string();
                self.overlay = Overlay::Open {
                    state: Box::new(state),
                };
                self.log_event(format!("Open dialog started at {}", cwd));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("open dialog: {}", e));
                self.log_event(format!("Open dialog failed: {}", e));
            }
        }
    }

    fn handle_open_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut open_selected: Option<PathBuf> = None;
        let mut cancelled = false;

        if let Overlay::Open { state } = &mut self.overlay {
            state.error = None;
            match key.code {
                KeyCode::Esc => {
                    close_overlay = true;
                    cancelled = true;
                }
                KeyCode::Backspace => {
                    if let Err(e) = state.explorer.handle(ExplorerInput::Left) {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
                KeyCode::Enter => {
                    let current = state.explorer.current().clone();
                    if current.is_dir {
                        if let Err(e) = state.explorer.handle(ExplorerInput::Right) {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    } else if is_toml_path(&current.path) {
                        open_selected = Some(current.path.clone());
                        close_overlay = true;
                    } else {
                        state.error = Some("select a .toml file".to_string());
                    }
                }
                _ => {
                    if let Some(input) = explorer_input_from_key(key)
                        && let Err(e) = state.explorer.handle(input)
                    {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
            }
        } else {
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("Open dialog cancelled");
            }
        }

        if let Some(path) = open_selected {
            self.load_theme_from_path(path);
        }
    }

    fn build_open_dialog_state(&self) -> Result<OpenDialogState, String> {
        let mut builder = FileExplorerBuilder::default()
            .show_hidden(false)
            .theme(build_explorer_theme())
            .filter_map(|file| {
                if file.is_dir || is_toml_path(&file.path) {
                    Some(file)
                } else {
                    None
                }
            });

        if let Some(path) = self.theme_path.as_ref() {
            if path.exists() {
                builder = builder.working_file(path.clone());
            } else if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                builder = builder.working_dir(parent.to_path_buf());
            }
        }

        let explorer = builder
            .build()
            .map_err(|e| format!("failed to initialize file explorer: {}", e))?;

        Ok(OpenDialogState {
            explorer,
            error: None,
        })
    }

    fn load_theme_from_path(&mut self, path: PathBuf) {
        self.log_event(format!("Opening theme {}", path.display()));
        match load_theme_file(&path) {
            Ok(t) => {
                let count = t.widgets.len();
                self.theme = Some(t);
                self.theme_path = Some(path.clone());
                self.dirty = false;
                self.selected_widget = if count > 0 { Some(0) } else { None };
                self.prop_cursor = 0;
                self.prop_error = None;
                self.push_status = PushStatus::OpenOk;
                self.log_event(format!("Opened theme {}", path.display()));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("open: {}", e));
                self.log_event(format!("Open failed: {}", e));
            }
        }
    }

    // ── Push to device ────────────────────────────────────────────────────────

    fn rotate_next_manual_orientation(&mut self) {
        let (code, next_idx) = next_rotation_code(self.next_rotation_code_idx);
        self.next_rotation_code_idx = next_idx;
        self.start_rotation_worker(RotationAction::RawCode(code));
    }

    fn enable_auto_rotation(&mut self) {
        self.start_rotation_worker(RotationAction::EnableAuto);
    }

    fn start_rotation_worker(&mut self, action: RotationAction) {
        if self.rotate_result_rx.is_some() {
            self.push_status = PushStatus::Err("rotation already in progress".into());
            self.log_event("Rotation skipped: operation already in progress");
            return;
        }

        if self.push_result_rx.is_some() {
            self.push_status = PushStatus::Err("push in progress; wait before rotating".into());
            self.log_event("Rotation skipped: push still in progress");
            return;
        }

        let host = self.host.clone();
        let port = self.port;
        let recv_timeout_ms = self.recv_timeout_ms;

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        self.rotate_result_rx = Some(rx);
        self.push_status = PushStatus::RotateInProgress;
        match action {
            RotationAction::RawCode(code) => {
                self.log_event(format!("Rotation started: code {:02X}", code.as_u8()));
            }
            RotationAction::EnableAuto => {
                self.log_event("Rotation started: enable auto-rotation");
            }
        }

        let handle = std::thread::spawn(move || {
            let result = match action {
                RotationAction::RawCode(code) => {
                    let frame = match crate::protocol::cmd::build_cmd38_frame(code) {
                        Ok(f) => f,
                        Err(e) => {
                            let _ = tx.send(Err(format!("build cmd38 frame: {}", e)));
                            return;
                        }
                    };

                    let rt = match tokio::runtime::Builder::new_current_thread()
                        .enable_io()
                        .enable_time()
                        .build()
                    {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = tx.send(Err(format!("create runtime: {}", e)));
                            return;
                        }
                    };

                    rt.block_on(async move {
                        crate::device::connection::send_frame(&host, port, &frame, recv_timeout_ms)
                            .await
                            .map_err(|e| format!("sending cmd38: {}", e))?;
                        Ok(format!("rotation code {:02X} applied", code.as_u8()))
                    })
                }
                RotationAction::EnableAuto => {
                    if !crate::device::adb::adb_available() {
                        Err("adb not found in PATH (required for auto-rotation)".into())
                    } else if !crate::device::adb::adb_settings_put_system(
                        "accelerometer_rotation",
                        "1",
                    ) {
                        Err("failed to enable auto-rotation via adb".into())
                    } else {
                        Ok("auto-rotation enabled".to_string())
                    }
                }
            };

            let _ = tx.send(result);
        });

        self.rotate_worker = Some(handle);
    }

    /// Start a background push of the current theme to the device.
    ///
    /// The worker thread pushes local assets first, then sends cmd3A frames.
    /// Completion status is returned via `push_result_rx` and polled by the
    /// event loop.
    fn push_to_device(&mut self) {
        if self.push_result_rx.is_some() {
            self.push_status = PushStatus::Err("push already in progress".into());
            self.log_event("Push skipped: operation already in progress");
            return;
        }

        let theme = match &self.theme {
            Some(t) => t.clone(),
            None => {
                self.push_status = PushStatus::Err("no theme loaded".into());
                self.log_event("Push failed: no theme loaded");
                return;
            }
        };

        let frames = match crate::daemon::runner::build_theme_frames(&theme) {
            Ok(f) => f,
            Err(e) => {
                self.push_status = PushStatus::Err(format!("build frames: {}", e));
                self.log_event(format!("Push failed while building frames: {}", e));
                return;
            }
        };

        let host = self.host.clone();
        let port = self.port;
        let recv_timeout_ms = self.recv_timeout_ms;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancel);
        let inter_frame_delay_ms = crate::device::connection::INTER_FRAME_DELAY.as_millis() as u64;

        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        self.push_result_rx = Some(rx);
        self.push_cancel = Some(cancel);
        self.push_status = PushStatus::PushInProgress;
        self.log_event(format!("Push started ({} frames)", frames.len()));

        let handle = std::thread::spawn(move || {
            crate::daemon::runner::push_theme_assets(&theme, false, Some(&cancel_worker));

            if cancel_worker.load(Ordering::Relaxed) {
                let _ = tx.send(Err("push cancelled".into()));
                return;
            }

            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(format!("create runtime: {}", e)));
                    return;
                }
            };

            let result = rt.block_on(async move {
                for (i, frame) in frames.iter().enumerate() {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return Err("push cancelled".to_string());
                    }

                    crate::device::connection::send_frame(&host, port, frame, recv_timeout_ms)
                        .await
                        .map_err(|e| format!("sending frame {}: {}", i, e))?;

                    if inter_frame_delay_ms > 0 && i + 1 < frames.len() {
                        tokio::time::sleep(Duration::from_millis(inter_frame_delay_ms)).await;
                    }
                }
                Ok(())
            });

            let _ = tx.send(result);
        });

        self.push_worker = Some(handle);
    }
}

fn normalize_color_value(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_uppercase()))
    } else {
        None
    }
}

fn sync_color_input_from_cursor(cursor: usize, input: &mut TextInput) {
    if let Some(color) = COLOR_PALETTE.get(cursor) {
        input.value = (*color).to_string();
        input.cursor = input.value.len();
    }
}

fn next_rotation_code(idx: usize) -> (crate::protocol::cmd::OrientationCode, usize) {
    let i = idx % ROTATION_CODES.len();
    let next = (i + 1) % ROTATION_CODES.len();
    (ROTATION_CODES[i], next)
}

fn explorer_input_from_key(key: KeyEvent) -> Option<ExplorerInput> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(ExplorerInput::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(ExplorerInput::Down),
        KeyCode::Left | KeyCode::Char('h') => Some(ExplorerInput::Left),
        KeyCode::Right | KeyCode::Char('l') => Some(ExplorerInput::Right),
        KeyCode::Home => Some(ExplorerInput::Home),
        KeyCode::End => Some(ExplorerInput::End),
        KeyCode::PageUp => Some(ExplorerInput::PageUp),
        KeyCode::PageDown => Some(ExplorerInput::PageDown),
        KeyCode::Char('.') => Some(ExplorerInput::ToggleShowHidden),
        _ => None,
    }
}

fn is_toml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
}

fn widget_log_label(widget: &Widget) -> String {
    match &widget.kind {
        WidgetKind::Metric { source, .. } => format!("Metric {:?}", source),
        WidgetKind::Clock { time_format } => format!("Clock {:?}", time_format),
        WidgetKind::Image { path } => {
            if path.is_empty() {
                "Image".to_string()
            } else {
                format!("Image {}", file_name_or_path(path))
            }
        }
        WidgetKind::Video { path } => {
            if path.is_empty() {
                "Video".to_string()
            } else {
                format!("Video {}", file_name_or_path(path))
            }
        }
        WidgetKind::Text { content } => {
            let mut chars = content.chars();
            let preview: String = chars.by_ref().take(12).collect();
            if chars.next().is_some() {
                format!("Text \"{}…\"", preview)
            } else {
                format!("Text \"{}\"", preview)
            }
        }
    }
}

fn file_name_or_path(raw: &str) -> String {
    let path = Path::new(raw);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn expand_tilde_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(stripped) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    PathBuf::from(raw)
}

fn default_new_theme_path(current_theme_path: Option<&Path>) -> PathBuf {
    if let Some(path) = current_theme_path
        && let Some(parent) = path.parent()
    {
        return parent.join("new_theme.toml");
    }

    if let Ok(Some(default_path)) = crate::config::get_default_theme_path()
        && let Some(parent) = default_path.parent()
    {
        return parent.join("new_theme.toml");
    }

    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    config_dir.join("trv").join("themes").join("new_theme.toml")
}

fn normalize_theme_file_path(raw: &str) -> PathBuf {
    let mut path = expand_tilde_path(raw.trim());
    if !is_toml_path(&path) {
        path.set_extension("toml");
    }
    path
}

fn default_theme_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(|name| name.replace(['_', '-'], " "))
        .unwrap_or_else(|| "New Theme".to_string())
}

fn build_explorer_theme() -> ExplorerTheme {
    ExplorerTheme::new()
        .with_style(Style::default().fg(palette::TEXT))
        .with_item_style(Style::default().fg(palette::TEXT))
        .with_dir_style(Style::default().fg(palette::PEACH))
        .with_highlight_item_style(
            Style::default()
                .fg(palette::CRUST)
                .bg(palette::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .with_highlight_dir_style(
            Style::default()
                .fg(palette::CRUST)
                .bg(palette::SAPPHIRE)
                .add_modifier(Modifier::BOLD),
        )
        .with_highlight_symbol("> ")
        .with_highlight_spacing(HighlightSpacing::Always)
}

fn metric_value_to_spark_sample(key: &str, value: f64) -> f64 {
    match key {
        "cpu_usage" | "gpu_usage" | "mem_usage" => value.clamp(0.0, 100.0),
        "cpu_temp" | "gpu_temp" => ((value / 120.0) * 100.0).clamp(0.0, 100.0),
        _ => value.clamp(0.0, 100.0),
    }
}

fn no_ctrl_alt(key: &KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::theme::model::ThemeMeta;

    fn test_widget() -> Widget {
        Widget {
            kind: WidgetKind::Text {
                content: "hello".to_string(),
            },
            x: 10,
            y: 10,
            width: 100,
            height: 40,
            text_size: 20,
            color: "FFFFFF".to_string(),
            alpha: 1.0,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            font: String::new(),
        }
    }

    fn test_theme() -> Theme {
        Theme {
            meta: ThemeMeta {
                name: "test".to_string(),
                description: String::new(),
            },
            widgets: vec![test_widget()],
        }
    }

    #[test]
    fn typing_q_during_property_edit_does_not_quit() {
        let mut app = App::new(
            Some(test_theme()),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        app.focus = Focus::Properties;
        app.selected_widget = Some(0);
        app.prop_cursor = 0;
        app.prop_input = Some(TextInput::new(""));

        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()));

        assert!(!app.should_quit);
        assert_eq!(app.prop_input.as_ref().map(|i| i.value.as_str()), Some("q"));
    }

    #[test]
    fn paste_inserts_into_property_input() {
        let mut app = App::new(
            Some(test_theme()),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        app.focus = Focus::Properties;
        app.selected_widget = Some(0);
        app.prop_cursor = 0;
        app.prop_input = Some(TextInput::new("ab"));

        app.handle_paste("cd\n");

        assert_eq!(
            app.prop_input.as_ref().map(|i| i.value.as_str()),
            Some("abcd")
        );
    }

    #[test]
    fn expand_tilde_path_uses_home_directory() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(expand_tilde_path("~"), home);
        assert_eq!(
            expand_tilde_path("~/themes/a.toml"),
            home.join("themes/a.toml")
        );
    }

    #[test]
    fn moving_video_widget_in_canvas_does_not_change_position_or_dirty() {
        let mut app = App::new(
            Some(Theme {
                meta: ThemeMeta {
                    name: "test".to_string(),
                    description: String::new(),
                },
                widgets: vec![Widget {
                    kind: WidgetKind::Video {
                        path: "/tmp/bg.mp4".to_string(),
                    },
                    x: 10,
                    y: 20,
                    width: 100,
                    height: 50,
                    text_size: 40,
                    color: "FFFFFF".to_string(),
                    alpha: 1.0,
                    bold: false,
                    italic: false,
                    underline: false,
                    strikethrough: false,
                    font: String::new(),
                }],
            }),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        app.focus = Focus::Canvas;
        app.selected_widget = Some(0);
        app.dirty = false;

        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));

        let w = app.selected_widget_ref().expect("selected widget");
        assert_eq!(w.x, 10);
        assert_eq!(w.y, 20);
        assert!(!app.dirty);
    }

    #[test]
    fn rotation_code_cycle_wraps_through_all_raw_codes() {
        let mut idx = 0;
        let mut seen = Vec::new();
        for _ in 0..5 {
            let (code, next) = next_rotation_code(idx);
            seen.push(code.as_u8());
            idx = next;
        }

        assert_eq!(seen, vec![0x00, 0x01, 0x02, 0x03, 0x00]);
    }

    #[test]
    fn log_scrolling_pages_and_clamps() {
        let mut app = App::new(
            Some(test_theme()),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        for i in 0..20 {
            app.log_event(format!("line {}", i));
        }

        app.scroll_log_page_up();
        assert_eq!(app.log_scroll, LOG_VISIBLE_ROWS.min(app.max_log_scroll()));

        app.scroll_log_page_up();
        assert_eq!(
            app.log_scroll,
            (LOG_VISIBLE_ROWS * 2).min(app.max_log_scroll())
        );

        for _ in 0..20 {
            app.scroll_log_page_up();
        }
        assert_eq!(app.log_scroll, app.max_log_scroll());

        app.scroll_log_page_down();
        assert_eq!(
            app.log_scroll,
            app.max_log_scroll().saturating_sub(LOG_VISIBLE_ROWS)
        );

        for _ in 0..20 {
            app.scroll_log_page_down();
        }
        assert_eq!(app.log_scroll, 0);
    }

    #[test]
    fn key_pageup_and_pagedown_scroll_log_panel() {
        let mut app = App::new(
            Some(test_theme()),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        for i in 0..12 {
            app.log_event(format!("line {}", i));
        }

        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()));
        assert_eq!(app.log_scroll, LOG_VISIBLE_ROWS.min(app.max_log_scroll()));

        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty()));
        assert_eq!(app.log_scroll, 0);
    }

    #[test]
    fn log_scroll_tracks_new_entries_when_scrolled_back() {
        let mut app = App::new(
            Some(test_theme()),
            None,
            "127.0.0.1".to_string(),
            22222,
            1000,
        );

        for i in 0..12 {
            app.log_event(format!("line {}", i));
        }

        app.scroll_log_page_up();
        let before = app.log_scroll;
        app.log_event("new line");

        assert_eq!(app.log_scroll, (before + 1).min(app.max_log_scroll()));
    }

    #[test]
    fn helper_maps_explorer_keys_and_toml_paths() {
        assert_eq!(
            explorer_input_from_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty())),
            Some(ExplorerInput::PageUp)
        );
        assert_eq!(
            explorer_input_from_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::empty())),
            Some(ExplorerInput::ToggleShowHidden)
        );
        assert_eq!(
            explorer_input_from_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty())),
            None
        );

        assert!(is_toml_path(Path::new("/tmp/theme.toml")));
        assert!(is_toml_path(Path::new("/tmp/theme.TOML")));
        assert!(!is_toml_path(Path::new("/tmp/theme.txt")));
    }

    #[test]
    fn explorer_theme_makes_selection_visible() {
        let theme = build_explorer_theme();
        assert_eq!(theme.highlight_symbol(), Some("> "));
        assert_eq!(theme.highlight_spacing(), &HighlightSpacing::Always);

        let highlight_item = *theme.highlight_item_style();
        let highlight_dir = *theme.highlight_dir_style();
        assert_eq!(highlight_item.bg, Some(palette::BLUE));
        assert_eq!(highlight_dir.bg, Some(palette::SAPPHIRE));
    }

    #[test]
    fn save_dialog_enter_file_prefills_then_confirms_save_path() {
        let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("trv-save-test-{}", nonce));
        fs::create_dir_all(&root).expect("create temp root");
        let file_path = root.join("demo.toml");
        fs::write(&file_path, "").expect("create temp file");

        let state = SaveDialogState {
            explorer: FileExplorerBuilder::default()
                .working_file(file_path.clone())
                .build()
                .expect("build explorer"),
            path_input: TextInput::new(""),
            input_active: false,
            error: None,
        };
        app.overlay = Overlay::Save {
            state: Box::new(state),
        };

        app.handle_save_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        match &app.overlay {
            Overlay::Save { state } => {
                assert!(state.input_active);
                assert_eq!(state.path_input.value, file_path.display().to_string());
            }
            _ => panic!("save overlay should remain open"),
        }

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(root);
    }

    #[test]
    fn update_metrics_collects_sparkline_history() {
        let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

        let mut samples = HashMap::new();
        samples.insert("cpu_temp".to_string(), 60.0);
        samples.insert("cpu_usage".to_string(), 25.0);
        samples.insert("mem_usage".to_string(), 33.0);
        samples.insert("gpu_temp".to_string(), 48.0);
        samples.insert("gpu_usage".to_string(), 40.0);

        let mut values = HashMap::new();
        values.insert("cpu_temp".to_string(), "60.0°C".to_string());
        values.insert("cpu_usage".to_string(), "25.0%".to_string());
        values.insert("mem_usage".to_string(), "33.0%".to_string());
        values.insert("gpu_temp".to_string(), "48°C".to_string());
        values.insert("gpu_usage".to_string(), "40.0%".to_string());

        app.update_metrics(MetricsSnapshot { values, samples });

        let cpu_usage_hist = app
            .metric_history
            .get("cpu_usage")
            .expect("cpu_usage history present");
        assert_eq!(cpu_usage_hist.back().copied(), Some(25));
    }

    #[test]
    fn ctrl_n_opens_new_theme_dialog() {
        let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        match &app.overlay {
            Overlay::NewTheme { state } => {
                assert_eq!(state.active_field, 0);
                assert!(state.file_input.value.ends_with(".toml"));
            }
            _ => panic!("expected new theme overlay"),
        }
    }

    #[test]
    fn new_theme_dialog_defaults_to_toml_extension() {
        let path = normalize_theme_file_path("/tmp/my_theme");
        assert_eq!(path, PathBuf::from("/tmp/my_theme.toml"));
    }
}
