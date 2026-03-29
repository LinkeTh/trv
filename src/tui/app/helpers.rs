use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    style::{Modifier, Style},
    widgets::HighlightSpacing,
};
use ratatui_explorer::{Input as ExplorerInput, Theme as ExplorerTheme};

use crate::theme::model::{Widget, WidgetKind};

use super::{COLOR_PALETTE, MediaPathKind, TextInput};
use crate::tui::palette;

pub(super) fn normalize_color_value(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_uppercase()))
    } else {
        None
    }
}

pub(super) fn sync_color_input_from_cursor(cursor: usize, input: &mut TextInput) {
    if let Some(color) = COLOR_PALETTE.get(cursor) {
        input.value = (*color).to_string();
        input.cursor = input.value.len();
    }
}

pub(super) fn next_rotation_code(idx: usize) -> (crate::protocol::cmd::OrientationCode, usize) {
    let i = idx % super::ROTATION_CODES.len();
    let next = (i + 1) % super::ROTATION_CODES.len();
    (super::ROTATION_CODES[i], next)
}

pub(super) fn explorer_input_from_key(key: KeyEvent) -> Option<ExplorerInput> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => Some(ExplorerInput::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(ExplorerInput::Down),
        KeyCode::Left | KeyCode::Char('h') => Some(ExplorerInput::Left),
        KeyCode::Right | KeyCode::Char('l') => Some(ExplorerInput::Right),
        KeyCode::Home => Some(ExplorerInput::Home),
        KeyCode::End => Some(ExplorerInput::End),
        KeyCode::PageUp => Some(ExplorerInput::PageUp),
        KeyCode::PageDown => Some(ExplorerInput::PageDown),
        KeyCode::Char('.') => Some(ExplorerInput::ToggleShowHidden),
        _ => None,
    }
}

pub(super) fn is_toml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
}

pub(super) fn is_media_path(path: &Path, media_kind: MediaPathKind) -> bool {
    match media_kind {
        MediaPathKind::Image => is_image_path(path),
        MediaPathKind::Video => is_video_path(path),
    }
}

fn is_image_path(path: &Path) -> bool {
    const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "webp", "gif", "heic", "heif"];
    has_extension_in(path, IMAGE_EXTENSIONS)
}

fn is_video_path(path: &Path) -> bool {
    const VIDEO_EXTENSIONS: &[&str] = &[
        "mp4", "mov", "mkv", "avi", "webm", "m4v", "mpg", "mpeg", "wmv",
    ];
    has_extension_in(path, VIDEO_EXTENSIONS)
}

fn has_extension_in(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            allowed
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

pub(super) fn media_picker_working_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }

    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn absolutize_path(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    if path.is_absolute() {
        return path;
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(&path))
        .unwrap_or_else(|_| path)
}

pub(super) fn widget_log_label(widget: &Widget) -> String {
    match &widget.kind {
        WidgetKind::Metric { source, .. } => format!("Metric {:?}", source),
        WidgetKind::Clock { time_format } => format!("Clock {:?}", time_format),
        WidgetKind::Image { path } => {
            if path.is_empty() {
                "Image".to_string()
            } else {
                format!("Image {}", file_name_or_path(path))
            }
        }
        WidgetKind::Video { path } => {
            if path.is_empty() {
                "Video".to_string()
            } else {
                format!("Video {}", file_name_or_path(path))
            }
        }
        WidgetKind::Text { content } => {
            let mut chars = content.chars();
            let preview: String = chars.by_ref().take(12).collect();
            if chars.next().is_some() {
                format!("Text \"{}…\"", preview)
            } else {
                format!("Text \"{}\"", preview)
            }
        }
    }
}

fn file_name_or_path(raw: &str) -> String {
    let path = Path::new(raw);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| raw.to_string())
}

pub(super) fn expand_tilde_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(stripped) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    PathBuf::from(raw)
}

pub(super) fn default_new_theme_path(current_theme_path: Option<&Path>) -> PathBuf {
    if let Some(path) = current_theme_path
        && let Some(parent) = path.parent()
    {
        return parent.join("new_theme.toml");
    }

    if let Ok(Some(default_path)) = crate::config::get_default_theme_path()
        && let Some(parent) = default_path.parent()
    {
        return parent.join("new_theme.toml");
    }

    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    config_dir.join("trv").join("themes").join("new_theme.toml")
}

pub(super) fn normalize_theme_file_path(raw: &str) -> PathBuf {
    let mut path = expand_tilde_path(raw.trim());
    if !is_toml_path(&path) {
        path.set_extension("toml");
    }
    path
}

pub(super) fn default_theme_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(|name| name.replace(['_', '-'], " "))
        .unwrap_or_else(|| "New Theme".to_string())
}

pub(super) fn build_explorer_theme() -> ExplorerTheme {
    ExplorerTheme::new()
        .with_style(Style::default().fg(palette::TEXT))
        .with_item_style(Style::default().fg(palette::TEXT))
        .with_dir_style(Style::default().fg(palette::PEACH))
        .with_highlight_item_style(
            Style::default()
                .fg(palette::CRUST)
                .bg(palette::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .with_highlight_dir_style(
            Style::default()
                .fg(palette::CRUST)
                .bg(palette::SAPPHIRE)
                .add_modifier(Modifier::BOLD),
        )
        .with_highlight_symbol("> ")
        .with_highlight_spacing(HighlightSpacing::Always)
}

pub(super) fn metric_value_to_spark_sample(key: &str, value: f64) -> f64 {
    match key {
        "cpu_usage" | "gpu_usage" | "mem_usage" => value.clamp(0.0, 100.0),
        "cpu_temp" | "gpu_temp" | "liquid_temp" => ((value / 120.0) * 100.0).clamp(0.0, 100.0),
        "cpu_freq" | "gpu_freq" => ((value / 3000.0) * 100.0).clamp(0.0, 100.0),
        "fan_speed" => ((value / 4000.0) * 100.0).clamp(0.0, 100.0),
        "net_down" | "net_up" | "disk_read" | "disk_write" => {
            ((value / 100.0) * 100.0).clamp(0.0, 100.0)
        }
        _ => value.clamp(0.0, 100.0),
    }
}

pub(super) fn no_ctrl_alt(key: &KeyEvent) -> bool {
    !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)
}
