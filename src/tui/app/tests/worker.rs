use super::super::*;
use super::common::*;
use crate::tui::palette;
use ratatui::widgets::HighlightSpacing;
use std::collections::HashMap;

#[test]
fn rotation_code_cycle_wraps_through_all_raw_codes() {
    let mut idx = 0;
    let mut seen = Vec::new();
    for _ in 0..5 {
        let (code, next) = next_rotation_code(idx);
        seen.push(code.as_u8());
        idx = next;
    }

    assert_eq!(seen, vec![0x00, 0x01, 0x02, 0x03, 0x00]);
}

#[test]
fn log_scrolling_pages_and_clamps() {
    let mut app = App::new(
        Some(test_theme()),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    );

    for i in 0..20 {
        app.log_event(format!("line {}", i));
    }

    app.scroll_log_page_up();
    assert_eq!(app.log_scroll, LOG_VISIBLE_ROWS.min(app.max_log_scroll()));

    app.scroll_log_page_up();
    assert_eq!(
        app.log_scroll,
        (LOG_VISIBLE_ROWS * 2).min(app.max_log_scroll())
    );

    for _ in 0..20 {
        app.scroll_log_page_up();
    }
    assert_eq!(app.log_scroll, app.max_log_scroll());

    app.scroll_log_page_down();
    assert_eq!(
        app.log_scroll,
        app.max_log_scroll().saturating_sub(LOG_VISIBLE_ROWS)
    );

    for _ in 0..20 {
        app.scroll_log_page_down();
    }
    assert_eq!(app.log_scroll, 0);
}

#[test]
fn log_scroll_tracks_new_entries_when_scrolled_back() {
    let mut app = App::new(
        Some(test_theme()),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    );

    for i in 0..12 {
        app.log_event(format!("line {}", i));
    }

    app.scroll_log_page_up();
    let before = app.log_scroll;
    app.log_event("new line");

    assert_eq!(app.log_scroll, (before + 1).min(app.max_log_scroll()));
}

#[test]
fn explorer_theme_makes_selection_visible() {
    let theme = build_explorer_theme();
    assert_eq!(theme.highlight_symbol(), Some("> "));
    assert_eq!(theme.highlight_spacing(), &HighlightSpacing::Always);

    let highlight_item = *theme.highlight_item_style();
    let highlight_dir = *theme.highlight_dir_style();
    assert_eq!(highlight_item.bg, Some(palette::BLUE));
    assert_eq!(highlight_dir.bg, Some(palette::SAPPHIRE));
}

#[test]
fn update_metrics_collects_sparkline_history() {
    let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

    let mut samples = HashMap::new();
    samples.insert("cpu_temp".to_string(), 60.0);
    samples.insert("cpu_usage".to_string(), 25.0);
    samples.insert("mem_usage".to_string(), 33.0);
    samples.insert("gpu_temp".to_string(), 48.0);
    samples.insert("gpu_usage".to_string(), 40.0);

    let mut values = HashMap::new();
    values.insert("cpu_temp".to_string(), "60.0°C".to_string());
    values.insert("cpu_usage".to_string(), "25.0%".to_string());
    values.insert("mem_usage".to_string(), "33.0%".to_string());
    values.insert("gpu_temp".to_string(), "48°C".to_string());
    values.insert("gpu_usage".to_string(), "40.0%".to_string());

    app.update_metrics(MetricsSnapshot { values, samples });

    let cpu_usage_hist = app
        .metric_history
        .get("cpu_usage")
        .expect("cpu_usage history present");
    assert_eq!(cpu_usage_hist.back().copied(), Some(25));
}
