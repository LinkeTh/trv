use super::*;

pub(super) fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Global",
            &[
                ("F1 / ?", "Toggle this help"),
                ("Tab / Shift+Tab", "Cycle panel focus"),
                ("Ctrl+n", "Create new theme"),
                ("Ctrl+s / Ctrl+o", "Save/Open theme explorer (.toml)"),
                ("p / r / Ctrl+r", "Push, rotate, auto-rotate"),
                ("PgUp / PgDn", "Scroll log panel history"),
                ("q / Ctrl+c", "Quit"),
            ],
        ),
        (
            "Sidebar panel",
            &[
                ("↑/k and ↓/j", "Select widget"),
                ("Enter", "Focus properties"),
                ("a / d", "Add or delete widget"),
                ("Ctrl+↑ / Ctrl+↓", "Reorder widget list"),
            ],
        ),
        (
            "Canvas panel",
            &[
                ("←↑↓→", "Move selected widget (1px)"),
                ("Shift+←↑↓→", "Move selected widget (10px)"),
                ("j / k", "Select next/previous widget"),
            ],
        ),
        (
            "Properties panel",
            &[
                ("↑/k and ↓/j", "Select field"),
                ("Enter", "Edit or toggle field"),
                ("Esc", "Cancel field edit / back to sidebar"),
                ("Enter (media path)", "Open image/video chooser"),
            ],
        ),
        (
            "Dialogs & overlays",
            &[
                ("Esc", "Cancel/close dialog"),
                ("Tab", "Switch list/path or picker input"),
                ("Backspace / .", "Parent directory / toggle hidden files"),
                ("Y/N or Enter", "Delete dialog: confirm/cancel"),
            ],
        ),
    ];

    let section_style = Style::default()
        .fg(palette::TEAL)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(palette::PEACH)
        .add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(palette::TEXT);

    let mut lines: Vec<Line> = Vec::new();
    for (section_idx, (section, bindings)) in sections.iter().enumerate() {
        if section_idx > 0 {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            format!(" {} ", section),
            section_style,
        )));

        for (keys, desc) in bindings.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<24}", keys), key_style),
                Span::styled(*desc, value_style),
            ]));
        }
    }

    let popup_w = 96u16.min(area.width.saturating_sub(4));
    let desired_h = lines.len().saturating_add(2) as u16;
    let popup_h = desired_h.min(area.height.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Help — Keybindings by panel ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette::LAVENDER));

    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, popup_area);
}

pub(super) fn draw_add_widget_overlay(f: &mut Frame, area: Rect, cursor: usize) {
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

pub(super) fn draw_field_dropdown_overlay(
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

pub(super) fn draw_color_picker_overlay(
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

pub(super) fn draw_delete_confirm_overlay(f: &mut Frame, area: Rect, idx: usize, app: &App) {
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
