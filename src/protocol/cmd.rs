/// Command payload builders for the TRV LCD protocol.
///
/// Each `build_cmdXX_payload` function returns a `Vec<u8>` suitable for
/// passing to `frame::build_frame_default`.
use crate::protocol::constants::{encode_show_value, show_offsets};

/// CMD 0x3A — custom theme definition.
/// Header: num_widgets(1) + theme_type(1) + widget_data...
///
/// `theme_type` 0x01 = clear existing + add, 0x00 = append.
pub fn build_cmd3a_payload(widget_payloads: &[&[u8]], theme_type: u8) -> Vec<u8> {
    debug_assert!(
        widget_payloads.len() <= u8::MAX as usize,
        "too many widgets in one cmd3A payload"
    );
    let num = widget_payloads.len() as u8;
    let mut out = Vec::new();
    out.push(num);
    out.push(theme_type);
    for w in widget_payloads {
        out.extend_from_slice(w);
    }
    out
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
    let content_hex = std::str::from_utf8(&content_chars).unwrap();
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
}
