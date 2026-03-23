/// Canvas panel — braille-based pixel preview of the 484×480 display.
///
/// The device display is 484 wide × 480 tall.  We map it to the available
/// terminal area by computing a scale factor, then draw widget bounding boxes
/// as thin border rectangles, color-coded by widget type.
///
/// Braille characters encode 2×4 dots per cell (ratatui's `Canvas` uses this
/// approach via `BrailleGrid`).  For simplicity in M4 we use ratatui's built-in
/// `canvas::Rectangle` shape which draws widget outlines directly.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::theme::model::{MetricSource, Theme, TimeFormat, Widget, WidgetKind, image_remote_name};

use super::palette;

/// Device display dimensions.
pub const DISPLAY_W: u16 = 484;
pub const DISPLAY_H: u16 = 480;

/// Return the terminal `Color` used to represent each widget type.
pub fn widget_color(widget: &Widget) -> Color {
    match &widget.kind {
        WidgetKind::Metric { .. } => palette::SAPPHIRE,
        WidgetKind::Clock { .. } => palette::PEACH,
        WidgetKind::Image { .. } => palette::GREEN,
        WidgetKind::Video { .. } => palette::MAUVE,
        WidgetKind::Text { .. } => palette::ROSEWATER,
    }
}

/// Short type label for display inside the bounding box on the canvas.
pub fn widget_type_label(widget: &Widget) -> &'static str {
    match &widget.kind {
        WidgetKind::Metric { .. } => "MET",
        WidgetKind::Clock { .. } => "CLK",
        WidgetKind::Image { .. } => "IMG",
        WidgetKind::Video { .. } => "VID",
        WidgetKind::Text { .. } => "TXT",
    }
}

/// Render the canvas panel into `area`.
///
/// Draws a device-outline rectangle and widget bounding boxes scaled to the
/// available terminal space.
pub fn render(
    f: &mut Frame,
    area: Rect,
    theme: Option<&Theme>,
    selected_idx: Option<usize>,
    focused: bool,
) {
    let border_style = if focused {
        Style::default()
            .fg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::SURFACE2)
    };

    let title = if focused { " ● Canvas " } else { " Canvas " };

    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        });

    if focused {
        block = block.title_bottom(Line::from(Span::styled(
            " ←↑↓→:move  Shift+←↑↓→:x10  j/k:select ",
            Style::default().fg(palette::OVERLAY1),
        )));
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 4 {
        return;
    }

    // Scale factors: device pixels → terminal cells
    // Terminal cells are roughly 2× taller than wide, so we apply a 2.0
    // aspect-ratio correction on the vertical axis.
    let scale_x = inner.width as f32 / DISPLAY_W as f32;
    let scale_y = inner.height as f32 / DISPLAY_H as f32;
    // Use the smaller scale to fit the whole display; preserve aspect ratio.
    let scale = scale_x.min(scale_y * 2.0);

    // Compute the pixel area used within `inner` (centered).
    let canvas_w = ((DISPLAY_W as f32) * scale) as u16;
    let canvas_h = ((DISPLAY_H as f32) * (scale / 2.0)) as u16;
    let off_x = inner.x + (inner.width.saturating_sub(canvas_w)) / 2;
    let off_y = inner.y + (inner.height.saturating_sub(canvas_h)) / 2;

    // Draw the device outline
    render_box(
        f,
        off_x,
        off_y,
        canvas_w,
        canvas_h,
        palette::OVERLAY0,
        false,
    );

    // Draw a "484×480" label in the bottom-right corner of the device outline
    let label = "484×480";
    if canvas_w >= label.len() as u16 + 2 && canvas_h >= 2 {
        let lbl_x = off_x + canvas_w - label.len() as u16 - 1;
        let lbl_y = off_y + canvas_h - 1;
        if lbl_y < inner.y + inner.height {
            let p = Paragraph::new(Line::from(Span::styled(
                label,
                Style::default().fg(palette::OVERLAY1),
            )));
            f.render_widget(p, Rect::new(lbl_x, lbl_y, label.len() as u16, 1));
        }
    }

    // Draw each widget bounding box
    if let Some(theme) = theme {
        for (i, widget) in theme.widgets.iter().enumerate() {
            let is_selected = selected_idx == Some(i);
            draw_widget_box(
                f,
                widget,
                i,
                is_selected,
                scale,
                off_x,
                off_y,
                canvas_w,
                canvas_h,
            );
        }
    } else {
        // No theme: show a placeholder message
        if canvas_h > 2 && canvas_w > 20 {
            let msg = "No theme loaded";
            let mx = (off_x + canvas_w / 2).saturating_sub(msg.len() as u16 / 2);
            let my = off_y + canvas_h / 2;
            if mx >= inner.x && my >= inner.y && mx + msg.len() as u16 <= inner.x + inner.width {
                let p = Paragraph::new(Line::from(Span::styled(
                    msg,
                    Style::default().fg(palette::SUBTEXT0),
                )));
                f.render_widget(p, Rect::new(mx, my, msg.len() as u16, 1));
            }
        }
    }
}

/// Convert device-pixel coordinates to terminal cell coordinates within the canvas area.
fn pixel_to_cell(px: u16, py: u16, scale: f32) -> (u16, u16) {
    let cx = (px as f32 * scale).round() as u16;
    let cy = (py as f32 * (scale / 2.0)).round() as u16;
    (cx, cy)
}

/// Return the widget rectangle in canvas-cell coordinates.
///
/// Video widgets are rendered fullscreen because the device app currently
/// ignores per-widget geometry for `viewType=0x05` playback.
fn widget_canvas_geometry(
    widget: &Widget,
    scale: f32,
    canvas_w: u16,
    canvas_h: u16,
) -> (u16, u16, u16, u16) {
    if matches!(widget.kind, WidgetKind::Video { .. }) {
        return (0, 0, canvas_w, canvas_h);
    }

    let (cx, cy) = pixel_to_cell(widget.x, widget.y, scale);
    let (cw_raw, ch_raw) = pixel_to_cell(widget.width.max(1), widget.height.max(1), scale);

    // Clamp to canvas area
    let cw = cw_raw.max(1).min(canvas_w.saturating_sub(cx));
    let ch = ch_raw.max(1).min(canvas_h.saturating_sub(cy));

    (cx, cy, cw, ch)
}

/// Draw a widget bounding box on the canvas.
#[allow(clippy::too_many_arguments)]
fn draw_widget_box(
    f: &mut Frame,
    widget: &Widget,
    idx: usize,
    selected: bool,
    scale: f32,
    off_x: u16,
    off_y: u16,
    canvas_w: u16,
    canvas_h: u16,
) {
    let color = if selected {
        palette::YELLOW
    } else {
        widget_color(widget)
    };

    let (cx, cy, cw, ch) = widget_canvas_geometry(widget, scale, canvas_w, canvas_h);

    let x = off_x + cx;
    let y = off_y + cy;

    render_box(f, x, y, cw, ch, color, selected);

    // Draw type+detail label inside the box (top-left corner, if space allows)
    let max_chars = cw.saturating_sub(2) as usize;
    if ch >= 1 {
        let Some(full_label) = widget_canvas_label(widget, idx, max_chars) else {
            return;
        };
        let p = Paragraph::new(Line::from(Span::styled(
            full_label,
            Style::default().fg(color),
        )));
        f.render_widget(p, Rect::new(x + 1, y, cw.saturating_sub(2), 1));
    }
}

fn widget_canvas_label(widget: &Widget, idx: usize, max_chars: usize) -> Option<String> {
    let base = format!("{}[{}]", widget_type_label(widget), idx + 1);
    let detail = widget_detail_label(widget);
    fit_canvas_label(&base, &detail, max_chars)
}

fn widget_detail_label(widget: &Widget) -> String {
    match &widget.kind {
        WidgetKind::Metric { source, label, .. } => {
            let label = label.trim();
            if label.is_empty() {
                metric_source_key(source).to_string()
            } else {
                label.to_string()
            }
        }
        WidgetKind::Clock { time_format } => clock_format_key(time_format).to_string(),
        WidgetKind::Image { path } | WidgetKind::Video { path } => image_remote_name(path),
        WidgetKind::Text { content } => normalize_single_line(content),
    }
}

fn fit_canvas_label(base: &str, detail: &str, max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }

    let base_len = base.chars().count();
    if base_len > max_chars {
        return Some(truncate_with_ellipsis(base, max_chars));
    }

    let detail = detail.trim();
    if detail.is_empty() {
        return Some(base.to_string());
    }

    let full = format!("{} {}", base, detail);
    if full.chars().count() <= max_chars {
        return Some(full);
    }

    let reserve_for_base = base_len + 1;
    if reserve_for_base >= max_chars {
        return Some(base.to_string());
    }

    let detail_budget = max_chars - reserve_for_base;
    let detail_fitted = truncate_with_ellipsis(detail, detail_budget);
    Some(format!("{} {}", base, detail_fitted))
}

fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }

    if max_chars == 1 {
        return "…".to_string();
    }

    let mut out: String = s.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

fn normalize_single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn metric_source_key(source: &MetricSource) -> &'static str {
    match source {
        MetricSource::CpuTemp => "cpu_temp",
        MetricSource::GpuTemp => "gpu_temp",
        MetricSource::CpuUsage => "cpu_usage",
        MetricSource::GpuUsage => "gpu_usage",
        MetricSource::MemUsage => "mem_usage",
    }
}

fn clock_format_key(format: &TimeFormat) -> &'static str {
    match format {
        TimeFormat::HhMmSs => "hh:mm:ss",
        TimeFormat::Date => "date",
        TimeFormat::Weekday => "weekday",
    }
}

/// Draw a hollow rectangle border at the given terminal coordinates.
fn render_box(f: &mut Frame, x: u16, y: u16, w: u16, h: u16, color: Color, bold: bool) {
    if w < 2 || h < 2 {
        return;
    }

    use ratatui::{style::Modifier, widgets::BorderType};

    let mut style = Style::default().fg(color);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    let border_type = if bold {
        BorderType::Thick
    } else {
        BorderType::Plain
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .border_type(border_type);

    f.render_widget(block, Rect::new(x, y, w, h));
}

#[cfg(test)]
mod tests;
