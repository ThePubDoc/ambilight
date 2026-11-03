//! Config persisted as JSON in the platform config directory.

use ambilight_core::Config;
use std::fs;
use std::path::PathBuf;

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Store { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    pub fn load(&self) -> Config {
        fs::read_to_string(self.path())
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).map_err(|e| log::warn!("config.json: {e}")).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Writes a temporary file and renames it over config.json, so a crash mid-write can't leave a
    /// truncated file (which would load as the defaults).
    pub fn save(&self, config: &Config) {
        let json = match serde_json::to_string_pretty(config) {
            Ok(json) => json,
            Err(e) => return log::warn!("encoding config: {e}"),
        };
        let tmp = self.dir.join("config.json.tmp");
        if let Err(e) = fs::write(&tmp, json).and_then(|_| fs::rename(&tmp, self.path())) {
            log::warn!("saving config: {e}");
        }
    }

    /// True exactly once per installation.
    pub fn first_run(&self) -> bool {
        let marker = self.dir.join(".initialized");
        if marker.exists() {
            return false;
        }
        let _ = fs::write(marker, "");
        true
    }
}
