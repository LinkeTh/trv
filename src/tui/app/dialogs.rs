use super::*;

impl App {
    // ── Save overlay ──────────────────────────────────────────────────────────

    pub(super) fn begin_save(&mut self) {
        match self.build_save_dialog_state() {
            Ok(state) => {
                let cwd = state.explorer.cwd().display().to_string();
                self.overlay = Overlay::Save {
                    state: Box::new(state),
                };
                self.log_event(format!("Save dialog started at {}", cwd));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("save dialog: {}", e));
                self.log_event(format!("Save dialog failed: {}", e));
            }
        }
    }

    pub(super) fn handle_save_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut cancelled = false;
        let mut save_target: Option<PathBuf> = None;

        if let Overlay::Save { state } = &mut self.overlay {
            state.error = None;

            if key.code == KeyCode::Esc {
                close_overlay = true;
                cancelled = true;
            } else if key.code == KeyCode::Tab {
                state.input_active = !state.input_active;
            } else if state.input_active {
                match state.path_input.handle_key(key) {
                    InputResult::Pending => {}
                    InputResult::Cancelled => {
                        state.input_active = false;
                    }
                    InputResult::Confirmed => {
                        let raw = state.path_input.value.trim();
                        if raw.is_empty() {
                            state.error = Some("enter a file path".to_string());
                        } else {
                            save_target = Some(expand_tilde_path(raw));
                            close_overlay = true;
                        }
                    }
                }
            } else {
                match key.code {
                    KeyCode::Backspace => {
                        if let Err(e) = state.explorer.handle(ExplorerInput::Left) {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    }
                    KeyCode::Enter => {
                        let current = state.explorer.current().clone();
                        if current.is_dir {
                            if let Err(e) = state.explorer.handle(ExplorerInput::Right) {
                                state.error = Some(format!("navigate: {}", e));
                            }
                        } else {
                            state.path_input = TextInput::new(current.path.display().to_string());
                            state.input_active = true;
                        }
                    }
                    _ => {
                        if let Some(input) = explorer_input_from_key(key)
                            && let Err(e) = state.explorer.handle(input)
                        {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    }
                }
            }
        } else {
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("Save dialog cancelled");
            }
        }

        if let Some(path) = save_target {
            self.save_theme_to_path(path);
        }
    }

    fn build_save_dialog_state(&self) -> Result<SaveDialogState, String> {
        let mut builder = FileExplorerBuilder::default()
            .show_hidden(false)
            .theme(build_explorer_theme())
            .filter_map(|file| {
                if file.is_dir || is_toml_path(&file.path) {
                    Some(file)
                } else {
                    None
                }
            });

        let initial_path = self
            .theme_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("theme.toml"));

        if initial_path.exists() {
            builder = builder.working_file(initial_path.clone());
        } else if let Some(parent) = initial_path.parent()
            && !parent.as_os_str().is_empty()
        {
            builder = builder.working_dir(parent.to_path_buf());
        }

        let explorer = builder
            .build()
            .map_err(|e| format!("failed to initialize save explorer: {}", e))?;

        Ok(SaveDialogState {
            explorer,
            path_input: TextInput::new(initial_path.display().to_string()),
            input_active: false,
            error: None,
        })
    }

    fn save_theme_to_path(&mut self, path: PathBuf) {
        self.log_event(format!("Saving theme to {}", path.display()));
        if let Some(theme) = &self.theme {
            match save_theme_file(theme, &path) {
                Ok(()) => {
                    self.theme_path = Some(path);
                    self.dirty = false;
                    self.push_status = PushStatus::SaveOk;
                    if let Some(saved_path) = self.theme_path.as_deref() {
                        self.log_event(format!("Saved theme {}", saved_path.display()));
                    }
                }
                Err(e) => {
                    self.log_event(format!("Save failed: {}", e));
                    self.push_status = PushStatus::Err(format!("save: {}", e));
                }
            }
        }
    }

    // ── Open overlay ──────────────────────────────────────────────────────────

    pub(super) fn begin_open(&mut self) {
        match self.build_open_dialog_state() {
            Ok(state) => {
                let cwd = state.explorer.cwd().display().to_string();
                self.overlay = Overlay::Open {
                    state: Box::new(state),
                };
                self.log_event(format!("Open dialog started at {}", cwd));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("open dialog: {}", e));
                self.log_event(format!("Open dialog failed: {}", e));
            }
        }
    }

    pub(super) fn handle_open_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut open_selected: Option<PathBuf> = None;
        let mut cancelled = false;

        if let Overlay::Open { state } = &mut self.overlay {
            state.error = None;
            match key.code {
                KeyCode::Esc => {
                    close_overlay = true;
                    cancelled = true;
                }
                KeyCode::Backspace => {
                    if let Err(e) = state.explorer.handle(ExplorerInput::Left) {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
                KeyCode::Enter => {
                    let current = state.explorer.current().clone();
                    if current.is_dir {
                        if let Err(e) = state.explorer.handle(ExplorerInput::Right) {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    } else if is_toml_path(&current.path) {
                        open_selected = Some(current.path.clone());
                        close_overlay = true;
                    } else {
                        state.error = Some("select a .toml file".to_string());
                    }
                }
                _ => {
                    if let Some(input) = explorer_input_from_key(key)
                        && let Err(e) = state.explorer.handle(input)
                    {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
            }
        } else {
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("Open dialog cancelled");
            }
        }

        if let Some(path) = open_selected {
            self.load_theme_from_path(path);
        }
    }

    fn build_open_dialog_state(&self) -> Result<OpenDialogState, String> {
        let mut builder = FileExplorerBuilder::default()
            .show_hidden(false)
            .theme(build_explorer_theme())
            .filter_map(|file| {
                if file.is_dir || is_toml_path(&file.path) {
                    Some(file)
                } else {
                    None
                }
            });

        if let Some(path) = self.theme_path.as_ref() {
            if path.exists() {
                builder = builder.working_file(path.clone());
            } else if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                builder = builder.working_dir(parent.to_path_buf());
            }
        }

        let explorer = builder
            .build()
            .map_err(|e| format!("failed to initialize file explorer: {}", e))?;

        Ok(OpenDialogState {
            explorer,
            error: None,
        })
    }

    pub(super) fn handle_media_path_key(&mut self, key: KeyEvent) {
        let mut close_overlay = false;
        let mut cancelled = false;
        let mut picked_value: Option<(&'static str, String, MediaPathKind)> = None;

        if let Overlay::MediaPath { state } = &mut self.overlay {
            state.error = None;

            match key.code {
                KeyCode::Esc => {
                    close_overlay = true;
                    cancelled = true;
                }
                KeyCode::Backspace => {
                    if let Err(e) = state.explorer.handle(ExplorerInput::Left) {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
                KeyCode::Enter => {
                    let current = state.explorer.current().clone();
                    if current.is_dir {
                        if let Err(e) = state.explorer.handle(ExplorerInput::Right) {
                            state.error = Some(format!("navigate: {}", e));
                        }
                    } else {
                        let absolute = absolutize_path(current.path);
                        picked_value = Some((
                            state.field_name,
                            absolute.display().to_string(),
                            state.media_kind,
                        ));
                        close_overlay = true;
                    }
                }
                _ => {
                    if let Some(input) = explorer_input_from_key(key)
                        && let Err(e) = state.explorer.handle(input)
                    {
                        state.error = Some(format!("navigate: {}", e));
                    }
                }
            }
        } else {
            return;
        }

        if close_overlay {
            self.overlay = Overlay::None;
            if cancelled {
                self.log_event("Media path picker cancelled");
            }
        }

        if let Some((field_name, value, media_kind)) = picked_value {
            self.apply_field_value(field_name, &value);
            if self.prop_error.is_none() {
                self.log_event(format!(
                    "{} path selected {}",
                    media_kind.title_name(),
                    value
                ));
            }
        }
    }

    pub(super) fn build_media_path_dialog_state(
        &self,
        field_name: &'static str,
        media_kind: MediaPathKind,
        current_value: &str,
    ) -> Result<MediaPathDialogState, String> {
        let mut builder = FileExplorerBuilder::default()
            .show_hidden(false)
            .theme(build_explorer_theme())
            .filter_map(move |file| {
                if file.is_dir || is_media_path(&file.path, media_kind) {
                    Some(file)
                } else {
                    None
                }
            });

        let mut initial_file: Option<PathBuf> = None;
        if !current_value.trim().is_empty() {
            let expanded = expand_tilde_path(current_value.trim());
            let absolute = absolutize_path(expanded);
            if absolute.exists() {
                if absolute.is_file() {
                    initial_file = Some(absolute.clone());
                }
                builder = builder.working_dir(media_picker_working_dir(&absolute));
            }
        }

        if initial_file.is_none()
            && let Some(path) = self.theme_path.as_ref()
            && !path.as_os_str().is_empty()
        {
            builder = builder.working_dir(media_picker_working_dir(path));
        }

        if let Some(file_path) = initial_file {
            builder = builder.working_file(file_path);
        }

        let explorer = builder
            .build()
            .map_err(|e| format!("failed to initialize media path picker: {}", e))?;

        Ok(MediaPathDialogState {
            explorer,
            field_name,
            media_kind,
            error: None,
        })
    }

    fn load_theme_from_path(&mut self, path: PathBuf) {
        self.log_event(format!("Opening theme {}", path.display()));
        match load_theme_file(&path) {
            Ok(t) => {
                let count = t.widgets.len();
                self.theme = Some(t);
                self.theme_path = Some(path.clone());
                self.dirty = false;
                self.selected_widget = if count > 0 { Some(0) } else { None };
                self.prop_cursor = 0;
                self.prop_error = None;
                self.push_status = PushStatus::OpenOk;
                self.log_event(format!("Opened theme {}", path.display()));
            }
            Err(e) => {
                self.push_status = PushStatus::Err(format!("open: {}", e));
                self.log_event(format!("Open failed: {}", e));
            }
        }
    }
}
