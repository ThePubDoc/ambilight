//! macOS capture through ScreenCaptureKit.

use super::{Backend, CaptureError, DisplayInfo, FrameSink};
use crate::frame::Frame;
use parking_lot::Mutex;
use screencapturekit::cm::{CMSampleBufferExt, SCFrameStatus};
use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;
use screencapturekit::stream::delegate_trait::SCStreamDelegateTrait;
use screencapturekit::utils::error::SCStreamErrorCode;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayIsAsleep(display: u32) -> i32;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGDisplayIsBuiltin(display: u32) -> u32;
}

/// The display to capture: `requested` if it's connected, otherwise the main display.
fn resolve(requested: u32) -> u32 {
    if requested != 0 && displays().iter().any(|d| d.id == requested) {
        requested
    } else {
        unsafe { CGMainDisplayID() }
    }
}

extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

/// Asks macOS for Screen Recording access. This adds Ambilight to the list in System Settings
/// (and shows the prompt the first time); it is ignored unless it runs on the main thread.
fn request_permission() {
    extern "C" fn on_main(_: *mut c_void) {
        unsafe { CGRequestScreenCaptureAccess() };
    }
    unsafe { dispatch_async_f(std::ptr::addr_of!(_dispatch_main_q), std::ptr::null_mut(), on_main) };
}

/// Capture width; plenty for averaging edge regions and cheap to process.
const WIDTH: u32 = 320;

static PERMISSION_REQUESTED: AtomicBool = AtomicBool::new(false);

struct Output {
    sink: Arc<FrameSink>,
}

impl SCStreamOutputTrait for Output {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
        if !matches!(kind, SCStreamOutputType::Screen) {
            return;
        }
        // Idle frames (screen unchanged) carry no new pixels: keep the last colors.
        if let Some(status) = sample.frame_status() {
            if status != SCFrameStatus::Complete {
                return;
            }
        }
        let Some(pb) = sample.pixel_buffer() else { return };
        let Ok(guard) = pb.lock(CVPixelBufferLockFlags::READ_ONLY) else { return };
        let Some(data) = (unsafe { guard.as_slice() }) else { return };
        self.sink.push(&Frame { width: guard.width(), height: guard.height(), stride: guard.bytes_per_row(), data });
    }
}

struct Delegate {
    failure: Arc<Mutex<Option<String>>>,
}

impl SCStreamDelegateTrait for Delegate {
    fn did_stop_with_error(&self, error: SCError) {
        *self.failure.lock() = Some(error.to_string());
    }
}

#[derive(Default)]
struct MacCapture {
    stream: Option<SCStream>,
    failure: Arc<Mutex<Option<String>>>,
}

fn permission_error(e: &SCError) -> bool {
    matches!(e.stream_error_code(), Some(SCStreamErrorCode::UserDeclined))
        || !unsafe { CGPreflightScreenCaptureAccess() }
}

impl Backend for MacCapture {
    fn start(&mut self, sink: Arc<FrameSink>, display: u32) -> Result<(), CaptureError> {
        self.stop();
        *self.failure.lock() = None;

        // Check with the cheap preflight first: probing ScreenCaptureKit without access makes
        // macOS post a new "would like to record" notification on every retry.
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            if !PERMISSION_REQUESTED.swap(true, Ordering::Relaxed) {
                request_permission();
            }
            return Err(CaptureError::PermissionDenied);
        }

        let content = SCShareableContent::get().map_err(|e| {
            if permission_error(&e) {
                // Shows the system prompt once; after that the user grants it in System Settings.
                if !PERMISSION_REQUESTED.swap(true, Ordering::Relaxed) {
                    request_permission();
                }
                CaptureError::PermissionDenied
            } else {
                CaptureError::Unavailable(e.to_string())
            }
        })?;

        let target = resolve(display);
        let displays = content.displays();
        let display = displays
            .iter()
            .find(|d| d.display_id() == target)
            .or_else(|| displays.first())
            .ok_or_else(|| CaptureError::Unavailable("No display found".into()))?;

        let height = ((WIDTH as f64 * display.height() as f64 / display.width().max(1) as f64).round() as u32).max(32);
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| CaptureError::Unavailable(e.to_string()))?;
        let config = SCStreamConfiguration::new()
            .with_width(WIDTH)
            .with_height(height)
            .with_pixel_format(PixelFormat::BGRA)
            .with_minimum_frame_interval(&CMTime::new(1, 30))
            .with_shows_cursor(false)
            .with_queue_depth(3);

        let delegate = Delegate { failure: self.failure.clone() };
        let mut stream = SCStream::new_with_delegate(&filter, &config, delegate)
            .map_err(|e| CaptureError::Unavailable(e.to_string()))?;
        stream
            .add_output_handler(Output { sink }, SCStreamOutputType::Screen)
            .map_err(|e| CaptureError::Unavailable(e.to_string()))?;
        stream.start_capture().map_err(|e| {
            if permission_error(&e) {
                CaptureError::PermissionDenied
            } else {
                CaptureError::Unavailable(e.to_string())
            }
        })?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.stop_capture();
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.lock().clone()
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(MacCapture::default())
}

pub fn open_permission_settings() {
    request_permission(); // makes sure Ambilight is listed there
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
}

pub fn displays() -> Vec<DisplayInfo> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    let main = unsafe { CGMainDisplayID() };
    let mut external = 0;
    ids[..count as usize]
        .iter()
        .map(|&id| {
            let (w, h) = unsafe { (CGDisplayPixelsWide(id), CGDisplayPixelsHigh(id)) };
            let name = if unsafe { CGDisplayIsBuiltin(id) } != 0 {
                "Built-in display".to_string()
            } else {
                external += 1;
                format!("External display {external}")
            };
            DisplayInfo { id, label: format!("{name} ({w} × {h})"), main: id == main }
        })
        .collect()
}

pub fn display_asleep(display: u32) -> bool {
    unsafe { CGDisplayIsAsleep(resolve(display)) != 0 }
}
