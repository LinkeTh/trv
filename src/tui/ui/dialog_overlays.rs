use super::*;

pub(super) fn draw_new_theme_overlay(f: &mut Frame, area: Rect, state: &NewThemeDialogState) {
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

pub(super) fn draw_save_overlay(f: &mut Frame, area: Rect, state: &SaveDialogState) {
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

pub(super) fn draw_open_overlay(f: &mut Frame, area: Rect, state: &OpenDialogState) {
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

pub(super) fn draw_media_path_overlay(f: &mut Frame, area: Rect, state: &MediaPathDialogState) {
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
