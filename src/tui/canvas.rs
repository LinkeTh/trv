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
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::theme::model::{Theme, Widget, WidgetKind};

/// Device display dimensions.
pub const DISPLAY_W: u16 = 484;
pub const DISPLAY_H: u16 = 480;

/// Return the terminal `Color` used to represent each widget type.
pub fn widget_color(widget: &Widget) -> Color {
    match &widget.kind {
        WidgetKind::Metric { .. } => Color::Cyan,
        WidgetKind::Clock { .. } => Color::Yellow,
        WidgetKind::Image { .. } => Color::Green,
        WidgetKind::Video { .. } => Color::LightMagenta,
        WidgetKind::Text { .. } => Color::White,
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
        Style::default().fg(Color::LightCyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Canvas ")
        .borders(Borders::ALL)
        .border_style(border_style);

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
    render_box(f, off_x, off_y, canvas_w, canvas_h, Color::DarkGray, false);

    // Draw a "484×480" label in the bottom-right corner of the device outline
    let label = "484×480";
    if canvas_w >= label.len() as u16 + 2 && canvas_h >= 2 {
        let lbl_x = off_x + canvas_w - label.len() as u16 - 1;
        let lbl_y = off_y + canvas_h - 1;
        if lbl_y < inner.y + inner.height {
            let p = Paragraph::new(Line::from(Span::styled(
                label,
                Style::default().fg(Color::DarkGray),
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
            let mx = off_x + canvas_w / 2 - msg.len() as u16 / 2;
            let my = off_y + canvas_h / 2;
            if mx >= inner.x && my >= inner.y && mx + msg.len() as u16 <= inner.x + inner.width {
                let p = Paragraph::new(Line::from(Span::styled(
                    msg,
                    Style::default().fg(Color::DarkGray),
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
        Color::LightYellow
    } else {
        widget_color(widget)
    };

    let (cx, cy) = pixel_to_cell(widget.x, widget.y, scale);
    let (cw_raw, ch_raw) = pixel_to_cell(widget.width.max(1), widget.height.max(1), scale);

    // Clamp to canvas area
    let cw = cw_raw.max(1).min(canvas_w.saturating_sub(cx));
    let ch = ch_raw.max(1).min(canvas_h.saturating_sub(cy));

    let x = off_x + cx;
    let y = off_y + cy;

    render_box(f, x, y, cw, ch, color, selected);

    // Draw type label inside the box (top-left corner, if space allows)
    let label = widget_type_label(widget);
    let num_label = format!("[{}]", idx + 1);
    let full_label = format!("{}{}", label, num_label);

    if ch >= 1 && cw >= full_label.len() as u16 + 1 {
        let p = Paragraph::new(Line::from(Span::styled(
            full_label,
            Style::default().fg(color),
        )));
        f.render_widget(p, Rect::new(x + 1, y, cw.saturating_sub(2), 1));
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
