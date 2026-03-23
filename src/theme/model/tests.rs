use super::*;

#[test]
fn test_metric_source_show_ids() {
    assert_eq!(MetricSource::CpuTemp.show_id(), "00");
    assert_eq!(MetricSource::CpuUsage.show_id(), "05");
    assert_eq!(MetricSource::MemUsage.show_id(), "06");
    assert_eq!(MetricSource::GpuTemp.show_id(), "0D");
    assert_eq!(MetricSource::GpuUsage.show_id(), "0E");
}

#[test]
fn test_normalize_font_option_accepts_legacy_names() {
    assert_eq!(normalize_font_option(""), Some("default"));
    assert_eq!(normalize_font_option("NI7SEG.TTF"), Some("ni7seg"));
    assert_eq!(
        normalize_font_option("HarmonyOS_Sans_Bold.ttf"),
        Some("harmonyos_bold")
    );
    assert_eq!(normalize_font_option("unknown_font.ttf"), None);
}

#[test]
fn test_font_bold_selector_maps_to_firmware_typo_token() {
    let w = Widget {
        kind: WidgetKind::Text {
            content: "CPU".into(),
        },
        x: 0,
        y: 0,
        width: 120,
        height: 40,
        text_size: 30,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: "harmonyos_bold".into(),
    };

    let p = crate::theme::hex::WidgetHexParams::try_from(&w).expect("text conversion");
    assert_eq!(p.typeface_type, 0x01);
    assert_eq!(p.typeface_path, "harmonyos_blod");
}

#[test]
fn test_widget_rejects_unknown_font_selector() {
    let w = Widget {
        kind: WidgetKind::Text {
            content: "CPU".into(),
        },
        x: 0,
        y: 0,
        width: 120,
        height: 40,
        text_size: 30,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: "my_custom_font.ttf".into(),
    };

    let err = crate::theme::hex::WidgetHexParams::try_from(&w).unwrap_err();
    assert!(err.contains("unsupported font"));
}

#[test]
fn test_widget_view_type() {
    let clock_widget = Widget {
        kind: WidgetKind::Clock {
            time_format: TimeFormat::HhMmSs,
        },
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        text_size: 40,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };
    assert_eq!(clock_widget.view_type(), 0x03);

    let metric_widget = Widget {
        kind: WidgetKind::Metric {
            source: MetricSource::CpuTemp,
            unit: "°C".into(),
            label: "CPU ".into(),
            show_label: true,
        },
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        text_size: 48,
        color: "00DDFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };
    assert_eq!(metric_widget.view_type(), 0x02);

    let video_widget = Widget {
        kind: WidgetKind::Video {
            path: "/tmp/a.mp4".into(),
        },
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        text_size: 48,
        color: "00DDFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };
    assert_eq!(video_widget.view_type(), 0x05);
}

#[test]
fn test_widget_alpha_byte() {
    let mut w = Widget {
        kind: WidgetKind::Text { content: "".into() },
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        text_size: 40,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };
    assert_eq!(w.alpha_byte(), 10);
    w.alpha = 0.5;
    assert_eq!(w.alpha_byte(), 5);
}

#[test]
fn test_text_widget_maps_content_to_image_path() {
    let w = Widget {
        kind: WidgetKind::Text {
            content: "CPU".into(),
        },
        x: 0,
        y: 0,
        width: 120,
        height: 40,
        text_size: 30,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };

    let p = crate::theme::hex::WidgetHexParams::try_from(&w).expect("text conversion");
    assert_eq!(p.view_type, 0x01);
    assert_eq!(p.animation, 0x00);
    assert_eq!(p.image_path, "CPU");
    assert!(p.num_text.is_empty());
    assert_eq!(p.show_text, 0x00);
}

#[test]
fn test_image_widget_uses_basename_for_image_path() {
    let w = Widget {
        kind: WidgetKind::Image {
            path: "/tmp/trv/assets/logo.png".into(),
        },
        x: 10,
        y: 20,
        width: 100,
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

    let p = crate::theme::hex::WidgetHexParams::try_from(&w).expect("image conversion");
    assert_eq!(p.view_type, 0x04);
    assert_eq!(p.image_path, "logo.png");
}

#[test]
fn test_video_widget_uses_basename_and_default_play_num() {
    let w = Widget {
        kind: WidgetKind::Video {
            path: "/tmp/trv/assets/bg.mp4".into(),
        },
        x: 10,
        y: 20,
        width: 100,
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

    let p = crate::theme::hex::WidgetHexParams::try_from(&w).expect("video conversion");
    assert_eq!(p.view_type, 0x05);
    assert_eq!(p.image_path, "bg.mp4");
    assert_eq!(p.play_num, 0x01);
}

#[test]
fn test_image_remote_name_windows_path() {
    assert_eq!(image_remote_name("C:\\tmp\\trv\\foo.jpg"), "foo.jpg");
}

#[test]
fn test_image_remote_name_edge_cases() {
    assert_eq!(image_remote_name("logo.png"), "logo.png");
    assert_eq!(image_remote_name(""), "");
    assert_eq!(image_remote_name("  \t\n"), "");
    assert_eq!(image_remote_name("/"), "");
    assert_eq!(image_remote_name("C:\\\\"), "");
    assert_eq!(image_remote_name("/tmp/trv/assets/"), "assets");
    assert_eq!(image_remote_name("C:/Users\\foo/bar.jpg"), "bar.jpg");
}

#[test]
fn test_text_widget_rejects_content_longer_than_150_bytes() {
    let w = Widget {
        kind: WidgetKind::Text {
            content: "A".repeat(151),
        },
        x: 0,
        y: 0,
        width: 120,
        height: 40,
        text_size: 30,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };

    let err = crate::theme::hex::WidgetHexParams::try_from(&w).unwrap_err();
    assert!(err.contains("text content too long"));
}
