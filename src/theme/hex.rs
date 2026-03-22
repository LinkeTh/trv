/// Widget hex encoder for cmd 3A custom themes.
///
/// Each widget is encoded as a fixed 494-hex-char string (247 bytes).
/// `WidgetHexParams` values are converted into `WidgetPayloadRaw` for on-wire
/// serialization.
use crate::protocol::{
    constants::{CMD_CUSTOM_THEME, WIDGET_HEX_LEN},
    frame::encode_ascii_padded_bytes,
    widget::WidgetPayloadRaw,
};

/// Parameters for building a single widget's binary data.
#[derive(Debug, Clone)]
pub struct WidgetHexParams {
    /// viewType: 01=text/bg, 02=metric, 03=clock, 04=image, 05=video, 07=marquee
    pub view_type: u8,
    pub pos_x: u16,
    pub pos_y: u16,
    pub width: u16,
    pub height: u16,
    pub text_size: u16,
    /// 6-char ASCII color like "FFFFFF"
    pub text_color: String,
    /// 0–10 (app divides by 10 for opacity: 10 = fully opaque)
    pub alpha: u8,
    pub animation: u8,
    pub bold: u8,
    pub italic: u8,
    pub underline: u8,
    pub del_line: u8,
    /// Show ID (e.g. 0x00 for CPU temp)
    pub num_type: u8,
    /// Unit string, max 5 UTF-8 bytes
    pub num_unit: String,
    /// 0x00 = value+unit only; 0x01 = label+value+unit
    pub show_text: u8,
    pub play_num: u8,
    /// 0x00 = HH:mm:ss, 0x01 = yyyy-MM-dd, 0x02 = EEEE (weekday)
    pub time_format: u8,
    /// Filename on device /sdcard/, max 150 bytes
    pub image_path: String,
    /// Label text, max 32 bytes
    pub num_text: String,
    /// 0x00 = default font, 0x01 = custom
    pub typeface_type: u8,
    /// Font filename on device, max 32 bytes
    pub typeface_path: String,
}

impl Default for WidgetHexParams {
    fn default() -> Self {
        Self {
            view_type: 0x02,
            pos_x: 0,
            pos_y: 0,
            width: 0,
            height: 0,
            text_size: 40,
            text_color: "FFFFFF".into(),
            alpha: 10,
            animation: 0,
            bold: 0,
            italic: 0,
            underline: 0,
            del_line: 0,
            num_type: 0x00,
            num_unit: String::new(),
            show_text: 0x00,
            play_num: 0x00,
            time_format: 0x00,
            image_path: String::new(),
            num_text: String::new(),
            typeface_type: 0x00,
            typeface_path: String::new(),
        }
    }
}

fn encode_padded_field<const N: usize>(text: &str) -> [u8; N] {
    encode_ascii_padded_bytes(text, N)
        .try_into()
        .expect("encode_ascii_padded_bytes must return exactly N bytes")
}

impl TryFrom<&WidgetHexParams> for WidgetPayloadRaw {
    type Error = String;

    fn try_from(p: &WidgetHexParams) -> Result<Self, Self::Error> {
        Ok(Self {
            view_type: p.view_type,
            pos_x_le: p.pos_x.to_le_bytes(),
            pos_y_le: p.pos_y.to_le_bytes(),
            width_le: p.width.to_le_bytes(),
            height_le: p.height.to_le_bytes(),
            text_size_le: p.text_size.to_le_bytes(),
            text_color: encode_padded_field::<6>(&p.text_color),
            alpha: p.alpha,
            animation: p.animation,
            bold: p.bold,
            italic: p.italic,
            underline: p.underline,
            del_line: p.del_line,
            num_type: p.num_type,
            num_unit: encode_padded_field::<5>(&p.num_unit),
            show_text: p.show_text,
            play_num: p.play_num,
            time_format: p.time_format,
            image_path: encode_padded_field::<150>(&p.image_path),
            num_text: encode_padded_field::<32>(&p.num_text),
            typeface_type: p.typeface_type,
            typeface_path: encode_padded_field::<32>(&p.typeface_path),
        })
    }
}

/// Build the 247-byte widget binary from a `WidgetHexParams`.
///
/// Field layout (494 hex chars = 247 bytes):
///   viewType(1) posX(2LE) posY(2LE) width(2LE) height(2LE) textSize(2LE)
///   textColor(6 ASCII) alpha(1) animation(1) bold(1) italic(1) underline(1) delLine(1)
///   numType(1) numUnit(5 UTF-8) showText(1) playNum(1) timeFormat(1)
///   imagePath(150 ASCII) numText(32 ASCII) typefaceType(1) typefacePath(32 ASCII)
pub(crate) fn build_widget_bytes(p: &WidgetHexParams) -> Result<Vec<u8>, String> {
    let raw = WidgetPayloadRaw::try_from(p)?;
    Ok(raw.to_bytes().to_vec())
}

/// Build the 494-hex-char widget string (uppercase) from a `WidgetHexParams`.
pub fn build_widget_hex(p: &WidgetHexParams) -> Result<String, String> {
    let bytes = build_widget_bytes(p)?;
    let hex = hex::encode_upper(&bytes);
    assert_eq!(
        hex.len(),
        WIDGET_HEX_LEN,
        "internal error: widget hex serialized length mismatch"
    );
    Ok(hex)
}

/// Split a list of widget byte arrays into individual cmd 3A frames to avoid
/// TCP fragmentation. Each widget becomes its own AAF5 frame.
///
/// Returns `Vec<Vec<u8>>` of complete frames — the first uses theme_type=0x01
/// (clear + add) and subsequent frames use 0x00 (append).
pub(crate) fn split_cmd3a_frames(widget_list: &[WidgetPayloadRaw]) -> Result<Vec<Vec<u8>>, String> {
    use crate::protocol::{cmd::build_cmd3a_payload, frame::build_frame_default};

    widget_list
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let ttype = if i == 0 { 0x01 } else { 0x00 };
            let payload = build_cmd3a_payload(std::slice::from_ref(w), ttype)?;
            build_frame_default(CMD_CUSTOM_THEME, &payload)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::constants::WIDGET_HEX_LEN;

    #[test]
    fn test_widget_hex_len() {
        let p = WidgetHexParams::default();
        let hex = build_widget_hex(&p).unwrap();
        assert_eq!(
            hex.len(),
            WIDGET_HEX_LEN,
            "widget hex must be exactly 494 chars"
        );
    }

    #[test]
    fn test_widget_viewtype_position() {
        let p = WidgetHexParams {
            view_type: 0x04,
            ..Default::default()
        };
        let hex = build_widget_hex(&p).unwrap();
        // First 2 hex chars = viewType byte
        assert_eq!(&hex[0..2], "04");
    }

    #[test]
    fn test_widget_pos_x_le() {
        let p = WidgetHexParams {
            pos_x: 100, // 0x0064 LE → [0x64, 0x00] → "6400"
            ..Default::default()
        };
        let hex = build_widget_hex(&p).unwrap();
        // viewType(2) + posX starts at offset 2
        assert_eq!(&hex[2..6], "6400");
    }

    #[test]
    fn test_widget_text_color_ascii_encoded() {
        let p = WidgetHexParams {
            text_color: "00DDFF".into(),
            ..Default::default()
        };
        let hex = build_widget_hex(&p).unwrap();
        // textColor is at hex offset 22..34 (after viewType+posX+posY+width+height+textSize = 11 bytes = 22 hex chars)
        // "00DDFF" as ASCII bytes = [0x30,0x30,0x44,0x44,0x46,0x46] = "303044444646"
        assert_eq!(&hex[22..34], "303044444646");
    }

    #[test]
    fn test_split_cmd3a_frames_empty_returns_empty() {
        let frames = split_cmd3a_frames(&[]).unwrap();
        assert!(
            frames.is_empty(),
            "empty widget list should produce zero frames"
        );
    }

    #[test]
    fn test_split_cmd3a_frames_first_clears() {
        let p = WidgetHexParams::default();
        let w = WidgetPayloadRaw::try_from(&p).unwrap();
        let frames = split_cmd3a_frames(&[w.clone(), w.clone()]).unwrap();
        assert_eq!(frames.len(), 2);

        // First frame payload[0] = 0x01 (num_widgets=1), payload[1] = 0x01 (clear+add)
        // Frame: AA F5 <len> 00 3A <payload> 00
        // Payload starts at byte 6
        assert_eq!(frames[0][7], 0x01, "first frame theme_type should be 0x01");
        assert_eq!(frames[1][7], 0x00, "second frame theme_type should be 0x00");
    }
}
