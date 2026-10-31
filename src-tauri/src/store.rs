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
            .and_then(|s| serde_json::from_str(&s).map_err(|e| log::warn!("config.json: {e}")).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config: &Config) {
        match serde_json::to_string_pretty(config) {
            Ok(json) => {
                if let Err(e) = fs::write(self.path(), json) {
                    log::warn!("saving config: {e}");
                }
            }
            Err(e) => log::warn!("encoding config: {e}"),
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
