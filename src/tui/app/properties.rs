use super::*;

impl App {
    // ── Properties keys ───────────────────────────────────────────────────────

    pub(super) fn handle_properties_key(&mut self, key: KeyEvent) {
        // If inline editor is active, route to it.
        if self.prop_input.is_some() {
            self.handle_prop_input_key(key);
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Sidebar;
            }
            KeyCode::Up => {
                if self.prop_cursor > 0 {
                    self.prop_cursor -= 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Char('k') if no_ctrl_alt(&key) => {
                if self.prop_cursor > 0 {
                    self.prop_cursor -= 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Down => {
                let max = self.field_count().saturating_sub(1);
                if self.prop_cursor < max {
                    self.prop_cursor += 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Char('j') if no_ctrl_alt(&key) => {
                let max = self.field_count().saturating_sub(1);
                if self.prop_cursor < max {
                    self.prop_cursor += 1;
                    self.prop_error = None;
                }
            }
            KeyCode::Enter => {
                self.activate_property_editor();
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
            FieldType::MediaPath(kind) => {
                self.begin_media_path_picker(&field, kind);
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

    fn begin_media_path_picker(&mut self, field: &Field, media_kind: MediaPathKind) {
        match self.build_media_path_dialog_state(field.name, media_kind, &field.value) {
            Ok(state) => {
                let cwd = state.explorer.cwd().display().to_string();
                self.overlay = Overlay::MediaPath {
                    state: Box::new(state),
                };
                self.prop_error = None;
                self.log_event(format!(
                    "{} path picker opened at {}",
                    media_kind.title_name(),
                    cwd
                ));
            }
            Err(e) => {
                self.prop_error = Some(format!("{} path picker: {}", media_kind.display_name(), e));
            }
        }
    }

    pub(super) fn apply_field_value(&mut self, field_name: &'static str, value: &str) {
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

    pub(super) fn handle_field_dropdown_key(&mut self, key: KeyEvent) {
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
                    if *cursor + 1 < options.len() {
                        *cursor += 1;
                    }
                }
                KeyCode::Char('j') if no_ctrl_alt(&key) => {
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

    pub(super) fn handle_color_picker_key(&mut self, key: KeyEvent) {
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
                            if let Some(normalized) = normalize_color_value(&input.value)
                                && let Some(idx) = COLOR_PALETTE
                                    .iter()
                                    .position(|opt| opt.eq_ignore_ascii_case(&normalized))
                            {
                                *cursor = idx;
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
                    KeyCode::Left => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('h') if no_ctrl_alt(&key) => {
                        if *cursor > 0 {
                            *cursor -= 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Right => {
                        if *cursor + 1 < len {
                            *cursor += 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('l') if no_ctrl_alt(&key) => {
                        if *cursor + 1 < len {
                            *cursor += 1;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Up => {
                        if *cursor >= cols {
                            *cursor -= cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('k') if no_ctrl_alt(&key) => {
                        if *cursor >= cols {
                            *cursor -= cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Down => {
                        if *cursor + cols < len {
                            *cursor += cols;
                            sync_color_input_from_cursor(*cursor, input);
                        }
                    }
                    KeyCode::Char('j') if no_ctrl_alt(&key) => {
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
}
