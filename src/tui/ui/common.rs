use super::*;

pub(super) fn display_field_value(field: &Field) -> String {
    match field.kind {
        FieldType::Toggle => {
            if field.value.eq_ignore_ascii_case("true") {
                "[x] true".to_string()
            } else {
                "[ ] false".to_string()
            }
        }
        _ => field.value.clone(),
    }
}

pub(super) fn parse_hex_color(hex: &str) -> Option<Color> {
    let normalized = hex.trim().trim_start_matches('#');
    if normalized.len() != 6 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let r = u8::from_str_radix(&normalized[0..2], 16).ok()?;
    let g = u8::from_str_radix(&normalized[2..4], 16).ok()?;
    let b = u8::from_str_radix(&normalized[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

pub(super) fn panel_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::SURFACE2)
    }
}

pub(super) fn panel_border_type(focused: bool) -> BorderType {
    if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    }
}

pub(super) fn widget_icon(w: &Widget) -> &'static str {
    match &w.kind {
        WidgetKind::Metric { .. } => "▸",
        WidgetKind::Clock { .. } => "⏱",
        WidgetKind::Image { .. } => "▣",
        WidgetKind::Video { .. } => "▶",
        WidgetKind::Text { .. } => "T",
    }
}

pub(super) fn widget_short_label(w: &Widget) -> String {
    match &w.kind {
        WidgetKind::Metric { source, unit, .. } => {
            format!("{:?} {}", source, unit)
        }
        WidgetKind::Clock { time_format } => format!("Clock {:?}", time_format),
        WidgetKind::Image { path } => {
            if path.is_empty() {
                "Image".to_string()
            } else {
                format!("Img:{}", path)
            }
        }
        WidgetKind::Video { path } => {
            if path.is_empty() {
                "Video".to_string()
            } else {
                format!("Vid:{}", path)
            }
        }
        WidgetKind::Text { content } => {
            let mut chars = content.chars();
            let preview: String = chars.by_ref().take(12).collect();
            if chars.next().is_some() {
                format!("\"{}…\"", preview)
            } else {
                format!("\"{}\"", content)
            }
        }
    }
}

/// Return a `Rect` centered within `r` with the given width and height.
pub(super) fn centered_rect(w: u16, h: u16, r: Rect) -> Rect {
    let x = r.x + r.width.saturating_sub(w) / 2;
    let y = r.y + r.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(r.width), h.min(r.height))
}
