use anyhow::{Context, Result};
/// TOML serialization/deserialization for Theme.
use std::path::Path;

use crate::theme::model::Theme;

/// Load a theme from a TOML file.
pub fn load_theme_file(path: &Path) -> Result<Theme> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading theme file {:?}", path))?;
    parse_theme_toml(&content).with_context(|| format!("parsing theme file {:?}", path))
}

/// Parse a TOML string into a Theme.
pub fn parse_theme_toml(content: &str) -> Result<Theme> {
    toml::from_str(content).map_err(|e| anyhow::anyhow!("TOML parse error: {}", e))
}

/// Serialize a Theme to a TOML string.
pub fn serialize_theme(theme: &Theme) -> Result<String> {
    toml::to_string_pretty(theme).map_err(|e| anyhow::anyhow!("TOML serialize error: {}", e))
}

/// Save a theme to a TOML file atomically (write to a temp file, then rename).
///
/// Using a temp-file + rename ensures the destination is never left in a
/// partial/corrupt state if the process is interrupted mid-write.
pub fn save_theme_file(theme: &Theme, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating theme directory {:?}", parent))?;
    }
    let content = serialize_theme(theme)?;

    // Write to a sibling temp file, then atomically rename into place.
    let tmp_path = path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, &content)
        .with_context(|| format!("writing temp theme file {:?}", tmp_path))?;
    std::fs::rename(&tmp_path, path)
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })
        .with_context(|| format!("renaming temp file {:?} -> {:?}", tmp_path, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::model::{MetricSource, TimeFormat, WidgetKind};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_TOML: &str = r##"
[meta]
name = "Test Theme"
description = "Unit test theme"

[[widget]]
type = "clock"
x = 100
y = 15
text_size = 52
color = "#FFFFFF"
alpha = 1.0
time_format = "hh:mm:ss"

[[widget]]
type = "metric"
x = 15
y = 150
width = 230
height = 160
text_size = 48
color = "#00DDFF"
alpha = 1.0
source = "cpu_temp"
unit = "°C"
label = "CPU "
show_label = true
"##;

    #[test]
    fn test_parse_sample_theme() {
        let theme = parse_theme_toml(SAMPLE_TOML).expect("should parse");
        assert_eq!(theme.meta.name, "Test Theme");
        assert_eq!(theme.widgets.len(), 2);

        // Check clock widget
        if let WidgetKind::Clock { time_format } = &theme.widgets[0].kind {
            assert_eq!(*time_format, TimeFormat::HhMmSs);
        } else {
            panic!("expected clock widget");
        }

        // Check metric widget
        if let WidgetKind::Metric {
            source,
            unit,
            label,
            show_label,
        } = &theme.widgets[1].kind
        {
            assert_eq!(*source, MetricSource::CpuTemp);
            assert_eq!(unit, "°C");
            assert_eq!(label, "CPU ");
            assert!(*show_label);
        } else {
            panic!("expected metric widget");
        }
    }

    #[test]
    fn test_round_trip() {
        let theme = parse_theme_toml(SAMPLE_TOML).expect("should parse");
        let serialized = serialize_theme(&theme).expect("should serialize");
        let reparsed = parse_theme_toml(&serialized).expect("should reparse");
        assert_eq!(reparsed.meta.name, theme.meta.name);
        assert_eq!(reparsed.widgets.len(), theme.widgets.len());
    }

    #[test]
    fn test_save_theme_creates_parent_dirs() {
        let theme = parse_theme_toml(SAMPLE_TOML).expect("should parse");

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("trv_theme_save_test_{unique}"));
        let path: PathBuf = root.join("nested").join("theme.toml");

        save_theme_file(&theme, &path).expect("save should create dirs");
        assert!(path.is_file());

        let loaded = load_theme_file(&path).expect("re-load saved theme");
        assert_eq!(loaded.meta.name, theme.meta.name);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&root);
    }
}
