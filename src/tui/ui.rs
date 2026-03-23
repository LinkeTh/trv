/// Main UI layout renderer — M5 edition.
///
/// Layout (horizontal split):
///   ┌─ Sidebar (22%) ─┬───── Canvas (56%) ─────┬─ Properties (22%) ─┐
///   │ Widget list     │  device preview          │ Editable fields    │
///   └─────────────────┴─────────────────────────┴────────────────────┘
///   ┤ Metrics preview (25%) │ Log panel (75%, 5 rows visible)          │
///   ┤ Status bar (1 row at bottom)                                    │
///
/// Overlays are rendered on top as centered popups:
///   Help, AddWidget picker, DeleteConfirm, New theme, Save dialog, Open dialog.
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, FrameExt as _, List, ListItem, ListState, Paragraph,
        Sparkline, Wrap,
    },
};

use crate::theme::model::{Widget, WidgetKind};

use super::app::{
    App, COLOR_PALETTE, COLOR_PALETTE_COLUMNS, Focus, LOG_VISIBLE_ROWS, MediaPathDialogState,
    NewThemeDialogState, NewWidgetKind, OpenDialogState, Overlay, PushStatus, SaveDialogState,
};
use super::canvas;
use super::fields::{Field, FieldType, widget_fields};
use super::input::TextInput;
use super::palette;

mod common;
mod dialog_overlays;
mod overlays;
mod panels;

use self::common::*;
use self::dialog_overlays::*;
use self::overlays::*;
use self::panels::*;

// ─── Public entry point ──────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Split main content, log panel, and status bar.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(LOG_VISIBLE_ROWS as u16 + 2),
            Constraint::Length(1),
        ])
        .split(area);

    let main_area = rows[0];
    let lower_area = rows[1];
    let status_area = rows[2];

    let lower_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(lower_area);
    let metrics_area = lower_cols[0];
    let log_area = lower_cols[1];

    // 3-panel horizontal split
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(22),
            Constraint::Percentage(56),
            Constraint::Percentage(22),
        ])
        .split(main_area);

    let sidebar_area = cols[0];
    let canvas_area = cols[1];
    let props_area = cols[2];

    draw_sidebar(f, app, sidebar_area);
    canvas::render(
        f,
        canvas_area,
        app.theme.as_ref(),
        app.selected_widget,
        app.focus == Focus::Canvas,
        app.display_size,
    );
    draw_properties(f, app, props_area);
    draw_metric_preview_panel(f, app, metrics_area);
    draw_log_panel(f, app, log_area);
    draw_status_bar(f, app, status_area);

    // Overlays drawn last (on top of everything)
    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help_overlay(f, area),
        Overlay::AddWidget { cursor } => draw_add_widget_overlay(f, area, *cursor),
        Overlay::FieldDropdown {
            field_name,
            options,
            cursor,
        } => draw_field_dropdown_overlay(f, area, field_name, options, *cursor),
        Overlay::ColorPicker {
            field_name,
            cursor,
            input,
            input_active,
        } => draw_color_picker_overlay(f, area, field_name, *cursor, input, *input_active),
        Overlay::DeleteConfirm { idx } => draw_delete_confirm_overlay(f, area, *idx, app),
        Overlay::NewTheme { state } => draw_new_theme_overlay(f, area, state),
        Overlay::Save { state } => draw_save_overlay(f, area, state),
        Overlay::Open { state } => draw_open_overlay(f, area, state),
        Overlay::MediaPath { state } => draw_media_path_overlay(f, area, state),
    }
}
