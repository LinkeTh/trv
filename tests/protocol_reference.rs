/// M1 integration tests — byte-for-byte comparison against protocol
/// reference vectors captured during development.
use trv::protocol::{cmd::build_cmd15_payload, frame::build_frame_default};
use trv::theme::hex::{WidgetHexParams, build_widget_hex};

/// Helper: decode a hex string to bytes, panicking on error.
fn from_hex(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap_or_else(|e| panic!("invalid hex in test: {}", e))
}

// ─── cmd 0x15 — live metric update ───────────────────────────────────────────

/// Reference vector for cmd15 with 4 metrics at known values.
#[test]
fn test_cmd15_four_metrics_matches_reference() {
    let vals = [
        ("00", 40.0f64),
        ("05", 0.0f64),
        ("0D", 46.0f64),
        ("0E", 15.0f64),
    ];
    let payload = build_cmd15_payload(&vals).expect("build_cmd15_payload");
    let frame = build_frame_default(0x15, &payload);

    let expected = from_hex(
        "AAF50023001590010000000000000000000000000000000000000000000000000000002E000F0000",
    );
    assert_eq!(
        frame,
        expected,
        "cmd15 frame mismatch.\n  got:      {}\n  expected: {}",
        hex::encode_upper(&frame),
        hex::encode_upper(&expected),
    );
}

// ─── cmd 0x3A widget encoder ─────────────────────────────────────────────────

/// Reference vector for a default widget (viewType=02, all zeros/defaults).
const REFERENCE_DEFAULT_WIDGET_HEX: &str = concat!(
    "02000000000000000028004646464646460A00000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000"
);

#[test]
fn test_default_widget_hex_matches_reference() {
    let p = WidgetHexParams::default();
    let got = build_widget_hex(&p).expect("build_widget_hex");

    // Normalise: remove whitespace/newlines from the const (line-folded for readability)
    let expected: String = REFERENCE_DEFAULT_WIDGET_HEX
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert_eq!(got.len(), 494, "widget hex must be 494 chars");
    assert_eq!(expected.len(), 494, "reference hex must be 494 chars");
    assert_eq!(
        got,
        expected,
        "default widget hex mismatch.\n  got:      {}\n  expected: {}",
        &got[..40],
        &expected[..40],
    );
}

// ─── frame builder sanity ────────────────────────────────────────────────────

/// Reference vector for a minimal cmd15 frame (single metric, CPU temp = 40°C).
#[test]
fn test_frame_builder_single_metric() {
    let vals = [("00", 40.0f64)];
    let payload = build_cmd15_payload(&vals).expect("build_cmd15_payload");
    let frame = build_frame_default(0x15, &payload);

    // AAF5 + len(0x0004=SN+CMD+2payload) + 00 (SN) + 15 (CMD) + 9001 + 00 (tail)
    let expected = from_hex("AAF500040015900100");
    assert_eq!(
        frame,
        expected,
        "single-metric frame mismatch.\n  got:      {}\n  expected: {}",
        hex::encode_upper(&frame),
        hex::encode_upper(&expected),
    );
}
