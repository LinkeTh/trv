//! AAF5 protocol frame builder and utilities.
//!
//! Frame format:
//!   `AAF5` (2 bytes magic)
//!   + length (2 bytes, big-endian) — covers SN + CMD + payload bytes, NOT the tail
//!   + SN (1 byte)
//!   + CMD (1 byte)
//!   + payload (variable)
//!   + tail (1 byte, always 0x00)

/// Build a complete AAF5 frame.
///
/// `cmd`, `sn`, `tail` are raw bytes (single byte each).
/// `payload` is the raw payload bytes.
///
/// Returns the complete frame as a `Vec<u8>`, or an error if the payload is
/// too large to fit in the 2-byte length field.
pub fn build_frame(cmd: u8, payload: &[u8], sn: u8, tail: u8) -> Result<Vec<u8>, String> {
    // length = SN(1) + CMD(1) + payload — tail NOT included
    let length_raw = 1usize + 1usize + payload.len();
    if length_raw > u16::MAX as usize {
        return Err(format!(
            "frame payload too large: {} bytes (max {})",
            payload.len(),
            u16::MAX as usize - 2
        ));
    }
    let length: u16 = length_raw as u16;

    let mut frame = Vec::with_capacity(2 + 2 + 1 + 1 + payload.len() + 1);
    frame.extend_from_slice(b"\xAA\xF5");
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(sn);
    frame.push(cmd);
    frame.extend_from_slice(payload);
    frame.push(tail);
    Ok(frame)
}

/// Build a frame with default SN=0x00 and tail=0x00.
pub fn build_frame_default(cmd: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    build_frame(cmd, payload, 0x00, 0x00)
}

/// Encode a `u16` value (in tenths of a degree C) as 2-byte little-endian hex.
/// `value_c` is the floating-point Celsius value.
pub fn u16le_tenths(value_c: f64) -> [u8; 2] {
    let raw = (value_c * 10.0).round() as i64;
    let clamped = raw.clamp(0, 0xFFFF) as u16;
    clamped.to_le_bytes()
}

/// Encode a `u32` value as little-endian bytes, clamped to fit `byte_len` bytes.
pub fn encode_unsigned_le(raw: i64, byte_len: usize) -> Vec<u8> {
    let max_val = if byte_len >= 8 {
        i64::MAX
    } else {
        ((1i64) << (8 * byte_len)) - 1
    };
    let clamped = raw.clamp(0, max_val) as u64;
    clamped.to_le_bytes()[..byte_len].to_vec()
}

/// Encode an ASCII/UTF-8 string as a fixed-length byte field, null-padded or truncated.
///
/// Truncation is performed on a UTF-8 character boundary — never in the middle
/// of a multi-byte codepoint — so the resulting bytes are always valid UTF-8
/// (padded with NUL bytes to `byte_len`).
/// Returns raw bytes (not hex).
pub fn encode_ascii_padded_bytes(text: &str, byte_len: usize) -> Vec<u8> {
    let raw = text.as_bytes();
    let mut buf = vec![0u8; byte_len];
    if raw.len() <= byte_len {
        buf[..raw.len()].copy_from_slice(raw);
    } else {
        // Walk backwards from byte_len to find a valid UTF-8 char boundary.
        let mut safe_len = byte_len;
        while safe_len > 0 && !text.is_char_boundary(safe_len) {
            safe_len -= 1;
        }
        buf[..safe_len].copy_from_slice(&raw[..safe_len]);
    }
    buf
}

/// Same as `encode_ascii_padded_bytes` but returns uppercase hex string.
pub fn encode_ascii_padded(text: &str, byte_len: usize) -> String {
    hex::encode_upper(encode_ascii_padded_bytes(text, byte_len))
}

/// Normalize a color string: strip '#', uppercase, validate 6 hex digits.
pub fn normalize_color(color: &str) -> Result<String, String> {
    let c = color.trim().trim_start_matches('#').to_uppercase();
    if c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(c)
    } else {
        Err(format!("Invalid color: {:?}", color))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_frame_basic() {
        // Frame format: AAF5 + length + 00 + 15 + payload + 00
        // payload = bytes.fromhex("9001000000000000") = 8 bytes
        // length = 1(sn) + 1(cmd) + 8(payload) = 10 = 0x000A
        let payload = hex::decode("9001000000000000").unwrap();
        let frame = build_frame(0x15, &payload, 0x00, 0x00).unwrap();
        let hex = hex::encode_upper(&frame);
        assert!(
            hex.starts_with("AAF5000A0015"),
            "frame prefix wrong: {}",
            hex
        );
        assert!(
            hex.ends_with("00"),
            "frame should end with tail 00: {}",
            hex
        );
    }

    #[test]
    fn test_u16le_tenths() {
        // 40.0°C → 400 tenths → 0x0190 LE → [90, 01]
        let b = u16le_tenths(40.0);
        assert_eq!(b, [0x90, 0x01]);
    }

    #[test]
    fn test_encode_ascii_padded() {
        // "C" padded to 3 bytes → "43 00 00" = "430000"
        let h = encode_ascii_padded("C", 3);
        assert_eq!(h, "430000");
    }

    #[test]
    fn test_normalize_color() {
        assert_eq!(normalize_color("#00ddff"), Ok("00DDFF".to_string()));
        assert_eq!(normalize_color("FFFFFF"), Ok("FFFFFF".to_string()));
        assert!(normalize_color("xyz").is_err());
    }

    #[test]
    fn test_encode_unsigned_le() {
        // 400 as 2-byte LE = [0x90, 0x01]
        let b = encode_unsigned_le(400, 2);
        assert_eq!(b, vec![0x90, 0x01]);
    }

    #[test]
    fn test_encode_ascii_padded_utf8_boundary() {
        // "°" (U+00B0) encodes as 2 bytes [0xC2, 0xB0].
        // A 5-byte field with "°C" (3 bytes) fits fully.
        let b = encode_ascii_padded_bytes("°C", 5);
        assert_eq!(b, vec![0xC2, 0xB0, 0x43, 0x00, 0x00]);

        // Truncating "°C" to 1 byte must NOT cut mid-codepoint — result is
        // a single NUL-padded byte (the 2-byte °  doesn't fit at all).
        let b1 = encode_ascii_padded_bytes("°C", 1);
        assert_eq!(b1, vec![0x00]);

        // Truncating to 2 bytes fits exactly one "°".
        let b2 = encode_ascii_padded_bytes("°C", 2);
        assert_eq!(b2, vec![0xC2, 0xB0]);
    }

    #[test]
    fn test_build_frame_rejects_oversized_payload() {
        let payload = vec![0u8; 65_534];
        let result = build_frame(0x15, &payload, 0x00, 0x00);
        assert!(result.is_err(), "oversized payload should return Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("too large"),
            "error message should mention 'too large': {}",
            msg
        );
    }
}
