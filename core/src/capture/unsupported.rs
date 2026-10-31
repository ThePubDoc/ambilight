use super::{Backend, CaptureError, FrameSink};
use std::sync::Arc;

struct Unsupported;

impl Backend for Unsupported {
    fn start(&mut self, _sink: Arc<FrameSink>) -> Result<(), CaptureError> {
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

pub fn display_asleep() -> bool {
    false
}
