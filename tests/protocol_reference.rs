/// M1 integration tests — byte-for-byte comparison against protocol
/// reference vectors captured during development.
use trv::protocol::{
    cmd::{Cmd15Field, ShowId, build_cmd15_frame},
    frame::build_frame_default,
};
use trv::theme::hex::{WidgetHexParams, build_widget_hex};

/// Helper: decode a hex string to bytes, panicking on error.
fn from_hex(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap_or_else(|e| panic!("invalid hex in test: {}", e))
}

// ─── cmd 0x15 — live metric update ───────────────────────────────────────────

/// Reference vector for cmd15 with 4 metrics at known values.
#[test]
fn test_cmd15_four_metrics_matches_reference() {
    let fields = [
        Cmd15Field {
            show_id: ShowId::try_from("00").expect("valid show id 00"),
            value: 40.0,
        },
        Cmd15Field {
            show_id: ShowId::try_from("05").expect("valid show id 05"),
            value: 0.0,
        },
        Cmd15Field {
            show_id: ShowId::try_from("0D").expect("valid show id 0D"),
            value: 46.0,
        },
        Cmd15Field {
            show_id: ShowId::try_from("0E").expect("valid show id 0E"),
            value: 15.0,
        },
    ];
    let frame = build_cmd15_frame(&fields).expect("build_cmd15_frame");

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
    let fields = [Cmd15Field {
        show_id: ShowId::try_from("00").expect("valid show id 00"),
        value: 40.0,
    }];
    let payload =
        trv::protocol::cmd::build_cmd15_payload(&fields).expect("build_cmd15_payload_typed");
    let frame = build_frame_default(0x15, &payload).expect("build_frame_default");

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
