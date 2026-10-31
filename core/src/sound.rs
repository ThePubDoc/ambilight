//! Turns audio loudness into LED colors for Sound mode: a slowly drifting hue whose brightness
//! follows the overall level, with a punch on bass hits and faster drift when treble is busy.

use crate::audio::{Bands, SPECTRUM_BANDS};
use crate::config::Rgb;
use crate::layout::Region;

/// Automatic gain for one band: tracks a decaying peak so quiet films and loud music both use
/// the full brightness range.
#[derive(Debug, Clone, Copy)]
struct Band {
    peak: f32,
    value: f32,
}

impl Band {
    /// Below this RMS the input counts as silence (avoids amplifying noise).
    const GATE: f32 = 0.002;
    const PEAK_FLOOR: f32 = 0.01;

    fn new() -> Self {
        Band { peak: Self::PEAK_FLOOR, value: 0.0 }
    }

    fn update(&mut self, rms: f32, dt: f32) -> f32 {
        // Peak decays by about half over 4 seconds.
        self.peak = (self.peak * (1.0 - dt * 0.17)).max(rms).max(Self::PEAK_FLOOR);
        let target = if rms < Self::GATE { 0.0 } else { (rms / self.peak).clamp(0.0, 1.0) };
        // Fast attack, slower release.
        let rate = if target > self.value { 25.0 } else { 4.0 };
        self.value += (target - self.value) * (dt * rate).min(1.0);
        self.value
    }
}

#[derive(Debug, Clone)]
pub struct SoundVisualizer {
    full: Band,
    bass: Band,
    treble: Band,
    spectrum: [Band; SPECTRUM_BANDS],
    hue: f32,
}

impl Default for SoundVisualizer {
    fn default() -> Self {
        SoundVisualizer {
            full: Band::new(),
            bass: Band::new(),
            treble: Band::new(),
            spectrum: [Band::new(); SPECTRUM_BANDS],
            hue: 0.6,
        }
    }
}

impl SoundVisualizer {
    /// Dim glow shown in silence so the room isn't suddenly dark.
    const IDLE: f32 = 0.06;

    pub fn update(&mut self, bands: Bands, dt: f32) {
        self.full.update(bands.full, dt);
        self.bass.update(bands.bass, dt);
        let treble = self.treble.update(bands.treble, dt);
        for (band, rms) in self.spectrum.iter_mut().zip(bands.spectrum) {
            band.update(rms, dt);
        }
        self.hue = (self.hue + dt * (0.008 + 0.04 * treble)).rem_euclid(1.0);
    }

    /// Overall loudness, 0..1 after automatic gain.
    pub fn level(&self) -> f32 {
        self.full.value
    }

    /// Bass hit strength, 0..1 (squared so only real hits stand out).
    pub fn punch(&self) -> f32 {
        self.bass.value * self.bass.value
    }

    /// Spectrum along the screen edge: bass glows red at the bottom, treble violet at the top.
    pub fn render_spectrum(&self, regions: &[Region]) -> Vec<Rgb> {
        let last = (SPECTRUM_BANDS - 1) as f32;
        regions
            .iter()
            .map(|r| {
                let height = 1.0 - (r.y0 + r.y1) / 2.0; // 0 at the bottom, 1 at the top
                let pos = height.clamp(0.0, 1.0) * last;
                let i = (pos as usize).min(SPECTRUM_BANDS - 2);
                let f = pos - i as f32;
                let value = self.spectrum[i].value * (1.0 - f) + self.spectrum[i + 1].value * f;
                Rgb::from_hue(height * 0.75).scale(Self::IDLE + (1.0 - Self::IDLE) * value)
            })
            .collect()
    }

    pub fn render(&self, n: usize) -> Vec<Rgb> {
        let level = self.full.value;
        let punch = self.bass.value * self.bass.value;
        let value = (Self::IDLE + 0.65 * level + 0.35 * punch).min(1.0);
        (0..n)
            .map(|i| {
                // A gentle gradient around the screen so it isn't one flat color.
                let c = Rgb::from_hue(self.hue + 0.12 * i as f32 / n.max(1) as f32);
                Rgb::new(c.r * value, c.g * value, c.b * value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(v: &SoundVisualizer) -> f32 {
        let c = v.render(1)[0];
        c.r.max(c.g).max(c.b)
    }

    #[test]
    fn silence_stays_at_idle_glow() {
        let mut v = SoundVisualizer::default();
        for _ in 0..100 {
            v.update(Bands::default(), 0.02);
        }
        assert!((brightness(&v) - SoundVisualizer::IDLE).abs() < 1e-3);
    }

    #[test]
    fn loud_bass_brightens_quickly() {
        let mut v = SoundVisualizer::default();
        for _ in 0..10 {
            v.update(Bands { full: 0.3, bass: 0.3, treble: 0.05, ..Default::default() }, 0.02);
        }
        assert!(brightness(&v) > 0.8, "{}", brightness(&v));
    }

    #[test]
    fn quiet_audio_is_normalized_up() {
        let mut v = SoundVisualizer::default();
        for _ in 0..200 {
            v.update(Bands { full: 0.02, ..Default::default() }, 0.02);
        }
        // A quiet but steady signal still drives the lights well above idle.
        assert!(brightness(&v) > 0.5, "{}", brightness(&v));
    }

    #[test]
    fn renders_requested_count() {
        assert_eq!(SoundVisualizer::default().render(139).len(), 139);
    }
}
