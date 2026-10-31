//! Update checks against the latest GitHub release (signed with the project's updater key).

use crate::AppState;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let info = update.map(|u| UpdateInfo { version: u.version, notes: u.body });
    *app.state::<AppState>().update.lock() = info.clone();
    if let Some(info) = &info {
        log::info!("update available: {}", info.version);
        let _ = app.emit("update", info);
    }
    Ok(info)
}

pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Already up to date")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    log::info!("installed {}; restarting", update.version);
    app.restart();
}

/// Checks shortly after launch and then every six hours, while automatic updates are on.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(Duration::from_secs(20)).await;
        loop {
            let enabled = app.state::<AppState>().config.lock().auto_update;
            if enabled && !cfg!(debug_assertions) {
                if let Err(e) = check(&app).await {
                    log::info!("update check failed: {e}");
                }
            }
            tokio_sleep(Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}
