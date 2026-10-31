//! The engine thread: owns the serial link and the screen capture, and pushes an
//! Adalight frame to the strip 50 times a second.

use crate::adalight;
use crate::capture::{self, Backend, CaptureError, FrameSink};
use crate::config::{Config, Mode, Rgb};
use crate::layout::{self, Region, Side};
use crate::serial::{self, LinkError};
use parking_lot::Mutex;
use serde::Serialize;
use serialport::SerialPort;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const TICK: Duration = Duration::from_millis(20);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);
const CAPTURE_RETRY_EVERY: Duration = Duration::from_secs(3);
/// A wall-clock jump this large between ticks means the machine was asleep.
const SLEEP_GAP: Duration = Duration::from_secs(5);
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
    /// Short name of the connected port, while connected.
    pub port: Option<String>,
    pub link_error: Option<String>,
    pub capture: CaptureState,
}

enum Command {
    Config(Config),
    Identify,
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

    /// Shows a layout test pattern for a few seconds.
    pub fn identify(&self) {
        let _ = self.tx.send(Command::Identify);
    }

    /// Reopens the serial port and restarts screen capture.
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
    buf: Vec<u8>,
    started: Instant,
    identify_until: Option<Instant>,

    port: Option<Box<dyn SerialPort>>,
    next_connect: Instant,

    sink: Arc<FrameSink>,
    backend: Box<dyn Backend>,
    capturing: bool,
    next_capture: Instant,

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
            buf: Vec::new(),
            started: now,
            identify_until: None,
            port: None,
            next_connect: now,
            sink: Arc::new(FrameSink::default()),
            backend: capture::backend(),
            capturing: false,
            next_capture: now,
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
                    Command::Identify => self.identify_until = Some(Instant::now() + Duration::from_secs(6)),
                    Command::Reconnect => self.restart_all(),
                    Command::Shutdown(ack) => {
                        self.blackout();
                        self.stop_capture();
                        self.port = None;
                        let _ = ack.send(());
                        return;
                    }
                }
            }
            self.watch_sleep();
            self.update_capture();
            self.ensure_port();
            if self.port.is_some() && !self.display_asleep {
                self.render();
            }
            thread::sleep(TICK.saturating_sub(tick_start.elapsed()));
        }
    }

    fn apply(&mut self, c: Config) {
        let link_changed = c.port != self.config.port || c.baud != self.config.baud;
        self.regions = layout::build(&c);
        if self.current.len() != self.regions.len() {
            self.current = vec![Rgb::BLACK; self.regions.len()];
        }
        self.sink.configure(self.regions.clone(), c.ignore_black_bars);
        self.config = c;
        if link_changed {
            self.port = None;
            self.next_connect = Instant::now();
        }
    }

    /// Fresh serial connection and capture stream (after sleep, or on request).
    fn restart_all(&mut self) {
        self.port = None;
        self.next_connect = Instant::now();
        self.stop_capture();
        self.next_capture = Instant::now();
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
            let asleep = capture::display_asleep();
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

    fn update_capture(&mut self) {
        let wanted = self.config.enabled && self.config.mode == Mode::Screen && !self.display_asleep;
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
        log::debug!("starting screen capture");
        match self.backend.start(self.sink.clone()) {
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

    fn ensure_port(&mut self) {
        if self.port.is_some() || Instant::now() < self.next_connect {
            return;
        }
        self.next_connect = Instant::now() + RECONNECT_EVERY;
        let path = if self.config.port.is_empty() {
            match serial::available_ports().into_iter().next() {
                Some(p) => p.path,
                None => return self.set_link(None, Some(LinkError::NotFound.to_string())),
            }
        } else {
            self.config.port.clone()
        };
        match serial::open(&path, self.config.baud) {
            Ok(port) => {
                log::info!("opened {path} at {} baud", self.config.baud);
                self.port = Some(port);
                self.set_link(Some(serial::short_name(&path).to_string()), None);
            }
            Err(e) => self.set_link(None, Some(e.to_string())),
        }
    }

    fn render(&mut self) {
        let n = self.regions.len();
        if n == 0 {
            return;
        }
        let now = Instant::now();
        let mut gamma = 1.0;
        let target: Vec<Rgb> = if self.identify_until.is_some_and(|t| now < t) {
            self.current = self.identify_pattern();
            self.current.clone()
        } else if !self.config.enabled {
            vec![Rgb::BLACK; n]
        } else {
            match self.config.mode {
                Mode::Screen => {
                    gamma = SCREEN_GAMMA;
                    // Hold the last colors until capture delivers frames.
                    self.sink.latest().filter(|c| c.len() == n).unwrap_or_else(|| self.current.clone())
                }
                Mode::Color => vec![self.config.color; n],
                Mode::Rainbow => {
                    let t = self.started.elapsed().as_secs_f32() * 0.08;
                    (0..n).map(|i| Rgb::from_hue(i as f32 / n as f32 + t)).collect()
                }
            }
        };

        if self.config.smoothing_ms > 0.0 {
            // Time constant so a change is ~95% complete after smoothing_ms.
            let tau = self.config.smoothing_ms / 1000.0 / 3.0;
            let a = 1.0 - (-TICK.as_secs_f32() / tau).exp();
            for (c, t) in self.current.iter_mut().zip(&target) {
                c.r += (t.r - c.r) * a;
                c.g += (t.g - c.g) * a;
                c.b += (t.b - c.b) * a;
            }
        } else {
            self.current = target;
        }

        adalight::encode(&self.current, gamma, self.config.brightness, &mut self.buf);
        self.write_frame();
    }

    fn write_frame(&mut self) {
        let Some(port) = self.port.as_mut() else { return };
        if let Err(e) = port.write_all(&self.buf) {
            log::warn!("serial write failed: {e}");
            self.port = None;
            self.next_connect = Instant::now() + RECONNECT_EVERY;
            self.set_link(None, Some("LED controller disconnected. Reconnecting…".into()));
        }
    }

    fn blackout(&mut self) {
        if self.port.is_some() && !self.regions.is_empty() {
            self.current.fill(Rgb::BLACK);
            adalight::encode(&self.current, 1.0, 0.0, &mut self.buf);
            self.write_frame();
        }
    }

    /// LED 1 white; right red, top green, left blue, bottom yellow.
    fn identify_pattern(&self) -> Vec<Rgb> {
        self.regions
            .iter()
            .enumerate()
            .map(|(i, r)| match (i, r.side) {
                (0, _) => Rgb::WHITE,
                (_, Side::Right) => Rgb::new(1.0, 0.0, 0.0),
                (_, Side::Top) => Rgb::new(0.0, 1.0, 0.0),
                (_, Side::Left) => Rgb::new(0.0, 0.0, 1.0),
                (_, Side::Bottom) => Rgb::new(1.0, 0.8, 0.0),
            })
            .collect()
    }

    fn set_link(&mut self, port: Option<String>, error: Option<String>) {
        if self.status.port != port || self.status.link_error != error {
            self.status.port = port;
            self.status.link_error = error;
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
