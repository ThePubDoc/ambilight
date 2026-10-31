//! The Adalight serial protocol: "Ada", LED count - 1 (big endian), checksum, then RGB bytes.

use crate::config::Rgb;

/// Encodes one frame into `out` (cleared first).
pub fn encode(colors: &[Rgb], gamma: f32, brightness: f32, out: &mut Vec<u8>) {
    out.clear();
    if colors.is_empty() {
        return;
    }
    let n = colors.len() - 1;
    let (hi, lo) = ((n >> 8) as u8, n as u8);
    out.extend_from_slice(&[b'A', b'd', b'a', hi, lo, hi ^ lo ^ 0x55]);
    let byte = |v: f32| (v.clamp(0.0, 1.0).powf(gamma) * brightness.clamp(0.0, 1.0) * 255.0).round() as u8;
    for c in colors {
        out.extend_from_slice(&[byte(c.r), byte(c.g), byte(c.b)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_for_139_leds() {
        let mut out = Vec::new();
        encode(&vec![Rgb::BLACK; 139], 1.0, 1.0, &mut out);
        assert_eq!(&out[..6], &[b'A', b'd', b'a', 0x00, 138, 138 ^ 0x55]);
        assert_eq!(out.len(), 6 + 139 * 3);
    }

    #[test]
    fn header_above_255_leds() {
        let mut out = Vec::new();
        encode(&vec![Rgb::BLACK; 300], 1.0, 1.0, &mut out);
        assert_eq!(&out[3..6], &[0x01, 0x2B, 0x01 ^ 0x2B ^ 0x55]);
    }

    #[test]
    fn brightness_and_gamma() {
        let mut out = Vec::new();
        encode(&[Rgb::new(1.0, 0.5, 0.0)], 1.0, 0.5, &mut out);
        assert_eq!(&out[6..], &[128, 64, 0]);
        encode(&[Rgb::new(0.5, 0.5, 0.5)], 2.0, 1.0, &mut out);
        assert_eq!(out[6], 64);
    }
}
