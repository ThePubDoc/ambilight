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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Screen,
    Color,
    Rainbow,
    /// Follows the computer's audio output.
    Sound,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    pub mode: Mode,
    /// 0..=1
    pub brightness: f32,
    pub color: Rgb,
    pub smoothing_ms: f32,
    pub ignore_black_bars: bool,

    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
    pub start: Corner,
    pub direction: Direction,

    /// Serial port path or name; empty picks the first USB serial device.
    pub port: String,
    pub baud: u32,

    /// Streaming services (keys of `foreground::STREAMING_SERVICES`) that switch Screen mode to
    /// `auto_switch_mode` while their window is in front. They black out copy-protected video
    /// whenever the screen is being captured.
    pub auto_switch_services: Vec<String>,
    pub auto_switch_mode: Mode,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            mode: Mode::Screen,
            brightness: 0.8,
            color: Rgb::new(1.0, 0.55, 0.2),
            smoothing_ms: 150.0,
            ignore_black_bars: true,
            top: 30,
            right: 15,
            bottom: 30,
            left: 15,
            start: Corner::BottomLeft,
            direction: Direction::Clockwise,
            port: String::new(),
            baud: 115_200,
            auto_switch_services: crate::foreground::STREAMING_SERVICES.iter().map(|s| s.key.to_string()).collect(),
            auto_switch_mode: Mode::Sound,
        }
    }
}

impl Config {
    pub fn led_count(&self) -> usize {
        self.top as usize + self.right as usize + self.bottom as usize + self.left as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let c: Config = serde_json::from_str(r#"{"top": 49, "mode": "rainbow"}"#).unwrap();
        assert_eq!(c.top, 49);
        assert_eq!(c.mode, Mode::Rainbow);
        assert_eq!(c.right, Config::default().right);
    }

    #[test]
    fn hue_wheel_endpoints() {
        assert_eq!(Rgb::from_hue(0.0), Rgb::new(1.0, 0.0, 0.0));
        assert_eq!(Rgb::from_hue(1.0 / 3.0).g, 1.0);
        assert_eq!(Rgb::from_hue(1.0), Rgb::from_hue(0.0));
    }
}
