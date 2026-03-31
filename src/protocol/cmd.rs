/// Command payload builders for the TRV LCD protocol.
///
/// Each `build_cmdXX_payload` function returns a `Vec<u8>` suitable for
/// passing to `frame::build_frame_default`.
use std::fmt;

use crate::protocol::constants::{
    CMD_CUSTOM_THEME, CMD_METRIC_UPDATE, CMD_ORIENTATION, CMD_SLEEP_WAKE, CMD_TIME_SYNC,
    WIDGET_BYTES_LEN, encode_show_value, show_offsets,
};
use crate::protocol::frame::build_frame_default;
use crate::protocol::widget::WidgetPayloadRaw;

/// cmd3A apply mode: clear+add for first frame, append for subsequent frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeApplyMode {
    ClearAndAdd,
    Append,
}

impl ThemeApplyMode {
    const fn as_byte(self) -> u8 {
        match self {
            ThemeApplyMode::ClearAndAdd => 0x01,
            ThemeApplyMode::Append => 0x00,
        }
    }
}

/// Sleep/wake state for cmd24.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Sleep,
    Wake,
}

impl PowerState {
    const fn as_byte(self) -> u8 {
        match self {
            PowerState::Sleep => 0x00,
            PowerState::Wake => 0x01,
        }
    }
}

/// Raw orientation code for cmd38.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationCode {
    Raw0,
    Raw1,
    Raw2,
    Raw3,
}

impl OrientationCode {
    pub const fn as_u8(self) -> u8 {
        match self {
            OrientationCode::Raw0 => 0x00,
            OrientationCode::Raw1 => 0x01,
            OrientationCode::Raw2 => 0x02,
            OrientationCode::Raw3 => 0x03,
        }
    }
}

impl TryFrom<u8> for OrientationCode {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(OrientationCode::Raw0),
            0x01 => Ok(OrientationCode::Raw1),
            0x02 => Ok(OrientationCode::Raw2),
            0x03 => Ok(OrientationCode::Raw3),
            _ => Err(format!(
                "invalid orientation code: {value:02X} (expected 00..03)"
            )),
        }
    }
}

/// Validated cmd15 show ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShowId(u8);

impl ShowId {
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    pub fn as_hex(self) -> String {
        format!("{:02X}", self.0)
    }
}

impl fmt::Display for ShowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02X}", self.0)
    }
}

impl TryFrom<u8> for ShowId {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let hex = format!("{value:02X}");
        if show_offsets().contains_key(hex.as_str()) {
            Ok(ShowId(value))
        } else {
            Err(format!("unknown show id: {value:02X}"))
        }
    }
}

impl TryFrom<&str> for ShowId {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.len() != 2 || !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "invalid show id format: '{value}' (expected 2 hex chars)"
            ));
        }
        let raw = u8::from_str_radix(trimmed, 16)
            .map_err(|e| format!("invalid show id '{value}': {e}"))?;
        ShowId::try_from(raw)
    }
}

/// One cmd15 metric field update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cmd15Field {
    pub show_id: ShowId,
    pub value: f64,
}

/// CMD 0x3A — custom theme definition.
/// Header: num_widgets(1) + theme_type(1) + widget_data...
///
/// `theme_type` 0x01 = clear existing + add, 0x00 = append.
pub(crate) fn build_cmd3a_payload(
    widget_payloads: &[WidgetPayloadRaw],
    mode: ThemeApplyMode,
) -> Result<Vec<u8>, String> {
    if widget_payloads.len() > u8::MAX as usize {
        return Err(format!(
            "too many widgets in one cmd3A payload: {} (max {})",
            widget_payloads.len(),
            u8::MAX
        ));
    }
    let num = widget_payloads.len() as u8;
    let mut out = Vec::with_capacity(2 + widget_payloads.len() * WIDGET_BYTES_LEN);
    out.push(num);
    out.push(mode.as_byte());
    for w in widget_payloads {
        out.extend_from_slice(&w.to_bytes());
    }
    Ok(out)
}

/// Build a complete cmd3A frame for one or more typed widget payloads.
pub(crate) fn build_cmd3a_frame(
    widget_payloads: &[WidgetPayloadRaw],
    mode: ThemeApplyMode,
) -> Result<Vec<u8>, String> {
    let payload = build_cmd3a_payload(widget_payloads, mode)?;
    build_frame_default(CMD_CUSTOM_THEME, &payload)
}

/// CMD 0x15 — live data update.
///
/// Builds a payload containing metric values at their correct show-ID offsets.
pub fn build_cmd15_payload(fields: &[Cmd15Field]) -> Result<Vec<u8>, String> {
    if fields.is_empty() {
        return Err("fields is empty".into());
    }

    let offsets = show_offsets();

    // Determine max byte offset needed
    let mut max_end_hex = 0usize;
    let mut pairs: Vec<(usize, usize, String)> = Vec::new();

    for field in fields {
        let show_upper = field.show_id.as_hex();
        let (start, end) = offsets
            .get(show_upper.as_str())
            .ok_or_else(|| format!("Unknown show id: {}", field.show_id))?;
        let field_hex = encode_show_value(&show_upper, field.value)?;
        if field_hex.len() != (end - start) {
            return Err(format!("Field width mismatch for show {}", field.show_id));
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

/// Build a complete cmd15 frame from typed metric fields.
pub fn build_cmd15_frame(fields: &[Cmd15Field]) -> Result<Vec<u8>, String> {
    let payload = build_cmd15_payload(fields)?;
    build_frame_default(CMD_METRIC_UPDATE, &payload)
}

/// CMD 0x24 — sleep/wake toggle.
pub fn build_cmd24_payload(state: PowerState) -> Vec<u8> {
    vec![state.as_byte()]
}

/// Build a complete cmd24 frame from typed state.
pub fn build_cmd24_frame(state: PowerState) -> Result<Vec<u8>, String> {
    let payload = build_cmd24_payload(state);
    build_frame_default(CMD_SLEEP_WAKE, &payload)
}

/// CMD 0x38 — display orientation selector.
///
/// The device app interprets this as a raw orientation code (`0x00..=0x03`).
pub fn build_cmd38_payload(code: OrientationCode) -> Vec<u8> {
    vec![code.as_u8()]
}

/// Build a complete cmd38 frame from typed orientation code.
pub fn build_cmd38_frame(code: OrientationCode) -> Result<Vec<u8>, String> {
    let payload = build_cmd38_payload(code);
    build_frame_default(CMD_ORIENTATION, &payload)
}

/// CMD 0x36 legacy payload — 14-digit date string.
///
/// Accepts a 14-char date string `"YYYYMMDDHHmmss"` and appends two NUL bytes.
/// This matches the original app behavior on firmware paths that call
/// `Utils.setDeviceTimeStr(...)`.
///
/// On some firmware variants cmd36 does **not** use that branch and instead
/// routes through `Utils.setDeviceTime(long)`. For those variants, prefer
/// [`build_cmd36_payload_inverse_long`].
///
/// Payload layout (16 bytes):
///   \[date string as ASCII bytes: 14\] + \[0x00, 0x00: 2-byte padding\]
///
/// The firmware converts the raw payload to a hex string, trims 5 trailing
/// chars (4 from padding + 1 from the frame tail byte that leaks into
/// `content.element`), then hex-decodes back to ASCII to recover the original
/// 14-char date string.
pub fn build_cmd36_payload(local_time_str: &str) -> Result<Vec<u8>, String> {
    if local_time_str.len() != 14 || !local_time_str.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "cmd36 time must be exactly 14 ASCII digits (YYYYMMDDHHmmss), got: {:?}",
            local_time_str
        ));
    }
    let mut payload = Vec::with_capacity(16);
    payload.extend_from_slice(local_time_str.as_bytes()); // 14 bytes
    payload.extend_from_slice(&[0x00, 0x00]); // 2-byte padding
    Ok(payload) // 16 bytes total
}

/// Build a complete cmd36 frame from a local-time date string.
pub fn build_cmd36_frame(local_time_str: &str) -> Result<Vec<u8>, String> {
    let payload = build_cmd36_payload(local_time_str)?;
    build_frame_default(CMD_TIME_SYNC, &payload)
}

/// Build cmd36 payload that decodes through firmware `TextUtil.stringToLong`
/// into a target decimal value and then routes to `Utils.setDeviceTime(long)`.
///
/// This is the reliable path on firmware variants where cmd36 does not hit the
/// 14-char `setDeviceTimeStr` branch.
///
/// Payload layout is variable-length ASCII-hex bytes plus trailing `0x00 0x00`.
/// The trailing zeros are required to match firmware trimming behavior before
/// `stringToLong` conversion.
pub fn build_cmd36_payload_inverse_long(target_decimal: u64) -> Result<Vec<u8>, String> {
    let mut hx = format!("{:X}", target_decimal);
    if hx.len() % 2 != 0 {
        hx.insert(0, '0');
    }

    // Mirror TextUtil.reverseShex: swap byte-pairs from ends towards center.
    let mut chars: Vec<char> = hx.chars().collect();
    let length = chars.len();
    let times = length / 2;
    let mut i = 0usize;
    while i < times {
        let j = i + 1;
        let k = (length - i) - 2;
        let l = (length - i) - 1;
        let c1 = chars[i];
        let c2 = chars[j];
        chars[i] = chars[k];
        chars[j] = chars[l];
        chars[k] = c1;
        chars[l] = c2;
        i += 2;
    }

    let core_hex: String = chars.into_iter().collect();
    let payload_hex = format!("{}0000", core_hex);
    hex::decode(&payload_hex)
        .map_err(|e| format!("failed to decode inverse-long cmd36 payload: {e}"))
}

/// Build a complete cmd36 frame using the inverse-long payload strategy.
pub fn build_cmd36_frame_inverse_long(target_decimal: u64) -> Result<Vec<u8>, String> {
    let payload = build_cmd36_payload_inverse_long(target_decimal)?;
    build_frame_default(CMD_TIME_SYNC, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cmd15_duplicate_show_id_last_wins() {
        // Passing the same show ID twice: the last value's bytes overwrite the first.
        // Both writes target the same byte range; the final result should reflect
        // the last value in the slice.
        let vals = [
            Cmd15Field {
                show_id: ShowId::try_from("00").unwrap(),
                value: 30.0,
            },
            Cmd15Field {
                show_id: ShowId::try_from("00").unwrap(),
                value: 50.0,
            },
        ];
        let p = build_cmd15_payload(&vals).unwrap();
        // show "00" is CPU temp in TENTHS: 50.0 × 10 = 500 = 0x01F4 LE
        assert_eq!(&p[0..2], &[0xF4, 0x01]);
    }

    #[test]
    fn test_build_cmd15_empty_errors() {
        let vals: [Cmd15Field; 0] = [];
        assert!(build_cmd15_payload(&vals).is_err());
    }

    #[test]
    fn test_build_cmd15_four_metrics() {
        // Reference payload with 4 active metrics.
        // shows: 00=0, 05=0, 0D=0, 0E=0
        // Expected: 66 hex chars = 33 bytes of content
        let vals = [
            Cmd15Field {
                show_id: ShowId::try_from("00").unwrap(),
                value: 40.0,
            },
            Cmd15Field {
                show_id: ShowId::try_from("05").unwrap(),
                value: 0.0,
            },
            Cmd15Field {
                show_id: ShowId::try_from("0D").unwrap(),
                value: 46.0,
            },
            Cmd15Field {
                show_id: ShowId::try_from("0E").unwrap(),
                value: 15.0,
            },
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
        assert_eq!(build_cmd24_payload(PowerState::Wake), vec![0x01]);
        assert_eq!(build_cmd24_payload(PowerState::Sleep), vec![0x00]);
    }

    #[test]
    fn test_cmd38_orientation_payload() {
        assert_eq!(build_cmd38_payload(OrientationCode::Raw0), vec![0x00]);
        assert_eq!(build_cmd38_payload(OrientationCode::Raw3), vec![0x03]);
        assert_eq!(build_cmd38_payload(OrientationCode::Raw2), vec![0x02]);
    }

    #[test]
    fn test_orientation_code_try_from_rejects_invalid() {
        assert!(OrientationCode::try_from(0x04).is_err());
    }

    #[test]
    fn test_show_id_try_from_rejects_unknown() {
        assert!(ShowId::try_from("FF").is_err());
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

        let payload =
            build_cmd3a_payload(std::slice::from_ref(&widget), ThemeApplyMode::ClearAndAdd)
                .unwrap();
        assert_eq!(
            payload.len(),
            2 + crate::protocol::constants::WIDGET_BYTES_LEN
        );
        assert_eq!(payload[0], 0x01);
        assert_eq!(payload[1], 0x01);
        assert_eq!(payload[2], 0x02);
    }

    #[test]
    fn test_build_cmd36_payload_format() {
        let payload = build_cmd36_payload("20250331100000").unwrap();
        assert_eq!(payload.len(), 16);
        assert_eq!(&payload[..14], b"20250331100000");
        assert_eq!(&payload[14..], &[0x00, 0x00]);
    }

    #[test]
    fn test_build_cmd36_rejects_short() {
        assert!(build_cmd36_payload("2025033110000").is_err()); // 13 chars
    }

    #[test]
    fn test_build_cmd36_rejects_long() {
        assert!(build_cmd36_payload("202503311000000").is_err()); // 15 chars
    }

    #[test]
    fn test_build_cmd36_rejects_non_digit() {
        assert!(build_cmd36_payload("2025033110000a").is_err());
    }

    #[test]
    fn test_build_cmd36_frame_structure() {
        let frame = build_cmd36_frame("20250331100000").unwrap();
        // AAF5 + len(0012=18=1+1+16) + SN(00) + CMD(36) + 16 payload bytes + tail(00)
        assert_eq!(frame.len(), 2 + 2 + 1 + 1 + 16 + 1); // 23 bytes
        let hex = hex::encode_upper(&frame);
        assert!(hex.starts_with("AAF500120036"), "prefix: {}", hex);
        assert!(hex.ends_with("00"), "tail: {}", hex);
    }
}
