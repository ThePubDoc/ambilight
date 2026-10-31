//! Turns colors into the RGB bytes sent to the strip: gamma, brightness, white balance, and a
//! current limit that keeps the strip within the power supply's rating.

use crate::config::Rgb;

/// Typical WS2812B draw per color channel at full brightness.
const MA_PER_CHANNEL: f32 = 20.0;
/// Quiescent draw of each LED's driver chip, even when dark.
const MA_IDLE_PER_LED: f32 = 1.0;
/// Stay this far below the supply's rating.
const SAFETY: f32 = 0.9;

pub struct Pipeline {
    pub gamma: f32,
    pub brightness: f32,
    pub white_balance: Rgb,
    /// Supply rating in amps; 0 disables the limit.
    pub max_current_a: f32,
}

impl Pipeline {
    /// RGB bytes for `colors` into `out` (cleared first).
    pub fn encode(&self, colors: &[Rgb], out: &mut Vec<u8>) {
        out.clear();
        out.reserve(colors.len() * 3);
        let b = self.brightness.clamp(0.0, 1.0);
        let wb = self.white_balance;
        let byte =
            |v: f32, gain: f32| (v.clamp(0.0, 1.0).powf(self.gamma) * b * gain.clamp(0.0, 1.0) * 255.0).round() as u8;
        for c in colors {
            out.extend_from_slice(&[byte(c.r, wb.r), byte(c.g, wb.g), byte(c.b, wb.b)]);
        }
        if self.max_current_a > 0.0 {
            limit_current(out, self.max_current_a);
        }
    }
}

/// Estimated draw of the strip in milliamps for these RGB bytes.
pub fn estimate_ma(rgb: &[u8]) -> f32 {
    let channels: u32 = rgb.iter().map(|&v| v as u32).sum();
    channels as f32 / 255.0 * MA_PER_CHANNEL + (rgb.len() / 3) as f32 * MA_IDLE_PER_LED
}

/// Scales all channels down uniformly so the estimate stays under 90% of `max_a`.
pub fn limit_current(rgb: &mut [u8], max_a: f32) {
    let idle = (rgb.len() / 3) as f32 * MA_IDLE_PER_LED;
    let budget = max_a * 1000.0 * SAFETY - idle;
    let drive = estimate_ma(rgb) - idle;
    if drive <= budget || drive <= 0.0 {
        return;
    }
    let k = (budget / drive).max(0.0);
    for v in rgb.iter_mut() {
        *v = (*v as f32 * k) as u8; // round down: never exceed the budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> Pipeline {
        Pipeline { gamma: 1.0, brightness: 1.0, white_balance: Rgb::WHITE, max_current_a: 0.0 }
    }

    #[test]
    fn brightness_and_gamma() {
        let mut out = Vec::new();
        Pipeline { brightness: 0.5, ..pipeline() }.encode(&[Rgb::new(1.0, 0.5, 0.0)], &mut out);
        assert_eq!(out, [128, 64, 0]);
        Pipeline { gamma: 2.0, ..pipeline() }.encode(&[Rgb::new(0.5, 0.5, 0.5)], &mut out);
        assert_eq!(out[0], 64);
    }

    #[test]
    fn white_balance_scales_channels() {
        let mut out = Vec::new();
        Pipeline { white_balance: Rgb::new(1.0, 0.8, 0.5), ..pipeline() }.encode(&[Rgb::WHITE], &mut out);
        assert_eq!(out, [255, 204, 128]);
    }

    #[test]
    fn full_white_139_leds_draws_about_8_amps() {
        let rgb = vec![255u8; 139 * 3];
        assert!((estimate_ma(&rgb) - 8479.0).abs() < 1.0);
    }

    #[test]
    fn limiter_keeps_draw_under_90_percent() {
        let mut out = Vec::new();
        Pipeline { max_current_a: 4.0, ..pipeline() }.encode(&vec![Rgb::WHITE; 139], &mut out);
        let ma = estimate_ma(&out);
        assert!(ma <= 3600.0, "{ma}");
        assert!(ma > 3400.0, "limited too much: {ma}");
        assert!(out.iter().all(|&v| v == out[0]), "uniform scaling keeps colors");
    }

    #[test]
    fn limiter_leaves_dim_frames_alone() {
        let mut out = Vec::new();
        Pipeline { max_current_a: 4.0, brightness: 0.1, ..pipeline() }.encode(&vec![Rgb::WHITE; 139], &mut out);
        assert!(out.iter().all(|&v| v == 26));
    }
}
