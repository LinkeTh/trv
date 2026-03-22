/// Main UI layout renderer — M5 edition.
///
/// Layout (horizontal split):
///   ┌─ Sidebar (22%) ─┬───── Canvas (56%) ─────┬─ Properties (22%) ─┐
///   │ Widget list     │  484×480 preview        │ Editable fields    │
///   └─────────────────┴─────────────────────────┴────────────────────┘
///   ┤ Status bar (1 row at bottom)                                    │
///
/// Overlays are rendered on top as centered popups:
///   Help, AddWidget picker, DeleteConfirm, Save dialog, Open dialog.
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::theme::model::{Widget, WidgetKind};

use super::app::{
    App, COLOR_PALETTE, COLOR_PALETTE_COLUMNS, Focus, NewWidgetKind, Overlay, PushStatus,
};
use super::canvas;
use super::fields::{Field, FieldType, widget_fields};
use super::input::TextInput;

// ─── Public entry point ──────────────────────────────────────────────────────

/// Draw the full TUI frame.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Split off the bottom status bar (1 row)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = rows[0];
    let status_area = rows[1];

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
        Overlay::Save { input } => draw_path_dialog(f, area, "Save theme as", &input.display()),
        Overlay::Open { input } => draw_path_dialog(f, area, "Open theme", &input.display()),
    }
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Sidebar;
    let border_style = panel_border_style(focused);

    let title = format!(" Widgets ({}) ", app.widget_count());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let hint_line = Line::from(Span::styled(
        " a:add  d:del  ^↑/↓:reorder",
        Style::default().fg(Color::DarkGray),
    ));

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
                    Style::default().fg(Color::Black).bg(if focused {
                        Color::LightCyan
                    } else {
                        Color::Gray
                    })
                } else {
                    Style::default().fg(canvas::widget_color(w))
                };
                ListItem::new(Line::from(Span::styled(text, style)))
            })
            .collect();

        let mut state = ListState::default();
        state.select(app.selected_widget);

        // Reserve 1 row at the bottom of the inner area for the hint.
        if inner.height > 1 {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);

            let list = List::new(items).block(block);
            f.render_stateful_widget(list, area, &mut state);
            f.render_widget(Paragraph::new(hint_line), split[1]);
        } else {
            let list = List::new(items).block(block);
            f.render_stateful_widget(list, area, &mut state);
        }
    } else {
        let hint = Paragraph::new(Line::from(Span::styled(
            " No theme loaded\n Use --theme",
            Style::default().fg(Color::DarkGray),
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

    let block = Block::default()
        .title(" Properties ")
        .borders(Borders::ALL)
        .border_style(border_style);

    match app.selected_widget_ref() {
        None => {
            let p = Paragraph::new(Line::from(Span::styled(
                " Select a widget",
                Style::default().fg(Color::DarkGray),
            )))
            .block(block);
            f.render_widget(p, area);
        }
        Some(w) => {
            let inner = block.inner(area);
            f.render_widget(block, area);

            let fields = widget_fields(w);
            let field_count = fields.len();

            // Reserve 1 row at the bottom for either the edit input or an error.
            let (list_area, footer_area) = if inner.height >= 3 {
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
                            .fg(Color::Black)
                            .bg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    let val_style = if is_cursor && focused {
                        Style::default().fg(Color::Black).bg(Color::LightCyan)
                    } else {
                        Style::default().fg(Color::White)
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

            // Footer: validation error or edit hint
            if let Some(footer) = footer_area {
                let footer_content = if let Some(err) = &app.prop_error {
                    Line::from(Span::styled(
                        format!(" ✖ {}", err),
                        Style::default().fg(Color::Red),
                    ))
                } else if app.prop_input.is_some() {
                    Line::from(Span::styled(
                        " Enter:confirm  Esc:cancel",
                        Style::default().fg(Color::DarkGray),
                    ))
                } else if focused {
                    Line::from(Span::styled(
                        " Enter:edit/select  Space:toggle  ↑↓:navigate",
                        Style::default().fg(Color::DarkGray),
                    ))
                } else {
                    Line::from(Span::raw(""))
                };
                f.render_widget(Paragraph::new(footer_content), footer);
            }
        }
    }
}

// ─── Status bar ──────────────────────────────────────────────────────────────

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let dirty_marker = if app.dirty { "*" } else { " " };
    let theme_indicator = format!("{}{} ", dirty_marker, app.theme_name());

    let push_part = match &app.push_status {
        PushStatus::None => Span::raw(""),
        PushStatus::PushInProgress => {
            Span::styled(" … pushing … ", Style::default().fg(Color::Yellow))
        }
        PushStatus::PushOk => Span::styled(" ✓ pushed ", Style::default().fg(Color::Green)),
        PushStatus::SaveOk => Span::styled(" ✓ saved ", Style::default().fg(Color::Green)),
        PushStatus::OpenOk => Span::styled(" ✓ opened ", Style::default().fg(Color::Green)),
        PushStatus::Err(e) => Span::styled(format!(" ✖ {} ", e), Style::default().fg(Color::Red)),
    };

    // Live metrics strip (shown when at least one reading is available).
    let m = &app.metrics.values;
    let mut metrics_spans: Vec<Span> = Vec::new();
    for (key, label) in &[
        ("cpu_temp", "CPU"),
        ("cpu_usage", "CPU%"),
        ("mem_usage", "MEM%"),
        ("gpu_temp", "GPU"),
        ("gpu_usage", "GPU%"),
    ] {
        if let Some(val) = m.get(*key) {
            metrics_spans.push(Span::styled(
                format!(" {}:{} ", label, val),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

    let hints = " Tab:focus  ↑↓/jk:nav  Enter:edit/select  Space:toggle  a:add  d:del  P:push  ^S:save  ^O:open  q:quit  ?:help";

    let left = Span::styled(
        theme_indicator,
        Style::default().fg(Color::White).bg(Color::DarkGray),
    );
    let right = Span::styled(hints, Style::default().fg(Color::DarkGray));

    let mut spans = vec![left, push_part];
    spans.extend(metrics_spans);
    spans.push(right);

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
    f.render_widget(bar, area);
}

// ─── Help overlay ────────────────────────────────────────────────────────────

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup_w = 56u16.min(area.width.saturating_sub(4));
    let popup_h = 26u16.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help — Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan));

    let keys: &[(&str, &str)] = &[
        ("Tab / Shift+Tab", "Cycle panel focus"),
        ("↑ / k", "Previous widget / field"),
        ("↓ / j", "Next widget / field"),
        ("Enter", "Edit/select/toggle field"),
        ("Space", "Toggle boolean field"),
        ("Esc", "Cancel / back to sidebar"),
        ("a", "Add new widget (sidebar)"),
        ("d", "Delete selected widget"),
        ("Ctrl+↑ / Ctrl+↓", "Reorder widget in list"),
        ("← ↑ → ↓", "Move widget (canvas, 1px)"),
        ("Shift+arrows", "Move widget (canvas, 10px)"),
        ("P", "Push theme to device"),
        ("Ctrl+S", "Save theme to file"),
        ("Ctrl+O", "Open theme from file"),
        ("q / Ctrl+C", "Quit"),
        ("F1 / ?", "Toggle this help"),
    ];

    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<20}", k),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*v, Style::default().fg(Color::White)),
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
        .border_style(Style::default().fg(Color::LightCyan));

    let items: Vec<ListItem> = NewWidgetKind::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let style = if i == cursor {
                Style::default().fg(Color::Black).bg(Color::LightCyan)
            } else {
                Style::default().fg(Color::White)
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
        .border_style(Style::default().fg(Color::LightCyan));

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == cursor {
                Style::default().fg(Color::Black).bg(Color::LightCyan)
            } else {
                Style::default().fg(Color::White)
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
        .border_style(Style::default().fg(Color::LightCyan));
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let mut lines: Vec<Line> = Vec::new();

    let hint = if input_active {
        " Enter:apply  Tab:palette  Esc:cancel"
    } else {
        " Arrows/hjkl:select  Enter:apply  Tab:hex  Esc:cancel"
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));

    for row in 0..rows {
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        for col in 0..COLOR_PALETTE_COLUMNS {
            let idx = row * COLOR_PALETTE_COLUMNS + col;
            if idx >= COLOR_PALETTE.len() {
                break;
            }

            let hex = COLOR_PALETTE[idx];
            let swatch = parse_hex_color(hex).unwrap_or(Color::Black);
            let selected = idx == cursor && !input_active;
            let border_style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
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
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let hex_value = if input_active {
        input.display()
    } else {
        input.value.clone()
    };
    let hex_value_style = if input_active {
        Style::default().fg(Color::Black).bg(Color::LightCyan)
    } else {
        Style::default().fg(Color::White)
    };

    lines.push(Line::from(vec![
        Span::styled(" Hex: ", hex_label_style),
        Span::styled(hex_value, hex_value_style),
    ]));

    let preview = parse_hex_color(&input.value).unwrap_or(Color::Black);
    lines.push(Line::from(vec![
        Span::styled(" Preview: ", Style::default().fg(Color::DarkGray)),
        Span::styled("      ", Style::default().bg(preview)),
        Span::raw(" "),
        Span::styled(input.value.clone(), Style::default().fg(Color::White)),
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
        .map(|w| widget_short_label(w))
        .unwrap_or_else(|| format!("widget #{}", idx + 1));

    let block = Block::default()
        .title(" Confirm delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let lines = vec![
        Line::from(Span::styled(
            format!("  Delete \"{}\"?", label),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled(
                "  y",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("/Enter", Style::default().fg(Color::White)),
            Span::styled(" — yes      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "n",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("/Esc — no", Style::default().fg(Color::White)),
        ]),
    ];

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, popup_area);
}

// ─── Save / Open path dialog ──────────────────────────────────────────────────

fn draw_path_dialog(f: &mut Frame, area: Rect, title: &str, input_display: &str) {
    let popup_w = 60u16.min(area.width.saturating_sub(4));
    let popup_h = 5u16.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let title = format!(" {} ", title);
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", input_display),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            " Enter:confirm  Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    f.render_widget(Paragraph::new(lines), inner);
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
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
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
