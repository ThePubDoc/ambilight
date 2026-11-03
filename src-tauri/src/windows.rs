//! The tray popover and the settings window.

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};

const POPOVER: &str = "popover";
const SETTINGS: &str = "settings";
const POPOVER_WIDTH: f64 = 330.0;

#[cfg(target_os = "macos")]
const TRAY_POSITION: Position = Position::TrayCenter;
#[cfg(not(target_os = "macos"))]
const TRAY_POSITION: Position = Position::TrayBottomCenter;

/// The windows can call native commands, so they may only show the bundled pages
/// (`tauri://localhost` on macOS and Linux, `http(s)://tauri.localhost` on Windows).
fn is_app_page(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

pub fn create_popover(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, POPOVER, WebviewUrl::App("popover.html".into()))
        .on_navigation(is_app_page)
        .title("Ambilight")
        .inner_size(POPOVER_WIDTH, 300.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .shadow(true)
        .build()?;
    apply_material(&window);

    let w = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let _ = w.hide();
        }
    });
    Ok(())
}

pub fn toggle_popover(app: &AppHandle) {
    let Some(window) = app.get_webview_window(POPOVER) else { return };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let _ = window.move_window_constrained(TRAY_POSITION).or_else(|_| window.move_window(TRAY_POSITION));
    let _ = window.show();
    let _ = window.set_focus();
}

/// The popover's content decides its height (the color swatches come and go).
pub fn fit_popover(app: &AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(POPOVER) else { return };
    let _ = window.set_size(LogicalSize::new(POPOVER_WIDTH, height.clamp(120.0, 600.0)));
    if window.is_visible().unwrap_or(false) {
        // Keep it attached to the tray icon (matters when the tray is at the bottom).
        let _ = window.move_window_constrained(TRAY_POSITION);
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(popover) = app.get_webview_window(POPOVER) {
        let _ = popover.hide();
    }
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .on_navigation(is_app_page)
        .title("Ambilight Settings")
        .inner_size(500.0, 720.0)
        .min_inner_size(440.0, 400.0)
        .center()
        .focused(true)
        .build();
    match built {
        Ok(window) => {
            #[cfg(target_os = "macos")]
            let _ = app.show(); // bring the (Dock-less) app forward
            let _ = window.set_focus();
        }
        Err(e) => log::warn!("settings window: {e}"),
    }
}

fn apply_material(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = apply_vibrancy(window, NSVisualEffectMaterial::Popover, Some(NSVisualEffectState::Active), Some(12.0));
    }
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_mica};
        if apply_mica(window, None).is_err() {
            let _ = apply_acrylic(window, Some((32, 32, 32, 160)));
        }
    }
}
