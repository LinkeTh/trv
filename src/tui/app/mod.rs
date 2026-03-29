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
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Input as ExplorerInput};

use crate::theme::model::{MetricSource, Theme, ThemeMeta, TimeFormat, Widget, WidgetKind};
use crate::theme::toml::{load_theme_file, save_theme_file};

use super::event::MetricsSnapshot;
use super::fields::{Field, FieldType, MediaPathKind, apply_field, widget_fields};
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

pub const LOG_VISIBLE_ROWS: usize = 5;
const LOG_HISTORY_CAPACITY: usize = 512;
const METRIC_HISTORY_CAPACITY: usize = 64;
const METRIC_KEYS: [&str; 13] = [
    "cpu_temp",
    "cpu_freq",
    "cpu_usage",
    "mem_usage",
    "gpu_temp",
    "gpu_usage",
    "gpu_freq",
    "fan_speed",
    "liquid_temp",
    "net_down",
    "net_up",
    "disk_read",
    "disk_write",
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
    /// Media path picker used by image/video widget `path` fields.
    MediaPath {
        state: Box<MediaPathDialogState>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPathDialogState {
    pub explorer: FileExplorer,
    pub field_name: &'static str,
    pub media_kind: MediaPathKind,
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

    /// Canvas preview device dimensions (width, height).
    pub display_size: (u16, u16),

    // ── Live metrics ──────────────────────────────────────────────────────────
    /// Most recent metrics snapshot from the background poller.
    pub metrics: MetricsSnapshot,

    /// Rolling history used by sparkline previews in the metrics panel.
    pub metric_history: HashMap<String, VecDeque<u64>>,
}

mod canvas_keys;
mod core;
mod device;
mod dialogs;
mod helpers;
mod properties;
mod routing;
mod sidebar;
mod widget_overlays;

use self::helpers::*;

#[cfg(test)]
mod tests;
