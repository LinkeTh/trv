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

pub const FONT_OPTION_DEFAULT: &str = "default";

/// Supported font selectors for custom-theme widgets.
pub const FONT_OPTIONS: &[&str] = &[
    FONT_OPTION_DEFAULT,
    "msyh",
    "arial",
    "impact",
    "calibri",
    "georgia",
    "ni7seg",
    "harmonyos_black",
    "harmonyos_bold",
    "harmonyos_light",
    "harmonyos_medium",
    "harmonyos_thin",
];

/// Normalize a user/font-file string into a canonical font selector.
///
/// Accepts both selector values (e.g. `harmonyos_bold`) and legacy asset-like
/// names (e.g. `NI7SEG.TTF`, `HarmonyOS_Sans_Bold.ttf`).
pub fn normalize_font_option(font: &str) -> Option<&'static str> {
    let trimmed = font.trim();
    if trimmed.is_empty() {
        return Some(FONT_OPTION_DEFAULT);
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower == FONT_OPTION_DEFAULT {
        return Some(FONT_OPTION_DEFAULT);
    }

    if lower.contains("msyh") {
        return Some("msyh");
    }
    if lower.contains("arial") {
        return Some("arial");
    }
    if lower.contains("impact") {
        return Some("impact");
    }
    if lower.contains("calibri") {
        return Some("calibri");
    }
    if lower.contains("georgia") {
        return Some("georgia");
    }
    if lower.contains("ni7seg") {
        return Some("ni7seg");
    }

    if lower.contains("harmonyos") {
        if lower.contains("black") {
            return Some("harmonyos_black");
        }
        if lower.contains("bold") || lower.contains("blod") {
            return Some("harmonyos_bold");
        }
        if lower.contains("light") {
            return Some("harmonyos_light");
        }
        if lower.contains("medium") {
            return Some("harmonyos_medium");
        }
        if lower.contains("thin") {
            return Some("harmonyos_thin");
        }
    }

    None
}

/// Map canonical font selector to the protocol-side typeface string.
fn font_option_to_protocol(option: &str) -> Option<&'static str> {
    match option {
        FONT_OPTION_DEFAULT => None,
        "msyh" => Some("msyh"),
        "arial" => Some("arial"),
        "impact" => Some("impact"),
        "calibri" => Some("calibri"),
        "georgia" => Some("georgia"),
        "ni7seg" => Some("ni7seg"),
        "harmonyos_black" => Some("harmonyos_black"),
        // Firmware checks for the typo token "blod" when selecting bold.
        "harmonyos_bold" => Some("harmonyos_blod"),
        "harmonyos_light" => Some("harmonyos_light"),
        "harmonyos_medium" => Some("harmonyos_medium"),
        "harmonyos_thin" => Some("harmonyos_thin"),
        _ => None,
    }
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
    /// viewType 0x05 — video loaded from /sdcard/ and played in loop by firmware
    Video {
        /// Local video path (daemon auto-pushes to /sdcard/; basename is sent in protocol)
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
            WidgetKind::Video { .. } => 0x05,
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

        let font_option = normalize_font_option(&w.font).ok_or_else(|| {
            format!(
                "unsupported font '{}': choose from fixed font selectors",
                w.font
            )
        })?;
        let (typeface_type, typeface_path) = match font_option_to_protocol(font_option) {
            Some(v) => (0x01, v),
            None => (0x00, ""),
        };

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
            typeface_path: typeface_path.to_string(),
            typeface_type,
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
                if unit.len() > 5 {
                    return Err(format!(
                        "metric unit too long ({} bytes, max 5): '{}'",
                        unit.len(),
                        unit
                    ));
                }
                p.num_unit = unit.clone();
                if label.len() > 32 {
                    return Err(format!(
                        "metric label too long ({} bytes, max 32): '{}'",
                        label.len(),
                        label
                    ));
                }
                p.num_text = label.clone();
                p.show_text = if *show_label { 0x01 } else { 0x00 };
            }
            WidgetKind::Image { path } => {
                let remote_name = image_remote_name(path);
                if remote_name.len() > 150 {
                    return Err(format!(
                        "image path basename too long ({} bytes, max 150)",
                        remote_name.len()
                    ));
                }
                p.image_path = remote_name;
            }
            WidgetKind::Video { path } => {
                let remote_name = image_remote_name(path);
                if remote_name.len() > 150 {
                    return Err(format!(
                        "video path basename too long ({} bytes, max 150)",
                        remote_name.len()
                    ));
                }
                p.image_path = remote_name;
                // Keep hidden for now: firmware uses play_num as queue weight.
                // We default to 1 for endless loop semantics with a single video.
                p.play_num = 0x01;
            }
            WidgetKind::Text { content } => {
                // view_type=0x01 text content is read from image_path.
                if content.len() > 150 {
                    return Err(format!(
                        "text content too long ({} bytes, max 150)",
                        content.len()
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
/// If two metric widgets share the same show_id (same `MetricSource`), only the
/// first occurrence is kept to avoid duplicate cmd15 writes at the same offset.
pub fn theme_metric_sources(theme: &Theme) -> Vec<(String, MetricSource)> {
    let mut seen = std::collections::HashSet::new();
    theme
        .widgets
        .iter()
        .filter_map(|w| {
            if let WidgetKind::Metric { source, .. } = &w.kind {
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
mod tests;
