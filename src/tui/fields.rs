/// Widget field display and in-place edit helpers for the Properties panel.
///
/// `widget_fields` returns a flat ordered list of `(name, current_value_string)`
/// pairs for a given widget.  `apply_field` takes a field name and a new
/// string value, validates it, and applies it to the widget in-place.
use crate::theme::model::{
    FONT_OPTION_DEFAULT, FONT_OPTIONS, MetricSource, TimeFormat, Widget, WidgetKind,
    normalize_font_option,
};

// ── Field list ────────────────────────────────────────────────────────────────

pub const SOURCE_OPTIONS: &[&str] = &[
    "cpu_temp",
    "gpu_temp",
    "cpu_usage",
    "gpu_usage",
    "mem_usage",
];

pub const TIME_FORMAT_OPTIONS: &[&str] = &["hh:mm:ss", "date", "weekday"];

/// What kind of editor the TUI should open for a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// Free-text input (the default).
    Text,
    /// Fixed set of choices (dropdown selector).
    Dropdown(&'static [&'static str]),
    /// Boolean toggle (true/false).
    Toggle,
    /// Colour picker.
    Color,
    /// File chooser for image/video asset paths.
    MediaPath(MediaPathKind),
}

/// Media widget kind for `path` chooser fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPathKind {
    Image,
    Video,
}

impl MediaPathKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    pub fn title_name(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }
}

/// A named field and its current string representation.
#[derive(Debug, Clone)]
pub struct Field {
    pub name: &'static str,
    pub value: String,
    pub kind: FieldType,
}

/// Return an ordered list of editable fields for `widget`.
///
/// The list always starts with the common positional/style fields, then
/// appends kind-specific fields.
pub fn widget_fields(w: &Widget) -> Vec<Field> {
    if let WidgetKind::Video { path } = &w.kind {
        return vec![Field {
            name: "path",
            value: path.clone(),
            kind: FieldType::MediaPath(MediaPathKind::Video),
        }];
    }

    let mut fields = vec![
        Field {
            name: "x",
            value: w.x.to_string(),
            kind: FieldType::Text,
        },
        Field {
            name: "y",
            value: w.y.to_string(),
            kind: FieldType::Text,
        },
        Field {
            name: "width",
            value: w.width.to_string(),
            kind: FieldType::Text,
        },
        Field {
            name: "height",
            value: w.height.to_string(),
            kind: FieldType::Text,
        },
        Field {
            name: "text_size",
            value: w.text_size.to_string(),
            kind: FieldType::Text,
        },
        Field {
            name: "color",
            value: format!("#{}", w.color_hex()),
            kind: FieldType::Color,
        },
        Field {
            name: "alpha",
            value: format!("{:.2}", w.alpha),
            kind: FieldType::Text,
        },
        Field {
            name: "bold",
            value: w.bold.to_string(),
            kind: FieldType::Toggle,
        },
        Field {
            name: "italic",
            value: w.italic.to_string(),
            kind: FieldType::Toggle,
        },
        Field {
            name: "underline",
            value: w.underline.to_string(),
            kind: FieldType::Toggle,
        },
        Field {
            name: "strike",
            value: w.strikethrough.to_string(),
            kind: FieldType::Toggle,
        },
        Field {
            name: "font",
            value: normalize_font_option(&w.font)
                .unwrap_or(w.font.trim())
                .to_string(),
            kind: FieldType::Dropdown(FONT_OPTIONS),
        },
    ];

    match &w.kind {
        WidgetKind::Metric {
            source,
            unit,
            label,
            show_label,
        } => {
            fields.push(Field {
                name: "source",
                value: source_to_str(source).to_string(),
                kind: FieldType::Dropdown(SOURCE_OPTIONS),
            });
            fields.push(Field {
                name: "unit",
                value: unit.clone(),
                kind: FieldType::Text,
            });
            fields.push(Field {
                name: "label",
                value: label.clone(),
                kind: FieldType::Text,
            });
            fields.push(Field {
                name: "show_label",
                value: show_label.to_string(),
                kind: FieldType::Toggle,
            });
        }
        WidgetKind::Clock { time_format } => {
            fields.push(Field {
                name: "time_format",
                value: time_format_to_str(time_format).to_string(),
                kind: FieldType::Dropdown(TIME_FORMAT_OPTIONS),
            });
        }
        WidgetKind::Image { path } => {
            fields.push(Field {
                name: "path",
                value: path.clone(),
                kind: FieldType::MediaPath(MediaPathKind::Image),
            });
        }
        WidgetKind::Video { .. } => unreachable!("video fields handled above"),
        WidgetKind::Text { content } => {
            fields.push(Field {
                name: "content",
                value: content.clone(),
                kind: FieldType::Text,
            });
        }
    }

    fields
}

// ── Apply a field edit ────────────────────────────────────────────────────────

/// Apply a new string `value` to the named `field` of `widget`.
///
/// Returns `Ok(())` on success or an error message string on validation failure.
pub fn apply_field(w: &mut Widget, field: &str, value: &str) -> Result<(), String> {
    let v = value.trim();
    match field {
        "x" => w.x = parse_u16(v, "x")?,
        "y" => w.y = parse_u16(v, "y")?,
        "width" => w.width = parse_u16(v, "width")?,
        "height" => w.height = parse_u16(v, "height")?,
        "text_size" => w.text_size = parse_u16(v, "text_size")?,
        "color" => {
            let hex = v.trim_start_matches('#');
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("color must be #RRGGBB".into());
            }
            w.color = hex.to_uppercase();
        }
        "alpha" => {
            let a: f32 = v.parse().map_err(|_| "alpha must be 0.0–1.0".to_string())?;
            if !(0.0..=1.0).contains(&a) {
                return Err("alpha must be 0.0–1.0".into());
            }
            w.alpha = a;
        }
        "bold" => w.bold = parse_bool(v, "bold")?,
        "italic" => w.italic = parse_bool(v, "italic")?,
        "underline" => w.underline = parse_bool(v, "underline")?,
        "strike" => w.strikethrough = parse_bool(v, "strike")?,
        "font" => {
            let option = normalize_font_option(v).ok_or_else(|| {
                format!("unknown font '{}' — valid: {}", v, FONT_OPTIONS.join(", "))
            })?;
            if option == FONT_OPTION_DEFAULT {
                w.font.clear();
            } else {
                w.font = option.to_string();
            }
        }

        // Kind-specific
        "source" => {
            if let WidgetKind::Metric { source, .. } = &mut w.kind {
                *source = parse_source(v)?;
            }
        }
        "unit" => {
            if let WidgetKind::Metric { unit, .. } = &mut w.kind {
                *unit = v.to_string();
            }
        }
        "label" => {
            if let WidgetKind::Metric { label, .. } = &mut w.kind {
                *label = v.to_string();
            }
        }
        "show_label" => {
            if let WidgetKind::Metric { show_label, .. } = &mut w.kind {
                *show_label = parse_bool(v, "show_label")?;
            }
        }
        "time_format" => {
            if let WidgetKind::Clock { time_format } = &mut w.kind {
                *time_format = parse_time_format(v)?;
            }
        }
        "path" => {
            if let WidgetKind::Image { path } | WidgetKind::Video { path } = &mut w.kind {
                *path = v.to_string();
            }
        }
        "content" => {
            if let WidgetKind::Text { content } = &mut w.kind {
                *content = v.to_string();
            }
        }
        other => return Err(format!("unknown field '{}'", other)),
    }
    Ok(())
}

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Return the string key used to display / parse a `MetricSource`.
pub fn source_to_str(src: &MetricSource) -> &'static str {
    match src {
        MetricSource::CpuTemp => "cpu_temp",
        MetricSource::GpuTemp => "gpu_temp",
        MetricSource::CpuUsage => "cpu_usage",
        MetricSource::GpuUsage => "gpu_usage",
        MetricSource::MemUsage => "mem_usage",
    }
}

/// Parse a `MetricSource` from its string key (TOML names accepted).
pub fn parse_source(s: &str) -> Result<MetricSource, String> {
    match s.to_ascii_lowercase().as_str() {
        "cpu_temp" => Ok(MetricSource::CpuTemp),
        "gpu_temp" => Ok(MetricSource::GpuTemp),
        "cpu_usage" => Ok(MetricSource::CpuUsage),
        "gpu_usage" => Ok(MetricSource::GpuUsage),
        "mem_usage" => Ok(MetricSource::MemUsage),
        other => Err(format!(
            "unknown source '{}' — valid: cpu_temp, gpu_temp, cpu_usage, gpu_usage, mem_usage",
            other
        )),
    }
}

/// Return the display/parse string for a `TimeFormat`.
pub fn time_format_to_str(tf: &TimeFormat) -> &'static str {
    match tf {
        TimeFormat::HhMmSs => "hh:mm:ss",
        TimeFormat::Date => "date",
        TimeFormat::Weekday => "weekday",
    }
}

/// Parse a `TimeFormat` from its string representation.
pub fn parse_time_format(s: &str) -> Result<TimeFormat, String> {
    match s.to_ascii_lowercase().as_str() {
        "hh:mm:ss" | "hhmm" | "time" => Ok(TimeFormat::HhMmSs),
        "date" => Ok(TimeFormat::Date),
        "weekday" => Ok(TimeFormat::Weekday),
        other => Err(format!(
            "unknown time_format '{}' — valid: hh:mm:ss, date, weekday",
            other
        )),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn parse_u16(s: &str, field: &str) -> Result<u16, String> {
    s.parse::<u16>()
        .map_err(|_| format!("'{}' must be an integer 0–65535", field))
}

fn parse_bool(s: &str, field: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!("'{}' must be true/false, got '{}'", field, other)),
    }
}

#[cfg(test)]
mod tests;
