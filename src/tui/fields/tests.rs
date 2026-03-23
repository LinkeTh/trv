use super::*;
use crate::theme::model::{MetricSource, Widget, WidgetKind};

fn metric_widget() -> Widget {
    Widget {
        kind: WidgetKind::Metric {
            source: MetricSource::CpuTemp,
            unit: "°C".into(),
            label: "CPU".into(),
            show_label: true,
        },
        x: 10,
        y: 20,
        width: 100,
        height: 50,
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

fn video_widget() -> Widget {
    Widget {
        kind: WidgetKind::Video {
            path: "/tmp/bg.mp4".into(),
        },
        x: 10,
        y: 20,
        width: 100,
        height: 50,
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
fn apply_x_y() {
    let mut w = metric_widget();
    apply_field(&mut w, "x", "42").unwrap();
    apply_field(&mut w, "y", "99").unwrap();
    assert_eq!(w.x, 42);
    assert_eq!(w.y, 99);
}

#[test]
fn apply_color_valid() {
    let mut w = metric_widget();
    apply_field(&mut w, "color", "#00AABB").unwrap();
    assert_eq!(w.color, "00AABB");
}

#[test]
fn apply_color_invalid() {
    let mut w = metric_widget();
    assert!(apply_field(&mut w, "color", "#ZZZ").is_err());
    assert!(apply_field(&mut w, "color", "123").is_err());
}

#[test]
fn apply_alpha_clamped() {
    let mut w = metric_widget();
    assert!(apply_field(&mut w, "alpha", "1.5").is_err());
    assert!(apply_field(&mut w, "alpha", "-0.1").is_err());
    apply_field(&mut w, "alpha", "0.5").unwrap();
    assert!((w.alpha - 0.5).abs() < 0.001);
}

#[test]
fn apply_source() {
    let mut w = metric_widget();
    apply_field(&mut w, "source", "gpu_usage").unwrap();
    if let WidgetKind::Metric { source, .. } = &w.kind {
        assert_eq!(*source, MetricSource::GpuUsage);
    } else {
        panic!("wrong kind");
    }
}

#[test]
fn widget_fields_count() {
    let w = metric_widget();
    let fields = widget_fields(&w);
    assert_eq!(fields.len(), 16);
}

#[test]
fn widget_fields_types() {
    let w = metric_widget();
    let fields = widget_fields(&w);

    let source = fields.iter().find(|f| f.name == "source").unwrap();
    assert_eq!(source.kind, FieldType::Dropdown(SOURCE_OPTIONS));

    let show_label = fields.iter().find(|f| f.name == "show_label").unwrap();
    assert_eq!(show_label.kind, FieldType::Toggle);

    let color = fields.iter().find(|f| f.name == "color").unwrap();
    assert_eq!(color.kind, FieldType::Color);

    let font = fields.iter().find(|f| f.name == "font").unwrap();
    assert_eq!(font.kind, FieldType::Dropdown(FONT_OPTIONS));
}

#[test]
fn apply_font_normalizes_to_canonical_option() {
    let mut w = metric_widget();
    apply_field(&mut w, "font", "NI7SEG.TTF").unwrap();
    assert_eq!(w.font, "ni7seg");

    apply_field(&mut w, "font", "default").unwrap();
    assert!(w.font.is_empty());
}

#[test]
fn video_widget_path_field_round_trip() {
    let mut w = video_widget();
    let fields = widget_fields(&w);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "path");
    assert_eq!(fields[0].kind, FieldType::MediaPath(MediaPathKind::Video));
    let path = fields.iter().find(|f| f.name == "path").unwrap();
    assert_eq!(path.value, "/tmp/bg.mp4");

    apply_field(&mut w, "path", "/tmp/new.mp4").unwrap();
    if let WidgetKind::Video { path } = &w.kind {
        assert_eq!(path, "/tmp/new.mp4");
    } else {
        panic!("wrong kind");
    }
}

#[test]
fn image_widget_path_uses_media_path_field_kind() {
    let w = Widget {
        kind: WidgetKind::Image {
            path: "/tmp/logo.png".into(),
        },
        x: 10,
        y: 20,
        width: 100,
        height: 50,
        text_size: 40,
        color: "FFFFFF".into(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    };

    let path_field = widget_fields(&w)
        .into_iter()
        .find(|field| field.name == "path")
        .expect("path field");

    assert_eq!(path_field.kind, FieldType::MediaPath(MediaPathKind::Image));
}
