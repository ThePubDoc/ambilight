//! Which app and window are in front: for app profiles, and for switching away from the screen
//! modes while a streaming service is watched (they black out copy-protected video whenever the
//! screen is being captured).
//!
//! The front window's title tells us the site: browsers title their windows after the active
//! tab ("Netflix", "… | Prime Video"), and the streaming apps title theirs after the service.

use crate::config::{Config, Effect, Mode};
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
    /// The front app (bundle ID on macOS, lower-case executable name on Windows) and the title
    /// of its front window, when known.
    Window {
        app: Option<String>,
        title: Option<String>,
    },
}

pub fn frontmost() -> Foreground {
    match platform::frontmost() {
        Some((id, _)) if OWN_IDS.iter().any(|own| own.eq_ignore_ascii_case(&id)) => Foreground::Ambilight,
        Some((id, title)) => Foreground::Window { app: (!id.is_empty()).then_some(id), title },
        None => Foreground::Window { app: None, title: None },
    }
}

/// An app the user can make a profile for.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunningApp {
    pub id: String,
    pub name: String,
}

/// Apps with windows, sorted by name, without Ambilight itself.
pub fn running_apps() -> Vec<RunningApp> {
    let mut apps = platform::running_apps();
    apps.retain(|a| !OWN_IDS.iter().any(|own| own.eq_ignore_ascii_case(&a.id)));
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id.eq_ignore_ascii_case(&b.id));
    apps
}

/// Profile for the front app first, then the streaming switch on top of a screen mode.
pub fn resolve<'a>(
    config: &'a Config,
    app: Option<&str>,
    title: Option<&str>,
) -> (Mode, Effect, Option<&'a str>, Option<&'static str>) {
    let profile = app.and_then(|id| config.app_profiles.iter().find(|p| p.app.eq_ignore_ascii_case(id)));
    let (mode, effect) = profile.map_or((config.mode, config.effect), |p| (p.mode, p.effect));
    let streaming = title
        .filter(|_| mode.captures_screen())
        .and_then(|t| matching(&config.auto_switch_services, t))
        .map(|s| s.label);
    match streaming {
        Some(_) => (config.auto_switch_mode, effect, profile.map(|p| p.name.as_str()), streaming),
        None => (mode, effect, profile.map(|p| p.name.as_str()), None),
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
    use super::RunningApp;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
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

    /// Regular apps (ones with a Dock icon).
    pub fn running_apps() -> Vec<RunningApp> {
        objc2::rc::autoreleasepool(|_| {
            NSWorkspace::sharedWorkspace()
                .runningApplications()
                .iter()
                .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
                .filter_map(|a| {
                    let id = a.bundleIdentifier()?.to_string();
                    let name = a.localizedName().map_or_else(|| id.clone(), |n| n.to_string());
                    Some(RunningApp { id, name })
                })
                .collect()
        })
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::RunningApp;
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, GW_OWNER,
    };

    /// Lower-case executable name of the process that owns `window`.
    unsafe fn exe_of(window: HWND) -> Option<String> {
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n);
        let _ = CloseHandle(process);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        Some(path.rsplit('\\').next().unwrap_or(&path).to_lowercase())
    }

    /// Executable name (lower case) and title of the foreground window.
    pub fn frontmost() -> Option<(String, Option<String>)> {
        unsafe {
            let window = GetForegroundWindow();
            let mut text = [0u16; 512];
            let len = GetWindowTextW(window, &mut text);
            let title = (len > 0).then(|| String::from_utf16_lossy(&text[..len as usize]));
            Some((exe_of(window).unwrap_or_default(), title))
        }
    }

    /// Processes that own a visible, titled top-level window.
    pub fn running_apps() -> Vec<RunningApp> {
        unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
            let apps = &mut *(list.0 as *mut Vec<RunningApp>);
            let owned = GetWindow(window, GW_OWNER).map(|o| !o.is_invalid()).unwrap_or(false);
            if IsWindowVisible(window).as_bool() && !owned && GetWindowTextLengthW(window) > 0 {
                if let Some(exe) = exe_of(window) {
                    let stem = exe.trim_end_matches(".exe");
                    let mut name: String = stem.chars().take(1).flat_map(char::to_uppercase).collect();
                    name.push_str(&stem[stem.chars().next().map_or(0, char::len_utf8)..]);
                    apps.push(RunningApp { id: exe.clone(), name });
                }
            }
            BOOL(1)
        }
        let mut apps: Vec<RunningApp> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut apps as *mut _ as isize));
        }
        apps
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::RunningApp;

    pub fn frontmost() -> Option<(String, Option<String>)> {
        None
    }

    pub fn running_apps() -> Vec<RunningApp> {
        Vec::new()
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

    fn with_profile(app: &str, mode: Mode) -> Config {
        Config {
            app_profiles: vec![crate::config::AppProfile {
                app: app.into(),
                name: "Game".into(),
                mode,
                ..Default::default()
            }],
            ..Config::default()
        }
    }

    #[test]
    fn profile_applies_to_its_app_only() {
        let c = with_profile("com.example.game", Mode::ScreenSound);
        assert_eq!(resolve(&c, Some("com.example.game"), None), (Mode::ScreenSound, c.effect, Some("Game"), None));
        assert_eq!(resolve(&c, Some("COM.EXAMPLE.GAME"), None).0, Mode::ScreenSound);
        assert_eq!(resolve(&c, Some("com.other"), None), (Mode::Screen, c.effect, None, None));
        assert_eq!(resolve(&c, None, None).0, Mode::Screen);
    }

    #[test]
    fn streaming_overrides_a_screen_profile() {
        let c = with_profile("com.brave.Browser", Mode::ScreenSound);
        let (mode, _, profile, streaming) = resolve(&c, Some("com.brave.Browser"), Some("Netflix"));
        assert_eq!((mode, profile, streaming), (Mode::Sound, Some("Game"), Some("Netflix")));
    }

    #[test]
    fn streaming_leaves_non_screen_modes_alone() {
        let c = with_profile("com.brave.Browser", Mode::Color);
        assert_eq!(resolve(&c, Some("com.brave.Browser"), Some("Netflix")).0, Mode::Color);
        let plain = Config { mode: Mode::Effect, ..Config::default() };
        assert_eq!(resolve(&plain, None, Some("Netflix")).0, Mode::Effect);
    }

    #[test]
    fn respects_disabled_services() {
        assert!(matching(&["prime".into()], "Netflix").is_none());
        assert!(matching(&[], "Netflix").is_none());
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "diagnostic: lists the apps running on this machine"]
    fn list_running_apps() {
        for app in super::running_apps() {
            println!("{} — {}", app.name, app.id);
        }
        println!("front: {:?}", super::frontmost());
    }
}
