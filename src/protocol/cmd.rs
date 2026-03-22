/// Command payload builders for the TRV LCD protocol.
///
/// Each `build_cmdXX_payload` function returns a `Vec<u8>` suitable for
/// passing to `frame::build_frame_default`.
use crate::protocol::constants::{encode_show_value, show_offsets};
use crate::protocol::widget::WidgetPayloadRaw;

/// CMD 0x3A — custom theme definition.
/// Header: num_widgets(1) + theme_type(1) + widget_data...
///
/// `theme_type` 0x01 = clear existing + add, 0x00 = append.
pub(crate) fn build_cmd3a_payload(
    widget_payloads: &[WidgetPayloadRaw],
    theme_type: u8,
) -> Result<Vec<u8>, String> {
    if widget_payloads.len() > u8::MAX as usize {
        return Err(format!(
            "too many widgets in one cmd3A payload: {} (max {})",
            widget_payloads.len(),
            u8::MAX
        ));
    }
    let num = widget_payloads.len() as u8;
    let mut out = Vec::with_capacity(
        2 + widget_payloads.len() * crate::protocol::constants::WIDGET_BYTES_LEN,
    );
    out.push(num);
    out.push(theme_type);
    for w in widget_payloads {
        out.extend_from_slice(&w.to_bytes());
    }
    Ok(out)
}

/// CMD 0x15 — live data update.
///
/// Builds a payload containing metric values at their correct show-ID offsets.
/// `show_values` is a slice of `(show_id_str, value_f64)` pairs.
pub fn build_cmd15_payload(show_values: &[(&str, f64)]) -> Result<Vec<u8>, String> {
    if show_values.is_empty() {
        return Err("show_values is empty".into());
    }

    let offsets = show_offsets();

    // Determine max byte offset needed
    let mut max_end_hex = 0usize;
    let mut pairs: Vec<(usize, usize, String)> = Vec::new();

    for (show, value) in show_values {
        let show_upper = show.to_uppercase();
        let (start, end) = offsets
            .get(show_upper.as_str())
            .ok_or_else(|| format!("Unknown show id: {}", show))?;
        let field_hex = encode_show_value(&show_upper, *value)?;
        if field_hex.len() != (end - start) {
            return Err(format!("Field width mismatch for show {}", show));
        }
        pairs.push((*start, *end, field_hex));
        if *end > max_end_hex {
            max_end_hex = *end;
        }
    }

    // Build content as a hex char array, then decode to bytes
    let mut content_chars: Vec<u8> = vec![b'0'; max_end_hex];
    for (start, end, field_hex) in &pairs {
        content_chars[*start..*end].copy_from_slice(field_hex.as_bytes());
    }
    let content_hex =
        std::str::from_utf8(&content_chars).map_err(|e| format!("utf8 encode error: {}", e))?;
    hex::decode(content_hex).map_err(|e| format!("hex decode error: {}", e))
}

/// CMD 0x24 — sleep/wake toggle.
/// `wake = true` → payload `[0x01]`, `wake = false` → `[0x00]`.
pub fn build_cmd24_payload(wake: bool) -> Vec<u8> {
    vec![if wake { 0x01 } else { 0x00 }]
}

/// CMD 0x38 — display orientation selector.
///
/// The device app interprets this as a raw orientation code (`0x00..=0x03`).
pub fn build_cmd38_payload(orientation_code: u8) -> Vec<u8> {
    vec![orientation_code]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cmd15_duplicate_show_id_last_wins() {
        // Passing the same show ID twice: the last value's bytes overwrite the first.
        // Both writes target the same byte range; the final result should reflect
        // the last value in the slice.
        let vals = [("00", 30.0f64), ("00", 50.0f64)];
        let p = build_cmd15_payload(&vals).unwrap();
        // show "00" is CPU temp in TENTHS: 50.0 × 10 = 500 = 0x01F4 LE
        assert_eq!(&p[0..2], &[0xF4, 0x01]);
    }

    #[test]
    fn test_build_cmd15_unknown_show_id_errors() {
        let vals = [("ZZ", 0.0f64)];
        assert!(build_cmd15_payload(&vals).is_err());
    }

    #[test]
    fn test_build_cmd15_four_metrics() {
        // Reference payload with 4 active metrics.
        // shows: 00=0, 05=0, 0D=0, 0E=0
        // Expected: 66 hex chars = 33 bytes of content
        let vals = [
            ("00", 40.0f64),
            ("05", 0.0f64),
            ("0D", 46.0f64),
            ("0E", 15.0f64),
        ];
        let p = build_cmd15_payload(&vals).unwrap();
        // Payload bytes for 4 metrics must cover up to offset 66/2=33 bytes
        assert_eq!(p.len(), 33);
        // CPU temp at offset 0..2 — show "00" is TENTHS: 40.0°C × 10 = 400 = 0x0190 LE
        assert_eq!(&p[0..2], &[0x90, 0x01]);
        // GPU temp at offset 29..31 — show "0D" is RAW integer (NOT in TENTHS_SHOWS): 46 = 0x002E LE
        assert_eq!(&p[29..31], &[0x2E, 0x00]);
        // GPU usage at offset 31..33 — show "0E" is RAW integer: 15 = 0x000F LE
        assert_eq!(&p[31..33], &[0x0F, 0x00]);
    }

    #[test]
    fn test_cmd24_wake() {
        assert_eq!(build_cmd24_payload(true), vec![0x01]);
        assert_eq!(build_cmd24_payload(false), vec![0x00]);
    }

    #[test]
    fn test_cmd38_orientation_payload() {
        assert_eq!(build_cmd38_payload(0x00), vec![0x00]);
        assert_eq!(build_cmd38_payload(0x03), vec![0x03]);
    }

    #[test]
    fn test_build_cmd3a_payload_single_widget_header_and_length() {
        let widget = WidgetPayloadRaw {
            view_type: 0x02,
            pos_x_le: [0x00, 0x00],
            pos_y_le: [0x00, 0x00],
            width_le: [0x00, 0x00],
            height_le: [0x00, 0x00],
            text_size_le: [0x28, 0x00],
            text_color: *b"FFFFFF",
            alpha: 0x0A,
            animation: 0x00,
            bold: 0x00,
            italic: 0x00,
            underline: 0x00,
            del_line: 0x00,
            num_type: 0x00,
            num_unit: [0x00; 5],
            show_text: 0x00,
            play_num: 0x00,
            time_format: 0x00,
            image_path: [0x00; 150],
            num_text: [0x00; 32],
            typeface_type: 0x00,
            typeface_path: [0x00; 32],
        };

        let payload = build_cmd3a_payload(std::slice::from_ref(&widget), 0x01).unwrap();
        assert_eq!(
            payload.len(),
            2 + crate::protocol::constants::WIDGET_BYTES_LEN
        );
        assert_eq!(payload[0], 0x01);
        assert_eq!(payload[1], 0x01);
        assert_eq!(payload[2], 0x02);
    }
}
