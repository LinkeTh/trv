/// Single-line text input widget with cursor, backspace, and char insertion.
///
/// Used for in-place field editing in the Properties panel and for path
/// dialogs (Ctrl+S / Ctrl+O).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Result of processing a key event through a [`TextInput`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputResult {
    /// The user pressed Enter — the current value is confirmed.
    Confirmed,
    /// The user pressed Esc — the edit is cancelled, value unchanged.
    Cancelled,
    /// The key was consumed but editing is still in progress.
    Pending,
}

/// A single-line in-place text input with cursor navigation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    /// Current buffer contents.
    pub value: String,
    /// Cursor position in bytes (always on a char boundary).
    pub cursor: usize,
}

impl TextInput {
    /// Create a new `TextInput` pre-populated with `initial` and cursor at end.
    pub fn new(initial: impl Into<String>) -> Self {
        let value = initial.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    /// Return the display string (value with a visual cursor character injected).
    /// Used by the renderer to show where the cursor is.
    pub fn display(&self) -> String {
        let mut s = self.value.clone();
        let pos = self.cursor.min(s.len());
        debug_assert!(s.is_char_boundary(pos), "cursor must be char boundary");
        let safe_pos = if s.is_char_boundary(pos) {
            pos
        } else {
            self.prev_char_boundary(pos)
        };
        s.insert(safe_pos, '▮');
        s
    }

    /// Insert a string at the current cursor position.
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let pos = self.cursor.min(self.value.len());
        let safe_pos = if self.value.is_char_boundary(pos) {
            pos
        } else {
            self.prev_char_boundary(pos)
        };
        self.value.insert_str(safe_pos, s);
        self.cursor = safe_pos + s.len();
    }

    /// Process a key event and return the `InputResult`.
    pub fn handle_key(&mut self, key: KeyEvent) -> InputResult {
        match key.code {
            KeyCode::Enter => return InputResult::Confirmed,
            KeyCode::Esc => return InputResult::Cancelled,

            KeyCode::Backspace => {
                if self.cursor > 0 {
                    // Find the byte index of the previous char boundary
                    let prev = self.prev_char_boundary(self.cursor);
                    self.value.remove(prev);
                    self.cursor = prev;
                }
            }

            KeyCode::Delete => {
                if self.cursor < self.value.len() {
                    self.value.remove(self.cursor);
                    // cursor stays in place
                }
            }

            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.prev_char_boundary(self.cursor);
                }
            }

            KeyCode::Right => {
                if self.cursor < self.value.len() {
                    self.cursor = self.next_char_boundary(self.cursor);
                }
            }

            KeyCode::Home => {
                self.cursor = 0;
            }

            KeyCode::End => {
                self.cursor = self.value.len();
            }

            // Ctrl+A — go to start
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = 0;
            }

            // Ctrl+E — go to end
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.value.len();
            }

            // Ctrl+U — clear to start
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.value.drain(..self.cursor);
                self.cursor = 0;
            }

            // Ctrl+K — clear to end
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.value.truncate(self.cursor);
            }

            // Printable characters (no Ctrl modifier)
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.value.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }

            _ => {}
        }

        InputResult::Pending
    }

    // ── private helpers ───────────────────────────────────────────────────────

    fn prev_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos.saturating_sub(1);
        while p > 0 && !self.value.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos + 1;
        while p < self.value.len() && !self.value.is_char_boundary(p) {
            p += 1;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn insert_and_backspace() {
        let mut inp = TextInput::new("ab");
        assert_eq!(inp.cursor, 2);
        inp.handle_key(key(KeyCode::Char('c')));
        assert_eq!(inp.value, "abc");
        inp.handle_key(key(KeyCode::Backspace));
        assert_eq!(inp.value, "ab");
        assert_eq!(inp.cursor, 2);
    }

    #[test]
    fn enter_confirms() {
        let mut inp = TextInput::new("hello");
        let r = inp.handle_key(key(KeyCode::Enter));
        assert_eq!(r, InputResult::Confirmed);
    }

    #[test]
    fn esc_cancels() {
        let mut inp = TextInput::new("hello");
        let r = inp.handle_key(key(KeyCode::Esc));
        assert_eq!(r, InputResult::Cancelled);
    }

    #[test]
    fn cursor_navigation() {
        let mut inp = TextInput::new("abc");
        inp.handle_key(key(KeyCode::Left));
        assert_eq!(inp.cursor, 2);
        inp.handle_key(key(KeyCode::Home));
        assert_eq!(inp.cursor, 0);
        inp.handle_key(key(KeyCode::End));
        assert_eq!(inp.cursor, 3);
    }

    #[test]
    fn delete_forward() {
        let mut inp = TextInput::new("abc");
        inp.handle_key(key(KeyCode::Home));
        inp.handle_key(key(KeyCode::Delete));
        assert_eq!(inp.value, "bc");
        assert_eq!(inp.cursor, 0);
    }

    #[test]
    fn display_shows_cursor() {
        let inp = TextInput::new("ab");
        // cursor is at end (position 2)
        let d = inp.display();
        assert!(d.contains('▮'));
        assert!(d.ends_with('▮'));
    }

    #[test]
    fn insert_str_appends_at_cursor() {
        let mut inp = TextInput::new("ab");
        inp.insert_str("cd");
        assert_eq!(inp.value, "abcd");
        assert_eq!(inp.cursor, 4);
    }

    #[test]
    fn insert_str_respects_current_cursor_position() {
        let mut inp = TextInput::new("ab");
        inp.handle_key(key(KeyCode::Left)); // cursor between a|b
        inp.insert_str("ZZ");
        assert_eq!(inp.value, "aZZb");
        assert_eq!(inp.cursor, 3);
    }
}
