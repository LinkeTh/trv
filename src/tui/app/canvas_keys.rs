use super::*;

impl App {
    // ── Canvas keys ───────────────────────────────────────────────────────────

    pub(super) fn handle_canvas_key(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let step: u16 = if shift { 10 } else { 1 };

        match key.code {
            KeyCode::Up => self.move_widget_by(0, step, true),
            KeyCode::Down => self.move_widget_by(0, step, false),
            KeyCode::Left => self.move_widget_by(step, 0, true),
            KeyCode::Right => self.move_widget_by(step, 0, false),
            // j/k scroll widget selection in canvas too
            KeyCode::Char('k') if no_ctrl_alt(&key) => self.sidebar_up(),
            KeyCode::Char('j') if no_ctrl_alt(&key) => self.sidebar_down(),
            _ => {}
        }
    }

    pub(super) fn move_widget_by(&mut self, dx: u16, dy: u16, subtract: bool) {
        if let Some(w) = self.selected_widget_mut() {
            if matches!(w.kind, WidgetKind::Video { .. }) {
                return;
            }

            let before_x = w.x;
            let before_y = w.y;

            if dx > 0 {
                if subtract {
                    w.x = w.x.saturating_sub(dx);
                } else {
                    w.x = w.x.saturating_add(dx).min(483);
                }
            }
            if dy > 0 {
                if subtract {
                    w.y = w.y.saturating_sub(dy);
                } else {
                    w.y = w.y.saturating_add(dy).min(479);
                }
            }
            if w.x != before_x || w.y != before_y {
                self.dirty = true;
            }
        }
    }
}
