use super::*;

fn widget(kind: WidgetKind) -> Widget {
    Widget {
        kind,
        x: 0,
        y: 0,
        width: 100,
        height: 80,
        text_size: 40,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    }
}

#[test]
fn metric_detail_prefers_label() {
    let w = widget(WidgetKind::Metric {
        source: MetricSource::CpuTemp,
        unit: "°C".into(),
        label: "CPU Main".into(),
        show_label: true,
    });
    assert_eq!(widget_detail_label(&w), "CPU Main");
}

#[test]
fn metric_detail_falls_back_to_raw_key() {
    let w = widget(WidgetKind::Metric {
        source: MetricSource::CpuTemp,
        unit: "°C".into(),
        label: "   ".into(),
        show_label: false,
    });
    assert_eq!(widget_detail_label(&w), "cpu_temp");
}

#[test]
fn metric_detail_for_new_source_key() {
    let w = widget(WidgetKind::Metric {
        source: MetricSource::NetDown,
        unit: "MB/s".into(),
        label: String::new(),
        show_label: false,
    });
    assert_eq!(widget_detail_label(&w), "net_down");
}

#[test]
fn image_detail_uses_basename() {
    let w = widget(WidgetKind::Image {
        path: "/tmp/trv/assets/logo.png".into(),
    });
    assert_eq!(widget_detail_label(&w), "logo.png");
}

#[test]
fn clock_detail_uses_raw_format_key() {
    let w = widget(WidgetKind::Clock {
        time_format: TimeFormat::Weekday,
    });
    assert_eq!(widget_detail_label(&w), "weekday");
}

#[test]
fn text_detail_normalizes_whitespace() {
    let w = widget(WidgetKind::Text {
        content: "cpu\n temp\twidget".into(),
    });
    assert_eq!(widget_detail_label(&w), "cpu temp widget");
}

#[test]
fn canvas_label_includes_detail_when_room_allows() {
    let w = widget(WidgetKind::Image {
        path: "/tmp/logo.png".into(),
    });
    let label = widget_canvas_label(&w, 0, 24).expect("label");
    assert_eq!(label, "IMG[1] logo.png");
}

#[test]
fn canvas_label_truncates_detail_when_narrow() {
    let w = widget(WidgetKind::Video {
        path: "/tmp/very_long_background_video_name.mp4".into(),
    });
    let label = widget_canvas_label(&w, 0, 12).expect("label");
    assert_eq!(label, "VID[1] very…");
}

#[test]
fn canvas_label_keeps_base_when_only_base_fits() {
    let w = widget(WidgetKind::Image {
        path: "/tmp/logo.png".into(),
    });
    let label = widget_canvas_label(&w, 0, 6).expect("label");
    assert_eq!(label, "IMG[1]");
}

#[test]
fn canvas_label_truncates_base_when_extremely_tight() {
    let w = widget(WidgetKind::Text {
        content: "hello".into(),
    });
    let label = widget_canvas_label(&w, 11, 5).expect("label");
    assert_eq!(label, "TXT[…");
}

#[test]
fn widget_color_by_kind_matches_palette() {
    let metric = widget(WidgetKind::Metric {
        source: MetricSource::CpuTemp,
        unit: "°C".into(),
        label: String::new(),
        show_label: false,
    });
    let clock = widget(WidgetKind::Clock {
        time_format: TimeFormat::HhMmSs,
    });
    let image = widget(WidgetKind::Image {
        path: "logo.png".into(),
    });
    let video = widget(WidgetKind::Video {
        path: "bg.mp4".into(),
    });
    let text = widget(WidgetKind::Text {
        content: "CPU".into(),
    });

    assert_eq!(widget_color(&metric), palette::SAPPHIRE);
    assert_eq!(widget_color(&clock), palette::PEACH);
    assert_eq!(widget_color(&image), palette::GREEN);
    assert_eq!(widget_color(&video), palette::MAUVE);
    assert_eq!(widget_color(&text), palette::ROSEWATER);
}

#[test]
fn video_geometry_is_fullscreen() {
    let w = widget(WidgetKind::Video {
        path: "bg.mp4".into(),
    });
    let geometry = widget_canvas_geometry(&w, 1.0, 120, 80);
    assert_eq!(geometry, (0, 0, 120, 80));
}

#[test]
fn non_video_geometry_uses_widget_rect() {
    let mut w = widget(WidgetKind::Image {
        path: "logo.png".into(),
    });
    w.x = 10;
    w.y = 20;
    w.width = 30;
    w.height = 40;

    let geometry = widget_canvas_geometry(&w, 2.0, 200, 120);
    assert_eq!(geometry, (20, 20, 60, 40));
}
