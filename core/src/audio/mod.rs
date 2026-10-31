//! System audio capture for Sound mode. Each platform backend feeds raw samples of what the
//! computer is playing into an [`AudioSink`], which tracks loudness in a few frequency bands.
//! Only loudness is kept; no audio is stored.

use parking_lot::Mutex;
use std::f32::consts::TAU;
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
pub enum AudioError {
    PermissionDenied,
    Unavailable(String),
}

/// A running system-audio capture.
pub trait Backend: Send {
    fn start(&mut self, sink: Arc<AudioSink>) -> Result<(), AudioError>;
    fn stop(&mut self);
    /// Set when a started capture has died on its own.
    fn failure(&self) -> Option<String>;
}

pub fn backend() -> Box<dyn Backend> {
    platform::new_backend()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControllerState {
    Stopped,
    Starting,
    Running,
    Failed(AudioError),
}

enum Request {
    Start(Arc<AudioSink>),
    Stop,
}

/// Runs a [`Backend`] on its own thread. Starting a capture can block for as long as macOS shows
/// its permission prompt, and the engine must keep driving the LEDs meanwhile.
pub struct Controller {
    tx: std::sync::mpsc::Sender<Request>,
    state: Arc<Mutex<ControllerState>>,
}

impl Controller {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Request>();
        let state = Arc::new(Mutex::new(ControllerState::Stopped));
        let shared = state.clone();
        std::thread::Builder::new()
            .name("ambilight-audio-control".into())
            .spawn(move || {
                let mut backend = backend();
                let mut running = false;
                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                        Ok(Request::Start(sink)) => {
                            *shared.lock() = ControllerState::Starting;
                            let result = backend.start(sink);
                            running = result.is_ok();
                            *shared.lock() = match result {
                                Ok(()) => ControllerState::Running,
                                Err(e) => ControllerState::Failed(e),
                            };
                        }
                        Ok(Request::Stop) => {
                            backend.stop();
                            running = false;
                            *shared.lock() = ControllerState::Stopped;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if running {
                                if let Some(err) = backend.failure() {
                                    backend.stop();
                                    running = false;
                                    *shared.lock() = ControllerState::Failed(AudioError::Unavailable(err));
                                }
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            backend.stop();
                            return;
                        }
                    }
                }
            })
            .expect("spawn audio thread");
        Controller { tx, state }
    }

    pub fn start(&self, sink: Arc<AudioSink>) {
        *self.state.lock() = ControllerState::Starting;
        let _ = self.tx.send(Request::Start(sink));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Request::Stop);
    }

    pub fn state(&self) -> ControllerState {
        self.state.lock().clone()
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

/// RMS loudness (0..1 full scale) of the audio since the previous [`AudioSink::take`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bands {
    pub full: f32,
    /// Below ~150 Hz: kick drums, explosions.
    pub bass: f32,
    /// Above ~2 kHz: cymbals, voices' sibilance, sparkle.
    pub treble: f32,
}

#[derive(Default)]
struct SinkState {
    bass_lp: f32,
    treble_lp: f32,
    sum_full: f64,
    sum_bass: f64,
    sum_treble: f64,
    count: u64,
}

#[derive(Default)]
pub struct AudioSink {
    state: Mutex<SinkState>,
}

impl AudioSink {
    /// Interleaved f32 samples, `channels` per frame.
    pub fn push(&self, samples: &[f32], channels: usize, sample_rate: f32) {
        let channels = channels.max(1);
        let bass_a = 1.0 - (-TAU * 150.0 / sample_rate).exp();
        let treble_a = 1.0 - (-TAU * 2000.0 / sample_rate).exp();
        let mut s = self.state.lock();
        for frame in samples.chunks_exact(channels) {
            let x = frame.iter().sum::<f32>() / channels as f32;
            s.bass_lp += (x - s.bass_lp) * bass_a;
            s.treble_lp += (x - s.treble_lp) * treble_a;
            let treble = x - s.treble_lp;
            s.sum_full += (x * x) as f64;
            s.sum_bass += (s.bass_lp * s.bass_lp) as f64;
            s.sum_treble += (treble * treble) as f64;
            s.count += 1;
        }
    }

    /// Loudness since the last call; zero when nothing arrived (silence or no capture).
    pub fn take(&self) -> Bands {
        let mut s = self.state.lock();
        if s.count == 0 {
            return Bands::default();
        }
        let n = s.count as f64;
        let bands = Bands {
            full: (s.sum_full / n).sqrt() as f32,
            bass: (s.sum_bass / n).sqrt() as f32,
            treble: (s.sum_treble / n).sqrt() as f32,
        };
        s.sum_full = 0.0;
        s.sum_bass = 0.0;
        s.sum_treble = 0.0;
        s.count = 0;
        bands
    }

    pub fn clear(&self) {
        let _ = self.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(rate * secs) as usize).map(|i| amp * (TAU * freq * i as f32 / rate).sin()).collect()
    }

    #[test]
    fn low_tone_lands_in_bass() {
        let sink = AudioSink::default();
        sink.push(&sine(60.0, 48_000.0, 0.5, 0.5), 1, 48_000.0);
        let b = sink.take();
        assert!((b.full - 0.354).abs() < 0.01, "{b:?}");
        assert!(b.bass > 0.25, "{b:?}");
        assert!(b.treble < 0.02, "{b:?}");
    }

    #[test]
    fn high_tone_lands_in_treble() {
        let sink = AudioSink::default();
        sink.push(&sine(8000.0, 48_000.0, 0.5, 0.5), 1, 48_000.0);
        let b = sink.take();
        assert!(b.treble > 0.25, "{b:?}");
        assert!(b.bass < 0.02, "{b:?}");
    }

    #[test]
    fn stereo_is_mixed_and_take_resets() {
        let sink = AudioSink::default();
        sink.push(&[0.5, -0.5, 0.5, -0.5], 2, 48_000.0); // opposite channels cancel
        assert_eq!(sink.take().full, 0.0);
        assert_eq!(sink.take(), Bands::default());
    }
}
