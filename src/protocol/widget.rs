use crate::protocol::constants::WIDGET_BYTES_LEN;

/// Raw on-wire layout for one cmd3A widget payload (247 bytes).
///
/// Fields are stored exactly as transmitted, with little-endian byte arrays for
/// numeric 16-bit values and fixed-size byte arrays for text/path fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WidgetPayloadRaw {
    pub(crate) view_type: u8,
    pub(crate) pos_x_le: [u8; 2],
    pub(crate) pos_y_le: [u8; 2],
    pub(crate) width_le: [u8; 2],
    pub(crate) height_le: [u8; 2],
    pub(crate) text_size_le: [u8; 2],
    pub(crate) text_color: [u8; 6],
    pub(crate) alpha: u8,
    pub(crate) animation: u8,
    pub(crate) bold: u8,
    pub(crate) italic: u8,
    pub(crate) underline: u8,
    pub(crate) del_line: u8,
    pub(crate) num_type: u8,
    pub(crate) num_unit: [u8; 5],
    pub(crate) show_text: u8,
    pub(crate) play_num: u8,
    pub(crate) time_format: u8,
    pub(crate) image_path: [u8; 150],
    pub(crate) num_text: [u8; 32],
    pub(crate) typeface_type: u8,
    pub(crate) typeface_path: [u8; 32],
}

impl WidgetPayloadRaw {
    pub(crate) fn to_bytes(&self) -> [u8; WIDGET_BYTES_LEN] {
        let mut out = [0u8; WIDGET_BYTES_LEN];
        let mut idx = 0usize;

        out[idx] = self.view_type;
        idx += 1;
        out[idx..idx + 2].copy_from_slice(&self.pos_x_le);
        idx += 2;
        out[idx..idx + 2].copy_from_slice(&self.pos_y_le);
        idx += 2;
        out[idx..idx + 2].copy_from_slice(&self.width_le);
        idx += 2;
        out[idx..idx + 2].copy_from_slice(&self.height_le);
        idx += 2;
        out[idx..idx + 2].copy_from_slice(&self.text_size_le);
        idx += 2;
        out[idx..idx + 6].copy_from_slice(&self.text_color);
        idx += 6;
        out[idx] = self.alpha;
        idx += 1;
        out[idx] = self.animation;
        idx += 1;
        out[idx] = self.bold;
        idx += 1;
        out[idx] = self.italic;
        idx += 1;
        out[idx] = self.underline;
        idx += 1;
        out[idx] = self.del_line;
        idx += 1;
        out[idx] = self.num_type;
        idx += 1;
        out[idx..idx + 5].copy_from_slice(&self.num_unit);
        idx += 5;
        out[idx] = self.show_text;
        idx += 1;
        out[idx] = self.play_num;
        idx += 1;
        out[idx] = self.time_format;
        idx += 1;
        out[idx..idx + 150].copy_from_slice(&self.image_path);
        idx += 150;
        out[idx..idx + 32].copy_from_slice(&self.num_text);
        idx += 32;
        out[idx] = self.typeface_type;
        idx += 1;
        out[idx..idx + 32].copy_from_slice(&self.typeface_path);
        idx += 32;

        assert_eq!(
            idx, WIDGET_BYTES_LEN,
            "internal error: widget payload serialized length mismatch"
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_payload_raw_to_bytes_len() {
        let raw = WidgetPayloadRaw {
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

        let bytes = raw.to_bytes();
        assert_eq!(bytes.len(), WIDGET_BYTES_LEN);
    }

    #[test]
    fn test_widget_payload_raw_field_offsets() {
        let raw = WidgetPayloadRaw {
            view_type: 0xAA,
            pos_x_le: [0x01, 0x02],
            pos_y_le: [0x03, 0x04],
            width_le: [0x05, 0x06],
            height_le: [0x07, 0x08],
            text_size_le: [0x09, 0x0A],
            text_color: [0x10, 0x11, 0x12, 0x13, 0x14, 0x15],
            alpha: 0x16,
            animation: 0x17,
            bold: 0x18,
            italic: 0x19,
            underline: 0x1A,
            del_line: 0x1B,
            num_type: 0x1C,
            num_unit: [0x1D, 0x1E, 0x1F, 0x20, 0x21],
            show_text: 0x22,
            play_num: 0x23,
            time_format: 0x24,
            image_path: [0x25; 150],
            num_text: [0x26; 32],
            typeface_type: 0x27,
            typeface_path: [0x28; 32],
        };

        let bytes = raw.to_bytes();
        assert_eq!(bytes[0], 0xAA);
        assert_eq!(&bytes[1..3], &[0x01, 0x02]);
        assert_eq!(&bytes[3..5], &[0x03, 0x04]);
        assert_eq!(&bytes[5..7], &[0x05, 0x06]);
        assert_eq!(&bytes[7..9], &[0x07, 0x08]);
        assert_eq!(&bytes[9..11], &[0x09, 0x0A]);
        assert_eq!(&bytes[11..17], &[0x10, 0x11, 0x12, 0x13, 0x14, 0x15]);
        assert_eq!(bytes[17], 0x16);
        assert_eq!(bytes[18], 0x17);
        assert_eq!(bytes[19], 0x18);
        assert_eq!(bytes[20], 0x19);
        assert_eq!(bytes[21], 0x1A);
        assert_eq!(bytes[22], 0x1B);
        assert_eq!(bytes[23], 0x1C);
        assert_eq!(&bytes[24..29], &[0x1D, 0x1E, 0x1F, 0x20, 0x21]);
        assert_eq!(bytes[29], 0x22);
        assert_eq!(bytes[30], 0x23);
        assert_eq!(bytes[31], 0x24);
        assert_eq!(&bytes[32..182], &[0x25; 150]);
        assert_eq!(&bytes[182..214], &[0x26; 32]);
        assert_eq!(bytes[214], 0x27);
        assert_eq!(&bytes[215..247], &[0x28; 32]);
    }
}
