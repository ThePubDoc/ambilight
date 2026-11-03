//! One running copy per user: a second launch opens the first copy's popover and exits.
//!
//! macOS/Linux use a Unix socket in the per-user cache folder. tauri-plugin-single-instance
//! uses a fixed path in the shared /tmp there, so another account on the same Mac could
//! create it first and make Ambilight exit at every launch. Windows keeps the plugin,
//! whose mutex and window are per session.

#[cfg(unix)]
pub use unix::claim;

#[cfg(unix)]
mod unix {
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use tauri::AppHandle;

    /// Returns false when another copy is already running (and has been asked to show itself).
    pub fn claim(app: &AppHandle, dir: &Path) -> bool {
        if let Err(e) = std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)))
        {
            log::warn!("single instance: {e}");
            return true;
        }
        let path = dir.join("instance.sock");
        match UnixStream::connect(&path) {
            Ok(_) => return false, // the running copy shows its popover when we connect
            Err(e) if e.kind() == ErrorKind::NotFound || e.kind() == ErrorKind::ConnectionRefused => {
                let _ = std::fs::remove_file(&path); // left over from a copy that didn't exit cleanly
            }
            Err(e) => log::warn!("single instance: {e}"),
        }
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(e) => {
                log::warn!("single instance: {e}");
                return true;
            }
        };
        let app = app.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || crate::windows::toggle_popover(&handle));
            }
        });
        true
    }
}
