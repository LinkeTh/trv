/// Theme model: high-level structs for theme definition and widget layout.
///
/// These are the Rust-native types that map from TOML configuration.
/// The protocol hex encoding lives in `theme::hex`.
use serde::{Deserialize, Serialize};

/// Complete theme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    #[serde(default)]
    pub meta: ThemeMeta,
    #[serde(default)]
    pub background: Background,
    #[serde(rename = "widget", default)]
    pub widgets: Vec<Widget>,
}

/// Theme metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeMeta {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Background image configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Background {
    /// Filename on the device at /sdcard/
    #[serde(default)]
    pub image: String,
    /// Optional local path: if set, image will be center-cropped and pushed on startup
    #[serde(default)]
    pub local_path: String,
}

/// A single display widget in the theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    /// Widget kind determines the protocol viewType.
    /// Flattened so the `type` field lives at the widget table level in TOML.
    #[serde(flatten)]
    pub kind: WidgetKind,

    /// Position and size (in display pixels)
    #[serde(default)]
    pub x: u16,
    #[serde(default)]
    pub y: u16,
    #[serde(default)]
    pub width: u16,
    #[serde(default)]
    pub height: u16,

    /// Text size in pixels
    #[serde(default = "default_text_size")]
    pub text_size: u16,

    /// Color as "#RRGGBB" or "RRGGBB"
    #[serde(default = "default_color")]
    pub color: String,

    /// Opacity: 0.0 (transparent) – 1.0 (opaque). Protocol encodes as 0–10 integer.
    #[serde(default = "default_alpha")]
    pub alpha: f32,

    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,

    /// Custom font filename on device (e.g. "NI7SEG.TTF"). Empty = default.
    #[serde(default)]
    pub font: String,
}

fn default_text_size() -> u16 {
    40
}
fn default_color() -> String {
    "FFFFFF".into()
}
fn default_alpha() -> f32 {
    1.0
}

/// Widget kind — determines what the widget displays and its protocol viewType.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidgetKind {
    /// viewType 0x03 — digital clock rendered by TextClock
    Clock {
        /// "hh:mm:ss" | "date" | "weekday"
        #[serde(default = "default_time_format")]
        time_format: TimeFormat,
    },
    /// viewType 0x02 — live metric value, updated by cmd15
    Metric {
        /// Metric data source
        source: MetricSource,
        /// Unit string displayed after value (e.g. "°C", "%")
        #[serde(default)]
        unit: String,
        /// Label prepended to value (e.g. "CPU ")
        #[serde(default)]
        label: String,
        /// If true, show label+value+unit; if false, show value+unit only
        #[serde(default)]
        show_label: bool,
    },
    /// viewType 0x04 — static image loaded from /sdcard/ via Glide
    Image {
        /// Local image path (daemon auto-pushes to /sdcard/; basename is sent in protocol)
        #[serde(default)]
        path: String,
    },
    /// viewType 0x01 — static text label (encoded in image_path per device firmware)
    Text {
        #[serde(default)]
        content: String,
    },
}

/// Convert a local or remote path-like string to the device-side filename used
/// in protocol `image_path` fields.
pub(crate) fn image_remote_name(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = trimmed
        .rsplit(['/', '\\'])
        .find(|seg| !seg.is_empty())
        .unwrap_or("");

    if candidate.ends_with(':') {
        String::new()
    } else {
        candidate.to_string()
    }
}

fn default_time_format() -> TimeFormat {
    TimeFormat::HhMmSs
}

/// Clock time format options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    /// HH:mm:ss (protocol: 0x00)
    #[serde(alias = "hh:mm:ss")]
    HhMmSs,
    /// yyyy-MM-dd (protocol: 0x01)
    Date,
    /// EEEE (weekday) (protocol: 0x02)
    Weekday,
}

impl TimeFormat {
    pub fn to_protocol_byte(&self) -> u8 {
        match self {
            TimeFormat::HhMmSs => 0x00,
            TimeFormat::Date => 0x01,
            TimeFormat::Weekday => 0x02,
        }
    }
}

/// Metric data sources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    CpuTemp,
    GpuTemp,
    CpuUsage,
    GpuUsage,
    MemUsage,
    /// Fixed value (e.g. for placeholders)
    Fixed(f64),
}

impl MetricSource {
    /// Returns the protocol show ID hex string for this source.
    pub fn show_id(&self) -> &'static str {
        match self {
            MetricSource::CpuTemp => "00",
            MetricSource::CpuUsage => "05",
            MetricSource::MemUsage => "06",
            MetricSource::GpuTemp => "0D",
            MetricSource::GpuUsage => "0E",
            MetricSource::Fixed(_) => "00", // default, caller should handle fixed specially
        }
    }

    /// Returns true if this source reads a temperature value.
    pub fn is_temperature(&self) -> bool {
        matches!(self, MetricSource::CpuTemp | MetricSource::GpuTemp)
    }
}

impl Widget {
    /// Returns the protocol viewType byte for this widget.
    pub fn view_type(&self) -> u8 {
        match &self.kind {
            WidgetKind::Text { .. } => 0x01,
            WidgetKind::Metric { .. } => 0x02,
            WidgetKind::Clock { .. } => 0x03,
            WidgetKind::Image { .. } => 0x04,
        }
    }

    /// Returns the protocol alpha byte (alpha × 10, clamped to 0–10).
    pub fn alpha_byte(&self) -> u8 {
        (self.alpha * 10.0).round().clamp(0.0, 10.0) as u8
    }

    /// Normalize color to 6-char uppercase hex without '#'.
    pub fn color_hex(&self) -> String {
        self.color.trim().trim_start_matches('#').to_uppercase()
    }
}

/// Convert a `Widget` to `WidgetHexParams` for protocol encoding.
impl TryFrom<&Widget> for crate::theme::hex::WidgetHexParams {
    type Error = String;

    fn try_from(w: &Widget) -> Result<Self, Self::Error> {
        use crate::protocol::frame::normalize_color;
        use crate::theme::hex::WidgetHexParams;

        let text_color =
            normalize_color(&w.color).map_err(|e| format!("widget color error: {}", e))?;

        let mut p = WidgetHexParams {
            view_type: w.view_type(),
            pos_x: w.x,
            pos_y: w.y,
            width: w.width,
            height: w.height,
            text_size: w.text_size,
            text_color,
            alpha: w.alpha_byte(),
            bold: if w.bold { 1 } else { 0 },
            italic: if w.italic { 1 } else { 0 },
            underline: if w.underline { 1 } else { 0 },
            del_line: if w.strikethrough { 1 } else { 0 },
            typeface_path: w.font.clone(),
            typeface_type: if w.font.is_empty() { 0x00 } else { 0x01 },
            ..Default::default()
        };

        match &w.kind {
            WidgetKind::Clock { time_format } => {
                p.time_format = time_format.to_protocol_byte();
            }
            WidgetKind::Metric {
                source,
                unit,
                label,
                show_label,
            } => {
                p.num_type = u8::from_str_radix(source.show_id(), 16)
                    .map_err(|_| format!("invalid show_id for {:?}", source))?;
                p.num_unit = unit.clone();
                p.num_text = label.clone();
                p.show_text = if *show_label { 0x01 } else { 0x00 };
            }
            WidgetKind::Image { path } => {
                let remote_name = image_remote_name(path);
                if remote_name.as_bytes().len() > 150 {
                    return Err(format!(
                        "image path basename too long ({} bytes, max 150)",
                        remote_name.as_bytes().len()
                    ));
                }
                p.image_path = remote_name;
            }
            WidgetKind::Text { content } => {
                // view_type=0x01 text content is read from image_path.
                if content.as_bytes().len() > 150 {
                    return Err(format!(
                        "text content too long ({} bytes, max 150)",
                        content.as_bytes().len()
                    ));
                }
                p.animation = 0x00;
                p.image_path = content.clone();
            }
        }

        Ok(p)
    }
}

/// Extract all unique `(show_id, MetricSource)` pairs from a theme's metric widgets.
///
/// `Fixed` sources are excluded — they are static values embedded in the widget
/// definition and do not need to be updated via cmd15.
///
/// If two metric widgets share the same show_id (same `MetricSource`), only the
/// first occurrence is kept to avoid duplicate cmd15 writes at the same offset.
pub fn theme_metric_sources(theme: &Theme) -> Vec<(String, MetricSource)> {
    let mut seen = std::collections::HashSet::new();
    theme
        .widgets
        .iter()
        .filter_map(|w| {
            if let WidgetKind::Metric { source, .. } = &w.kind {
                // Exclude Fixed — static values need no runtime update.
                if matches!(source, MetricSource::Fixed(_)) {
                    return None;
                }
                let id = source.show_id().to_string();
                if seen.insert(id.clone()) {
                    Some((id, source.clone()))
                } else {
                    None // duplicate show_id — skip
                }
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
        assert_eq!(w.alpha_byte(), 10); // 1.0 × 10 = 10
        w.alpha = 0.5;
        assert_eq!(w.alpha_byte(), 5); // 0.5 × 10 = 5
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
}
