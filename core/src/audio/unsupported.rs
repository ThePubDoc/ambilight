use super::{AudioError, AudioSink, Backend};
use std::sync::Arc;

struct Unsupported;

impl Backend for Unsupported {
    fn start(&mut self, _sink: Arc<AudioSink>) -> Result<(), AudioError> {
        Err(AudioError::Unavailable("System audio capture isn't supported on this platform yet".into()))
    }
    fn stop(&mut self) {}
    fn failure(&self) -> Option<String> {
        None
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(Unsupported)
}
