//! The Adalight serial protocol: "Ada", LED count - 1 (big endian), checksum, then RGB bytes.

/// Wraps RGB bytes (3 per LED) in an Adalight frame, into `out` (cleared first).
pub fn frame(rgb: &[u8], out: &mut Vec<u8>) {
    out.clear();
    let leds = rgb.len() / 3;
    if leds == 0 {
        return;
    }
    let n = leds - 1;
    let (hi, lo) = ((n >> 8) as u8, n as u8);
    out.extend_from_slice(&[b'A', b'd', b'a', hi, lo, hi ^ lo ^ 0x55]);
    out.extend_from_slice(&rgb[..leds * 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_for_139_leds() {
        let mut out = Vec::new();
        frame(&[0; 139 * 3], &mut out);
        assert_eq!(&out[..6], &[b'A', b'd', b'a', 0x00, 138, 138 ^ 0x55]);
        assert_eq!(out.len(), 6 + 139 * 3);
    }

    #[test]
    fn header_above_255_leds() {
        let mut out = Vec::new();
        frame(&[0; 300 * 3], &mut out);
        assert_eq!(&out[3..6], &[0x01, 0x2B, 0x01 ^ 0x2B ^ 0x55]);
    }

    #[test]
    fn payload_follows_header() {
        let mut out = Vec::new();
        frame(&[1, 2, 3], &mut out);
        assert_eq!(&out[6..], &[1, 2, 3]);
    }
}
