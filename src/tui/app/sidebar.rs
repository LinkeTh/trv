use super::*;

impl App {
    // ── Sidebar keys ──────────────────────────────────────────────────────────

    pub(super) fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if ctrl => self.sidebar_move_up(),
            KeyCode::Down | KeyCode::Char('j') if ctrl => self.sidebar_move_down(),
            KeyCode::Up => self.sidebar_up(),
            KeyCode::Down => self.sidebar_down(),
            KeyCode::Char('k') if no_ctrl_alt(&key) => self.sidebar_up(),
            KeyCode::Char('j') if no_ctrl_alt(&key) => self.sidebar_down(),
            KeyCode::Enter => {
                if self.selected_widget.is_some() {
                    self.focus = Focus::Properties;
                    self.prop_cursor = 0;
                }
            }
            // Add widget
            KeyCode::Char('a') if no_ctrl_alt(&key) => {
                self.overlay = Overlay::AddWidget { cursor: 0 };
            }
            // Delete widget
            KeyCode::Char('d') if no_ctrl_alt(&key) => {
                if let Some(idx) = self.selected_widget {
                    self.overlay = Overlay::DeleteConfirm { idx };
                }
            }
            _ => {}
        }
    }

    pub(super) fn sidebar_up(&mut self) {
        if let Some(ref mut idx) = self.selected_widget
            && *idx > 0
        {
            *idx -= 1;
            self.prop_cursor = 0;
        }
    }

    pub(super) fn sidebar_down(&mut self) {
        let count = self.widget_count();
        if let Some(ref mut idx) = self.selected_widget
            && *idx + 1 < count
        {
            *idx += 1;
            self.prop_cursor = 0;
        }
    }

    pub(super) fn sidebar_move_up(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut())
            && idx > 0
        {
            theme.widgets.swap(idx, idx - 1);
            self.selected_widget = Some(idx - 1);
            self.dirty = true;
        }
    }

    pub(super) fn sidebar_move_down(&mut self) {
        if let (Some(idx), Some(theme)) = (self.selected_widget, self.theme.as_mut())
            && idx + 1 < theme.widgets.len()
        {
            theme.widgets.swap(idx, idx + 1);
            self.selected_widget = Some(idx + 1);
            self.dirty = true;
        }
    }
}
