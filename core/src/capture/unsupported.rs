use super::{Backend, CaptureError, DisplayInfo, FrameSink};
use std::sync::Arc;

struct Unsupported;

impl Backend for Unsupported {
    fn start(&mut self, _sink: Arc<FrameSink>, _display: u32) -> Result<(), CaptureError> {
        Err(CaptureError::Unavailable("Screen capture isn't supported on this platform yet".into()))
    }
    fn stop(&mut self) {}
    fn failure(&self) -> Option<String> {
        None
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(Unsupported)
}

pub fn open_permission_settings() {}

pub fn displays() -> Vec<DisplayInfo> {
    Vec::new()
}

pub fn display_asleep(_display: u32) -> bool {
    false
}
