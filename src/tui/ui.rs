/// Main UI layout renderer — M5 edition.
///
/// Layout (horizontal split):
///   ┌─ Sidebar (22%) ─┬───── Canvas (56%) ─────┬─ Properties (22%) ─┐
///   │ Widget list     │  484×480 preview        │ Editable fields    │
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

// ─── Public entry point ──────────────────────────────────────────────────────

/// Draw the full TUI frame.
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

// ─── Sidebar ─────────────────────────────────────────────────────────────────

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Sidebar;
    let border_style = panel_border_style(focused);

    let title = if focused {
        format!(" ● Widgets ({}) ", app.widget_count())
    } else {
        format!(" Widgets ({}) ", app.widget_count())
    };
    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(panel_border_type(focused));

    if focused {
        block = block.title_bottom(Line::from(Span::styled(
            " ↑/↓:select  ⏎:edit  a:add  d:del  Ctrl+↑/↓:reorder ",
            Style::default().fg(palette::OVERLAY1),
        )));
    }

    let inner = block.inner(area);

    if let Some(theme) = &app.theme {
        let items: Vec<ListItem> = theme
            .widgets
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let icon = widget_icon(w);
                let label = widget_short_label(w);
                let text = format!("{} {}", icon, label);
                let style = if app.selected_widget == Some(i) {
                    Style::default().fg(palette::CRUST).bg(if focused {
                        palette::BLUE
                    } else {
                        palette::SURFACE2
                    })
                } else {
                    Style::default().fg(canvas::widget_color(w))
                };
                ListItem::new(Line::from(Span::styled(text, style)))
            })
            .collect();

        let mut state = ListState::default();
        state.select(app.selected_widget);

        let list = List::new(items).block(block);
        f.render_stateful_widget(list, area, &mut state);
    } else {
        let hint = Paragraph::new(Line::from(Span::styled(
            " No theme loaded\n Use --theme",
            Style::default().fg(palette::SUBTEXT0),
        )))
        .block(block)
        .wrap(Wrap { trim: false });
        f.render_widget(hint, area);
        let _ = inner;
    }
}

// ─── Properties panel ────────────────────────────────────────────────────────

fn draw_properties(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Properties;
    let border_style = panel_border_style(focused);

    let title = if focused {
        " ● Properties "
    } else {
        " Properties "
    };
    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(panel_border_type(focused));

    if focused {
        let hint = if app.prop_input.is_some() {
            " ⏎:confirm  Esc:cancel "
        } else {
            " ⏎:edit  ↑/↓:select "
        };
        block = block.title_bottom(Line::from(Span::styled(
            hint,
            Style::default().fg(palette::OVERLAY1),
        )));
    }

    match app.selected_widget_ref() {
        None => {
            let p = Paragraph::new(Line::from(Span::styled(
                " Select a widget",
                Style::default().fg(palette::SUBTEXT0),
            )))
            .block(block);
            f.render_widget(p, area);
        }
        Some(w) => {
            let inner = block.inner(area);
            f.render_widget(block, area);

            let fields = widget_fields(w);
            let field_count = fields.len();

            // Reserve 1 row at the bottom only for validation errors.
            let (list_area, error_area) = if app.prop_error.is_some() && inner.height >= 3 {
                let s = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(inner);
                (s[0], Some(s[1]))
            } else {
                (inner, None)
            };

            // Field rows
            let lines: Vec<ListItem> = fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let is_cursor = i == app.prop_cursor;
                    let name_style = if is_cursor && focused {
                        Style::default()
                            .fg(palette::CRUST)
                            .bg(palette::MAUVE)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(palette::SUBTEXT0)
                    };
                    let val_style = if is_cursor && focused {
                        Style::default().fg(palette::CRUST).bg(palette::MAUVE)
                    } else {
                        Style::default().fg(palette::TEXT)
                    };
                    // If this row is being edited, show the live text-input value
                    let value = if is_cursor && app.prop_input.is_some() {
                        app.prop_input
                            .as_ref()
                            .map(|inp| inp.display())
                            .unwrap_or_else(|| field.value.clone())
                    } else {
                        display_field_value(field)
                    };
                    let line = Line::from(vec![
                        Span::styled(format!(" {:<12}", field.name), name_style),
                        Span::styled(value, val_style),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let mut list_state = ListState::default();
            if focused {
                list_state.select(Some(app.prop_cursor.min(field_count.saturating_sub(1))));
            }

            let list = List::new(lines);
            f.render_stateful_widget(list, list_area, &mut list_state);

            if let (Some(footer), Some(err)) = (error_area, app.prop_error.as_ref()) {
                let footer_content = Line::from(Span::styled(
                    format!(" ✖ {}", err),
                    Style::default().fg(palette::RED),
                ));
                f.render_widget(Paragraph::new(footer_content), footer);
            }
        }
    }
}

// ─── Log panel ────────────────────────────────────────────────────────────────

fn draw_metric_preview_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Metrics ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::SAPPHIRE))
        .border_type(BorderType::Rounded);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let metric_rows = [
        ("CPU", "cpu_temp", palette::PEACH),
        ("CPU%", "cpu_usage", palette::SAPPHIRE),
        ("MEM%", "mem_usage", palette::TEAL),
        ("GPU", "gpu_temp", palette::MAUVE),
        ("GPU%", "gpu_usage", palette::BLUE),
    ];

    if inner.height == 0 {
        return;
    }

    let constraints: Vec<Constraint> = (0..metric_rows.len())
        .map(|_| Constraint::Length(1))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (row_area, (label, key, color)) in rows.iter().zip(metric_rows.iter()) {
        if row_area.height == 0 {
            continue;
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(14), Constraint::Min(0)])
            .split(*row_area);

        let (value, value_style) = if let Some(value) = app.metrics.values.get(*key) {
            (
                value.as_str(),
                Style::default()
                    .fg(palette::TEXT)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("--", Style::default().fg(palette::OVERLAY1))
        };

        let label_line = Line::from(vec![
            Span::styled(
                format!("{:<4}", label),
                Style::default().fg(palette::SUBTEXT0),
            ),
            Span::styled(format!(" {:>7}", value), value_style),
            Span::styled(" |", Style::default().fg(palette::OVERLAY1)),
        ]);
        f.render_widget(Paragraph::new(label_line), cols[0]);

        if cols[1].width > 0 {
            let values: Vec<u64> = app
                .metric_history
                .get(*key)
                .map(|history| history.iter().copied().collect())
                .unwrap_or_else(|| vec![0]);
            let sparkline = Sparkline::default()
                .data(values.as_slice())
                .max(100)
                .style(Style::default().fg(*color));
            f.render_widget(sparkline, cols[1]);
        }
    }
}

fn draw_log_panel(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" Log ({}) ", app.log_lines.len());

    let hint = if app.log_is_scrolled() {
        format!(" PageUp/PageDown:scroll  +{} ", app.log_scroll)
    } else {
        " PageUp/PageDown:scroll ".to_string()
    };

    let block = Block::default()
        .title(title)
        .title_bottom(Line::from(Span::styled(
            hint,
            Style::default().fg(palette::OVERLAY1),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::TEAL))
        .border_type(BorderType::Rounded);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = app
        .visible_log_lines()
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                format!(" {}", line),
                Style::default().fg(palette::SUBTEXT0),
            ))
        })
        .collect();

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " waiting for activity...",
            Style::default().fg(palette::OVERLAY1),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

// ─── Status bar ──────────────────────────────────────────────────────────────

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let dirty_marker = if app.dirty { "*" } else { " " };
    let theme_indicator = format!("{}{} ", dirty_marker, app.theme_name());

    let push_part = match &app.push_status {
        PushStatus::None => Span::raw(""),
        PushStatus::PushInProgress => {
            Span::styled(" … pushing … ", Style::default().fg(palette::YELLOW))
        }
        PushStatus::RotateInProgress => {
            Span::styled(" … rotating … ", Style::default().fg(palette::YELLOW))
        }
        PushStatus::RotateOk(msg) => {
            Span::styled(format!(" ✓ {} ", msg), Style::default().fg(palette::GREEN))
        }
        PushStatus::PushOk => Span::styled(" ✓ pushed ", Style::default().fg(palette::GREEN)),
        PushStatus::SaveOk => Span::styled(" ✓ saved ", Style::default().fg(palette::GREEN)),
        PushStatus::OpenOk => Span::styled(" ✓ opened ", Style::default().fg(palette::GREEN)),
        PushStatus::Err(e) => Span::styled(format!(" ✖ {} ", e), Style::default().fg(palette::RED)),
    };

    let focus_name = match app.focus {
        Focus::Sidebar => "Sidebar",
        Focus::Canvas => "Canvas",
        Focus::Properties => "Properties",
    };

    let hints = " Ctrl+n:new  Ctrl+s:save  Ctrl+o:open  p:push  r:rotate  Ctrl+r:auto-rotate  Tab:focus  q:quit  ?:help";

    let left = Span::styled(
        theme_indicator,
        Style::default().fg(palette::TEXT).bg(palette::SURFACE0),
    );
    let hints_span = Span::styled(hints, Style::default().fg(palette::SUBTEXT0));
    let left_line = Line::from(vec![left, push_part, hints_span]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(18)])
        .split(area);

    let left_bar = Paragraph::new(left_line).style(Style::default().bg(palette::BASE));
    f.render_widget(left_bar, cols[0]);

    let focus_badge = format!(" Focus:{:<10} ", focus_name);
    let right_bar = Paragraph::new(Line::from(Span::styled(
        focus_badge,
        Style::default()
            .fg(palette::CRUST)
            .bg(palette::TEAL)
            .add_modifier(Modifier::BOLD),
    )))
    .style(Style::default().bg(palette::BASE))
    .alignment(Alignment::Right);
    f.render_widget(right_bar, cols[1]);
}

// ─── Help overlay ────────────────────────────────────────────────────────────

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup_w = 84u16.min(area.width.saturating_sub(4));
    let popup_h = 28u16.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help — Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER));

    let keys: &[(&str, &str)] = &[
        ("Tab / Shift+Tab", "Cycle panel focus"),
        ("↑ / k", "Previous widget / field"),
        ("↓ / j", "Next widget / field"),
        ("Enter", "Edit/select/toggle field"),
        ("Esc", "Cancel / back to sidebar"),
        ("a", "Add new widget (sidebar)"),
        ("d", "Delete selected widget"),
        ("Ctrl+↑ / Ctrl+↓", "Reorder widget in list"),
        ("←↑↓→", "Move widget (canvas, 1px)"),
        ("Shift+←↑↓→", "Move widget (canvas, 10px)"),
        ("p", "Push theme to device"),
        ("r", "Cycle rotations"),
        ("Ctrl+n", "Create new theme"),
        ("Ctrl+r", "Enable auto-rotation on device"),
        ("Ctrl+s", "Save theme explorer (.toml)"),
        ("Ctrl+o", "Open theme explorer (.toml)"),
        ("Enter (media path)", "Open image/video file chooser"),
        ("PgUp / PgDn", "Scroll log panel history"),
        ("Tab (save/open)", "Switch file list/path input"),
        ("Backspace", "Save/Open dialog: parent directory"),
        (".", "Save/Open dialog: toggle hidden files"),
        ("q / Ctrl+c", "Quit"),
        ("F1 / ?", "Toggle this help"),
    ];

    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<20}", k),
                    Style::default()
                        .fg(palette::PEACH)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*v, Style::default().fg(palette::TEXT)),
            ])
        })
        .collect();

    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, popup_area);
}

// ─── Add-widget overlay ───────────────────────────────────────────────────────

fn draw_add_widget_overlay(f: &mut Frame, area: Rect, cursor: usize) {
    let popup_w = 30u16.min(area.width.saturating_sub(4));
    let popup_h = (NewWidgetKind::ALL.len() as u16 + 4).min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Add widget — type ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER));

    let items: Vec<ListItem> = NewWidgetKind::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let style = if i == cursor {
                Style::default().fg(palette::CRUST).bg(palette::BLUE)
            } else {
                Style::default().fg(palette::TEXT)
            };
            ListItem::new(Line::from(Span::styled(format!("  {} ", k.label()), style)))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(cursor));

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_field_dropdown_overlay(
    f: &mut Frame,
    area: Rect,
    field_name: &str,
    options: &[&str],
    cursor: usize,
) {
    let max_opt_len = options.iter().map(|o| o.len()).max().unwrap_or(8) as u16;
    let popup_w = (max_opt_len + 10).max(26).min(area.width.saturating_sub(4));
    let popup_h = (options.len() as u16 + 4).min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(" Select {} ", field_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER));

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == cursor {
                Style::default().fg(palette::CRUST).bg(palette::BLUE)
            } else {
                Style::default().fg(palette::TEXT)
            };
            ListItem::new(Line::from(Span::styled(format!("  {} ", opt), style)))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(cursor.min(options.len().saturating_sub(1))));

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, popup_area, &mut state);
}

fn draw_color_picker_overlay(
    f: &mut Frame,
    area: Rect,
    field_name: &str,
    cursor: usize,
    input: &TextInput,
    input_active: bool,
) {
    let rows = COLOR_PALETTE.len().div_ceil(COLOR_PALETTE_COLUMNS.max(1));
    let popup_w = 44u16.min(area.width.saturating_sub(4));
    let popup_h = (rows as u16 + 8).min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(" Color picker — {} ", field_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::TEAL));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let mut lines: Vec<Line> = Vec::new();

    let hint = if input_active {
        " ⏎:apply  Tab:palette  Esc:cancel"
    } else {
        " ←↑↓→:select  ⏎:apply  Tab:hex  Esc:cancel"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(palette::OVERLAY1),
    )));

    for row in 0..rows {
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        for col in 0..COLOR_PALETTE_COLUMNS {
            let idx = row * COLOR_PALETTE_COLUMNS + col;
            if idx >= COLOR_PALETTE.len() {
                break;
            }

            let hex = COLOR_PALETTE[idx];
            let swatch = parse_hex_color(hex).unwrap_or(palette::BASE);
            let selected = idx == cursor && !input_active;
            let border_style = if selected {
                Style::default()
                    .fg(palette::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette::OVERLAY0)
            };

            spans.push(Span::styled(if selected { "[" } else { " " }, border_style));
            spans.push(Span::styled("  ", Style::default().bg(swatch)));
            spans.push(Span::styled(if selected { "]" } else { " " }, border_style));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(Span::raw("")));

    let hex_label_style = if input_active {
        Style::default()
            .fg(palette::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::SUBTEXT0)
    };
    let hex_value = if input_active {
        input.display()
    } else {
        input.value.clone()
    };
    let hex_value_style = if input_active {
        Style::default().fg(palette::CRUST).bg(palette::BLUE)
    } else {
        Style::default().fg(palette::TEXT)
    };

    lines.push(Line::from(vec![
        Span::styled(" Hex: ", hex_label_style),
        Span::styled(hex_value, hex_value_style),
    ]));

    let preview = parse_hex_color(&input.value).unwrap_or(palette::BASE);
    lines.push(Line::from(vec![
        Span::styled(" Preview: ", Style::default().fg(palette::SUBTEXT0)),
        Span::styled("      ", Style::default().bg(preview)),
        Span::raw(" "),
        Span::styled(input.value.clone(), Style::default().fg(palette::TEXT)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

// ─── Delete-confirm overlay ──────────────────────────────────────────────────

fn draw_delete_confirm_overlay(f: &mut Frame, area: Rect, idx: usize, app: &App) {
    let popup_w = 44u16.min(area.width.saturating_sub(4));
    let popup_h = 6u16.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let label = app
        .selected_widget_ref()
        .map(widget_short_label)
        .unwrap_or_else(|| format!("widget #{}", idx + 1));

    let block = Block::default()
        .title(" Confirm delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::RED));

    let lines = vec![
        Line::from(Span::styled(
            format!("  Delete \"{}\"?", label),
            Style::default().fg(palette::TEXT),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled(
                "  y",
                Style::default()
                    .fg(palette::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/⏎", Style::default().fg(palette::TEXT)),
            Span::styled(" — yes      ", Style::default().fg(palette::OVERLAY1)),
            Span::styled(
                "n",
                Style::default()
                    .fg(palette::RED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/Esc — no", Style::default().fg(palette::TEXT)),
        ]),
    ];

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, popup_area);
}

// ─── Save / Open explorer overlays ───────────────────────────────────────────

fn draw_new_theme_overlay(f: &mut Frame, area: Rect, state: &NewThemeDialogState) {
    let popup_w = 72u16.min(area.width.saturating_sub(4));
    let popup_h = 11u16.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" New theme ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER))
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let file_value = if state.active_field == 0 {
        state.file_input.display()
    } else {
        state.file_input.value.clone()
    };
    let name_value = if state.active_field == 1 {
        state.name_input.display()
    } else {
        state.name_input.value.clone()
    };
    let desc_value = if state.active_field == 2 {
        state.description_input.display()
    } else {
        state.description_input.value.clone()
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" File: ", Style::default().fg(palette::SUBTEXT0)),
            Span::styled(
                file_value,
                if state.active_field == 0 {
                    Style::default().fg(palette::CRUST).bg(palette::BLUE)
                } else {
                    Style::default().fg(palette::TEXT)
                },
            ),
        ])),
        sections[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Name: ", Style::default().fg(palette::SUBTEXT0)),
            Span::styled(
                name_value,
                if state.active_field == 1 {
                    Style::default().fg(palette::CRUST).bg(palette::BLUE)
                } else {
                    Style::default().fg(palette::TEXT)
                },
            ),
        ])),
        sections[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Description: ", Style::default().fg(palette::SUBTEXT0)),
            Span::styled(
                desc_value,
                if state.active_field == 2 {
                    Style::default().fg(palette::CRUST).bg(palette::BLUE)
                } else {
                    Style::default().fg(palette::TEXT)
                },
            ),
        ])),
        sections[2],
    );

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ⏎:next/create  Tab:next  Shift+Tab:prev  Esc:cancel",
            Style::default().fg(palette::OVERLAY1),
        ))),
        sections[3],
    );

    if let Some(err) = &state.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" ✖ {}", err),
                Style::default().fg(palette::RED),
            ))),
            sections[4],
        );
    }
}

fn draw_save_overlay(f: &mut Frame, area: Rect, state: &SaveDialogState) {
    let popup_w = area.width.saturating_sub(8).max(40);
    let popup_h = area.height.saturating_sub(6).max(12);
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(" Save theme — {} ", state.explorer.cwd().display());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER))
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget_ref(state.explorer.widget(), sections[0]);

    let selected = state.explorer.current();
    let selected_style = if selected.is_dir {
        Style::default().fg(palette::PEACH)
    } else {
        Style::default().fg(palette::GREEN)
    };
    let selected_line = Line::from(vec![
        Span::styled(" Selected: ", Style::default().fg(palette::SUBTEXT0)),
        Span::styled(selected.path.display().to_string(), selected_style),
    ]);
    f.render_widget(Paragraph::new(selected_line), sections[1]);

    let input_value = if state.input_active {
        state.path_input.display()
    } else {
        state.path_input.value.clone()
    };
    let input_line = Line::from(vec![
        Span::styled(" Path: ", Style::default().fg(palette::SUBTEXT0)),
        Span::styled(
            input_value,
            if state.input_active {
                Style::default().fg(palette::CRUST).bg(palette::BLUE)
            } else {
                Style::default().fg(palette::TEXT)
            },
        ),
    ]);
    f.render_widget(Paragraph::new(input_line), sections[2]);

    let footer = if let Some(err) = &state.error {
        Line::from(Span::styled(
            format!(" ✖ {}", err),
            Style::default().fg(palette::RED),
        ))
    } else if state.input_active {
        Line::from(Span::styled(
            " ⏎:save  Tab:list  Esc:cancel",
            Style::default().fg(palette::OVERLAY1),
        ))
    } else {
        Line::from(Span::styled(
            " ⏎:pick/open dir  Backspace:parent  .:hidden  Tab:path  Esc:cancel",
            Style::default().fg(palette::OVERLAY1),
        ))
    };
    f.render_widget(Paragraph::new(footer), sections[3]);
}

fn draw_open_overlay(f: &mut Frame, area: Rect, state: &OpenDialogState) {
    let popup_w = area.width.saturating_sub(8).max(40);
    let popup_h = area.height.saturating_sub(6).max(12);
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(" Open theme — {} ", state.explorer.cwd().display());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER))
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget_ref(state.explorer.widget(), sections[0]);

    let selected = state.explorer.current();
    let selected_style = if selected.is_dir {
        Style::default().fg(palette::PEACH)
    } else {
        Style::default().fg(palette::GREEN)
    };
    let selected_line = Line::from(vec![
        Span::styled(" Selected: ", Style::default().fg(palette::SUBTEXT0)),
        Span::styled(selected.path.display().to_string(), selected_style),
    ]);
    f.render_widget(Paragraph::new(selected_line), sections[1]);

    let footer = if let Some(err) = &state.error {
        Line::from(Span::styled(
            format!(" ✖ {}", err),
            Style::default().fg(palette::RED),
        ))
    } else {
        Line::from(Span::styled(
            " ⏎:open  Backspace:parent  .:hidden  Esc:cancel",
            Style::default().fg(palette::OVERLAY1),
        ))
    };
    f.render_widget(Paragraph::new(footer), sections[2]);
}

fn draw_media_path_overlay(f: &mut Frame, area: Rect, state: &MediaPathDialogState) {
    let popup_w = area.width.saturating_sub(8).max(40);
    let popup_h = area.height.saturating_sub(6).max(12);
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(
        " Select {} path — {} ",
        state.media_kind.display_name(),
        state.explorer.cwd().display()
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER))
        .border_type(BorderType::Rounded);
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget_ref(state.explorer.widget(), sections[0]);

    let selected = state.explorer.current();
    let selected_style = if selected.is_dir {
        Style::default().fg(palette::PEACH)
    } else {
        Style::default().fg(palette::GREEN)
    };
    let selected_line = Line::from(vec![
        Span::styled(" Selected: ", Style::default().fg(palette::SUBTEXT0)),
        Span::styled(selected.path.display().to_string(), selected_style),
    ]);
    f.render_widget(Paragraph::new(selected_line), sections[1]);

    let footer = if let Some(err) = &state.error {
        Line::from(Span::styled(
            format!(" ✖ {}", err),
            Style::default().fg(palette::RED),
        ))
    } else {
        Line::from(Span::styled(
            " ⏎:pick/open dir  Backspace:parent  .:hidden  Esc:cancel",
            Style::default().fg(palette::OVERLAY1),
        ))
    };
    f.render_widget(Paragraph::new(footer), sections[2]);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn display_field_value(field: &Field) -> String {
    match field.kind {
        FieldType::Toggle => {
            if field.value.eq_ignore_ascii_case("true") {
                "[x] true".to_string()
            } else {
                "[ ] false".to_string()
            }
        }
        _ => field.value.clone(),
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let normalized = hex.trim().trim_start_matches('#');
    if normalized.len() != 6 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let r = u8::from_str_radix(&normalized[0..2], 16).ok()?;
    let g = u8::from_str_radix(&normalized[2..4], 16).ok()?;
    let b = u8::from_str_radix(&normalized[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

fn panel_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::SURFACE2)
    }
}

fn panel_border_type(focused: bool) -> BorderType {
    if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    }
}

fn widget_icon(w: &Widget) -> &'static str {
    match &w.kind {
        WidgetKind::Metric { .. } => "▸",
        WidgetKind::Clock { .. } => "⏱",
        WidgetKind::Image { .. } => "▣",
        WidgetKind::Video { .. } => "▶",
        WidgetKind::Text { .. } => "T",
    }
}

fn widget_short_label(w: &Widget) -> String {
    match &w.kind {
        WidgetKind::Metric { source, unit, .. } => {
            format!("{:?} {}", source, unit)
        }
        WidgetKind::Clock { time_format } => format!("Clock {:?}", time_format),
        WidgetKind::Image { path } => {
            if path.is_empty() {
                "Image".to_string()
            } else {
                format!("Img:{}", path)
            }
        }
        WidgetKind::Video { path } => {
            if path.is_empty() {
                "Video".to_string()
            } else {
                format!("Vid:{}", path)
            }
        }
        WidgetKind::Text { content } => {
            let mut chars = content.chars();
            let preview: String = chars.by_ref().take(12).collect();
            if chars.next().is_some() {
                format!("\"{}…\"", preview)
            } else {
                format!("\"{}\"", content)
            }
        }
    }
}

/// Return a `Rect` centered within `r` with the given width and height.
fn centered_rect(w: u16, h: u16, r: Rect) -> Rect {
    let x = r.x + r.width.saturating_sub(w) / 2;
    let y = r.y + r.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(r.width), h.min(r.height))
}
