/// Command payload builders for the TRV LCD protocol.
///
/// Each `build_cmdXX_payload` function returns a `Vec<u8>` suitable for
/// passing to `frame::build_frame_default`.
use crate::protocol::{
    constants::{encode_show_value, show_offsets},
    frame::{encode_ascii_padded_bytes, normalize_color, u16le_tenths},
};

/// CMD 0x22 — single metric + background image setup.
///
/// Payload layout:
///   device_num_type(1) + value_tenths(2 LE) + unit(3) + bg_filename(32)
pub fn build_cmd22_payload(
    device_num_type: u8,
    init_temp_c: f64,
    unit_text: &str,
    bg_name: &str,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.push(device_num_type);
    p.extend_from_slice(&u16le_tenths(init_temp_c));
    p.extend(encode_ascii_padded_bytes(unit_text, 3));
    p.extend(encode_ascii_padded_bytes(bg_name, 32));
    p
}

/// CMD 0x35 — multi-metric layout setup.
///
/// Supports num_model 0x02, 0x03, 0x04.
///
/// When `keep_bg_image` is true, the bgColor field is prefixed with `#` so
/// the app's `contains("#")` check keeps the ImageView visible (workaround).
pub struct Cmd35Params<'a> {
    pub num_model: u8,
    pub theme: u8,
    pub show1: &'a str,
    pub value1_c: f64,
    pub unit1: &'a str,
    pub show2: &'a str,
    pub value2_c: f64,
    pub unit2: &'a str,
    pub bg_name: &'a str,
    pub bg_color: &'a str,
    pub text_color: &'a str,
    pub show3: Option<&'a str>,
    pub value3_c: f64,
    pub unit3: &'a str,
    pub show4: Option<&'a str>,
    pub value4_c: f64,
    pub unit4: &'a str,
    pub keep_bg_image: bool,
}

pub fn build_cmd35_payload(p: &Cmd35Params) -> Result<Vec<u8>, String> {
    let nm = p.num_model;
    let th = p.theme;

    let n1_hex = encode_show_value(p.show1, p.value1_c)?;
    let n2_hex = encode_show_value(p.show2, p.value2_c)?;
    if n1_hex.len() != 4 || n2_hex.len() != 4 {
        return Err("cmd35 expects 2-byte numeric fields for show1/show2".into());
    }

    let bg_bytes = encode_ascii_padded_bytes(p.bg_name, 32);

    let bgc_bytes = if p.keep_bg_image {
        // Prefix '#' (0x23) to trick app into keeping image visible
        let color = normalize_color(p.bg_color)?;
        let prefixed = format!("#{}", &color[..5]);
        encode_ascii_padded_bytes(&prefixed, 6)
    } else {
        let color = normalize_color(p.bg_color)?;
        encode_ascii_padded_bytes(&color, 6)
    };
    let txt_bytes = {
        let color = normalize_color(p.text_color)?;
        encode_ascii_padded_bytes(&color, 6)
    };

    let mut out = Vec::new();
    out.push(nm);
    out.push(th);
    // show1 id
    let s1 = u8::from_str_radix(p.show1, 16).map_err(|_| format!("invalid show1: {}", p.show1))?;
    out.push(s1);
    out.extend(hex::decode(&n1_hex).unwrap());
    out.extend(encode_ascii_padded_bytes(p.unit1, 3));
    let s2 = u8::from_str_radix(p.show2, 16).map_err(|_| format!("invalid show2: {}", p.show2))?;
    out.push(s2);
    out.extend(hex::decode(&n2_hex).unwrap());
    out.extend(encode_ascii_padded_bytes(p.unit2, 3));

    if nm == 0x03 || nm == 0x04 {
        let show3 = p.show3.ok_or("num_model 03/04 requires show3")?;
        let n3_hex = encode_show_value(show3, p.value3_c)?;
        if n3_hex.len() != 4 {
            return Err("cmd35 expects 2-byte numeric field for show3".into());
        }
        let s3 = u8::from_str_radix(show3, 16).map_err(|_| format!("invalid show3: {}", show3))?;
        out.push(s3);
        out.extend(hex::decode(&n3_hex).unwrap());
        out.extend(encode_ascii_padded_bytes(p.unit3, 3));
    }

    if nm == 0x04 {
        let show4 = p.show4.ok_or("num_model 04 requires show4")?;
        let n4_hex = encode_show_value(show4, p.value4_c)?;
        if n4_hex.len() != 4 {
            return Err("cmd35 expects 2-byte numeric field for show4".into());
        }
        let s4 = u8::from_str_radix(show4, 16).map_err(|_| format!("invalid show4: {}", show4))?;
        out.push(s4);
        out.extend(hex::decode(&n4_hex).unwrap());
        out.extend(encode_ascii_padded_bytes(p.unit4, 3));
    }

    out.extend(bg_bytes);
    out.extend(bgc_bytes);
    out.extend(txt_bytes);

    if nm == 0x02 {
        // App requires content.length() >= 118 hex chars — append 1-byte pad
        out.push(0x00);
    }

    Ok(out)
}

/// CMD 0x3A — custom theme definition.
/// Header: num_widgets(1) + theme_type(1) + widget_data...
///
/// `theme_type` 0x01 = clear existing + add, 0x00 = append.
pub fn build_cmd3a_payload(widget_payloads: &[&[u8]], theme_type: u8) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cmd22_payload_length() {
        // device_num_type(1) + tenths(2) + unit(3) + bg(32) = 38 bytes
        let p = build_cmd22_payload(0x00, 40.0, "C", "background.jpg");
        assert_eq!(p.len(), 38);
        // First byte = device_num_type
        assert_eq!(p[0], 0x00);
        // Bytes 1-2 = 400 tenths LE = [0x90, 0x01]
        assert_eq!(&p[1..3], &[0x90, 0x01]);
        // Bytes 3-5 = "C\0\0"
        assert_eq!(&p[3..6], &[b'C', 0x00, 0x00]);
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
}
