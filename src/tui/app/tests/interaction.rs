use super::super::*;
use super::common::*;
use crate::theme::model::ThemeMeta;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_explorer::FileExplorerBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn typing_q_during_property_edit_does_not_quit() {
    let mut app = App::new(
        Some(test_theme()),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    );

    app.focus = Focus::Properties;
    app.selected_widget = Some(0);
    app.prop_cursor = 0;
    app.prop_input = Some(TextInput::new(""));

    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()));

    assert!(!app.should_quit);
    assert_eq!(app.prop_input.as_ref().map(|i| i.value.as_str()), Some("q"));
}

#[test]
fn paste_inserts_into_property_input() {
    let mut app = App::new(
        Some(test_theme()),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    );

    app.focus = Focus::Properties;
    app.selected_widget = Some(0);
    app.prop_cursor = 0;
    app.prop_input = Some(TextInput::new("ab"));

    app.handle_paste("cd\n");

    assert_eq!(
        app.prop_input.as_ref().map(|i| i.value.as_str()),
        Some("abcd")
    );
}

#[test]
fn moving_video_widget_in_canvas_does_not_change_position_or_dirty() {
    let mut app = App::new(
        Some(Theme {
            meta: ThemeMeta {
                name: "test".to_string(),
                description: String::new(),
            },
            widgets: vec![Widget {
                kind: WidgetKind::Video {
                    path: "/tmp/bg.mp4".to_string(),
                },
                x: 10,
                y: 20,
                width: 100,
                height: 50,
                text_size: 40,
                color: "FFFFFF".to_string(),
                alpha: 1.0,
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                font: String::new(),
            }],
        }),
        None,
        "127.0.0.1".to_string(),
        22222,
        1000,
    );

    app.focus = Focus::Canvas;
    app.selected_widget = Some(0);
    app.dirty = false;

    app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));

    let w = app.selected_widget_ref().expect("selected widget");
    assert_eq!(w.x, 10);
    assert_eq!(w.y, 20);
    assert!(!app.dirty);
}

#[test]
fn key_pageup_and_pagedown_scroll_log_panel() {
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

    app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()));
    assert_eq!(app.log_scroll, LOG_VISIBLE_ROWS.min(app.max_log_scroll()));

    app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty()));
    assert_eq!(app.log_scroll, 0);
}

#[test]
fn enter_on_image_path_opens_media_path_picker() {
    let (root, file_path) = create_temp_media_file("png");
    let mut app = app_with_widget(media_widget(WidgetKind::Image {
        path: file_path.display().to_string(),
    }));

    app.focus = Focus::Properties;
    app.selected_widget = Some(0);
    app.prop_cursor = path_field_index(&app);

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

    match &app.overlay {
        Overlay::MediaPath { state } => {
            assert_eq!(state.field_name, "path");
            assert_eq!(state.media_kind, MediaPathKind::Image);
        }
        _ => panic!("expected media path overlay"),
    }

    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir(root);
}

#[test]
fn media_path_picker_confirm_stores_absolute_path() {
    let (root, file_path) = create_temp_media_file("mp4");
    let mut app = app_with_widget(media_widget(WidgetKind::Video {
        path: String::new(),
    }));

    let state = MediaPathDialogState {
        explorer: FileExplorerBuilder::default()
            .working_file(file_path.clone())
            .build()
            .expect("build media explorer"),
        field_name: "path",
        media_kind: MediaPathKind::Video,
        error: None,
    };
    app.overlay = Overlay::MediaPath {
        state: Box::new(state),
    };
    app.dirty = false;

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

    assert!(matches!(app.overlay, Overlay::None));

    let expected = file_path.canonicalize().expect("canonical temp file");
    if let WidgetKind::Video { path } = &app.selected_widget_ref().expect("widget").kind {
        assert_eq!(PathBuf::from(path), expected);
        assert!(Path::new(path).is_absolute());
    } else {
        panic!("expected video widget");
    }
    assert!(app.dirty);

    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir(root);
}

#[test]
fn media_path_picker_cancel_keeps_existing_path() {
    let (root, file_path) = create_temp_media_file("png");
    let original = file_path.canonicalize().expect("canonical temp file");

    let mut app = app_with_widget(media_widget(WidgetKind::Image {
        path: original.display().to_string(),
    }));

    let state = MediaPathDialogState {
        explorer: FileExplorerBuilder::default()
            .working_file(file_path.clone())
            .build()
            .expect("build media explorer"),
        field_name: "path",
        media_kind: MediaPathKind::Image,
        error: None,
    };
    app.overlay = Overlay::MediaPath {
        state: Box::new(state),
    };
    app.dirty = false;

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()));

    assert!(matches!(app.overlay, Overlay::None));
    if let WidgetKind::Image { path } = &app.selected_widget_ref().expect("widget").kind {
        assert_eq!(PathBuf::from(path), original);
    } else {
        panic!("expected image widget");
    }
    assert!(!app.dirty);

    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir(root);
}

#[test]
fn save_dialog_enter_file_prefills_then_confirms_save_path() {
    let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("trv-save-test-{}", nonce));
    fs::create_dir_all(&root).expect("create temp root");
    let file_path = root.join("demo.toml");
    fs::write(&file_path, "").expect("create temp file");

    let state = SaveDialogState {
        explorer: FileExplorerBuilder::default()
            .working_file(file_path.clone())
            .build()
            .expect("build explorer"),
        path_input: TextInput::new(""),
        input_active: false,
        error: None,
    };
    app.overlay = Overlay::Save {
        state: Box::new(state),
    };

    app.handle_save_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

    match &app.overlay {
        Overlay::Save { state } => {
            assert!(state.input_active);
            assert_eq!(state.path_input.value, file_path.display().to_string());
        }
        _ => panic!("save overlay should remain open"),
    }

    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir(root);
}

#[test]
fn ctrl_n_opens_new_theme_dialog() {
    let mut app = App::new(None, None, "127.0.0.1".to_string(), 22222, 1000);

    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

    match &app.overlay {
        Overlay::NewTheme { state } => {
            assert_eq!(state.active_field, 0);
            assert!(state.file_input.value.ends_with(".toml"));
        }
        _ => panic!("expected new theme overlay"),
    }
}
