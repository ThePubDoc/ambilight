//! macOS system-audio capture with a Core Audio process tap (macOS 14.2+).
//!
//! A private, unmuted tap of everything the Mac plays is wrapped in a private aggregate
//! device, and an IO proc on that device receives the samples. Needs the "System Audio
//! Recording" permission; no microphone is involved.

use super::{AudioError, AudioSink, Backend};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_core_audio::*;
use objc2_core_audio_types::{kAudioFormatFlagIsFloat, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use std::ffi::{c_void, CStr};
use std::mem::size_of;
use std::ptr::{self, NonNull};
use std::sync::Arc;

struct Context {
    sink: Arc<AudioSink>,
    sample_rate: f32,
    /// Channels in the tap's stream, to recognize its buffer.
    channels: u32,
}

#[derive(Default)]
struct ProcessTap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    context: Option<Box<Context>>,
}

// The Core Audio object IDs and the boxed context are only touched from the engine thread
// (start/stop) and, read-only, from the IO proc.
unsafe impl Send for ProcessTap {}

fn check(status: i32, what: &str) -> Result<(), AudioError> {
    if status == 0 {
        Ok(())
    } else {
        Err(AudioError::Unavailable(format!("{what} failed (OSStatus {status})")))
    }
}

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Reads a fixed-size property (plain C data).
unsafe fn property<T: Copy>(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Result<T, i32> {
    let addr = address(selector);
    let mut value: T = std::mem::zeroed();
    let mut size = size_of::<T>() as u32;
    let status = AudioObjectGetPropertyData(
        object,
        NonNull::from(&addr),
        0,
        ptr::null(),
        NonNull::from(&mut size),
        NonNull::from(&mut value).cast(),
    );
    if status == 0 {
        Ok(value)
    } else {
        Err(status)
    }
}

/// UID of the output device the user is listening on.
unsafe fn default_output_uid() -> Result<Retained<NSString>, AudioError> {
    let device: AudioObjectID =
        property(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultSystemOutputDevice)
            .map_err(|s| AudioError::Unavailable(format!("no output device (OSStatus {s})")))?;
    let uid: usize = property(device, kAudioDevicePropertyDeviceUID)
        .map_err(|s| AudioError::Unavailable(format!("output device UID (OSStatus {s})")))?;
    // The property returns a retained CFStringRef, toll-free bridged to NSString.
    Retained::from_raw(uid as *mut NSString).ok_or_else(|| AudioError::Unavailable("output device has no UID".into()))
}

fn key(k: &CStr) -> Retained<NSString> {
    NSString::from_str(k.to_str().unwrap_or_default())
}

fn obj<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(value) }
}

fn dict(entries: Vec<(Retained<NSString>, Retained<AnyObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
    let (keys, values): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
    let keys: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    NSDictionary::from_retained_objects(&keys, &values)
}

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    let context = &*(client as *const Context);
    let list = input.as_ref();
    if list.mNumberBuffers == 0 {
        return 0;
    }
    // The aggregate device lists its sub-device's input streams (say a headset microphone) before
    // the tap's, so the tap is the last buffer: interleaved float32 with the tap's channel count.
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    let buffer = &buffers[buffers.len() - 1];
    if !buffer.mData.is_null() && buffer.mNumberChannels == context.channels {
        let samples = std::slice::from_raw_parts(buffer.mData as *const f32, buffer.mDataByteSize as usize / 4);
        context.sink.push(samples, buffer.mNumberChannels as usize, context.sample_rate);
    }
    0
}

impl ProcessTap {
    unsafe fn open(&mut self, sink: Arc<AudioSink>) -> Result<(), AudioError> {
        // Tap every process's audio (Ambilight makes none), unmuted, invisible to other apps.
        let description = CATapDescription::initStereoGlobalTapButExcludeProcesses(
            CATapDescription::alloc(),
            &NSArray::<NSNumber>::new(),
        );
        description.setName(&NSString::from_str("Ambilight"));
        description.setPrivate(true);
        description.setMuteBehavior(CATapMuteBehavior::Unmuted);

        let status = AudioHardwareCreateProcessTap(Some(&description), &mut self.tap);
        if status != 0 {
            log::warn!("AudioHardwareCreateProcessTap: OSStatus {status}");
            return Err(AudioError::PermissionDenied);
        }

        let output_uid = default_output_uid()?;
        let tap_uid = description.UUID().UUIDString();
        let sub_device = dict(vec![(key(kAudioSubDeviceUIDKey), obj(output_uid.clone()))]);
        let sub_tap = dict(vec![
            (key(kAudioSubTapUIDKey), obj(tap_uid)),
            (key(kAudioSubTapDriftCompensationKey), obj(NSNumber::new_bool(true))),
        ]);
        let aggregate = dict(vec![
            (key(kAudioAggregateDeviceNameKey), obj(NSString::from_str("Ambilight audio tap"))),
            (key(kAudioAggregateDeviceUIDKey), obj(NSUUID::new().UUIDString())),
            (key(kAudioAggregateDeviceMainSubDeviceKey), obj(output_uid)),
            (key(kAudioAggregateDeviceIsPrivateKey), obj(NSNumber::new_bool(true))),
            (key(kAudioAggregateDeviceIsStackedKey), obj(NSNumber::new_bool(false))),
            (key(kAudioAggregateDeviceTapAutoStartKey), obj(NSNumber::new_bool(true))),
            (key(kAudioAggregateDeviceSubDeviceListKey), obj(NSArray::from_retained_slice(&[sub_device]))),
            (key(kAudioAggregateDeviceTapListKey), obj(NSArray::from_retained_slice(&[sub_tap]))),
        ]);
        // NSDictionary is toll-free bridged to CFDictionary.
        let cf: &CFDictionary = &*(Retained::as_ptr(&aggregate) as *const CFDictionary);
        check(
            AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut self.aggregate)),
            "creating the audio tap device",
        )?;

        let format: AudioStreamBasicDescription = property(self.tap, kAudioTapPropertyFormat)
            .map_err(|s| AudioError::Unavailable(format!("tap format (OSStatus {s})")))?;
        if format.mFormatFlags & kAudioFormatFlagIsFloat == 0 || format.mBitsPerChannel != 32 {
            return Err(AudioError::Unavailable(format!(
                "unexpected tap format ({} bit, flags {:#x})",
                format.mBitsPerChannel, format.mFormatFlags
            )));
        }

        let context =
            Box::new(Context { sink, sample_rate: format.mSampleRate as f32, channels: format.mChannelsPerFrame });
        let client = &*context as *const Context as *mut c_void;
        self.context = Some(context);
        check(
            AudioDeviceCreateIOProcID(self.aggregate, Some(io_proc), client, NonNull::from(&mut self.proc_id)),
            "creating the audio callback",
        )?;
        check(AudioDeviceStart(self.aggregate, self.proc_id), "starting audio capture")?;
        log::info!("system audio tap running ({} Hz, {} ch)", format.mSampleRate, format.mChannelsPerFrame);
        Ok(())
    }

    unsafe fn close(&mut self) {
        if self.proc_id.is_some() {
            AudioDeviceStop(self.aggregate, self.proc_id);
            AudioDeviceDestroyIOProcID(self.aggregate, self.proc_id);
            self.proc_id = None;
        }
        if self.aggregate != 0 {
            AudioHardwareDestroyAggregateDevice(self.aggregate);
            self.aggregate = 0;
        }
        if self.tap != 0 {
            AudioHardwareDestroyProcessTap(self.tap);
            self.tap = 0;
        }
        self.context = None; // after the IO proc is gone
    }
}

impl Backend for ProcessTap {
    fn start(&mut self, sink: Arc<AudioSink>) -> Result<(), AudioError> {
        self.stop();
        objc2::rc::autoreleasepool(|_| unsafe {
            let result = self.open(sink);
            if result.is_err() {
                self.close();
            }
            result
        })
    }

    fn stop(&mut self) {
        unsafe { self.close() }
    }

    fn failure(&self) -> Option<String> {
        None
    }
}

impl Drop for ProcessTap {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(ProcessTap::default())
}
