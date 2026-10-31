//! Which window is in front, for switching away from Screen mode while a streaming service is
//! watched: they black out copy-protected video whenever the screen is being captured.
//!
//! The front window's title tells us the site: browsers title their windows after the active
//! tab ("Netflix", "… | Prime Video"), and the streaming apps title theirs after the service.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StreamingService {
    pub key: &'static str,
    pub label: &'static str,
    /// Case-insensitive substrings of a window title that identify the service.
    #[serde(skip)]
    patterns: &'static [&'static str],
}

pub const STREAMING_SERVICES: &[StreamingService] = &[
    StreamingService { key: "netflix", label: "Netflix", patterns: &["netflix"] },
    StreamingService { key: "prime", label: "Prime Video", patterns: &["prime video"] },
    StreamingService { key: "disney", label: "Disney+", patterns: &["disney+"] },
    StreamingService { key: "hotstar", label: "JioHotstar", patterns: &["hotstar", "jiocinema"] },
    StreamingService { key: "appletv", label: "Apple TV+", patterns: &["apple tv"] },
    StreamingService { key: "hulu", label: "Hulu", patterns: &["hulu"] },
    StreamingService { key: "paramount", label: "Paramount+", patterns: &["paramount+"] },
    StreamingService { key: "peacock", label: "Peacock", patterns: &["peacock"] },
    StreamingService { key: "sonyliv", label: "SonyLIV", patterns: &["sonyliv"] },
    StreamingService { key: "zee5", label: "ZEE5", patterns: &["zee5"] },
    StreamingService { key: "crunchyroll", label: "Crunchyroll", patterns: &["crunchyroll"] },
];

/// Identifiers of Ambilight itself; when it's in front (popover, settings) nothing changes.
const OWN_IDS: &[&str] = &["com.thepubdoc.ambilight", "ambilight.exe"];

#[derive(Debug, Clone, PartialEq)]
pub enum Foreground {
    Ambilight,
    /// Title of the front window, when it has one.
    Window(Option<String>),
}

pub fn frontmost() -> Foreground {
    match platform::frontmost() {
        Some((id, _)) if OWN_IDS.iter().any(|own| own.eq_ignore_ascii_case(&id)) => Foreground::Ambilight,
        Some((_, title)) => Foreground::Window(title),
        None => Foreground::Window(None),
    }
}

/// The enabled service whose name appears in `title`, if any.
pub fn matching(enabled: &[String], title: &str) -> Option<&'static StreamingService> {
    let title = title.to_lowercase();
    STREAMING_SERVICES
        .iter()
        .filter(|s| enabled.iter().any(|k| k == s.key))
        .find(|s| s.patterns.iter().any(|p| title.contains(p)))
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut c_void;
    }
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;

    /// Bundle ID of the frontmost app and the title of its frontmost window. Window titles of
    /// other apps are only visible with the Screen Recording permission, which Ambilight has.
    pub fn frontmost() -> Option<(String, Option<String>)> {
        objc2::rc::autoreleasepool(|_| {
            let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
            let id = app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default();
            let pid = app.processIdentifier();

            let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0) };
            // CFArray of CFDictionary, toll-free bridged; returned retained ("Copy").
            let windows: Retained<NSArray<NSDictionary<NSString, AnyObject>>> =
                unsafe { Retained::from_raw(raw.cast())? };
            let key_pid = NSString::from_str("kCGWindowOwnerPID");
            let key_layer = NSString::from_str("kCGWindowLayer");
            let key_name = NSString::from_str("kCGWindowName");
            let number = |w: &NSDictionary<NSString, AnyObject>, k: &NSString| {
                w.objectForKey(k)?.downcast::<NSNumber>().ok().map(|n| n.intValue())
            };
            // Front-to-back order: the first normal window of the app is the one being used.
            let title = windows.iter().find_map(|w| {
                if number(&w, &key_pid)? != pid || number(&w, &key_layer)? != 0 {
                    return None;
                }
                let name = w.objectForKey(&key_name)?.downcast::<NSString>().ok()?.to_string();
                (!name.is_empty()).then_some(name)
            });
            Some((id, title))
        })
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

    /// Executable name (lower case) and title of the foreground window.
    pub fn frontmost() -> Option<(String, Option<String>)> {
        unsafe {
            let window = GetForegroundWindow();
            let mut text = [0u16; 512];
            let len = GetWindowTextW(window, &mut text);
            let title = (len > 0).then(|| String::from_utf16_lossy(&text[..len as usize]));

            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            let mut exe = String::new();
            if pid != 0 {
                if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    let mut buf = [0u16; 1024];
                    let mut n = buf.len() as u32;
                    if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok()
                    {
                        let path = String::from_utf16_lossy(&buf[..n as usize]);
                        exe = path.rsplit('\\').next().unwrap_or(&path).to_lowercase();
                    }
                    let _ = CloseHandle(process);
                }
            }
            Some((exe, title))
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn frontmost() -> Option<(String, Option<String>)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        STREAMING_SERVICES.iter().map(|s| s.key.to_string()).collect()
    }

    #[test]
    fn recognizes_streaming_titles() {
        let label = |t: &str| matching(&all(), t).map(|s| s.label);
        assert_eq!(label("Netflix"), Some("Netflix"));
        assert_eq!(label("Watch The Boys - Season 4 | Prime Video"), Some("Prime Video"));
        assert_eq!(label("Loki | Disney+"), Some("Disney+"));
        assert_eq!(label("Never Gonna Give You Up - YouTube"), None);
        assert_eq!(label("ambilight — README.md"), None);
    }

    #[test]
    fn respects_disabled_services() {
        assert!(matching(&["prime".into()], "Netflix").is_none());
        assert!(matching(&[], "Netflix").is_none());
    }
}
