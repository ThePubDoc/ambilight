//! System-wide keyboard shortcuts: ⌃⌥L / Ctrl+Alt+L toggles the lights, M cycles modes,
//! ↑ and ↓ change brightness.

use crate::{apply_config, AppState};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const MODIFIERS: Modifiers = Modifiers::CONTROL.union(Modifiers::ALT);

fn all() -> [Shortcut; 4] {
    [
        Shortcut::new(Some(MODIFIERS), Code::KeyL),
        Shortcut::new(Some(MODIFIERS), Code::KeyM),
        Shortcut::new(Some(MODIFIERS), Code::ArrowUp),
        Shortcut::new(Some(MODIFIERS), Code::ArrowDown),
    ]
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                handle(app, shortcut.key);
            }
        })
        .build()
}

fn handle(app: &AppHandle, key: Code) {
    let mut config = app.state::<AppState>().config.lock().clone();
    match key {
        Code::KeyL => config.enabled = !config.enabled,
        Code::KeyM => {
            config.enabled = true;
            config.mode = config.mode.next();
        }
        Code::ArrowUp => config.brightness = (config.brightness + 0.1).min(1.0),
        Code::ArrowDown => config.brightness = (config.brightness - 0.1).max(0.05),
        _ => return,
    }
    apply_config(app, config);
}

/// Registers or removes the shortcuts to match the setting.
pub fn sync(app: &AppHandle, enabled: bool) {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    if enabled {
        for shortcut in all() {
            if let Err(e) = manager.register(shortcut) {
                log::warn!("shortcut {shortcut}: {e}");
            }
        }
    }
}
