//! Persistence for Argus under `%APPDATA%\Argus`: `config.json` (rules, preset, favorites,
//! settings; versioned, written atomically) and `history.sqlite` (the alert history).

mod config;
mod csv;
mod history;

pub use config::{Config, ConfigStore, LoadReport, PresetChoice, Settings, CURRENT_VERSION};
pub use csv::{format_utc, sensors_csv, to_csv, SensorRow};
pub use history::{HistoryError, HistoryFilter, HistoryStore, StoredEvent};

use std::path::PathBuf;

/// `%APPDATA%\Argus`, or a sibling of the executable when APPDATA is not set.
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARGUS_DATA_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    std::env::var_os("APPDATA")
        .map(|p| PathBuf::from(p).join("Argus"))
        .unwrap_or_else(|| PathBuf::from("Argus-data"))
}
