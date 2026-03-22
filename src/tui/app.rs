/// Application state for the TUI — M5 edition.
///
/// Manages:
/// - Active theme + path
/// - Panel focus
/// - Sidebar widget selection and reordering
/// - Properties editor (field cursor + inline TextInput)
/// - Add-widget / Delete-confirm / Save / Open overlay popups
/// - Push-to-device (background worker + status updates)
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};

use crate::theme::model::{MetricSource, Theme, TimeFormat, Widget, WidgetKind};
use crate::theme::toml::{load_theme_file, save_theme_file};

use super::event::MetricsSnapshot;
use super::fields::{Field, FieldType, apply_field, widget_fields};
use super::input::{InputResult, TextInput};

pub const COLOR_PALETTE_COLUMNS: usize = 8;
pub const COLOR_PALETTE: &[&str] = &[
    "#CDD6F4", "#BAC2DE", "#A6ADC8", "#9399B2", "#7F849C", "#6C7086", "#585B70", "#45475A",
    "#313244", "#1E1E2E", "#181825", "#11111B", "#89B4FA", "#74C7EC", "#89DCEB", "#94E2D5",
    "#A6E3A1", "#F9E2AF", "#FAB387", "#F38BA8", "#EBA0AC", "#F5C2E7", "#CBA6F7", "#B4BEFE",
    "#8BD5CA", "#91D7E3", "#7DC4E4", "#8AADF4", "#B7BDF8", "#EE99A0", "#F5A97F", "#EED49F",
    "#A6DA95", "#F5BDE6", "#C6A0F6", "#7AA2F7", "#2AC3DE", "#73DACA", "#FF9E64", "#E0AF68",
    "#DDB6F2", "#89DDFF", "#ADD7FF", "#C3E88D", "#FFCB6B", "#F78C6C", "#FF5370", "#C792EA",
];

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
    /// Save-as dialog — holds the path input
    Save {
        input: TextInput,
    },
    /// Open dialog — holds the path input
    Open {
        input: TextInput,
    },
}

// ─── Push status ─────────────────────────────────────────────────────────────

/// Status line shown after push or file operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushStatus {
    None,
    PushInProgress,
    PushOk,
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

    /// Push worker result channel while a push is running.
    push_result_rx: Option<Receiver<Result<(), String>>>,

    /// Cancellation flag for the active push worker.
    push_cancel: Option<Arc<AtomicBool>>,

    /// Join handle for the active push worker.
    push_worker: Option<JoinHandle<()>>,

    // ── Device connection ─────────────────────────────────────────────────────
    pub host: String,
    pub port: u16,
    pub recv_timeout_ms: u64,

    // ── Live metrics ──────────────────────────────────────────────────────────
    /// Most recent metrics snapshot from the background poller.
    pub metrics: MetricsSnapshot,
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
        Self {
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
            push_result_rx: None,
            push_cancel: None,
            push_worker: None,
            host,
            port,
            recv_timeout_ms,
            metrics: MetricsSnapshot {
                values: std::collections::HashMap::new(),
            },
        }
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
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) | KeyCode::Char('?') => {
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
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }
            KeyCode::F(1) | KeyCode::Char('?') => {
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
            KeyCode::Char('p') | KeyCode::Char('P') if self.prop_input.is_none() => {
                self.push_to_device();
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
            Overlay::Save { input } | Overlay::Open { input } => {
                input.insert_str(&pasted);
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
        let Some(rx) = self.push_result_rx.as_ref() else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(())) => {
                self.push_status = PushStatus::PushOk;
                if let Some(path) = self.theme_path.as_deref() {
                    if let Err(e) = crate::config::set_default_theme_path(path) {
                        self.push_status =
                            PushStatus::Err(format!("pushed, but failed to update config: {}", e));
                    }
                }
                self.push_result_rx = None;
                self.push_cancel = None;
                if let Some(handle) = self.push_worker.take() {
                    let _ = handle.join();
                }
            }
            Ok(Err(e)) => {
                self.push_status = PushStatus::Err(e);
                self.push_result_rx = None;
                self.push_cancel = None;
                if let Some(handle) = self.push_worker.take() {
                    let _ = handle.join();
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.push_status = PushStatus::Err("push worker disconnected".into());
                self.push_result_rx = None;
                self.push_cancel = None;
                if let Some(handle) = self.push_worker.take() {
                    let _ = handle.join();
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(cancel) = self.push_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.push_result_rx = None;
        if let Some(handle) = self.push_worker.take() {
            let _ = handle.join();
        }
    }

    // ── Sidebar keys ──────────────────────────────────────────────────────────

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if ctrl => self.sidebar_move_up(),
            KeyCode::Down | KeyCode::Char('j') if ctrl => self.sidebar_move_down(),
            KeyCode::Up | KeyCode::Char('k') => self.sidebar_up(),
            KeyCode::Down | KeyCode::Char('j') => self.sidebar_down(),
            KeyCode::Enter => {
                if self.selected_widget.is_some() {
                    self.focus = Focus::Properties;
                    self.prop_cursor = 0;
                }
            }
            // Add widget
            KeyCode::Char('a') => {
                self.overlay = Overlay::AddWidget { cursor: 0 };
            }
            // Delete widget
            KeyCode::Char('d') => {
                if let Some(idx) = self.selected_widget {
                    self.overlay = Overlay::DeleteConfirm { idx };
                }
            }
            _ => {}
        }
    }

    fn sidebar_up(&mut self) {
        if let Some(ref mut idx) = self.selected_widget {
            if *idx > 0 {
                *idx -= 1;
                self.prop_cursor = 0;
            }
        }
    }

    fn sidebar_down(&mut self) {
        let count = self.widget_count();
        if let Some(ref mut idx) = self.selected_widget {
            if *idx + 1 < count {
                *idx += 1;
                self.prop_cursor = 0;
            }
        }
    }

    fn sidebar_move_up(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut()) {
            if idx > 0 {
                theme.widgets.swap(idx, idx - 1);
                self.selected_widget = Some(idx - 1);
                self.dirty = true;
            }
        }
    }

    fn sidebar_move_down(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut()) {
            if idx + 1 < theme.widgets.len() {
                theme.widgets.swap(idx, idx + 1);
                self.selected_widget = Some(idx + 1);
                self.dirty = true;
            }
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
            KeyCode::Char('k') => self.sidebar_up(),
            KeyCode::Char('j') => self.sidebar_down(),
            _ => {}
        }
    }

    fn move_widget_by(&mut self, dx: u16, dy: u16, subtract: bool) {
        if let Some(w) = self.selected_widget_mut() {
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
            self.dirty = true;
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
            KeyCode::Up | KeyCode::Char('k') => {
                if self.prop_cursor > 0 {
                    self.prop_cursor -= 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.field_count().saturating_sub(1);
                if self.prop_cursor < max {
                    self.prop_cursor += 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Enter => {
                self.activate_property_editor();
            }
            KeyCode::Char(' ') => {
                if let Some(field) = self.current_field() {
                    if field.kind == FieldType::Toggle {
                        self.toggle_field(&field);
                    }
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
                KeyCode::Up | KeyCode::Char('k') => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
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
                            if let Some(normalized) = normalize_color_value(&input.value) {
                                if let Some(idx) = COLOR_PALETTE
                                    .iter()
                                    .position(|opt| opt.eq_ignore_ascii_case(&normalized))
                                {
                                    *cursor = idx;
                                }
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
                    KeyCode::Left | KeyCode::Char('h') => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        if *cursor + 1 < len {
                            *cursor += 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *cursor >= cols {
                            *cursor -= cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
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
                KeyCode::Up | KeyCode::Char('k') => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
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
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.overlay = Overlay::None;
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.delete_widget(idx);
            }
            _ => {}
        }
    }

    fn delete_widget(&mut self, idx: usize) {
        if let Some(theme) = &mut self.theme {
            if idx < theme.widgets.len() {
                theme.widgets.remove(idx);
                self.dirty = true;
                let count = theme.widgets.len();
                self.selected_widget = if count == 0 {
                    None
                } else {
                    Some(idx.min(count - 1))
                };
                self.prop_cursor = 0;
            }
        }
    }

    // ── Save overlay ──────────────────────────────────────────────────────────

    fn begin_save(&mut self) {
        let initial = self
            .theme_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        self.overlay = Overlay::Save {
            input: TextInput::new(initial),
        };
    }

    fn handle_save_key(&mut self, key: KeyEvent) {
        let result = if let Overlay::Save { ref mut input } = self.overlay {
            input.handle_key(key)
        } else {
            return;
        };

        match result {
            InputResult::Pending => {}
            InputResult::Cancelled => {
                self.overlay = Overlay::None;
            }
            InputResult::Confirmed => {
                let path_str = if let Overlay::Save { ref input } = self.overlay {
                    input.value.clone()
                } else {
                    return;
                };
                self.overlay = Overlay::None;

                if !path_str.is_empty() {
                    let path = expand_tilde_path(&path_str);
                    if let Some(theme) = &self.theme {
                        match save_theme_file(theme, &path) {
                            Ok(()) => {
                                self.theme_path = Some(path);
                                self.dirty = false;
                                self.push_status = PushStatus::SaveOk;
                            }
                            Err(e) => {
                                self.push_status = PushStatus::Err(format!("save: {}", e));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Open overlay ──────────────────────────────────────────────────────────

    fn begin_open(&mut self) {
        let initial = self
            .theme_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        self.overlay = Overlay::Open {
            input: TextInput::new(initial),
        };
    }

    fn handle_open_key(&mut self, key: KeyEvent) {
        let result = if let Overlay::Open { ref mut input } = self.overlay {
            input.handle_key(key)
        } else {
            return;
        };

        match result {
            InputResult::Pending => {}
            InputResult::Cancelled => {
                self.overlay = Overlay::None;
            }
            InputResult::Confirmed => {
                let path_str = if let Overlay::Open { ref input } = self.overlay {
                    input.value.clone()
                } else {
                    return;
                };
                self.overlay = Overlay::None;

                if !path_str.is_empty() {
                    let path = expand_tilde_path(&path_str);
                    match load_theme_file(&path) {
                        Ok(t) => {
                            let count = t.widgets.len();
                            self.theme = Some(t);
                            self.theme_path = Some(path);
                            self.dirty = false;
                            self.selected_widget = if count > 0 { Some(0) } else { None };
                            self.prop_cursor = 0;
                            self.prop_error = None;
                            self.push_status = PushStatus::OpenOk;
                        }
                        Err(e) => {
                            self.push_status = PushStatus::Err(format!("open: {}", e));
                        }
                    }
                }
            }
        }
    }

    // ── Push to device ────────────────────────────────────────────────────────

    /// Start a background push of the current theme to the device.
    ///
    /// The worker thread pushes local assets first, then sends cmd3A frames.
    /// Completion status is returned via `push_result_rx` and polled by the
    /// event loop.
    fn push_to_device(&mut self) {
        if self.push_result_rx.is_some() {
            self.push_status = PushStatus::Err("push already in progress".into());
            return;
        }

        let theme = match &self.theme {
            Some(t) => t.clone(),
            None => {
                self.push_status = PushStatus::Err("no theme loaded".into());
                return;
            }
        };

        let frames = match crate::daemon::runner::build_theme_frames(&theme) {
            Ok(f) => f,
            Err(e) => {
                self.push_status = PushStatus::Err(format!("build frames: {}", e));
                return;
            }
        };

        let host = self.host.clone();
        let port = self.port;
        let recv_timeout_ms = self.recv_timeout_ms;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancel);
        let inter_frame_delay_ms = crate::device::adb::INTER_FRAME_DELAY.as_millis() as u64;

        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        self.push_result_rx = Some(rx);
        self.push_cancel = Some(cancel);
        self.push_status = PushStatus::PushInProgress;

        let handle = std::thread::spawn(move || {
            crate::daemon::runner::push_theme_assets(&theme, false);

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

fn expand_tilde_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(stripped) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::theme::model::{Background, ThemeMeta};

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
            background: Background::default(),
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
}
