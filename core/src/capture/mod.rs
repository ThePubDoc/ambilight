//! Screen capture. Each platform backend delivers BGRA frames to a shared [`FrameSink`],
//! which turns them into one color per LED.

use crate::config::Rgb;
use crate::frame::{self, BlackBarDetector, Frame, Insets};
use crate::layout::Region;
use parking_lot::Mutex;
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use unsupported as platform;

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureError {
    PermissionDenied,
    Unavailable(String),
}

/// A display that can be captured.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayInfo {
    /// Stable for the session; pass to [`Backend::start`]. 0 is never used (it means "main").
    pub id: u32,
    pub label: String,
    pub main: bool,
}

/// A running screen capture.
pub trait Backend: Send {
    /// Captures `display` (an id from [`displays`], or 0 for the main display).
    fn start(&mut self, sink: Arc<FrameSink>, display: u32) -> Result<(), CaptureError>;
    fn stop(&mut self);
    /// Set when a started capture has died on its own.
    fn failure(&self) -> Option<String>;
}

pub fn backend() -> Box<dyn Backend> {
    platform::new_backend()
}

/// Opens the OS screen-recording privacy settings, where the platform has them.
pub fn open_permission_settings() {
    platform::open_permission_settings()
}

/// Displays that can be captured, main first.
pub fn displays() -> Vec<DisplayInfo> {
    let mut list = platform::displays();
    list.sort_by_key(|d| !d.main);
    list
}

/// True while the captured display is asleep (lid closed, display off).
pub fn display_asleep(display: u32) -> bool {
    platform::display_asleep(display)
}

#[derive(Default)]
struct SinkState {
    regions: Vec<Region>,
    ignore_black_bars: bool,
    detector: BlackBarDetector,
    latest: Option<Vec<Rgb>>,
}

/// Receives frames from a capture backend and keeps the latest per-LED colors.
#[derive(Default)]
pub struct FrameSink {
    state: Mutex<SinkState>,
}

impl FrameSink {
    pub fn configure(&self, regions: Vec<Region>, ignore_black_bars: bool) {
        let mut s = self.state.lock();
        if s.regions.len() != regions.len() {
            s.latest = None;
        }
        if !ignore_black_bars {
            s.detector.reset();
        }
        s.regions = regions;
        s.ignore_black_bars = ignore_black_bars;
    }

    pub fn latest(&self) -> Option<Vec<Rgb>> {
        self.state.lock().latest.clone()
    }

    pub fn clear(&self) {
        self.state.lock().latest = None;
    }

    pub fn push(&self, frame: &Frame) {
        let mut s = self.state.lock();
        if s.regions.is_empty() || frame.width == 0 || frame.height == 0 {
            return;
        }
        let insets = if s.ignore_black_bars { s.detector.detect(frame) } else { Insets::default() };
        let colors = frame::sample(frame, &s.regions, insets);
        s.latest = Some(colors);
    }
}
