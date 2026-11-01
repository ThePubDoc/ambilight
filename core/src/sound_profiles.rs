//! Sound mode profiles: different ways to draw music on the strip, each modelled on a well-known
//! audio-reactive effect from [LedFx](https://docs.ledfx.app) (Energy, Scroll, Power, Bands, Lava
//! lamp) or [WLED's sound-reactive fork](https://mm.kno.wled.ge/WLEDSR/Reactive-Animations/)
//! (Gravimeter, GEQ, Ripple Peak, Waterfall, Plasmoid, Midnoise).
//!
//! The calm profiles (Glow, Breathe, Lava, Waves) follow Philips Hue Sync's "Subtle" intensity:
//! no beat flashes, slow easing in and out, and gradual color fades.
//!
//! LEDs are placed by where they sit around the screen rather than by their index, so a profile
//! looks the same whichever corner the strip starts in: position 0 is the bottom center and 1 the
//! top center, mirrored on the left and right.

use std::collections::VecDeque;

use crate::config::{Rgb, SoundProfile};
use crate::layout::{Region, Side};
use crate::sound::SoundVisualizer;

const IDLE: f32 = SoundVisualizer::IDLE;
/// Colors for bass, mids and treble in Energy and Scroll, as in LedFx.
const BASS: Rgb = Rgb::new(1.0, 0.0, 0.0);
const MIDS: Rgb = Rgb::new(0.0, 1.0, 0.0);
const TREBLE: Rgb = Rgb::new(0.0, 0.25, 1.0);

#[derive(Debug, Clone, Copy)]
struct Ripple {
    center: f32,
    age: f32,
    color: Rgb,
    strength: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SoundProfiles {
    /// Where each LED sits, 0 (bottom center) .. 1 (top center).
    positions: Vec<f32>,
    /// Scroll: the newest color first.
    scroll: VecDeque<Rgb>,
    /// Meter: the peak dot and how fast it is falling.
    peak: f32,
    peak_speed: f32,
    ripples: Vec<Ripple>,
    next_hue: f32,
    /// Power: brightness of the spark on each LED.
    sparks: Vec<f32>,
    last_treble: f32,
    seed: u32,
    /// Calm profiles: slowly eased loudness, bass and hue.
    soft_level: f32,
    soft_bass: f32,
    soft_hue: Option<f32>,
    /// Calm profiles: slow movement, a little faster when the music is loud.
    drift: f32,
    /// Breathe: position in the current breath, 0..1.
    breath: f32,
}

impl SoundProfiles {
    /// Scroll: frames for a color to travel from the bottom to the top (~1.2 s).
    const SCROLL_FRAMES: usize = 60;
    /// Energy: softness of the bar ends, in strip lengths.
    const SOFT_EDGE: f32 = 0.06;
    /// Meter: how fast the peak dot falls, in strip lengths per second².
    const GRAVITY: f32 = 0.8;
    const DOT_WIDTH: f32 = 0.02;
    /// Ripple: speed and lifetime of a ripple, and the width of its ring.
    const RIPPLE_SPEED: f32 = 0.45;
    const RIPPLE_LIFE: f32 = 1.2;
    const RIPPLE_WIDTH: f32 = 0.05;
    const MAX_RIPPLES: usize = 8;
    /// Power: sparks per unit of treble rise, and how fast they fade.
    const SPARKS_PER_RISE: f32 = 8.0;
    const SPARK_DECAY_S: f32 = 0.12;
    /// Calm profiles: seconds to ease towards louder and quieter, and for the color to follow.
    const SOFT_RISE_S: f32 = 0.8;
    const SOFT_FALL_S: f32 = 2.5;
    const SOFT_HUE_S: f32 = 6.0;
    /// Calm profiles: brightest they get, so nothing ever glares.
    const SOFT_MAX: f32 = 0.6;
    /// Breathe: one breath per two bars (eight beats), kept between these lengths; this one
    /// without a tempo.
    const BREATH_S: (f32, f32) = (3.0, 8.0);
    const BREATH_FREE_S: f32 = 5.0;

    /// Call when the LED layout changes.
    pub fn configure(&mut self, regions: &[Region]) {
        self.positions = regions.iter().map(position).collect();
        self.sparks = vec![0.0; regions.len()];
    }

    /// One frame of `profile`, after `sound` has been updated for this frame.
    pub fn render(&mut self, profile: SoundProfile, sound: &SoundVisualizer, dt: f32) -> Vec<Rgb> {
        if matches!(profile, SoundProfile::Glow | SoundProfile::Breathe | SoundProfile::Lava | SoundProfile::Waves) {
            self.soften(sound, dt);
        }
        match profile {
            SoundProfile::Glow => self.glow(),
            SoundProfile::Breathe => self.breathe(sound, dt),
            SoundProfile::Lava => self.lava(),
            SoundProfile::Waves => self.waves(),
            SoundProfile::Pulse => sound.render(self.positions.len()),
            SoundProfile::Energy => self.energy(sound),
            SoundProfile::Scroll => self.scroll(sound),
            SoundProfile::Meter => self.meter(sound, dt),
            SoundProfile::Spectrum => self.spectrum(sound),
            SoundProfile::Ripple => self.ripple(sound, dt),
            SoundProfile::Power => self.power(sound, dt),
        }
    }

    /// Eases the calm profiles' inputs: quick enough to follow a song's sections, far too slow to
    /// flicker with individual notes or drums.
    fn soften(&mut self, sound: &SoundVisualizer, dt: f32) {
        let ease = |value: &mut f32, target: f32, rise_s: f32, fall_s: f32| {
            let tau = if target > *value { rise_s } else { fall_s };
            *value += (target - *value) * (1.0 - (-dt / tau).exp());
        };
        ease(&mut self.soft_level, sound.level(), Self::SOFT_RISE_S, Self::SOFT_FALL_S);
        ease(&mut self.soft_bass, sound.bass_mids_treble().0, Self::SOFT_RISE_S, Self::SOFT_FALL_S);
        let hue = self.soft_hue.get_or_insert(sound.hue());
        ease(hue, sound.hue(), Self::SOFT_HUE_S, Self::SOFT_HUE_S);
        self.drift += dt * (0.04 + 0.06 * self.soft_level);
    }

    /// Brightness for the calm profiles: a dim glow in silence up to `SOFT_MAX` when loud.
    fn soft_value(&self) -> f32 {
        IDLE + (Self::SOFT_MAX - IDLE) * self.soft_level
    }

    fn soft_hue(&self) -> f32 {
        self.soft_hue.unwrap_or(0.05)
    }

    /// Hue Sync "Subtle": one color that fades gently with the loudness and the music's
    /// character, with a slight gradient around the screen.
    fn glow(&self) -> Vec<Rgb> {
        let (hue, value) = (self.soft_hue(), self.soft_value());
        self.positions.iter().map(|&p| Rgb::from_hue(hue + 0.05 * p).scale(value)).collect()
    }

    /// The strip breathes in and out at the song's tempo, once every eight beats (or every few
    /// seconds without a clear beat), deeper when the music is louder.
    fn breathe(&mut self, sound: &SoundVisualizer, dt: f32) -> Vec<Rgb> {
        let length = sound
            .beat_period()
            .map_or(Self::BREATH_FREE_S, |beat| (8.0 * beat).clamp(Self::BREATH_S.0, Self::BREATH_S.1));
        self.breath = (self.breath + dt / length).fract();
        let breath = 0.5 - 0.5 * (std::f32::consts::TAU * self.breath).cos();
        let value = IDLE + (Self::SOFT_MAX - IDLE) * self.soft_level * (0.35 + 0.65 * breath);
        let hue = self.soft_hue() + 0.03 * breath;
        self.positions.iter().map(|&p| Rgb::from_hue(hue + 0.05 * p).scale(value)).collect()
    }

    /// LedFx Lava lamp / WLED Midnoise: soft blobs of neighbouring colors that drift slowly around
    /// the screen, brighter and a little faster when the music is loud.
    fn lava(&self) -> Vec<Rgb> {
        use std::f32::consts::TAU;
        let (t, hue, value) = (self.drift, self.soft_hue(), self.soft_value());
        self.positions
            .iter()
            .map(|&p| {
                // Smooth noise from three slow waves at unrelated scales and speeds.
                let n = 0.5
                    + ((TAU * (1.3 * p + 0.7 * t)).sin()
                        + (TAU * (2.1 * p - 0.45 * t + 0.3)).sin()
                        + (TAU * (0.6 * p + 0.25 * t + 0.7)).sin())
                        / 6.0;
                Rgb::from_hue(hue - 0.06 + 0.14 * n).scale(value * (0.35 + 0.65 * n))
            })
            .collect()
    }

    /// WLED Plasmoid: slow waves roll up from the bottom, swelling with the loudness; more bass
    /// warms the colors.
    fn waves(&self) -> Vec<Rgb> {
        use std::f32::consts::TAU;
        let hue = self.soft_hue() - 0.05 * self.soft_bass;
        let value = self.soft_value();
        self.positions
            .iter()
            .map(|&p| {
                let wave = 0.5 + 0.5 * (TAU * (1.5 * p - 2.0 * self.drift)).sin();
                Rgb::from_hue(hue + 0.12 * p).scale(value * (0.3 + 0.7 * wave * wave))
            })
            .collect()
    }

    /// LedFx Energy: bass, mids and treble each light a bar from the bottom center as long as the
    /// band is loud, mixed additively (red + green = yellow, and so on).
    fn energy(&self, sound: &SoundVisualizer) -> Vec<Rgb> {
        let (bass, mids, treble) = sound.bass_mids_treble();
        let base = Rgb::from_hue(sound.hue()).scale(IDLE);
        let fill = |level: f32, p: f32| ((level * level * 1.05 - p) / Self::SOFT_EDGE).clamp(0.0, 1.0);
        self.positions
            .iter()
            .map(|&p| {
                [(BASS, bass), (MIDS, mids), (TREBLE, treble)]
                    .into_iter()
                    .fold(base, |c, (color, level)| add(c, color.scale(fill(level, p))))
            })
            .collect()
    }

    /// LedFx Scroll / WLED Waterfall: the sound's color enters at the bottom and flows up both
    /// sides, fading as it goes.
    fn scroll(&mut self, sound: &SoundVisualizer) -> Vec<Rgb> {
        let (bass, mids, treble) = sound.bass_mids_treble();
        // Squared so the background stays dark and only louder moments show.
        let color = add(add(BASS.scale(bass * bass), MIDS.scale(mids * mids)), TREBLE.scale(treble * treble));
        if self.scroll.len() == Self::SCROLL_FRAMES {
            self.scroll.pop_back();
        }
        self.scroll.push_front(color);
        let last = (Self::SCROLL_FRAMES - 1) as f32;
        self.positions
            .iter()
            .map(|&p| {
                let at = p * last;
                let i = at as usize;
                let f = at - i as f32;
                let get = |i: usize| self.scroll.get(i).copied().unwrap_or(Rgb::BLACK);
                get(i).lerp(get(i + 1), f).scale(1.0 - 0.5 * p)
            })
            .collect()
    }

    /// WLED Gravimeter: the volume fills from the bottom up both sides, green to red, and a peak
    /// dot is knocked up by loud moments and falls back slowly.
    fn meter(&mut self, sound: &SoundVisualizer, dt: f32) -> Vec<Rgb> {
        let level = sound.level();
        let bar = level * level;
        if bar >= self.peak {
            self.peak = bar;
            self.peak_speed = 0.0;
        } else {
            self.peak_speed += Self::GRAVITY * dt;
            self.peak = (self.peak - self.peak_speed * dt).max(0.0);
        }
        let peak = self.peak;
        self.positions
            .iter()
            .map(|&p| {
                let lit = ((bar - p) / 0.02).clamp(0.0, 1.0);
                let color = Rgb::from_hue(0.33 * (1.0 - p)).scale(IDLE + (1.0 - IDLE) * lit);
                let dot = (1.0 - (p - peak).abs() / Self::DOT_WIDTH).max(0.0);
                if peak > 0.02 {
                    color.lerp(Rgb::WHITE, dot)
                } else {
                    color
                }
            })
            .collect()
    }

    /// WLED GEQ / LedFx Bands: eight frequency bands along the strip, bass red at the bottom to
    /// treble violet at the top, each as bright as its band is loud.
    fn spectrum(&self, sound: &SoundVisualizer) -> Vec<Rgb> {
        let bands = sound.spectrum();
        let last = (bands.len() - 1) as f32;
        self.positions
            .iter()
            .map(|&p| {
                let at = p * last;
                let i = (at as usize).min(bands.len() - 2);
                let f = at - i as f32;
                let value = bands[i] * (1.0 - f) + bands[i + 1] * f;
                Rgb::from_hue(p * 0.75).scale(IDLE + (1.0 - IDLE) * value * value)
            })
            .collect()
    }

    /// WLED Ripple Peak: each beat drops a ripple of a new color somewhere along the strip, which
    /// spreads out and fades over a dim glow that follows the loudness.
    fn ripple(&mut self, sound: &SoundVisualizer, dt: f32) -> Vec<Rgb> {
        for r in &mut self.ripples {
            r.age += dt;
        }
        self.ripples.retain(|r| r.age < Self::RIPPLE_LIFE);
        if let Some(strength) = sound.beat() {
            if self.ripples.len() == Self::MAX_RIPPLES {
                self.ripples.remove(0);
            }
            // Golden-ratio steps give each ripple a clearly different color.
            self.next_hue = (self.next_hue + 0.618).fract();
            let center = self.random();
            self.ripples.push(Ripple {
                center,
                age: 0.0,
                color: Rgb::from_hue(self.next_hue),
                strength: (0.5 * strength).clamp(0.6, 1.0),
            });
        }
        let base = Rgb::from_hue(sound.hue()).scale(IDLE + 0.15 * sound.calm());
        self.positions
            .iter()
            .map(|&p| {
                self.ripples.iter().fold(base, |c, r| {
                    let radius = r.age * Self::RIPPLE_SPEED;
                    let ring =
                        (-((p - r.center).abs() - radius).powi(2) / (Self::RIPPLE_WIDTH * Self::RIPPLE_WIDTH)).exp();
                    let fade = (1.0 - r.age / Self::RIPPLE_LIFE).powi(2);
                    add(c, r.color.scale(ring * fade * r.strength))
                })
            })
            .collect()
    }

    /// LedFx Power: a gradient that swells with the bass, with white sparks on treble hits
    /// (hi-hats, cymbals).
    fn power(&mut self, sound: &SoundVisualizer, dt: f32) -> Vec<Rgb> {
        let (bass, _, treble) = sound.bass_mids_treble();
        let decay = (-dt / Self::SPARK_DECAY_S).exp();
        for s in &mut self.sparks {
            *s *= decay;
        }
        let rise = (treble - self.last_treble).max(0.0);
        self.last_treble = treble;
        let mut count = rise * Self::SPARKS_PER_RISE;
        while count > 0.0 && !self.sparks.is_empty() {
            if count >= 1.0 || self.random() < count {
                let n = self.sparks.len();
                let i = (self.random() * n as f32) as usize;
                self.sparks[i.min(n - 1)] = 1.0;
            }
            count -= 1.0;
        }
        let value = 0.08 + 0.8 * bass * bass;
        self.positions
            .iter()
            .zip(&self.sparks)
            .map(|(&p, &spark)| Rgb::from_hue(sound.hue() + 0.2 * p).scale(value).lerp(Rgb::WHITE, spark))
            .collect()
    }

    /// 0..1, from a small xorshift generator (no need for a dependency).
    fn random(&mut self) -> f32 {
        let mut x = if self.seed == 0 { 0x9E37_79B9 } else { self.seed };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn add(a: Rgb, b: Rgb) -> Rgb {
    Rgb::new((a.r + b.r).min(1.0), (a.g + b.g).min(1.0), (a.b + b.b).min(1.0))
}

/// Where an LED sits along the screen's edge: 0 at the bottom center, through the corners and up
/// the sides, to 1 at the top center. The left and right halves mirror each other.
fn position(r: &Region) -> f32 {
    let from_center = ((r.x0 + r.x1) / 2.0 - 0.5).abs().min(0.5);
    let height = 1.0 - (r.y0 + r.y1) / 2.0; // 0 at the bottom
    let along = match r.side {
        Side::Bottom => from_center,
        Side::Left | Side::Right => 0.5 + height,
        Side::Top => 1.5 + (0.5 - from_center),
    };
    (along / 2.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{Bands, SPECTRUM_BANDS};
    use crate::config::{Config, Sensitivity};
    use crate::layout;

    const DT: f32 = 0.02;
    const CALM: [SoundProfile; 4] =
        [SoundProfile::Glow, SoundProfile::Breathe, SoundProfile::Lava, SoundProfile::Waves];
    const ALL: [SoundProfile; 11] = [
        SoundProfile::Glow,
        SoundProfile::Breathe,
        SoundProfile::Lava,
        SoundProfile::Waves,
        SoundProfile::Pulse,
        SoundProfile::Energy,
        SoundProfile::Scroll,
        SoundProfile::Meter,
        SoundProfile::Spectrum,
        SoundProfile::Ripple,
        SoundProfile::Power,
    ];

    fn regions() -> Vec<Region> {
        layout::build(&Config::default())
    }

    fn profiles() -> SoundProfiles {
        let mut p = SoundProfiles::default();
        p.configure(&regions());
        p
    }

    fn max(c: Rgb) -> f32 {
        c.r.max(c.g).max(c.b)
    }

    /// A 120 BPM beat: a kick every 500 ms with a hi-hat on the off-beat, over a quiet bed.
    fn beat_at(t: f32) -> Bands {
        let phase = t % 0.5;
        let kick = if phase < 0.06 { 0.3 } else { 0.03 };
        let hat = if (0.25..0.28).contains(&phase) { 0.08 } else { 0.01 };
        let mut spectrum = [0.02; SPECTRUM_BANDS];
        spectrum[0] = kick;
        spectrum[1] = kick * 0.5;
        spectrum[6] = hat;
        spectrum[7] = hat;
        Bands { full: kick + hat, bass: kick, treble: hat, spectrum }
    }

    #[test]
    fn positions_run_from_bottom_center_to_top_center_on_both_sides() {
        let r = regions();
        let pos: Vec<f32> = r.iter().map(position).collect();
        let at = |side: Side, pick: fn(&Region) -> f32| {
            r.iter().zip(&pos).filter(|(r, _)| r.side == side).map(|(r, p)| (pick(r), *p)).collect::<Vec<_>>()
        };
        let bottom = at(Side::Bottom, |r| ((r.x0 + r.x1) / 2.0 - 0.5).abs());
        let near = bottom.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap().1;
        assert!(near < 0.02, "bottom center: {near}");
        let top = at(Side::Top, |r| ((r.x0 + r.x1) / 2.0 - 0.5).abs());
        let near = top.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap().1;
        assert!(near > 0.98, "top center: {near}");
        // The sides mirror each other.
        let mut left: Vec<f32> = at(Side::Left, |_| 0.0).into_iter().map(|x| x.1).collect();
        let mut right: Vec<f32> = at(Side::Right, |_| 0.0).into_iter().map(|x| x.1).collect();
        left.sort_by(f32::total_cmp);
        right.sort_by(f32::total_cmp);
        assert_eq!(left, right);
        assert!(left.iter().all(|p| (0.25..=0.75).contains(p)));
    }

    #[test]
    fn every_profile_renders_every_led_in_range_and_stays_dim_in_silence() {
        for profile in ALL {
            let mut p = profiles();
            let mut sound = SoundVisualizer::default();
            for _ in 0..100 {
                sound.update(Bands::default(), DT);
                let frame = p.render(profile, &sound, DT);
                assert_eq!(frame.len(), regions().len(), "{profile:?}");
                for c in &frame {
                    assert!([c.r, c.g, c.b].iter().all(|v| (0.0..=1.0).contains(v)), "{profile:?}: {c:?}");
                    assert!(max(*c) <= 0.1, "{profile:?} lit in silence: {c:?}");
                }
            }
        }
    }

    #[test]
    fn every_profile_reacts_to_music() {
        for profile in ALL {
            let mut p = profiles();
            let mut sound = SoundVisualizer::default();
            sound.set_sensitivity(Sensitivity::High);
            let mut brightest: f32 = 0.0;
            for step in 0..300 {
                sound.update(beat_at(step as f32 * DT), DT);
                let frame = p.render(profile, &sound, DT);
                brightest = brightest.max(frame.into_iter().map(max).fold(0.0, f32::max));
            }
            let expected = if CALM.contains(&profile) { 0.3 } else { 0.4 };
            assert!(brightest > expected, "{profile:?} barely reacts: {brightest}");
        }
    }

    #[test]
    fn calm_profiles_never_flash_or_glare() {
        let song = crate::sound::music::bands(124.0, 10.0, 0.25, 0.4, 1.0);
        for profile in CALM {
            let mut p = profiles();
            let mut sound = SoundVisualizer::default();
            let mut previous: Option<Vec<f32>> = None;
            for bands in &song {
                sound.update(*bands, DT);
                let values: Vec<f32> = p.render(profile, &sound, DT).into_iter().map(max).collect();
                assert!(values.iter().all(|v| *v <= SoundProfiles::SOFT_MAX + 1e-3), "{profile:?} glares");
                if let Some(prev) = &previous {
                    let jump = values.iter().zip(prev).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
                    assert!(jump < 0.02, "{profile:?} jumps by {jump} in one frame");
                }
                previous = Some(values);
            }
        }
    }

    #[test]
    fn energy_lights_bass_from_the_bottom_in_red() {
        let mut p = profiles();
        let mut sound = SoundVisualizer::default();
        let bass = |level: f32| {
            let mut spectrum = [0.0; SPECTRUM_BANDS];
            spectrum[0] = level;
            spectrum[1] = level;
            Bands { full: level, bass: level, treble: 0.0, spectrum }
        };
        // A loud passage, then bass at about half of it.
        for step in 0..75 {
            sound.update(bass(if step < 50 { 0.4 } else { 0.2 }), DT);
        }
        let frame = p.render(SoundProfile::Energy, &sound, DT);
        for (pos, c) in p.positions.iter().zip(&frame) {
            if *pos < 0.15 {
                assert!(c.r > 0.9 && c.g < 0.2, "bottom should be red: {pos} {c:?}");
            } else if *pos > 0.6 {
                assert!(max(*c) < 0.1, "top should be dark: {pos} {c:?}");
            }
        }
    }

    #[test]
    fn meter_peak_dot_falls_back_slowly() {
        let mut p = profiles();
        let mut sound = SoundVisualizer::default();
        for _ in 0..50 {
            sound.update(beat_at(0.0), DT); // a held loud kick
            p.render(SoundProfile::Meter, &sound, DT);
        }
        let top = p.peak;
        assert!(top > 0.5, "peak: {top}");
        for _ in 0..10 {
            sound.update(Bands::default(), DT);
            p.render(SoundProfile::Meter, &sound, DT);
        }
        assert!(p.peak < top && p.peak > top - 0.1, "falls slowly: {top} → {}", p.peak);
    }

    #[test]
    fn ripples_start_on_beats_and_fade() {
        let mut p = profiles();
        let mut sound = SoundVisualizer::default();
        sound.set_sensitivity(Sensitivity::High);
        let mut beats = 0;
        for step in 0..200 {
            sound.update(beat_at(step as f32 * DT), DT);
            beats += sound.beat().is_some() as usize;
            p.render(SoundProfile::Ripple, &sound, DT);
        }
        assert!(beats >= 4, "beats: {beats}");
        assert!(!p.ripples.is_empty() && p.ripples.len() <= SoundProfiles::MAX_RIPPLES);
        for _ in 0..100 {
            sound.update(Bands::default(), DT);
            p.render(SoundProfile::Ripple, &sound, DT);
        }
        assert!(p.ripples.is_empty(), "ripples should fade out");
    }

    #[test]
    fn power_sparks_on_treble_hits() {
        let mut p = profiles();
        let mut sound = SoundVisualizer::default();
        let mut sparked = 0;
        for step in 0..150 {
            sound.update(beat_at(step as f32 * DT), DT);
            let frame = p.render(SoundProfile::Power, &sound, DT);
            sparked += frame.iter().filter(|c| c.r > 0.9 && c.g > 0.9 && c.b > 0.9).count().min(1);
        }
        assert!(sparked > 5, "frames with sparks: {sparked}");
    }

    /// How each profile looks on a synthetic song: how much of the strip is lit and how much it
    /// changes per frame. `cargo test -p ambilight-core profile_report -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn profile_report() {
        let song = crate::sound::music::bands(124.0, 12.0, 0.25, 0.4, 1.0);
        for profile in ALL {
            let mut p = profiles();
            let mut sound = SoundVisualizer::default();
            let (mut lit, mut change, mut frames) = (0.0, 0.0, 0.0);
            let mut previous: Option<Vec<f32>> = None;
            for (i, bands) in song.iter().enumerate() {
                sound.update(*bands, DT);
                let values: Vec<f32> = p.render(profile, &sound, DT).into_iter().map(max).collect();
                if i >= 150 {
                    lit += values.iter().sum::<f32>() / values.len() as f32;
                    if let Some(prev) = &previous {
                        change +=
                            values.iter().zip(prev).map(|(a, b)| (a - b).abs()).sum::<f32>() / values.len() as f32;
                    }
                    frames += 1.0;
                }
                previous = Some(values);
            }
            println!("{profile:?}: average brightness {:.2}, change per frame {:.3}", lit / frames, change / frames);
        }
    }
}
