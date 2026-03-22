/// Protocol constants: show ID offsets and encoding sets.
use std::collections::HashMap;
use std::sync::LazyLock;

// ── Command IDs ────────────────────────────────────────────────────────────

/// CMD 0x15 — metric update (cmd15): push live sensor readings to the device.
pub const CMD_METRIC_UPDATE: u8 = 0x15;
/// CMD 0x24 — sleep/wake toggle (cmd24).
pub const CMD_SLEEP_WAKE: u8 = 0x24;
/// CMD 0x38 — screen orientation (cmd38).
pub const CMD_ORIENTATION: u8 = 0x38;
/// CMD 0x3A — custom theme widget push (cmd3A).
pub const CMD_CUSTOM_THEME: u8 = 0x3A;

// ── Widget / payload sizes ─────────────────────────────────────────────────

/// Fixed widget hex length (494 hex chars = 247 bytes per widget in cmd 3A).
pub const WIDGET_HEX_LEN: usize = 494;

/// Show ID → (start_hex_char_offset, end_hex_char_offset) in the cmd15 payload.
/// Offsets are positions in the hex string (each byte = 2 hex chars).
///
/// Initialized once at first use via `LazyLock` to avoid a per-call HashMap allocation.
static SHOW_OFFSETS: LazyLock<HashMap<&'static str, (usize, usize)>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("00", (0usize, 4usize));
    m.insert("01", (4, 8));
    m.insert("02", (8, 12));
    m.insert("03", (12, 18));
    m.insert("04", (18, 22));
    m.insert("05", (22, 26));
    m.insert("06", (26, 30));
    m.insert("07", (30, 34));
    m.insert("08", (34, 42));
    m.insert("09", (42, 46));
    m.insert("0A", (46, 50));
    m.insert("0B", (50, 54));
    m.insert("0C", (54, 58));
    m.insert("0D", (58, 62));
    m.insert("0E", (62, 66));
    m.insert("0F", (66, 70));
    m.insert("10", (70, 74));
    m.insert("11", (74, 78));
    m.insert("16", (78, 82));
    m.insert("17", (82, 86));
    m.insert("18", (86, 90));
    m.insert("19", (90, 94));
    m.insert("1A", (94, 98));
    m.insert("1B", (98, 102));
    m.insert("1C", (102, 106));
    m.insert("1D", (106, 110));
    m.insert("1E", (110, 114));
    m.insert("1F", (114, 118));
    m.insert("20", (118, 122));
    m.insert("21", (122, 126));
    m.insert("22", (126, 130));
    m.insert("23", (130, 134));
    m.insert("24", (134, 138));
    m.insert("25", (138, 142));
    m.insert("26", (142, 148));
    m
});

/// Return a reference to the static show-offsets map.
pub fn show_offsets() -> &'static HashMap<&'static str, (usize, usize)> {
    &SHOW_OFFSETS
}

/// Shows encoded at tenths resolution (value × 10 before encoding).
pub const TENTHS_SHOWS: &[&str] = &["00", "01", "02", "04", "05"];

/// Shows encoded at hundredths resolution (value × 100).
pub const HUNDREDTHS_SHOWS: &[&str] = &["0A", "0B"];

/// Shows encoded at thousandths resolution (value × 1000).
pub const THOUSANDTHS_SHOWS: &[&str] = &["03", "26"];

/// Returns `true` if the given show ID uses tenths encoding.
pub fn is_tenths(show: &str) -> bool {
    TENTHS_SHOWS.contains(&show)
}

/// Returns `true` if the given show ID uses hundredths encoding.
pub fn is_hundredths(show: &str) -> bool {
    HUNDREDTHS_SHOWS.contains(&show)
}

/// Returns `true` if the given show ID uses thousandths encoding.
pub fn is_thousandths(show: &str) -> bool {
    THOUSANDTHS_SHOWS.contains(&show)
}

/// Encode a floating-point metric value for the given show ID.
/// Applies the correct scaling (×1, ×10, ×100, or ×1000) and encodes as
/// unsigned little-endian bytes of the appropriate width.
/// Returns the hex string of the encoded value.
pub fn encode_show_value(show: &str, value: f64) -> Result<String, String> {
    let offsets = show_offsets();
    let (start, end) = offsets
        .get(show)
        .ok_or_else(|| format!("Unknown show id: {:?}", show))?;
    let width = end - start; // hex char count
    if width % 2 != 0 {
        return Err(format!("Invalid width {} for show {}", width, show));
    }
    let byte_len = width / 2;

    let raw = if is_tenths(show) {
        (value * 10.0).round() as i64
    } else if is_hundredths(show) {
        (value * 100.0).round() as i64
    } else if is_thousandths(show) {
        (value * 1000.0).round() as i64
    } else {
        value.round() as i64
    };

    use crate::protocol::frame::encode_unsigned_le;
    let bytes = encode_unsigned_le(raw, byte_len);
    Ok(hex::encode_upper(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_offsets_coverage() {
        let offsets = show_offsets();
        assert_eq!(offsets["00"], (0, 4));
        assert_eq!(offsets["05"], (22, 26));
        assert_eq!(offsets["0D"], (58, 62));
        assert_eq!(offsets["0E"], (62, 66));
        assert_eq!(offsets["26"], (142, 148));
    }

    #[test]
    fn test_encode_show_value_cpu_temp() {
        // CPU temp show "00" is tenths — 40.0°C → raw=400 → 2-byte LE
        let h = encode_show_value("00", 40.0).unwrap();
        // 400 = 0x0190, LE = [0x90, 0x01] = "9001"
        assert_eq!(h, "9001");
    }

    #[test]
    fn test_encode_show_value_gpu_usage() {
        // GPU usage show "0E" is raw integer — 15.0 → raw=15 → 2-byte LE
        let h = encode_show_value("0E", 15.0).unwrap();
        // 15 = 0x000F, LE = [0x0F, 0x00] = "0F00"
        assert_eq!(h, "0F00");
    }

    #[test]
    fn test_encode_show_value_unknown() {
        assert!(encode_show_value("FF", 0.0).is_err());
    }
}
