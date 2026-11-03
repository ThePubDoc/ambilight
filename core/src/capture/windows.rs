//! Windows capture through DXGI Desktop Duplication.
//!
//! Each new desktop image is copied into a mipmapped texture on the GPU, the GPU builds the
//! mip chain, and only a small mip level (~256–512 px wide) is read back to the CPU.

use super::{Backend, CaptureError, DisplayInfo, FrameSink};
use crate::frame::Frame;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows::core::{Interface, Result as WinResult};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const TARGET_WIDTH: u32 = 256;

/// Every output of every adapter, in a stable order; display ids are 1-based indexes into it.
fn outputs() -> Vec<(IDXGIAdapter1, IDXGIOutput, DXGI_OUTPUT_DESC)> {
    let mut list = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return list };
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                if let Ok(desc) = output.GetDesc() {
                    if desc.AttachedToDesktop.as_bool() {
                        list.push((adapter.clone(), output, desc));
                    }
                }
                o += 1;
            }
            a += 1;
        }
    }
    list
}

/// The primary display sits at the desktop origin.
fn is_main(desc: &DXGI_OUTPUT_DESC) -> bool {
    desc.DesktopCoordinates.left == 0 && desc.DesktopCoordinates.top == 0
}

pub fn displays() -> Vec<DisplayInfo> {
    outputs()
        .iter()
        .enumerate()
        .map(|(i, (_, _, desc))| {
            let r = desc.DesktopCoordinates;
            DisplayInfo {
                id: i as u32 + 1,
                label: format!("Display {} ({} × {})", i + 1, r.right - r.left, r.bottom - r.top),
                main: is_main(desc),
            }
        })
        .collect()
}

#[derive(Default)]
struct DxgiCapture {
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Backend for DxgiCapture {
    fn start(&mut self, sink: Arc<FrameSink>, display: u32) -> Result<(), CaptureError> {
        self.stop();
        // Fail fast (and report why) if duplication isn't possible at all.
        let duplicator = Duplicator::new(display).map_err(|e| CaptureError::Unavailable(e.message().to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(Mutex::new(None));
        self.stop = stop.clone();
        self.failure = failure.clone();
        self.thread = Some(
            thread::Builder::new()
                .name("ambilight-capture".into())
                .spawn(move || capture_loop(duplicator, display, sink, stop, failure))
                .map_err(|e| CaptureError::Unavailable(e.to_string()))?,
        );
        Ok(())
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.lock().clone()
    }
}

fn capture_loop(
    mut dup: Duplicator,
    display: u32,
    sink: Arc<FrameSink>,
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
) {
    let mut lost_since: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        match dup.next_frame(&sink) {
            Ok(()) => lost_since = None,
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST || e.code() == DXGI_ERROR_INVALID_CALL => {
                // Mode change, UAC prompt, lock screen…: rebuild the duplication.
                let since = *lost_since.get_or_insert(started);
                thread::sleep(Duration::from_millis(250));
                match Duplicator::new(display) {
                    Ok(d) => dup = d,
                    Err(e) if since.elapsed() > Duration::from_secs(30) => {
                        *failure.lock() = Some(e.message().to_string());
                        return;
                    }
                    Err(_) => {}
                }
                continue;
            }
            Err(e) => {
                *failure.lock() = Some(e.message().to_string());
                return;
            }
        }
        thread::sleep(FRAME_INTERVAL.saturating_sub(started.elapsed()));
    }
}

struct Duplicator {
    _device: ID3D11Device, // keeps the D3D device alive with its textures
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    mip_texture: ID3D11Texture2D,
    mip_view: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
    mip_level: u32,
    width: u32,
    height: u32,
}

impl Duplicator {
    /// Duplicates `display` (1-based id from [`displays`]; 0 or unknown means the main display).
    fn new(display: u32) -> WinResult<Self> {
        unsafe {
            let mut outputs = outputs();
            let index = (display as usize)
                .checked_sub(1)
                .filter(|&i| i < outputs.len())
                .or_else(|| outputs.iter().position(|(_, _, d)| is_main(d)))
                .unwrap_or(0);
            if outputs.is_empty() {
                return Err(windows::core::Error::from(DXGI_ERROR_NOT_FOUND));
            }
            let (adapter, output, _) = outputs.swap_remove(index);

            // The device must live on the adapter that drives the display.
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device: ID3D11Device = device.unwrap();
            let context = context.unwrap();

            let output: IDXGIOutput1 = output.cast()?;
            let duplication = output.DuplicateOutput(&device)?;
            let desc = duplication.GetDesc();
            let (width, height) = (desc.ModeDesc.Width, desc.ModeDesc.Height);

            let mut mip_level = 0;
            while (width >> (mip_level + 1)) >= TARGET_WIDTH {
                mip_level += 1;
            }

            let mip_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: mip_level + 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
            };
            let mut mip_texture = None;
            device.CreateTexture2D(&mip_desc, None, Some(&mut mip_texture))?;
            let mip_texture = mip_texture.unwrap();
            let mut mip_view = None;
            device.CreateShaderResourceView(&mip_texture, None, Some(&mut mip_view))?;

            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: (width >> mip_level).max(1),
                Height: (height >> mip_level).max(1),
                MipLevels: 1,
                BindFlags: 0,
                MiscFlags: 0,
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..mip_desc
            };
            let mut staging = None;
            device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;

            Ok(Duplicator {
                _device: device,
                context,
                duplication,
                mip_texture,
                mip_view: mip_view.unwrap(),
                staging: staging.unwrap(),
                mip_level,
                width,
                height,
            })
        }
    }

    fn next_frame(&mut self, sink: &FrameSink) -> WinResult<()> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            if let Err(e) = self.duplication.AcquireNextFrame(100, &mut info, &mut resource) {
                return if e.code() == DXGI_ERROR_WAIT_TIMEOUT { Ok(()) } else { Err(e) };
            }
            // LastPresentTime == 0 means only the mouse moved.
            let result = if info.LastPresentTime != 0 {
                match resource {
                    Some(r) => self.read(&r.cast::<ID3D11Texture2D>()?, sink),
                    None => Ok(()),
                }
            } else {
                Ok(())
            };
            self.duplication.ReleaseFrame()?;
            result
        }
    }

    unsafe fn read(&self, desktop: &ID3D11Texture2D, sink: &FrameSink) -> WinResult<()> {
        self.context.CopySubresourceRegion(&self.mip_texture, 0, 0, 0, 0, desktop, 0, None);
        self.context.GenerateMips(&self.mip_view);
        self.context.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, &self.mip_texture, self.mip_level, None);

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        self.context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let w = (self.width >> self.mip_level).max(1) as usize;
        let h = (self.height >> self.mip_level).max(1) as usize;
        let stride = mapped.RowPitch as usize;
        if !mapped.pData.is_null() && stride >= w * 4 {
            // D3D11 guarantees every row but the last is RowPitch long; the last only w * 4 bytes.
            let data = std::slice::from_raw_parts(mapped.pData as *const u8, (h - 1) * stride + w * 4);
            sink.push(&Frame { width: w, height: h, stride, data });
        }
        self.context.Unmap(&self.staging, 0);
        Ok(())
    }
}

pub fn new_backend() -> Box<dyn Backend> {
    Box::new(DxgiCapture::default())
}

pub fn open_permission_settings() {}

pub fn display_asleep(_display: u32) -> bool {
    false
}
