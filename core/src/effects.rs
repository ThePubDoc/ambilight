//! Animated effects for Effect mode.

use crate::config::{Effect, Rgb};
use crate::layout::Region;
use crate::sound::SoundVisualizer;
use std::f32::consts::TAU;

#[derive(Default)]
pub struct Effects {
    flicker: Vec<f32>,
    targets: Vec<f32>,
    seed: u32,
}

impl Effects {
    /// Colors for `effect` at time `t` seconds; `dt` is the time since the previous frame.
    pub fn render(
        &mut self,
        effect: Effect,
        regions: &[Region],
        t: f32,
        dt: f32,
        color: Rgb,
        sound: &SoundVisualizer,
    ) -> Vec<Rgb> {
        let n = regions.len().max(1) as f32;
        match effect {
            Effect::Rainbow => (0..regions.len()).map(|i| Rgb::from_hue(i as f32 / n + t * 0.08)).collect(),
            Effect::Spectrum => sound.render_spectrum(regions),
            Effect::Breathing => {
                // One slow breath every 6 seconds, never fully dark.
                let v = 0.12 + 0.88 * (0.5 - 0.5 * (TAU * t / 6.0).cos());
                vec![color.scale(v); regions.len()]
            }
            Effect::Candle => self.candle(regions.len(), dt),
            Effect::Aurora => (0..regions.len())
                .map(|i| {
                    let u = i as f32 / n;
                    let wave = (TAU * (u * 2.0 + t * 0.05)).sin();
                    let ripple = (TAU * (u * 3.3 - t * 0.03) + 1.0).sin();
                    // Greens and teals, drifting into violet where the ripple crests.
                    let hue = 0.33 + 0.12 * (0.5 + 0.5 * wave) + 0.4 * (ripple - 0.6).max(0.0);
                    let glow = (0.5 + 0.5 * (TAU * (u * 1.7 + t * 0.07)).sin()).powi(2);
                    Rgb::from_hue(hue).scale(0.15 + 0.85 * glow)
                })
                .collect(),
        }
    }

    /// Warm flame flicker: each LED wanders toward a random target brightness.
    fn candle(&mut self, n: usize, dt: f32) -> Vec<Rgb> {
        if self.flicker.len() != n {
            self.flicker = vec![0.8; n];
            self.targets = vec![0.8; n];
        }
        for i in 0..n {
            if self.random() < dt * 8.0 {
                // Mostly bright, with the occasional deeper dip.
                let r = self.random();
                self.targets[i] = if r < 0.1 { 0.35 + r * 2.0 } else { 0.6 + 0.4 * self.random() };
            }
            self.flicker[i] += (self.targets[i] - self.flicker[i]) * (dt * 12.0).min(1.0);
        }
        let flame = Rgb::new(1.0, 0.42, 0.08);
        self.flicker.iter().map(|&v| flame.scale(v)).collect()
    }

    /// xorshift32 in 0..1.
    fn random(&mut self) -> f32 {
        if self.seed == 0 {
            self.seed = 0x9E37_79B9;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::layout;

    fn regions() -> Vec<Region> {
        layout::build(&Config { top: 10, right: 5, bottom: 10, left: 5, ..Config::default() })
    }

    #[test]
    fn every_effect_fills_the_strip_in_range() {
        let r = regions();
        let sound = SoundVisualizer::default();
        let mut fx = Effects::default();
        for effect in [Effect::Rainbow, Effect::Spectrum, Effect::Breathing, Effect::Candle, Effect::Aurora] {
            for step in 0..50 {
                let colors = fx.render(effect, &r, step as f32 * 0.1, 0.02, Rgb::new(1.0, 0.5, 0.2), &sound);
                assert_eq!(colors.len(), r.len());
                assert!(colors.iter().all(|c| [c.r, c.g, c.b].iter().all(|v| (0.0..=1.0).contains(v))), "{effect:?}");
            }
        }
    }

    #[test]
    fn breathing_never_goes_fully_dark() {
        let r = regions();
        let mut fx = Effects::default();
        let sound = SoundVisualizer::default();
        let min = (0..600)
            .map(|i| fx.render(Effect::Breathing, &r, i as f32 * 0.01, 0.01, Rgb::WHITE, &sound)[0].r)
            .fold(1.0f32, f32::min);
        assert!(min >= 0.12 - 1e-4);
    }

    #[test]
    fn candle_stays_warm() {
        let r = regions();
        let mut fx = Effects::default();
        let sound = SoundVisualizer::default();
        for i in 0..200 {
            for c in fx.render(Effect::Candle, &r, i as f32 * 0.02, 0.02, Rgb::WHITE, &sound) {
                assert!(c.r >= c.g && c.g >= c.b);
            }
        }
    }
}
