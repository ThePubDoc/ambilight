//! Windows system-audio capture: WASAPI loopback on the default output device (through cpal,
//! which switches to loopback when an output device is opened for input). No permission needed.

use super::{AudioError, AudioSink, Backend};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Default)]
struct Loopback {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    failure: Arc<Mutex<Option<String>>>,
}

fn open_stream(sink: Arc<AudioSink>, failure: Arc<Mutex<Option<String>>>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    if supported.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!("unsupported sample format {:?}", supported.sample_format()));
    }
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let rate = config.sample_rate as f32;
    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| sink.push(data, channels, rate),
            move |err| *failure.lock() = Some(err.to_string()),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

impl Backend for Loopback {
    fn start(&mut self, sink: Arc<AudioSink>) -> Result<(), AudioError> {
        self.stop();
        let failure = Arc::new(Mutex::new(None));
        self.failure = failure.clone();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel();
        // cpal streams aren't Send on every platform, so one thread owns it for its lifetime.
        let thread = thread::Builder::new()
            .name("ambilight-audio".into())
            .spawn(move || match open_stream(sink, failure) {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| AudioError::Unavailable(e.to_string()))?;
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {
                self.stop = Some(stop_tx);
                self.thread = Some(thread);
                Ok(())
            }
            Ok(Err(e)) => Err(AudioError::Unavailable(e)),
            Err(_) => Err(AudioError::Unavailable("audio device didn't respond".into())),
        }
    }

    fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.lock().clone()
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(Loopback::default())
}
