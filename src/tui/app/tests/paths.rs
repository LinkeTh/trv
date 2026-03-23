use super::super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_explorer::Input as ExplorerInput;
use std::path::{Path, PathBuf};

#[test]
fn expand_tilde_path_uses_home_directory() {
    let Some(home) = dirs::home_dir() else {
        return;
    };

    assert_eq!(expand_tilde_path("~"), home);
    assert_eq!(
        expand_tilde_path("~/themes/a.toml"),
        home.join("themes/a.toml")
    );
}

#[test]
fn helper_maps_explorer_keys_and_toml_paths() {
    assert_eq!(
        explorer_input_from_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty())),
        Some(ExplorerInput::PageUp)
    );
    assert_eq!(
        explorer_input_from_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::empty())),
        Some(ExplorerInput::ToggleShowHidden)
    );
    assert_eq!(
        explorer_input_from_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty())),
        None
    );

    assert!(is_toml_path(Path::new("/tmp/theme.toml")));
    assert!(is_toml_path(Path::new("/tmp/theme.TOML")));
    assert!(!is_toml_path(Path::new("/tmp/theme.txt")));

    assert!(is_media_path(
        Path::new("/tmp/logo.PNG"),
        MediaPathKind::Image
    ));
    assert!(!is_media_path(
        Path::new("/tmp/logo.mp4"),
        MediaPathKind::Image
    ));
    assert!(is_media_path(
        Path::new("/tmp/bg.MP4"),
        MediaPathKind::Video
    ));
    assert!(!is_media_path(
        Path::new("/tmp/bg.webp"),
        MediaPathKind::Video
    ));
}

#[test]
fn new_theme_dialog_defaults_to_toml_extension() {
    let path = normalize_theme_file_path("/tmp/my_theme");
    assert_eq!(path, PathBuf::from("/tmp/my_theme.toml"));
}
