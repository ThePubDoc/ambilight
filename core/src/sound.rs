//! Turns audio into LED colors for Sound and Mix modes.
//!
//! Three layers, chosen so music looks musical rather than nervous:
//! - a calm baseline that follows the loudness slowly,
//! - short pulses on beats (bass, or overall level, jumping above its recent average),
//! - a color that follows the sound's character: warm for bass-heavy, cool for bright music.

use crate::audio::{Bands, SPECTRUM_BANDS};
use crate::config::{Rgb, Sensitivity};
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

/// Finds hits (kicks, snares) as sudden *rises* of a band's energy: the rise in log energy from
/// one frame to the next, compared with how much that band usually rises. This catches a kick
/// on top of a running bassline, where the level itself barely changes.
#[derive(Debug, Clone, Default)]
struct Onset {
    previous: Option<f32>,
    history: std::collections::VecDeque<f32>,
    /// The latest rise, thresholded or not (feeds the tempo estimate).
    rise: f32,
}

impl Onset {
    /// About 1.2 s of frames.
    const HISTORY: usize = 60;

    /// Returns how far above the threshold this frame's rise is (> 1 means a hit). `spread` is how
    /// many standard deviations above the band's usual rise a hit has to be.
    fn update(&mut self, energy: f32, floor: f32, spread: f32) -> Option<f32> {
        let log = (energy + 1e-9).ln();
        let rise = self.previous.map_or(0.0, |p| (log - p).max(0.0));
        self.previous = Some(log);
        self.rise = rise;
        let n = self.history.len();
        let mean = self.history.iter().sum::<f32>() / n.max(1) as f32;
        let var = self.history.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n.max(1) as f32;
        let threshold = (mean + spread * var.sqrt()).max(floor);
        if n == Self::HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(rise);
        (n >= 10 && rise > threshold).then(|| rise / threshold)
    }
}

/// Follows the music's tempo and predicts where the next beat falls, so only hits on the beat
/// flash and sounds between beats (vocals, guitars, accents) are ignored.
#[derive(Debug, Clone, Default)]
struct BeatTracker {
    time: f32,
    /// Onset strength per frame over the last few seconds.
    novelty: std::collections::VecDeque<f32>,
    since_estimate: f32,
    /// Seconds per beat, once the tempo is clear.
    period: Option<f32>,
    /// How periodic the recent drums are, 0..1.
    confidence: f32,
    next_beat: Option<f32>,
    /// Whether the beat at `next_beat` has already flashed (predicted ahead of its hit).
    flashed: bool,
    /// Flash every other beat (on the stronger of each pair, usually the kick).
    half_time: bool,
}

/// What the tracker decided this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Beat {
    None,
    /// A detected hit on the beat, with its strength.
    Hit(f32),
    /// The beat is due but no hit was heard (quiet or masked); the tempo is still clear.
    Predicted,
}

impl BeatTracker {
    const FRAME: f32 = 0.02;
    /// Six seconds of history at 50 frames per second.
    const HISTORY: usize = 300;
    /// Tempo search range, in frames per beat: 17 ≈ 175 BPM … 43 ≈ 70 BPM.
    const MIN_LAG: usize = 17;
    const MAX_LAG: usize = 43;
    /// How close to the predicted time a hit must land to count as the beat.
    const WINDOW: f32 = 0.09;
    /// Below this the music isn't rhythmic enough to lock on to.
    const MIN_CONFIDENCE: f32 = 0.15;
    /// Above this, beats are flashed on time even without a detected hit.
    const PREDICT_CONFIDENCE: f32 = 0.3;
    /// Half-time only above 100 BPM; slower songs pulse on every beat anyway.
    const HALF_TIME_ABOVE_S: f32 = 0.6;

    fn locked(&self) -> bool {
        self.period.is_some() && self.confidence >= Self::MIN_CONFIDENCE
    }

    /// Time between flashes: the beat, or two beats in half-time.
    fn grid(&self) -> Option<f32> {
        self.period.map(|p| if self.half_time && p < Self::HALF_TIME_ABOVE_S { 2.0 * p } else { p })
    }

    fn set_half_time(&mut self, half_time: bool) {
        if self.half_time != half_time {
            self.half_time = half_time;
            self.next_beat = None; // re-phase on the new grid
        }
    }

    /// `novelty`: this frame's onset strength. `hit`: a detected kick or snare, if any.
    fn update(&mut self, novelty: f32, hit: Option<f32>, dt: f32, predict: bool) -> Beat {
        self.time += dt;
        if self.novelty.len() == Self::HISTORY {
            self.novelty.pop_front();
        }
        self.novelty.push_back(novelty);
        self.since_estimate += dt;
        if self.since_estimate >= 0.5 && self.novelty.len() >= Self::HISTORY / 2 {
            self.since_estimate = 0.0;
            self.estimate_tempo();
        }

        let Some(period) = self.grid().filter(|_| self.locked()) else {
            self.next_beat = None;
            return Beat::None;
        };
        let t = self.time;
        let Some(next) = self.next_beat else {
            // Locked on the tempo; the first clear hit sets the phase.
            if let Some(strength) = hit {
                self.next_beat = Some(t + period);
                self.flashed = false;
                return Beat::Hit(strength);
            }
            return Beat::None;
        };

        if let Some(strength) = hit.filter(|_| (t - next).abs() <= Self::WINDOW) {
            // The beat, heard: realign the grid on it.
            let already = self.flashed;
            self.next_beat = Some(t + period);
            self.flashed = false;
            return if already { Beat::None } else { Beat::Hit(strength) };
        }
        if t > next + Self::WINDOW {
            // Missed this beat; keep the grid going.
            self.next_beat = Some(next + period);
            self.flashed = false;
            return Beat::None;
        }
        if t >= next && !self.flashed && predict && self.confidence >= Self::PREDICT_CONFIDENCE {
            self.flashed = true;
            return Beat::Predicted;
        }
        Beat::None
    }

    /// Autocorrelation of the onset curve over the tempo range, favouring ~120 BPM.
    fn estimate_tempo(&mut self) {
        let n = self.novelty.len();
        let mean = self.novelty.iter().sum::<f32>() / n as f32;
        let x: Vec<f32> = self.novelty.iter().map(|v| v - mean).collect();
        let energy: f32 = x.iter().map(|v| v * v).sum();
        if energy <= 1e-6 {
            self.confidence = 0.0;
            return;
        }
        let corr = |lag: usize| x[lag..].iter().zip(&x).map(|(a, b)| a * b).sum::<f32>() / energy;
        let prior = |lag: f32| (-0.5 * ((lag / 25.0).log2() / 0.6).powi(2)).exp();
        let (best, score) = (Self::MIN_LAG..=Self::MAX_LAG)
            .map(|lag| (lag, corr(lag)))
            .max_by(|a, b| (a.1 * prior(a.0 as f32)).total_cmp(&(b.1 * prior(b.0 as f32))))
            .unwrap_or((25, 0.0));
        // Sub-frame precision from the neighbours (parabolic peak).
        let (l, r) = (corr(best - 1), corr((best + 1).min(n - 1)));
        let denom = l - 2.0 * score + r;
        let offset = if denom.abs() > 1e-6 { (0.5 * (l - r) / denom).clamp(-0.5, 0.5) } else { 0.0 };
        let period = (best as f32 + offset) * Self::FRAME;
        self.confidence = score.max(0.0);
        let period = match self.period {
            // Small tempo drift: follow smoothly. A new song: switch.
            Some(old) if (period / old - 1.0).abs() < 0.08 => old * 0.7 + period * 0.3,
            _ => period,
        };
        self.period = Some(period);
        if let Some(grid) = self.grid() {
            self.rephase(grid);
        }
    }

    /// Places the beat grid where the strongest hits of the last few seconds line up, so the
    /// lights follow the kicks and snares rather than off-beat sounds.
    fn rephase(&mut self, period: f32) {
        let frames = period / Self::FRAME;
        let last = self.novelty.len() - 1;
        let score = |offset: usize| -> f32 {
            (0..)
                .map(|beat| offset as f32 + beat as f32 * frames)
                .map(|back| back.round() as usize)
                .take_while(|&back| back <= last)
                .map(|back| {
                    // The strongest onset within a frame of where the beat should be.
                    let i = last - back;
                    let lo = i.saturating_sub(1);
                    let hi = (i + 1).min(last);
                    self.novelty.range(lo..=hi).cloned().fold(0.0, f32::max)
                })
                .sum()
        };
        let Some(best) = (0..frames.ceil() as usize).max_by(|&a, &b| score(a).total_cmp(&score(b))) else {
            return;
        };
        // The most recent beat on that grid was `best` frames ago.
        let next = self.time - best as f32 * Self::FRAME + period;
        let next = if next < self.time - Self::WINDOW { next + period } else { next };
        match self.next_beat {
            // Already on the grid (within the window): keep it, so beats don't jitter.
            Some(current)
                if ((current - next) / period).fract().abs().min(1.0 - ((current - next) / period).fract().abs())
                    * period
                    <= Self::WINDOW => {}
            _ => {
                self.next_beat = Some(next);
                self.flashed = false;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SoundVisualizer {
    full: Band,
    spectrum: [Band; SPECTRUM_BANDS],
    /// Kicks: 60–125 Hz.
    low_onset: Onset,
    /// Snares and claps: 250 Hz–1 kHz.
    mid_onset: Onset,
    /// Seconds since the last beat.
    since_beat: f32,
    /// Beat flash, 1 right on a beat and decaying quickly.
    pulse: f32,
    /// Slowly smoothed loudness, 0..1.
    calm: f32,
    hue: f32,
    sensitivity: Sensitivity,
    tracker: BeatTracker,
}

impl Default for SoundVisualizer {
    fn default() -> Self {
        SoundVisualizer {
            full: Band::new(),
            spectrum: [Band::new(); SPECTRUM_BANDS],
            low_onset: Onset::default(),
            mid_onset: Onset::default(),
            since_beat: 1.0,
            pulse: 0.0,
            calm: 0.0,
            hue: 0.05,
            sensitivity: Sensitivity::default(),
            tracker: BeatTracker::default(),
        }
    }
}

impl SoundVisualizer {
    /// Dim glow shown in silence so the room isn't suddenly dark.
    const IDLE: f32 = 0.06;
    /// Below this RMS nothing counts as a beat.
    const BEAT_GATE: f32 = 0.004;
    /// Smallest rise in log energy that counts as a kick (≈ ×3.7, so a new bass note alone rarely
    /// does) or a snare (≈ ×2.7); and how far above the band's usual rises a hit must be.
    const KICK_FLOOR: f32 = 1.3;
    const SNARE_FLOOR: f32 = 1.0;
    const SPREAD: f32 = 2.0;
    /// "Every hit": shortest gap between flashes, so one hit can't flash twice.
    const MIN_HIT_GAP: f32 = 0.15;
    const PULSE_DECAY_S: f32 = 0.16;
    const CALM_S: f32 = 0.5;
    const HUE_S: f32 = 2.5;
    /// Treble carries far less energy than bass in real music; weigh it up for the color.
    const CENTROID_WEIGHTS: [f32; SPECTRUM_BANDS] = [1.0, 1.2, 1.5, 2.0, 2.6, 3.4, 4.5, 6.0];

    pub fn set_sensitivity(&mut self, sensitivity: Sensitivity) {
        self.sensitivity = sensitivity;
    }

    pub fn update(&mut self, bands: Bands, dt: f32) {
        let level = self.full.update(bands.full, dt);
        for (band, rms) in self.spectrum.iter_mut().zip(bands.spectrum) {
            band.update(rms, dt);
        }

        // Beats: kicks in the low band, snares and claps in the mids.
        self.since_beat += dt;
        let sp = bands.spectrum;
        let low = sp[0] * sp[0] + sp[1] * sp[1];
        let mid = sp[2] * sp[2] + sp[3] * sp[3] + sp[4] * sp[4];
        let kick = self.low_onset.update(low, Self::KICK_FLOOR, Self::SPREAD);
        let snare = self.mid_onset.update(mid, Self::SNARE_FLOOR, Self::SPREAD).map(|r| r * 0.85);
        let loud_enough = bands.full > Self::BEAT_GATE;
        self.pulse *= (-dt / Self::PULSE_DECAY_S).exp();
        let hit = match (kick, snare) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
        .filter(|_| loud_enough);
        let novelty = if loud_enough { self.low_onset.rise + 0.8 * self.mid_onset.rise } else { 0.0 };

        let flash = match self.sensitivity {
            // Every hit, no tempo lock.
            Sensitivity::High => hit.filter(|_| self.since_beat >= Self::MIN_HIT_GAP),
            // On the beat only: Medium also keeps time through quiet beats, Low only flashes heard ones.
            Sensitivity::Medium | Sensitivity::Low => {
                self.tracker.set_half_time(self.sensitivity == Sensitivity::Low);
                match self.tracker.update(novelty, hit, dt, true) {
                    Beat::Hit(strength) => Some(strength.max(2.0)),
                    Beat::Predicted => Some(1.6),
                    // Not locked on a rhythm (yet): no flashes, just the calm glow.
                    Beat::None => None,
                }
            }
        };
        if let Some(strength) = flash {
            self.since_beat = 0.0;
            // Borderline hits flash softly, strong ones fully.
            self.pulse = self.pulse.max((0.35 * strength).clamp(0.35, 1.0));
        }

        self.calm += (level - self.calm) * (1.0 - (-dt / Self::CALM_S).exp());

        // Color from the spectral balance, changing slowly enough to feel deliberate.
        let weighted: Vec<f32> = bands.spectrum.iter().zip(Self::CENTROID_WEIGHTS).map(|(b, w)| b * w).collect();
        let total: f32 = weighted.iter().sum();
        if total > Self::BEAT_GATE {
            let centroid = weighted.iter().enumerate().map(|(i, v)| i as f32 * v).sum::<f32>() / total;
            let target = 0.72 * centroid / (SPECTRUM_BANDS - 1) as f32; // red (bass) .. violet (treble)
            self.hue += (target - self.hue) * (1.0 - (-dt / Self::HUE_S).exp());
        }
    }

    /// Brightness factor for Mix mode: the screen's colors, steady between beats, boosted on them.
    pub fn mix_brightness(&self) -> f32 {
        (0.6 + 0.2 * self.calm + 0.25 * self.pulse).min(1.0)
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

    /// Sound mode: the calm glow plus beat flashes, which also whiten the color a little.
    pub fn render(&self, n: usize) -> Vec<Rgb> {
        let value = (Self::IDLE + 0.4 * self.calm + 0.4 * self.pulse).min(1.0);
        let whiten = 0.06 * self.pulse;
        (0..n)
            .map(|i| {
                // A gentle gradient around the screen so it isn't one flat color.
                let c = Rgb::from_hue(self.hue + 0.06 * i as f32 / n.max(1) as f32);
                c.lerp(Rgb::WHITE, whiten).scale(value)
            })
            .collect()
    }

    #[cfg(test)]
    fn hue(&self) -> f32 {
        self.hue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 0.02;

    fn brightness(v: &SoundVisualizer) -> f32 {
        let c = v.render(1)[0];
        c.r.max(c.g).max(c.b)
    }

    fn bass(level: f32) -> Bands {
        let mut spectrum = [0.0; SPECTRUM_BANDS];
        spectrum[0] = level;
        spectrum[1] = level * 0.5;
        Bands { full: level, bass: level, treble: level * 0.05, spectrum }
    }

    #[test]
    fn silence_stays_at_idle_glow() {
        let mut v = SoundVisualizer::default();
        for _ in 0..100 {
            v.update(Bands::default(), DT);
        }
        assert!((brightness(&v) - SoundVisualizer::IDLE).abs() < 1e-3);
    }

    #[test]
    fn kicks_flash_and_fall_back_between_them() {
        let mut v = SoundVisualizer::default();
        v.set_sensitivity(Sensitivity::Medium);
        let (mut on_beat, mut between) = (Vec::new(), Vec::new());
        // 120 BPM: a 60 ms kick every 500 ms over a quieter bed. The tracker needs ~3 s to lock.
        for step in 0..450 {
            let t = step as f32 * DT;
            let phase = t % 0.5;
            v.update(bass(if phase < 0.06 { 0.3 } else { 0.05 }), DT);
            if t > 4.0 {
                if phase < 0.02 {
                    on_beat.push(brightness(&v));
                } else if (0.38..0.45).contains(&phase) {
                    between.push(brightness(&v));
                }
            }
        }
        let min_on = on_beat.iter().cloned().fold(1.0, f32::min);
        let max_between = between.iter().cloned().fold(0.0, f32::max);
        assert!(min_on > 0.45, "beats too weak: {on_beat:?}");
        assert!(max_between < min_on - 0.2, "no contrast: {between:?} vs {on_beat:?}");
    }

    #[test]
    fn steady_loud_sound_glows_without_flashing() {
        let mut v = SoundVisualizer::default();
        let mut samples = Vec::new();
        for step in 0..250 {
            v.update(bass(0.2), DT);
            if step > 100 {
                samples.push(brightness(&v));
            }
        }
        let spread = samples.iter().cloned().fold(0.0, f32::max) - samples.iter().cloned().fold(1.0, f32::min);
        assert!(spread < 0.05, "flickers on a steady tone: {spread}");
    }

    #[test]
    fn color_follows_the_sound_character() {
        let mut warm = SoundVisualizer::default();
        let mut cool = SoundVisualizer::default();
        let mut treble = [0.0; SPECTRUM_BANDS];
        treble[6] = 0.1;
        treble[7] = 0.1;
        for _ in 0..500 {
            warm.update(bass(0.2), DT);
            cool.update(Bands { full: 0.1, bass: 0.0, treble: 0.1, spectrum: treble }, DT);
        }
        assert!(warm.hue() < 0.15, "bass-heavy should be warm: {}", warm.hue());
        assert!(cool.hue() > 0.5, "treble-heavy should be cool: {}", cool.hue());
    }

    #[test]
    fn mix_brightness_stays_in_range() {
        let mut v = SoundVisualizer::default();
        for step in 0..200 {
            v.update(bass(if step % 25 < 3 { 0.4 } else { 0.05 }), DT);
            let m = v.mix_brightness();
            assert!((0.55..=1.0).contains(&m), "{m}");
        }
    }

    #[test]
    fn renders_requested_count() {
        assert_eq!(SoundVisualizer::default().render(139).len(), 139);
    }
}

#[cfg(test)]
mod music {
    //! Beat detection on a synthetic song: kick drums over a continuous bassline and hi-hats,
    //! run through the real audio analysis at 48 kHz.
    use super::*;
    use crate::audio::AudioSink;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;
    const TICK: f32 = 0.02;

    /// Returns (kick times, snare times, beat detections) for `seconds` of the song at `bpm`.
    /// Kicks fall on beats 1 and 3, snares on 2 and 4 (when `snare` > 0).
    /// `distractors`: vocal/guitar-like bursts per second, at random times off the beat.
    pub fn run_at(
        sensitivity: Sensitivity,
        bpm: f32,
        seconds: f32,
        bassline: f32,
        snare: f32,
        distractors: f32,
    ) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let beat = 60.0 / bpm;
        let mut noise = 12345u32;
        // Distractor onsets: random times, kept at least 100 ms away from any kick or snare.
        let mut rng = 987654321u32;
        let mut random = || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng as f32 / u32::MAX as f32
        };
        let mut bursts: Vec<(f32, f32)> = Vec::new(); // (start, pitch)
        let mut at = 0.0;
        while distractors > 0.0 && at < seconds {
            at += random() * 2.0 / distractors;
            let off = at % beat;
            if off > 0.1 && off < beat - 0.1 {
                bursts.push((at, 300.0 + 600.0 * random()));
            }
        }
        let samples: Vec<f32> = (0..(RATE * seconds) as usize)
            .map(|i| {
                let t = i as f32 / RATE;
                let since_kick = t % (2.0 * beat);
                let since_snare = (t + beat) % (2.0 * beat);
                // Kick: pitch-dropping 90→50 Hz thump with a fast decay.
                let kick = 0.6
                    * (-since_kick / 0.09).exp()
                    * (TAU * (50.0 + 40.0 * (-since_kick / 0.03).exp()) * since_kick).sin();
                // Bassline: a note every eighth, never silent.
                let eighth = (t / (beat / 2.0)) as usize;
                let note = [41.2, 41.2, 55.0, 49.0][eighth % 4];
                let since_note = t % (beat / 2.0);
                let env = 0.6 + 0.4 * (-since_note / 0.15).exp();
                let bass = bassline * env * (TAU * note * t).sin();
                // Hi-hat: short noise burst on every eighth.
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                let white = (noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let hat = 0.08 * white * (-since_note / 0.02).exp();
                // Snare: 190 Hz body plus a noise crack.
                let snare =
                    snare * (-since_snare / 0.07).exp() * (0.5 * (TAU * 190.0 * since_snare).sin() + 0.6 * white);
                // Distractors: a sung or strummed note with a sharp start, as loud as the snare.
                let voice: f32 = bursts
                    .iter()
                    .filter(|(start, _)| t >= *start && t < *start + 0.3)
                    .map(|(start, pitch)| {
                        let d = t - start;
                        0.35 * (-d / 0.12).exp() * ((TAU * pitch * d).sin() + 0.5 * (TAU * 2.0 * pitch * d).sin())
                    })
                    .sum();
                kick + bass + hat + snare + voice
            })
            .collect();

        let sink = AudioSink::default();
        let mut v = SoundVisualizer::default();
        v.set_sensitivity(sensitivity);
        let per_tick = (RATE * TICK) as usize;
        let mut beats = Vec::new();
        let mut last_since = v.since_beat;
        for (i, chunk) in samples.chunks(per_tick).enumerate() {
            sink.push(&chunk[..chunk.len() / 2], 1, RATE); // ~10 ms audio buffers, like the OS delivers
            sink.push(&chunk[chunk.len() / 2..], 1, RATE);
            v.update(sink.take(), TICK);
            if v.since_beat < last_since {
                beats.push((i + 1) as f32 * TICK);
            }
            last_since = v.since_beat;
        }
        let hits = |offset: f32| (0..(seconds / beat / 2.0) as usize).map(|k| offset + 2.0 * k as f32 * beat).collect();
        (hits(0.0), if snare > 0.0 { hits(beat) } else { Vec::new() }, beats)
    }

    /// (share of `hits` caught, false beats per second outside `all_hits`), after 2 s of warm-up.
    pub fn score(hits: &[f32], all_hits: &[f32], beats: &[f32], seconds: f32) -> (f32, f32) {
        let after = |t: f32| t >= 2.0;
        let near = |h: f32, b: f32| b >= h && b <= h + 0.08;
        let hits: Vec<f32> = hits.iter().cloned().filter(|&h| after(h)).collect();
        let caught = hits.iter().filter(|&&h| beats.iter().any(|&b| near(h, b))).count();
        let false_beats = beats.iter().filter(|&&b| after(b) && !all_hits.iter().any(|&h| near(h, b))).count();
        (caught as f32 / hits.len().max(1) as f32, false_beats as f32 / (seconds - 2.0))
    }

    fn song(bpm: f32, bassline: f32, snare: f32) -> (f32, f32, f32) {
        song_at(Sensitivity::High, bpm, bassline, snare)
    }

    fn song_at(sensitivity: Sensitivity, bpm: f32, bassline: f32, snare: f32) -> (f32, f32, f32) {
        let (kicks, snares, beats) = run_at(sensitivity, bpm, 12.0, bassline, snare, 0.0);
        let all: Vec<f32> = kicks.iter().chain(&snares).cloned().collect();
        let (k, false_rate) = score(&kicks, &all, &beats, 12.0);
        let (s, _) = score(&snares, &all, &beats, 12.0);
        (k, s, false_rate)
    }

    #[test]
    fn catches_kicks_and_snares_over_a_bassline() {
        for (bpm, bassline, snare) in [(90.0, 0.2, 0.3), (120.0, 0.35, 0.3), (170.0, 0.3, 0.2)] {
            let (k, s, f) = song(bpm, bassline, snare);
            assert!(k >= 0.95 && s >= 0.95, "{bpm} BPM: kicks {k}, snares {s}");
            assert!(f <= 1.0, "{bpm} BPM: {f} false beats per second");
        }
    }

    /// (beats caught, false beats per second) on a busy song with off-beat distractors.
    fn busy_song(sensitivity: Sensitivity, bpm: f32, distractors: f32) -> (f32, f32) {
        let (kicks, snares, beats) = run_at(sensitivity, bpm, 16.0, 0.3, 0.3, distractors);
        let all: Vec<f32> = kicks.iter().chain(&snares).cloned().collect();
        score(&all, &all, &beats, 16.0)
    }

    #[test]
    fn follows_the_beat_through_off_beat_sounds() {
        for bpm in [95.0, 120.0, 140.0] {
            let (caught, false_rate) = busy_song(Sensitivity::Medium, bpm, 2.0);
            assert!(caught >= 0.8, "{bpm} BPM: caught {caught}");
            assert!(false_rate <= 0.3, "{bpm} BPM: {false_rate} false flashes per second");
        }
    }

    #[test]
    fn calm_mode_pulses_on_every_other_beat() {
        for bpm in [120.0, 140.0] {
            let (caught, false_rate) = busy_song(Sensitivity::Low, bpm, 2.0);
            assert!((0.4..=0.62).contains(&caught), "{bpm} BPM: pulsed on {caught} of the beats");
            assert!(false_rate <= 0.3, "{bpm} BPM: {false_rate} false pulses per second");
        }
    }

    #[test]
    #[ignore = "diagnostic: cargo test -p ambilight-core busy -- --ignored --nocapture"]
    fn busy_report() {
        for bpm in [95.0, 120.0, 140.0] {
            for sens in [Sensitivity::Low, Sensitivity::Medium, Sensitivity::High] {
                let (c, f) = busy_song(sens, bpm, 2.0);
                println!("{sens:?} {bpm} BPM, 2 distractors/s: beats {:.0}%, {f:.2} false/s", c * 100.0);
            }
        }
    }

    #[test]
    #[ignore = "diagnostic: cargo test -p ambilight-core music -- --ignored --nocapture"]
    fn report() {
        for (bpm, bassline, snare) in [(90.0, 0.2, 0.3), (120.0, 0.35, 0.3), (128.0, 0.45, 0.25), (170.0, 0.3, 0.2)] {
            for sens in [Sensitivity::Low, Sensitivity::Medium, Sensitivity::High] {
                let (k, s, f) = song_at(sens, bpm, bassline, snare);
                println!(
                    "{sens:?} {bpm} BPM, bass {bassline}, snare {snare}: kicks {:.0}%, snares {:.0}%, {f:.2} false/s",
                    k * 100.0,
                    s * 100.0
                );
            }
        }
    }
}
