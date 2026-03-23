use super::super::*;
use crate::theme::model::ThemeMeta;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn test_widget() -> Widget {
    Widget {
        kind: WidgetKind::Text {
            content: "hello".to_string(),
        },
        x: 10,
        y: 10,
        width: 100,
        height: 40,
        text_size: 20,
        color: "FFFFFF".to_string(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    }
}

pub(super) fn test_theme() -> Theme {
    Theme {
        meta: ThemeMeta {
            name: "test".to_string(),
            description: String::new(),
        },
        widgets: vec![test_widget()],
    }
}

pub(super) fn media_widget(kind: WidgetKind) -> Widget {
    Widget {
        kind,
        x: 10,
        y: 10,
        width: 100,
        height: 40,
        text_size: 20,
        color: "FFFFFF".to_string(),
        alpha: 1.0,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        font: String::new(),
    }
}

pub(super) fn app_with_widget(widget: Widget) -> App {
    App::new(
        Some(Theme {
            meta: ThemeMeta {
                name: "test".to_string(),
                description: String::new(),
            },
            widgets: vec![widget],
        }),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    )
}

pub(super) fn path_field_index(app: &App) -> usize {
    let widget = app.selected_widget_ref().expect("selected widget");
    widget_fields(widget)
        .iter()
        .position(|field| field.name == "path")
        .expect("path field")
}

pub(super) fn create_temp_media_file(ext: &str) -> (PathBuf, PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("trv-media-test-{nonce}"));
    fs::create_dir_all(&root).expect("create temp root");
    let file_path = root.join(format!("sample.{ext}"));
    fs::write(&file_path, b"test").expect("create temp media file");
    (root, file_path)
}
