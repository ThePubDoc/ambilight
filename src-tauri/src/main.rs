// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod instance;
mod shortcuts;
mod store;
mod updates;
mod windows;

use ambilight_core::capture::DisplayInfo;
use ambilight_core::foreground::{self, RunningApp, StreamingService, STREAMING_SERVICES};
use ambilight_core::layout::{self, Side};
use ambilight_core::serial::{self, PortInfo};
use ambilight_core::{capture, Config, Engine, Status};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub struct AppState {
    engine: Engine,
    pub config: Mutex<Config>,
    store: store::Store,
    pub update: Mutex<Option<updates::UpdateInfo>>,
}

/// Saves `config`, hands it to the engine and tells every window.
pub fn apply_config(app: &AppHandle, config: Config) {
    let config = config.sanitized();
    let state = app.state::<AppState>();
    let shortcuts_changed = std::mem::replace(&mut *state.config.lock(), config.clone()).shortcuts != config.shortcuts;
    state.store.save(&config);
    state.engine.set_config(config.clone());
    if shortcuts_changed {
        shortcuts::sync(app, config.shortcuts);
    }
    let _ = app.emit("config", config);
}

#[derive(Serialize)]
struct Snapshot {
    config: Config,
    status: Status,
    version: &'static str,
    update: Option<updates::UpdateInfo>,
    platform: &'static str,
}

#[tauri::command]
fn get_state(state: State<AppState>) -> Snapshot {
    Snapshot {
        config: state.config.lock().clone(),
        status: state.engine.status(),
        version: env!("CARGO_PKG_VERSION"),
        update: state.update.lock().clone(),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn set_config(app: AppHandle, config: Config) {
    apply_config(&app, config);
}

#[tauri::command]
fn test_white(state: State<AppState>) {
    state.engine.test_white();
}

#[tauri::command]
fn list_displays() -> Vec<DisplayInfo> {
    capture::displays()
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<updates::UpdateInfo>, String> {
    updates::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    updates::install(&app).await
}

#[tauri::command]
fn identify(state: State<AppState>) {
    state.engine.identify();
}

#[tauri::command]
fn reconnect(state: State<AppState>) {
    state.engine.reconnect();
}

#[derive(Serialize)]
struct Dot {
    x: f32,
    y: f32,
}

/// Where each LED sits on the screen edge, in wiring order, for the settings preview.
#[tauri::command]
fn layout_preview(config: Config) -> Vec<Dot> {
    layout::build(&config.sanitized())
        .iter()
        .map(|r| {
            let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
            match r.side {
                Side::Top => Dot { x: cx, y: 0.0 },
                Side::Bottom => Dot { x: cx, y: 1.0 },
                Side::Left => Dot { x: 0.0, y: cy },
                Side::Right => Dot { x: 1.0, y: cy },
            }
        })
        .collect()
}

/// Streaming services that switch Screen mode off while their window is in front.
#[tauri::command]
fn streaming_services() -> &'static [StreamingService] {
    STREAMING_SERVICES
}

/// Apps that can get a profile: the ones running now with windows.
#[tauri::command]
fn running_apps() -> Vec<RunningApp> {
    foreground::running_apps()
}

#[tauri::command]
fn list_ports() -> Vec<PortInfo> {
    serial::available_ports()
}

#[tauri::command]
fn open_permission_settings() {
    capture::open_permission_settings();
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    windows::show_settings(&app);
}

#[tauri::command]
fn fit_popover(app: AppHandle, height: f64) {
    windows::fit_popover(&app, height);
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> bool {
    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };
    if let Err(e) = result {
        log::warn!("autostart: {e}");
    }
    manager.is_enabled().unwrap_or(false)
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Logs to stderr and to `ambilight.log` in the platform log folder (attach it to bug reports).
fn init_logging(app: &tauri::App) {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Ok(dir) = app.path().app_log_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(file) = std::fs::File::create(dir.join("ambilight.log")) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    let _ = builder.try_init();
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| windows::toggle_popover(app)));
    let app = builder
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(shortcuts::plugin())
        .setup(|app| {
            // Before logging starts: a second copy must not truncate the running copy's log.
            #[cfg(unix)]
            if !instance::claim(app.handle(), &app.path().app_cache_dir()?) {
                std::process::exit(0);
            }
            init_logging(app);
            log::info!("Ambilight {} starting", env!("CARGO_PKG_VERSION"));

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let store = store::Store::new(app.path().app_config_dir()?);
            let config = store.load();
            let handle = app.handle().clone();
            let engine = Engine::start(config.clone(), move |status| {
                let _ = handle.emit("status", status);
            });

            // Start at login by default; afterwards the Settings toggle decides.
            // (Skipped in dev builds, which would register the debug binary.)
            if !cfg!(debug_assertions) && store.first_run() {
                let _ = app.autolaunch().enable();
            }

            let shortcuts_on = config.shortcuts;
            app.manage(AppState { engine, config: Mutex::new(config), store, update: Mutex::new(None) });
            shortcuts::sync(app.handle(), shortcuts_on);
            updates::watch(app.handle().clone());
            build_tray(app.handle())?;
            windows::create_popover(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_config,
            identify,
            test_white,
            reconnect,
            list_displays,
            check_update,
            install_update,
            layout_preview,
            streaming_services,
            running_apps,
            list_ports,
            open_permission_settings,
            open_settings,
            fit_popover,
            get_autostart,
            set_autostart,
            quit,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build app");

    app.run(|app, event| match event {
        // Closing the settings window must not quit a tray app; only an explicit exit does.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => app.state::<AppState>().engine.shutdown(),
        _ => {}
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Ambilight", true, None::<&str>)?,
        ],
    )?;

    // macOS draws a monochrome template glyph in the menu bar; Windows shows the color icon.
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Ambilight")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => windows::show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                windows::toggle_popover(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
