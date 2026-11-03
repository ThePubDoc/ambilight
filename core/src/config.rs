use serde::{Deserialize, Serialize};

/// Color with channels in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Rgb { r, g, b }
    }

    /// Fully saturated color for a hue in 0..1.
    pub fn from_hue(h: f32) -> Self {
        let x = h.rem_euclid(1.0) * 6.0;
        let f = x - x.floor();
        match x as u32 % 6 {
            0 => Rgb::new(1.0, f, 0.0),
            1 => Rgb::new(1.0 - f, 1.0, 0.0),
            2 => Rgb::new(0.0, 1.0, f),
            3 => Rgb::new(0.0, 1.0 - f, 1.0),
            4 => Rgb::new(f, 0.0, 1.0),
            _ => Rgb::new(1.0, 0.0, 1.0 - f),
        }
    }

    pub fn scale(self, k: f32) -> Self {
        Rgb::new(self.r * k, self.g * k, self.b * k)
    }

    pub fn lerp(self, other: Rgb, t: f32) -> Self {
        Rgb::new(self.r + (other.r - self.r) * t, self.g + (other.g - self.g) * t, self.b + (other.b - self.b) * t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Follows the edges of the screen.
    #[default]
    Screen,
    /// Screen colors with brightness pulsing to the audio.
    ScreenSound,
    /// Follows the computer's audio output.
    Sound,
    /// One fixed color.
    Color,
    /// An animated effect (see [`Effect`]). Older configs called this "rainbow".
    #[serde(alias = "rainbow")]
    Effect,
}

impl Mode {
    /// Modes that read the screen (and so black out streaming video).
    pub fn captures_screen(self) -> bool {
        matches!(self, Mode::Screen | Mode::ScreenSound)
    }

    /// Order used by the "next mode" shortcut.
    pub fn next(self) -> Mode {
        match self {
            Mode::Screen => Mode::ScreenSound,
            Mode::ScreenSound => Mode::Sound,
            Mode::Sound => Mode::Color,
            Mode::Color => Mode::Effect,
            Mode::Effect => Mode::Screen,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    #[default]
    Rainbow,
    /// Audio spectrum: bass glows along the bottom, treble across the top.
    Spectrum,
    Breathing,
    Candle,
    Aurora,
}

impl Effect {
    pub fn needs_audio(self) -> bool {
        self == Effect::Spectrum
    }
}

/// A mode that applies while a particular app is in front.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppProfile {
    /// Bundle ID (macOS) or executable name (Windows).
    pub app: String,
    /// Display name.
    pub name: String,
    pub mode: Mode,
    /// Used when `mode` is Effect.
    pub effect: Effect,
}

impl Default for AppProfile {
    fn default() -> Self {
        AppProfile { app: String::new(), name: String::new(), mode: Mode::ScreenSound, effect: Effect::Rainbow }
    }
}

/// How Sound mode draws the music, modelled on well-known audio-reactive effects from LedFx and
/// WLED's sound-reactive fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SoundProfile {
    /// Calm: one color that fades gently with the loudness and the music's character, without
    /// beats (Philips Hue Sync's "Subtle" intensity).
    #[default]
    Glow,
    /// Calm: the strip slowly breathes in and out at the song's tempo, once every two bars.
    Breathe,
    /// Calm: soft blobs of neighbouring colors drift slowly around the screen (LedFx Lava lamp
    /// and Melt, WLED Midnoise).
    Lava,
    /// Calm: slow waves roll up from the bottom, swelling with the loudness (WLED Plasmoid).
    Waves,
    /// One color for the whole strip that follows the loudness, with soft pulses on beats.
    Pulse,
    /// Bass, mids and treble as red, green and blue bars growing from the bottom (LedFx Energy).
    Energy,
    /// The sound's color flows from the bottom around the screen (LedFx Scroll, WLED Waterfall).
    Scroll,
    /// A volume meter rising up both sides, with a falling peak dot (WLED Gravimeter).
    Meter,
    /// Eight frequency bands from bass at the bottom to treble at the top (WLED GEQ, LedFx Bands).
    Spectrum,
    /// Each beat drops a ripple of color that spreads out and fades (WLED Ripple Peak).
    Ripple,
    /// A gradient that swells with the bass, with white sparks on the treble (LedFx Power).
    Power,
}

/// How Sound and Mix modes flash on beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sensitivity {
    /// Follow the tempo and pulse on every other beat (the stronger one, usually the kick).
    #[default]
    Low,
    /// Follow the tempo and pulse on every beat.
    Medium,
    /// Pulse on every drum hit, without following the tempo.
    High,
}

/// Where LED 1 (the end wired to the controller) sits, seen from the front of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    #[default]
    BottomLeft,
}

/// Which way the strip runs from LED 1, seen from the front of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    #[default]
    Clockwise,
    Anticlockwise,
}

/// Where the LED data goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Output {
    /// A USB controller running Adalight firmware.
    #[default]
    Serial,
    /// A WLED controller on the network (UDP realtime protocol).
    Wled,
}

/// Lights off (or dimmed) during a daily time window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    pub enabled: bool,
    /// Start of the window, minutes after midnight.
    pub from: u16,
    /// End of the window, minutes after midnight (may be earlier than `from`: wraps past midnight).
    pub until: u16,
    /// Brightness factor inside the window; 0 turns the lights off.
    pub dim_to: f32,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule { enabled: false, from: 23 * 60 + 30, until: 7 * 60, dim_to: 0.0 }
    }
}

impl Schedule {
    /// Brightness factor at `minute` (minutes after midnight), when the window applies.
    pub fn factor_at(&self, minute: u16) -> Option<f32> {
        if !self.enabled || self.from == self.until {
            return None;
        }
        let inside = if self.from < self.until {
            (self.from..self.until).contains(&minute)
        } else {
            minute >= self.from || minute < self.until
        };
        inside.then_some(self.dim_to.clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    pub mode: Mode,
    pub effect: Effect,
    /// 0..=1
    pub brightness: f32,
    pub color: Rgb,
    pub smoothing_ms: f32,
    pub ignore_black_bars: bool,
    /// How far into the screen each LED looks, in percent of the screen height.
    pub edge_depth: f32,
    /// Per-channel gain to correct a strip's tint (1 = unchanged).
    pub white_balance: Rgb,
    /// Power supply rating in amps; the strip is kept below 90% of it. 0 disables the limit.
    pub max_current_a: f32,
    /// How often Sound and Mix modes pulse with the beat.
    pub beat_sensitivity: Sensitivity,
    /// How Sound mode draws the music.
    pub sound_profile: SoundProfile,

    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
    pub start: Corner,
    pub direction: Direction,
    /// LED 1 sits this many LEDs past the start corner, along the strip (negative: before it).
    pub offset: i16,
    /// LED-sized gap in the middle of the bottom edge with no LEDs (e.g. the monitor stand).
    pub bottom_gap: u16,

    /// Display to capture; 0 follows the main display.
    pub display: u32,

    pub output: Output,
    /// Serial port path or name; empty picks the first USB serial device.
    pub port: String,
    pub baud: u32,
    /// WLED host name or IP address.
    pub wled_host: String,

    /// Streaming services (keys of `foreground::STREAMING_SERVICES`) that switch Screen mode to
    /// `auto_switch_mode` while their window is in front. They black out copy-protected video
    /// whenever the screen is being captured.
    pub auto_switch_services: Vec<String>,
    pub auto_switch_mode: Mode,
    /// Modes for particular apps, used while that app is in front.
    pub app_profiles: Vec<AppProfile>,

    pub schedule: Schedule,
    pub shortcuts: bool,
    pub auto_update: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            mode: Mode::Screen,
            effect: Effect::Rainbow,
            brightness: 0.8,
            color: Rgb::new(1.0, 0.55, 0.2),
            smoothing_ms: 150.0,
            ignore_black_bars: true,
            edge_depth: 8.0,
            white_balance: Rgb::WHITE,
            max_current_a: 0.0,
            beat_sensitivity: Sensitivity::Low,
            sound_profile: SoundProfile::Glow,
            top: 30,
            right: 15,
            bottom: 30,
            left: 15,
            start: Corner::BottomLeft,
            direction: Direction::Clockwise,
            offset: 0,
            bottom_gap: 0,
            display: 0,
            output: Output::Serial,
            port: String::new(),
            baud: 115_200,
            wled_host: String::new(),
            auto_switch_services: crate::foreground::STREAMING_SERVICES.iter().map(|s| s.key.to_string()).collect(),
            auto_switch_mode: Mode::Sound,
            app_profiles: Vec::new(),
            schedule: Schedule::default(),
            shortcuts: true,
            auto_update: true,
        }
    }
}

/// Most LEDs per side (and gap slots) a config may ask for; the settings UI offers the same range.
pub const MAX_SIDE_LEDS: u16 = 500;

impl Config {
    pub fn led_count(&self) -> usize {
        self.top as usize + self.right as usize + self.bottom as usize + self.left as usize
    }

    /// Clamps values a hand-edited config.json could set out of range, so layout arithmetic and
    /// the Adalight/WLED headers (16-bit LED counts) stay valid.
    pub fn sanitized(mut self) -> Self {
        for v in [&mut self.top, &mut self.right, &mut self.bottom, &mut self.left, &mut self.bottom_gap] {
            *v = (*v).min(MAX_SIDE_LEDS);
        }
        self.brightness = self.brightness.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let c: Config = serde_json::from_str(r#"{"top": 49}"#).unwrap();
        assert_eq!(c.top, 49);
        assert_eq!(c.right, Config::default().right);
        assert_eq!(c.white_balance, Rgb::WHITE);
    }

    #[test]
    fn sanitized_caps_led_counts() {
        let c: Config = serde_json::from_str(r#"{"top": 65535, "bottomGap": 65535, "brightness": 7}"#).unwrap();
        let c = c.sanitized();
        assert_eq!((c.top, c.bottom_gap, c.brightness), (MAX_SIDE_LEDS, MAX_SIDE_LEDS, 1.0));
        assert_eq!(c.right, Config::default().right);
    }

    #[test]
    fn old_rainbow_mode_becomes_rainbow_effect() {
        let c: Config = serde_json::from_str(r#"{"mode": "rainbow"}"#).unwrap();
        assert_eq!((c.mode, c.effect), (Mode::Effect, Effect::Rainbow));
    }

    #[test]
    fn hue_wheel_endpoints() {
        assert_eq!(Rgb::from_hue(0.0), Rgb::new(1.0, 0.0, 0.0));
        assert_eq!(Rgb::from_hue(1.0 / 3.0).g, 1.0);
        assert_eq!(Rgb::from_hue(1.0), Rgb::from_hue(0.0));
    }

    #[test]
    fn schedule_window_wraps_midnight() {
        let s = Schedule { enabled: true, from: 23 * 60, until: 7 * 60, dim_to: 0.2 };
        assert_eq!(s.factor_at(23 * 60 + 30), Some(0.2));
        assert_eq!(s.factor_at(3 * 60), Some(0.2));
        assert_eq!(s.factor_at(7 * 60), None);
        assert_eq!(s.factor_at(12 * 60), None);
    }

    #[test]
    fn schedule_window_same_day_and_disabled() {
        let s = Schedule { enabled: true, from: 13 * 60, until: 14 * 60, dim_to: 0.0 };
        assert_eq!(s.factor_at(13 * 60 + 5), Some(0.0));
        assert_eq!(s.factor_at(14 * 60), None);
        assert_eq!(Schedule { enabled: false, ..s }.factor_at(13 * 60 + 5), None);
    }

    #[test]
    fn next_mode_cycles_through_all() {
        let mut m = Mode::Screen;
        let mut seen = vec![m];
        for _ in 0..4 {
            m = m.next();
            seen.push(m);
        }
        assert_eq!(m.next(), Mode::Screen);
        seen.dedup();
        assert_eq!(seen.len(), 5);
    }
}
