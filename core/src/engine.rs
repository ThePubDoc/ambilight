//! The engine thread: owns the output link, screen capture and audio capture, and pushes a
//! frame to the strip 50 times a second.

use crate::audio::{AudioError, AudioSink, Controller, ControllerState};
use crate::capture::{self, Backend, CaptureError, FrameSink};
use crate::config::{Config, Effect, Mode, Output, Rgb};
use crate::effects::Effects;
use crate::foreground::{self, Foreground};
use crate::layout::{self, Region, Side};
use crate::output::Link;
use crate::pixels::Pipeline;
use crate::serial::{self, LinkError};
use crate::sound::SoundVisualizer;
use crate::sound_profiles::SoundProfiles;
use chrono::Timelike;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const TICK: Duration = Duration::from_millis(20);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);
const CAPTURE_RETRY_EVERY: Duration = Duration::from_secs(3);
/// A wall-clock jump this large between ticks means the machine was asleep.
const SLEEP_GAP: Duration = Duration::from_secs(5);
const FOREGROUND_CHECK_EVERY: Duration = Duration::from_millis(500);
const TEST_PATTERN_FOR: Duration = Duration::from_secs(8);
/// Screen colors look washed out on LEDs without a little gamma.
const SCREEN_GAMMA: f32 = 1.5;

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "camelCase")]
pub enum CaptureState {
    #[default]
    Stopped,
    Running,
    NeedsPermission,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Short name of the connected port or WLED host, while connected.
    pub port: Option<String>,
    pub link_error: Option<String>,
    pub capture: CaptureState,
    /// System-audio capture for Sound, Screen + Sound and Spectrum.
    pub audio: CaptureState,
    /// Streaming service whose window being in front switched a screen mode to `auto_switch_mode`.
    pub auto_switched_for: Option<String>,
    /// App whose profile applies right now.
    pub profile_for: Option<String>,
    /// The mode and effect actually shown (after app profiles and the streaming switch).
    pub effective_mode: Mode,
    pub effective_effect: Effect,
    /// The night schedule currently dims (or turns off) the lights.
    pub scheduled: bool,
}

#[derive(Clone, Copy)]
enum Pattern {
    /// LED 1 white; right red, top green, left blue, bottom yellow.
    Identify,
    /// Every LED full white, for setting the white balance.
    White,
}

enum Command {
    Config(Config),
    Test(Pattern),
    Reconnect,
    Shutdown(Sender<()>),
}

/// Handle to the engine thread.
pub struct Engine {
    tx: Sender<Command>,
    status: Arc<Mutex<Status>>,
}

impl Engine {
    /// Starts the engine. `on_status` runs on the engine thread whenever [`Status`] changes.
    pub fn start(config: Config, on_status: impl Fn(&Status) + Send + 'static) -> Engine {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let worker = Worker::new(config, rx, status.clone(), Box::new(on_status));
        thread::Builder::new()
            .name("ambilight-engine".into())
            .spawn(move || worker.run())
            .expect("spawn engine thread");
        Engine { tx, status }
    }

    pub fn set_config(&self, config: Config) {
        let _ = self.tx.send(Command::Config(config));
    }

    /// Shows the layout test pattern for a few seconds.
    pub fn identify(&self) {
        let _ = self.tx.send(Command::Test(Pattern::Identify));
    }

    /// Shows full white for a few seconds (for white balance).
    pub fn test_white(&self) {
        let _ = self.tx.send(Command::Test(Pattern::White));
    }

    /// Reopens the output and restarts screen and audio capture.
    pub fn reconnect(&self) {
        let _ = self.tx.send(Command::Reconnect);
    }

    pub fn status(&self) -> Status {
        self.status.lock().clone()
    }

    /// Turns the strip off and stops the engine (waits up to a second).
    pub fn shutdown(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.tx.send(Command::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(1));
        }
    }
}

struct Worker {
    rx: Receiver<Command>,
    config: Config,
    regions: Vec<Region>,
    current: Vec<Rgb>,
    rgb: Vec<u8>,
    scratch: Vec<u8>,
    started: Instant,
    test: Option<(Pattern, Instant)>,

    link: Option<Link>,
    next_connect: Instant,

    sink: Arc<FrameSink>,
    backend: Box<dyn Backend>,
    capturing: bool,
    next_capture: Instant,

    audio_sink: Arc<AudioSink>,
    audio: Controller,
    /// Whether the engine has asked the audio controller to run.
    audio_requested: bool,
    next_audio: Instant,
    visualizer: SoundVisualizer,
    sound_profiles: SoundProfiles,
    effects: Effects,

    /// Front app and window title, as last seen (Ambilight itself excluded).
    front_app: Option<String>,
    front_title: Option<String>,
    mode: Mode,
    effect: Effect,
    next_foreground_check: Instant,
    /// Brightness factor from the night schedule, when it applies.
    schedule_factor: Option<f32>,
    next_schedule_check: Instant,

    display_asleep: bool,
    next_display_check: Instant,
    last_wall: SystemTime,

    status: Status,
    shared_status: Arc<Mutex<Status>>,
    on_status: Box<dyn Fn(&Status) + Send>,
}

impl Worker {
    fn new(
        config: Config,
        rx: Receiver<Command>,
        shared_status: Arc<Mutex<Status>>,
        on_status: Box<dyn Fn(&Status) + Send>,
    ) -> Self {
        let now = Instant::now();
        let mut w = Worker {
            rx,
            config: Config::default(),
            regions: Vec::new(),
            current: Vec::new(),
            rgb: Vec::new(),
            scratch: Vec::new(),
            started: now,
            test: None,
            link: None,
            next_connect: now,
            sink: Arc::new(FrameSink::default()),
            backend: capture::backend(),
            capturing: false,
            next_capture: now,
            audio_sink: Arc::new(AudioSink::default()),
            audio: Controller::new(),
            audio_requested: false,
            next_audio: now,
            visualizer: SoundVisualizer::default(),
            sound_profiles: SoundProfiles::default(),
            effects: Effects::default(),
            front_app: None,
            front_title: None,
            mode: Mode::Screen,
            effect: Effect::Rainbow,
            next_foreground_check: now,
            schedule_factor: None,
            next_schedule_check: now,
            display_asleep: false,
            next_display_check: now,
            last_wall: SystemTime::now(),
            status: Status::default(),
            shared_status,
            on_status,
        };
        w.apply(config);
        w
    }

    fn run(mut self) {
        loop {
            let tick_start = Instant::now();
            while let Ok(cmd) = self.rx.try_recv() {
                match cmd {
                    Command::Config(c) => self.apply(c),
                    Command::Test(p) => self.test = Some((p, Instant::now() + TEST_PATTERN_FOR)),
                    Command::Reconnect => self.restart_all(),
                    Command::Shutdown(ack) => {
                        self.blackout();
                        self.stop_capture();
                        self.stop_audio();
                        self.link = None;
                        let _ = ack.send(());
                        return;
                    }
                }
            }
            self.watch_sleep();
            self.watch_schedule();
            self.watch_foreground();
            self.update_capture();
            self.update_audio();
            self.ensure_link();
            if self.link.is_some() && !self.display_asleep {
                self.render();
            }
            thread::sleep(TICK.saturating_sub(tick_start.elapsed()));
        }
    }

    fn apply(&mut self, c: Config) {
        let link_changed = c.port != self.config.port
            || c.baud != self.config.baud
            || c.output != self.config.output
            || c.wled_host != self.config.wled_host;
        let display_changed = c.display != self.config.display;
        self.regions = layout::build(&c);
        if self.current.len() != self.regions.len() {
            self.current = vec![Rgb::BLACK; self.regions.len()];
        }
        self.sink.configure(self.regions.clone(), c.ignore_black_bars);
        self.visualizer.set_sensitivity(c.beat_sensitivity);
        self.sound_profiles.configure(&self.regions);
        self.config = c;
        self.resolve();
        self.next_schedule_check = Instant::now();
        if link_changed {
            self.link = None;
            self.next_connect = Instant::now();
        }
        if display_changed {
            self.stop_capture();
            self.next_capture = Instant::now();
        }
    }

    /// Fresh output connection, capture stream and audio tap (after sleep, or on request).
    fn restart_all(&mut self) {
        self.link = None;
        self.next_connect = Instant::now();
        self.stop_capture();
        self.next_capture = Instant::now();
        self.stop_audio();
        self.next_audio = Instant::now();
    }

    /// The mode actually shown: the front app's profile, or the configured mode; screen modes
    /// give way to `auto_switch_mode` while a streaming service is in front.
    fn effective_mode(&self) -> Mode {
        self.mode
    }

    fn effective_effect(&self) -> Effect {
        self.effect
    }

    /// Recomputes the effective mode from the config and the last seen front window.
    fn resolve(&mut self) {
        let (mode, effect, profile, streaming) =
            foreground::resolve(&self.config, self.front_app.as_deref(), self.front_title.as_deref());
        let profile = profile.map(str::to_string);
        let streaming = streaming.map(str::to_string);
        self.mode = mode;
        self.effect = effect;
        let s = &self.status;
        if s.effective_mode != mode
            || s.effective_effect != effect
            || s.profile_for != profile
            || s.auto_switched_for != streaming
        {
            log::info!(
                "mode: {mode:?}{}{}",
                profile.as_deref().map(|p| format!(" (profile for {p})")).unwrap_or_default(),
                streaming.as_deref().map(|s| format!(" ({s} in front)")).unwrap_or_default()
            );
            self.status.effective_mode = mode;
            self.status.effective_effect = effect;
            self.status.profile_for = profile;
            self.status.auto_switched_for = streaming;
            self.publish();
        }
    }

    /// Whether anything is shown at all (on, and not switched off by the night schedule).
    fn lit(&self) -> bool {
        self.config.enabled && self.schedule_factor != Some(0.0) && !self.display_asleep
    }

    fn watch_sleep(&mut self) {
        // Instant doesn't advance during sleep on every OS, so compare wall-clock time.
        let now = SystemTime::now();
        let gap = now.duration_since(self.last_wall).unwrap_or_default();
        self.last_wall = now;
        if gap > SLEEP_GAP {
            log::info!("resumed after {gap:?}; reconnecting");
            self.restart_all();
        }

        if Instant::now() >= self.next_display_check {
            self.next_display_check = Instant::now() + Duration::from_secs(1);
            let asleep = capture::display_asleep(self.config.display);
            if asleep != self.display_asleep {
                log::info!("display {}", if asleep { "asleep" } else { "awake" });
                self.display_asleep = asleep;
                if asleep {
                    self.blackout();
                } else {
                    self.restart_all();
                }
            }
        }
    }

    fn watch_schedule(&mut self) {
        if Instant::now() < self.next_schedule_check {
            return;
        }
        self.next_schedule_check = Instant::now() + Duration::from_secs(10);
        let now = chrono::Local::now();
        let factor = self.config.schedule.factor_at((now.hour() * 60 + now.minute()) as u16);
        if factor != self.schedule_factor {
            log::info!("schedule: {}", factor.map_or("off".into(), |f| format!("dim to {f}")));
            self.schedule_factor = factor;
            if self.status.scheduled != factor.is_some() {
                self.status.scheduled = factor.is_some();
                self.publish();
            }
        }
    }

    /// Tracks the front window in every mode, so switching to a screen mode from the panel
    /// (while Ambilight itself is in front) already knows a streaming service is behind it.
    fn watch_foreground(&mut self) {
        if Instant::now() < self.next_foreground_check {
            return;
        }
        self.next_foreground_check = Instant::now() + FOREGROUND_CHECK_EVERY;
        match foreground::frontmost() {
            // Adjusting Ambilight itself (popover, settings) keeps the current state.
            Foreground::Ambilight => {}
            Foreground::Window { app, title } => {
                if app != self.front_app || title != self.front_title {
                    self.front_app = app;
                    self.front_title = title;
                    self.resolve();
                }
            }
        }
    }

    fn update_capture(&mut self) {
        let wanted = self.lit() && self.effective_mode().captures_screen();
        if self.capturing {
            if let Some(err) = self.backend.failure() {
                log::warn!("capture stopped: {err}");
                self.stop_capture();
                self.set_capture(CaptureState::Failed(err));
                self.next_capture = Instant::now() + CAPTURE_RETRY_EVERY;
            } else if !wanted {
                self.stop_capture();
                self.set_capture(CaptureState::Stopped);
            }
            return;
        }
        if !wanted {
            if self.status.capture != CaptureState::Stopped {
                self.set_capture(CaptureState::Stopped);
            }
            return;
        }
        if Instant::now() < self.next_capture {
            return;
        }
        match self.backend.start(self.sink.clone(), self.config.display) {
            Ok(()) => {
                self.capturing = true;
                self.set_capture(CaptureState::Running);
            }
            Err(e) => {
                self.next_capture = Instant::now() + CAPTURE_RETRY_EVERY;
                self.set_capture(match e {
                    CaptureError::PermissionDenied => CaptureState::NeedsPermission,
                    CaptureError::Unavailable(msg) => CaptureState::Failed(msg),
                });
            }
        }
    }

    fn stop_capture(&mut self) {
        if self.capturing {
            self.backend.stop();
            self.capturing = false;
        }
        self.sink.clear();
    }

    fn wants_audio(&self) -> bool {
        match self.effective_mode() {
            Mode::Sound | Mode::ScreenSound => true,
            Mode::Effect => self.effective_effect().needs_audio(),
            Mode::Screen | Mode::Color => false,
        }
    }

    fn update_audio(&mut self) {
        if !(self.lit() && self.wants_audio()) {
            if self.audio_requested {
                self.stop_audio();
            }
            self.set_audio(CaptureState::Stopped);
            return;
        }
        match self.audio.state() {
            ControllerState::Running => self.set_audio(CaptureState::Running),
            // Still starting (possibly waiting on the macOS permission prompt): keep the LEDs going.
            ControllerState::Starting => {}
            ControllerState::Failed(e) => {
                if self.audio_requested {
                    self.audio_requested = false;
                    self.next_audio = Instant::now() + CAPTURE_RETRY_EVERY;
                    self.set_audio(match e {
                        AudioError::PermissionDenied => CaptureState::NeedsPermission,
                        AudioError::Unavailable(msg) => CaptureState::Failed(msg),
                    });
                } else if Instant::now() >= self.next_audio {
                    self.request_audio();
                }
            }
            ControllerState::Stopped => {
                if !self.audio_requested && Instant::now() >= self.next_audio {
                    self.request_audio();
                }
            }
        }
    }

    fn request_audio(&mut self) {
        self.audio_requested = true;
        self.audio.start(self.audio_sink.clone());
    }

    fn stop_audio(&mut self) {
        if self.audio_requested {
            self.audio.stop();
            self.audio_requested = false;
        }
        self.audio_sink.clear();
    }

    fn ensure_link(&mut self) {
        if self.link.is_some() || Instant::now() < self.next_connect {
            return;
        }
        self.next_connect = Instant::now() + RECONNECT_EVERY;
        match self.config.output {
            Output::Serial => {
                let path = if self.config.port.is_empty() {
                    match serial::available_ports().into_iter().next() {
                        Some(p) => p.path,
                        None => return self.set_link(None, Some(LinkError::NotFound.to_string())),
                    }
                } else {
                    self.config.port.clone()
                };
                match Link::open_serial(&path, self.config.baud) {
                    Ok(link) => {
                        log::info!("opened {path} at {} baud", self.config.baud);
                        self.link = Some(link);
                        self.set_link(Some(serial::short_name(&path).to_string()), None);
                    }
                    Err(e) => self.set_link(None, Some(e.to_string())),
                }
            }
            Output::Wled => match Link::open_wled(&self.config.wled_host) {
                Ok(link) => {
                    log::info!("sending to WLED at {}", self.config.wled_host);
                    self.link = Some(link);
                    self.set_link(Some(format!("WLED {}", self.config.wled_host.trim())), None);
                }
                Err(e) => self.set_link(None, Some(e)),
            },
        }
    }

    fn render(&mut self) {
        let n = self.regions.len();
        if n == 0 {
            return;
        }
        let now = Instant::now();
        let dt = TICK.as_secs_f32();
        let mut gamma = 1.0;
        let mut smooth = true;

        if let Some((pattern, until)) = self.test {
            if now < until {
                self.current = self.test_pattern(pattern);
                return self.send(1.0, 1.0);
            }
            self.test = None;
        }

        let target: Vec<Rgb> = if !self.lit() {
            vec![Rgb::BLACK; n]
        } else {
            match self.effective_mode() {
                Mode::Screen => {
                    gamma = SCREEN_GAMMA;
                    self.screen_colors()
                }
                Mode::ScreenSound => {
                    gamma = SCREEN_GAMMA;
                    self.visualizer.update(self.audio_sink.take(), dt);
                    // Screen colors at a steady brightness, boosted on each beat.
                    let factor = self.visualizer.mix_brightness();
                    self.screen_colors().into_iter().map(|c| c.scale(factor)).collect()
                }
                Mode::Sound => {
                    self.visualizer.update(self.audio_sink.take(), dt);
                    smooth = false; // the profiles shape attack and release themselves
                    self.sound_profiles.render(self.config.sound_profile, &self.visualizer, dt)
                }
                Mode::Color => vec![self.config.color; n],
                Mode::Effect => {
                    let effect = self.effective_effect();
                    if effect.needs_audio() {
                        self.visualizer.update(self.audio_sink.take(), dt);
                    }
                    smooth = effect != Effect::Spectrum;
                    let t = self.started.elapsed().as_secs_f32();
                    self.effects.render(effect, &self.regions, t, dt, self.config.color, &self.visualizer)
                }
            }
        };

        if smooth && self.config.smoothing_ms > 0.0 {
            // Time constant so a change is ~95% complete after smoothing_ms.
            let tau = self.config.smoothing_ms / 1000.0 / 3.0;
            let a = 1.0 - (-dt / tau).exp();
            for (c, t) in self.current.iter_mut().zip(&target) {
                *c = c.lerp(*t, a);
            }
        } else {
            self.current = target;
        }
        let brightness = self.config.brightness * self.schedule_factor.unwrap_or(1.0);
        self.send(gamma, brightness);
    }

    /// The latest captured colors; holds the previous ones until capture delivers frames.
    fn screen_colors(&self) -> Vec<Rgb> {
        let n = self.regions.len();
        self.sink.latest().filter(|c| c.len() == n).unwrap_or_else(|| self.current.clone())
    }

    fn send(&mut self, gamma: f32, brightness: f32) {
        let pipeline = Pipeline {
            gamma,
            brightness,
            white_balance: self.config.white_balance,
            max_current_a: self.config.max_current_a,
        };
        pipeline.encode(&self.current, &mut self.rgb);
        self.write();
    }

    fn write(&mut self) {
        let Some(link) = self.link.as_mut() else { return };
        if let Err(e) = link.send(&self.rgb, &mut self.scratch) {
            log::warn!("write failed: {e}");
            self.link = None;
            self.next_connect = Instant::now() + RECONNECT_EVERY;
            self.set_link(None, Some("LED controller disconnected. Reconnecting…".into()));
        }
    }

    fn blackout(&mut self) {
        if self.link.is_some() && !self.regions.is_empty() {
            self.current.fill(Rgb::BLACK);
            self.rgb.clear();
            self.rgb.resize(self.regions.len() * 3, 0);
            self.write();
        }
    }

    fn test_pattern(&self, pattern: Pattern) -> Vec<Rgb> {
        match pattern {
            Pattern::White => vec![Rgb::WHITE; self.regions.len()],
            Pattern::Identify => self
                .regions
                .iter()
                .enumerate()
                .map(|(i, r)| match (i, r.side) {
                    (0, _) => Rgb::WHITE,
                    (_, Side::Right) => Rgb::new(1.0, 0.0, 0.0),
                    (_, Side::Top) => Rgb::new(0.0, 1.0, 0.0),
                    (_, Side::Left) => Rgb::new(0.0, 0.0, 1.0),
                    (_, Side::Bottom) => Rgb::new(1.0, 0.8, 0.0),
                })
                .collect(),
        }
    }

    fn set_link(&mut self, port: Option<String>, error: Option<String>) {
        if self.status.port != port || self.status.link_error != error {
            self.status.port = port;
            self.status.link_error = error;
            self.publish();
        }
    }

    fn set_audio(&mut self, state: CaptureState) {
        if self.status.audio != state {
            log::info!("audio: {state:?}");
            self.status.audio = state;
            self.publish();
        }
    }

    fn set_capture(&mut self, state: CaptureState) {
        if self.status.capture != state {
            log::info!("capture: {state:?}");
            self.status.capture = state;
            self.publish();
        }
    }

    fn publish(&self) {
        *self.shared_status.lock() = self.status.clone();
        (self.on_status)(&self.status);
    }
}
