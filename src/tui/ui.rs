/// Main UI layout renderer — M5 edition.
///
/// Layout:
///   ┌─ Sidebar (22%) ─┬───── Canvas (56%) ─────┬─ Properties (22%) ─┐
///   │ Widget list     │  device preview          │ Editable fields    │
///   ├─────────────────┴─────────────────────────┴────────────────────┤
///   │ Metrics preview │ Log panel (right side, 5 rows visible)        │
///   ├──────────────────────────────────────────────────────────────────┤
///   │ Status bar (1 row at bottom)                                    │
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

    // Split content and status bar.
    let root_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    let content_area = root_rows[0];
    let status_area = root_rows[1];

    // Keep left column aligned with the sidebar boundary across the full height.
    let content_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(22), Constraint::Percentage(78)])
        .split(content_area);
    let left_col = content_cols[0];
    let right_col = content_cols[1];

    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(METRIC_PREVIEW_ROW_COUNT + 2),
        ])
        .split(left_col);
    let sidebar_area = left_rows[0];
    let metrics_area = left_rows[1];

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(LOG_VISIBLE_ROWS as u16 + 2),
        ])
        .split(right_col);
    let main_right_area = right_rows[0];
    let log_area = right_rows[1];

    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(56, 78), Constraint::Ratio(22, 78)])
        .split(main_right_area);
    let canvas_area = main_cols[0];
    let props_area = main_cols[1];

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
