/// Every command the UI may call. Each window is granted only the ones it uses, in
/// `capabilities/` (`allow-<command>`, with `_` written as `-`).
const COMMANDS: &[&str] = &[
    "get_state",
    "set_config",
    "identify",
    "test_white",
    "reconnect",
    "list_displays",
    "check_update",
    "install_update",
    "layout_preview",
    "streaming_services",
    "running_apps",
    "list_ports",
    "open_permission_settings",
    "open_settings",
    "fit_popover",
    "get_autostart",
    "set_autostart",
    "quit",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).expect("failed to run tauri-build");
}
