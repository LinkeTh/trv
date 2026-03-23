use super::*;

impl App {
    // ── Add widget overlay ────────────────────────────────────────────────────

    pub(super) fn handle_add_widget_key(&mut self, key: KeyEvent) {
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

    pub(super) fn handle_delete_confirm_key(&mut self, key: KeyEvent) {
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

    pub(super) fn begin_new_theme(&mut self) {
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

    pub(super) fn handle_new_theme_key(&mut self, key: KeyEvent) {
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
}
