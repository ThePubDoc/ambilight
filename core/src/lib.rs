//! Platform-independent core of Ambilight: LED layout, screen sampling, Adalight output
//! and the engine loop that ties them to a serial port and a screen capture backend.

pub mod adalight;
pub mod audio;
pub mod capture;
pub mod config;
pub mod effects;
pub mod engine;
pub mod foreground;
pub mod frame;
pub mod layout;
pub mod output;
pub mod pixels;
pub mod serial;
pub mod sound;

pub use config::{Config, Corner, Direction, Effect, Mode, Output, Rgb, Schedule};
pub use engine::{CaptureState, Engine, Status};
