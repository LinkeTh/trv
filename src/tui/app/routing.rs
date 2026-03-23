use super::*;

impl App {
    // ── Top-level key handler ─────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Route to overlay handler first if one is active.
        match &self.overlay {
            Overlay::None => {}
            Overlay::Help => {
                match key.code {
                    KeyCode::Esc | KeyCode::F(1) => {
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Char('q') | KeyCode::Char('?') if no_ctrl_alt(&key) => {
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                }
                return;
            }
            Overlay::AddWidget { .. } => {
                self.handle_add_widget_key(key);
                return;
            }
            Overlay::FieldDropdown { .. } => {
                self.handle_field_dropdown_key(key);
                return;
            }
            Overlay::ColorPicker { .. } => {
                self.handle_color_picker_key(key);
                return;
            }
            Overlay::DeleteConfirm { .. } => {
                self.handle_delete_confirm_key(key);
                return;
            }
            Overlay::NewTheme { .. } => {
                self.handle_new_theme_key(key);
                return;
            }
            Overlay::Save { .. } => {
                self.handle_save_key(key);
                return;
            }
            Overlay::Open { .. } => {
                self.handle_open_key(key);
                return;
            }
            Overlay::MediaPath { .. } => {
                self.handle_media_path_key(key);
                return;
            }
        }

        // While an inline property editor is active, route every key to that
        // editor before global shortcuts so normal text (e.g. 'q') does not
        // accidentally trigger app-level actions.
        if self.prop_input.is_some() {
            self.handle_properties_key(key);
            return;
        }

        // Global bindings (no overlay).
        match key.code {
            KeyCode::Char('q') if no_ctrl_alt(&key) => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }
            KeyCode::F(1) => {
                self.overlay = Overlay::Help;
                return;
            }
            KeyCode::Char('?') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::Help;
                return;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_save();
                return;
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_open();
                return;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.begin_new_theme();
                return;
            }
            KeyCode::Char('p') if no_ctrl_alt(&key) => {
                self.push_to_device();
                return;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.enable_auto_rotation();
                return;
            }
            KeyCode::Char('r') if no_ctrl_alt(&key) => {
                self.rotate_next_manual_orientation();
                return;
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                return;
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                return;
            }
            KeyCode::PageUp => {
                self.scroll_log_page_up();
                return;
            }
            KeyCode::PageDown => {
                self.scroll_log_page_down();
                return;
            }
            _ => {}
        }

        // Panel-specific keys.
        match self.focus {
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Canvas => self.handle_canvas_key(key),
            Focus::Properties => self.handle_properties_key(key),
        }
    }

    pub fn handle_mouse(&mut self, _mouse: MouseEvent) {}

    pub fn handle_paste(&mut self, text: &str) {
        let pasted: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        if pasted.is_empty() {
            return;
        }

        if let Some(inp) = &mut self.prop_input {
            inp.insert_str(&pasted);
            self.prop_error = None;
            return;
        }

        match &mut self.overlay {
            Overlay::NewTheme { state } => {
                match state.active_field {
                    0 => state.file_input.insert_str(&pasted),
                    1 => state.name_input.insert_str(&pasted),
                    2 => state.description_input.insert_str(&pasted),
                    _ => {}
                }
                state.error = None;
            }
            Overlay::Save { state } => {
                state.path_input.insert_str(&pasted);
                state.input_active = true;
            }
            Overlay::ColorPicker {
                input,
                input_active,
                ..
            } => {
                *input_active = true;
                input.insert_str(&pasted);
            }
            _ => {}
        }
    }

    pub fn poll_push_result(&mut self) {
        if let Some(rx) = self.push_result_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.push_status = PushStatus::PushOk;
                    self.log_event("Push completed");
                    if let Some(path) = self.theme_path.as_deref()
                        && let Err(e) = crate::config::set_default_theme_path(path)
                    {
                        self.push_status =
                            PushStatus::Err(format!("pushed, but failed to update config: {}", e));
                        self.log_event(format!("Config update failed after push: {}", e));
                    }
                    self.cleanup_push_worker();
                }
                Ok(Err(e)) => {
                    self.log_event(format!("Push failed: {}", e));
                    self.push_status = PushStatus::Err(e);
                    self.cleanup_push_worker();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.push_status = PushStatus::Err("push worker disconnected".into());
                    self.log_event("Push worker disconnected");
                    self.cleanup_push_worker();
                }
            }
        }

        if let Some(rx) = self.rotate_result_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.log_event(format!("Rotation completed: {}", msg));
                    self.push_status = PushStatus::RotateOk(msg);
                    self.cleanup_rotate_worker();
                }
                Ok(Err(e)) => {
                    self.log_event(format!("Rotation failed: {}", e));
                    self.push_status = PushStatus::Err(e);
                    self.cleanup_rotate_worker();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.push_status = PushStatus::Err("rotation worker disconnected".into());
                    self.log_event("Rotation worker disconnected");
                    self.cleanup_rotate_worker();
                }
            }
        }
    }

    /// Tear down the push worker channel and join its thread.
    fn cleanup_push_worker(&mut self) {
        self.push_result_rx = None;
        self.push_cancel = None;
        if let Some(handle) = self.push_worker.take() {
            let _ = handle.join();
        }
    }

    /// Tear down the rotate worker channel and join its thread.
    fn cleanup_rotate_worker(&mut self) {
        self.rotate_result_rx = None;
        if let Some(handle) = self.rotate_worker.take() {
            let _ = handle.join();
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(cancel) = self.push_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cleanup_push_worker();
        self.cleanup_rotate_worker();
    }
}
