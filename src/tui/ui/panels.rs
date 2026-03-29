use super::*;

pub(super) fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_properties(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_metric_preview_panel(f: &mut Frame, app: &App, area: Rect) {
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
            .constraints([Constraint::Length(16), Constraint::Min(0)])
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

pub(super) fn draw_log_panel(f: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
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
