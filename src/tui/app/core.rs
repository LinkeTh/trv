use std::sync::OnceLock;

use super::*;

const DEFAULT_DISPLAY_SIZE: (u16, u16) = (
    crate::tui::canvas::DEFAULT_DISPLAY_W,
    crate::tui::canvas::DEFAULT_DISPLAY_H,
);

static DETECTED_DISPLAY_SIZE: OnceLock<(u16, u16)> = OnceLock::new();

fn detect_display_size() -> (u16, u16) {
    *DETECTED_DISPLAY_SIZE
        .get_or_init(|| crate::device::adb::adb_display_size().unwrap_or(DEFAULT_DISPLAY_SIZE))
}

impl App {
    pub fn new(
        theme: Option<Theme>,
        theme_path: Option<PathBuf>,
        host: String,
        port: u16,
        recv_timeout_ms: u64,
    ) -> Self {
        let selected_widget = if theme.as_ref().map_or(0, |t| t.widgets.len()) > 0 {
            Some(0)
        } else {
            None
        };
        let mut app = Self {
            theme,
            theme_path,
            focus: Focus::Sidebar,
            overlay: Overlay::None,
            should_quit: false,
            selected_widget,
            dirty: false,
            prop_cursor: 0,
            prop_input: None,
            prop_error: None,
            push_status: PushStatus::None,
            log_lines: VecDeque::with_capacity(LOG_HISTORY_CAPACITY),
            log_scroll: 0,
            push_result_rx: None,
            push_cancel: None,
            push_worker: None,
            rotate_result_rx: None,
            rotate_worker: None,
            next_rotation_code_idx: 0,
            host,
            port,
            recv_timeout_ms,
            display_size: detect_display_size(),
            metrics: MetricsSnapshot {
                values: HashMap::new(),
                samples: HashMap::new(),
            },
            metric_history: HashMap::new(),
        };
        app.log_event("TUI started");
        app.log_event(format!(
            "Canvas display size: {}x{}",
            app.display_size.0, app.display_size.1
        ));
        app
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    pub fn selected_widget_ref(&self) -> Option<&Widget> {
        let idx = self.selected_widget?;
        self.theme.as_ref()?.widgets.get(idx)
    }

    pub fn selected_widget_mut(&mut self) -> Option<&mut Widget> {
        let idx = self.selected_widget?;
        self.theme.as_mut()?.widgets.get_mut(idx)
    }

    pub fn widget_count(&self) -> usize {
        self.theme.as_ref().map_or(0, |t| t.widgets.len())
    }

    pub fn theme_name(&self) -> &str {
        self.theme
            .as_ref()
            .map(|t| t.meta.name.as_str())
            .unwrap_or("(no theme)")
    }

    pub fn visible_log_lines(&self) -> Vec<String> {
        if self.log_lines.is_empty() {
            return Vec::new();
        }

        let max_scroll = self.max_log_scroll();
        let scroll = self.log_scroll.min(max_scroll);
        let end = self.log_lines.len().saturating_sub(scroll);
        let start = end.saturating_sub(LOG_VISIBLE_ROWS);

        self.log_lines
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect()
    }

    pub fn max_log_scroll(&self) -> usize {
        self.log_lines.len().saturating_sub(LOG_VISIBLE_ROWS)
    }

    pub fn scroll_log_page_up(&mut self) {
        self.log_scroll = self
            .log_scroll
            .saturating_add(LOG_VISIBLE_ROWS)
            .min(self.max_log_scroll());
    }

    pub fn scroll_log_page_down(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(LOG_VISIBLE_ROWS);
    }

    pub fn log_is_scrolled(&self) -> bool {
        self.log_scroll > 0
    }

    pub fn log_event(&mut self, message: impl Into<String>) {
        if self.log_lines.len() >= LOG_HISTORY_CAPACITY {
            self.log_lines.pop_front();
        }
        self.log_lines.push_back(message.into());
        if self.log_scroll > 0 {
            self.log_scroll = self.log_scroll.saturating_add(1).min(self.max_log_scroll());
        }
    }

    pub fn update_metrics(&mut self, snapshot: MetricsSnapshot) {
        for key in METRIC_KEYS {
            let sample = snapshot
                .samples
                .get(key)
                .copied()
                .map(|raw| metric_value_to_spark_sample(key, raw))
                .unwrap_or(0.0);
            let value = sample.round().clamp(0.0, 100.0) as u64;

            let history = self
                .metric_history
                .entry(key.to_string())
                .or_insert_with(|| VecDeque::with_capacity(METRIC_HISTORY_CAPACITY));
            if history.len() >= METRIC_HISTORY_CAPACITY {
                history.pop_front();
            }
            history.push_back(value);
        }

        self.metrics = snapshot;
    }

    /// Number of editable fields for the currently selected widget.
    pub fn field_count(&self) -> usize {
        self.selected_widget_ref()
            .map(|w| widget_fields(w).len())
            .unwrap_or(0)
    }
}
