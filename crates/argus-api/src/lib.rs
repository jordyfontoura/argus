//! The contract between the backend and the UI, defined once, in Rust.
//!
//! Commands are invoked by the UI (`invoke(name, args)`); events are emitted by the backend
//! (`listen(name, handler)`). Payload types are exported to TypeScript under `ui/generated/`
//! and a test fails when the committed files drift from these definitions.
//! Regenerate with `ARGUS_BLESS=1 cargo test -p argus-api`.

use argus_alerts::{AlertEvent, EventKind, RuleStatus, Severity};
use argus_core::{Availability, Reading, SensorId, SensorMeta, Threshold};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Command names (UI -> backend).
pub mod commands {
    pub const ACTIVATE_CPU_MEASUREMENTS: &str = "activate_cpu_measurements";
    pub const GET_STATIC_INFO: &str = "get_static_info";
    pub const GET_CPU_DETAILS: &str = "get_cpu_details";
    pub const GET_GPU_DETAILS: &str = "get_gpu_details";
    pub const GET_MEMORY_DETAILS: &str = "get_memory_details";
    pub const GET_NETWORK_ADAPTERS: &str = "get_network_adapters";
    pub const LIST_RULES: &str = "list_rules";
    pub const SAVE_RULE: &str = "save_rule";
    pub const DELETE_RULE: &str = "delete_rule";
    pub const ACKNOWLEDGE_RULE: &str = "acknowledge_rule";
    pub const SIMULATE_RULE: &str = "simulate_rule";
    pub const SET_MUTE: &str = "set_mute";
    pub const GET_ALERTS_STATE: &str = "get_alerts_state";
    pub const SET_CRITICAL_BYPASS: &str = "set_critical_bypass";
    pub const LIST_PRESETS: &str = "list_presets";
    pub const PREVIEW_PRESET: &str = "preview_preset";
    pub const APPLY_PRESET: &str = "apply_preset";
    pub const QUERY_HISTORY: &str = "query_history";
    pub const EXPORT_HISTORY: &str = "export_history";
    pub const CLEAR_HISTORY: &str = "clear_history";
    pub const SET_HISTORY_RETENTION: &str = "set_history_retention";
    pub const GET_BACKGROUND_STATE: &str = "get_background_state";
    pub const SET_BACKGROUND: &str = "set_background";
    pub const GET_SESSION_RANGES: &str = "get_session_ranges";
    pub const RESET_SESSION_RANGES: &str = "reset_session_ranges";
    pub const GET_FAVORITES: &str = "get_favorites";
    pub const SET_FAVORITE: &str = "set_favorite";
    pub const LIST_PROCESSES: &str = "list_processes";
    pub const GET_DISKS: &str = "get_disks";
    pub const GET_VOLUMES: &str = "get_volumes";
    pub const GET_SYSTEM_DETAILS: &str = "get_system_details";
    pub const GET_REPORT: &str = "get_report";
    pub const EXPORT_SENSOR_CSV: &str = "export_sensor_csv";
    pub const GET_SETTINGS: &str = "get_settings";
    pub const SET_SETTINGS: &str = "set_settings";
    pub const LIST_SENSORS: &str = "list_sensors";
    pub const GET_HISTORY: &str = "get_history";
    pub const SET_SAMPLE_INTERVAL: &str = "set_sample_interval";
}

/// Event names (backend -> UI).
pub mod events {
    pub const SENSORS_TICK: &str = "sensors:tick";
    pub const ALERTS_STATE: &str = "alerts:state";
    pub const ALERTS_EVENT: &str = "alerts:event";
    pub const TOAST: &str = "toast";
}

/// Error returned by every command; the UI shows `message` and keeps working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

/// A catalog entry returned by `list_sensors`: what the sensor is and its warn/crit limits.
/// The limits come from the backend's `ThresholdRegistry`; the UI keeps no copy of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SensorInfo {
    pub meta: SensorMeta,
    pub threshold: Option<Threshold>,
}

/// Live state of every rule plus the mute, pushed on `alerts:state` and returned by
/// `get_alerts_state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AlertsState {
    pub statuses: Vec<RuleStatus>,
    /// Notifications are muted until this time (ms since epoch); 0 or in the past means not muted.
    pub muted_until_ms: i64,
    pub critical_bypasses_mute: bool,
    /// Rules firing right now (drives the badge in the menu).
    pub active_count: u32,
    /// The protection preset applied last, if any, and its sensitivity.
    pub preset_label: Option<String>,
    pub preset_sensitivity: Option<String>,
    /// Real (non-test) events in the history, for the counter in the menu.
    pub history_count: u64,
}

/// What to show in the History screen. Empty `kinds` means every kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct HistoryQuery {
    pub severity: Option<Severity>,
    pub rule_id: Option<u64>,
    pub kinds: Vec<EventKind>,
    pub include_tests: bool,
    pub limit: Option<u32>,
}

/// Answer to `query_history`: the events (newest first) and the counts for the summary line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct HistoryPage {
    pub events: Vec<AlertEvent>,
    /// Events matching the query (before `limit`).
    pub matching: u64,
    /// All events visible without the filters (tests hidden unless asked for).
    pub total: u64,
    /// Critical rules that fired among the matching events.
    pub critical_fired: u64,
    pub retention_days: u32,
}

/// How the app behaves in the background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BackgroundState {
    /// Starts with Windows (the real state of the Run entry, not just a saved flag).
    pub autostart: bool,
    /// Closing the window hides it to the tray; collection and alerts keep running.
    pub minimize_to_tray: bool,
}

/// Minimum and maximum of a sensor since the app started or the user reset them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SessionRange {
    pub sensor: SensorId,
    pub min: f64,
    pub max: f64,
}

/// The settings the Settings screen controls. Units are display preferences only: the backend
/// keeps canonical units (°C, bytes) and the UI converts when it formats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AppSettings {
    /// Time between collection cycles, in milliseconds (500, 1000, 2000 or 5000).
    pub sample_interval_ms: u64,
    /// "C" or "F".
    pub temperature_unit: String,
    /// "GB" (1000-based) or "GiB" (1024-based).
    pub memory_unit: String,
    pub light_theme: bool,
    pub history_retention_days: u32,
}

/// Payload of the `toast` event: something to show on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ToastPayload {
    pub event: AlertEvent,
    /// Text for the title line, already prefixed with "[teste]" for simulations.
    pub title: String,
    pub body: String,
}

/// Payload of the `sensors:tick` event: every reading produced by one collection cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TickBatch {
    pub readings: Vec<Reading>,
}

/// One point of a sensor's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SamplePoint {
    pub ts_ms: i64,
    pub value: Availability,
}

/// Result of `get_history` for one sensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SensorHistory {
    pub sensor: SensorId,
    pub samples: Vec<SamplePoint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::{
        CpuDetails, Field, GpuDetails, MemoryDetails, NetworkAdapter, SensorMeta, StaticInfo, Unit,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use ts_rs::Config;

    fn committed_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/generated")
    }

    fn generate(dir: &Path) {
        let cfg = Config::new().with_out_dir(dir).with_large_int("number");
        ApiError::export_all(&cfg).unwrap();
        TickBatch::export_all(&cfg).unwrap();
        SensorHistory::export_all(&cfg).unwrap();
        SensorInfo::export_all(&cfg).unwrap();
        AlertsState::export_all(&cfg).unwrap();
        HistoryQuery::export_all(&cfg).unwrap();
        HistoryPage::export_all(&cfg).unwrap();
        BackgroundState::export_all(&cfg).unwrap();
        AppSettings::export_all(&cfg).unwrap();
        SessionRange::export_all(&cfg).unwrap();
        argus_core::ProcessRow::export_all(&cfg).unwrap();
        argus_core::DiskInfo::export_all(&cfg).unwrap();
        argus_core::VolumeInfo::export_all(&cfg).unwrap();
        argus_core::SystemDetails::export_all(&cfg).unwrap();
        argus_alerts::PresetInfo::export_all(&cfg).unwrap();
        argus_alerts::PlannedRule::export_all(&cfg).unwrap();
        argus_alerts::Sensitivity::export_all(&cfg).unwrap();
        ToastPayload::export_all(&cfg).unwrap();
        argus_alerts::Rule::export_all(&cfg).unwrap();
        argus_alerts::SimResult::export_all(&cfg).unwrap();
        StaticInfo::export_all(&cfg).unwrap();
        CpuDetails::export_all(&cfg).unwrap();
        GpuDetails::export_all(&cfg).unwrap();
        MemoryDetails::export_all(&cfg).unwrap();
        NetworkAdapter::export_all(&cfg).unwrap();
        SensorMeta::export_all(&cfg).unwrap();
        // The command and event names are part of the contract too.
        let names = format!(
            "// Generated by argus-api. Do not edit.\nexport const COMMANDS = {{\n  activateCpuMeasurements: \"{}\",\n  getStaticInfo: \"{}\",\n  getCpuDetails: \"{}\",\n  getGpuDetails: \"{}\",\n  getMemoryDetails: \"{}\",\n  getNetworkAdapters: \"{}\",\n  listRules: \"{}\",\n  saveRule: \"{}\",\n  deleteRule: \"{}\",\n  acknowledgeRule: \"{}\",\n  simulateRule: \"{}\",\n  setMute: \"{}\",\n  getAlertsState: \"{}\",\n  setCriticalBypass: \"{}\",\n  listPresets: \"{}\",\n  previewPreset: \"{}\",\n  applyPreset: \"{}\",\n  queryHistory: \"{}\",\n  exportHistory: \"{}\",\n  clearHistory: \"{}\",\n  setHistoryRetention: \"{}\",\n  getBackgroundState: \"{}\",\n  setBackground: \"{}\",\n  getSessionRanges: \"{}\",\n  resetSessionRanges: \"{}\",\n  getFavorites: \"{}\",\n  setFavorite: \"{}\",\n  listProcesses: \"{}\",\n  getDisks: \"{}\",\n  getVolumes: \"{}\",\n  getSystemDetails: \"{}\",\n  getReport: \"{}\",\n  exportSensorCsv: \"{}\",\n  getSettings: \"{}\",\n  setSettings: \"{}\",\n  listSensors: \"{}\",\n  getHistory: \"{}\",\n  setSampleInterval: \"{}\",\n}};\nexport const EVENTS = {{\n  sensorsTick: \"{}\",\n  alertsState: \"{}\",\n  alertsEvent: \"{}\",\n  toast: \"{}\",\n}};\n",
            commands::ACTIVATE_CPU_MEASUREMENTS,
            commands::GET_STATIC_INFO,
            commands::GET_CPU_DETAILS,
            commands::GET_GPU_DETAILS,
            commands::GET_MEMORY_DETAILS,
            commands::GET_NETWORK_ADAPTERS,
            commands::LIST_RULES,
            commands::SAVE_RULE,
            commands::DELETE_RULE,
            commands::ACKNOWLEDGE_RULE,
            commands::SIMULATE_RULE,
            commands::SET_MUTE,
            commands::GET_ALERTS_STATE,
            commands::SET_CRITICAL_BYPASS,
            commands::LIST_PRESETS,
            commands::PREVIEW_PRESET,
            commands::APPLY_PRESET,
            commands::QUERY_HISTORY,
            commands::EXPORT_HISTORY,
            commands::CLEAR_HISTORY,
            commands::SET_HISTORY_RETENTION,
            commands::GET_BACKGROUND_STATE,
            commands::SET_BACKGROUND,
            commands::GET_SESSION_RANGES,
            commands::RESET_SESSION_RANGES,
            commands::GET_FAVORITES,
            commands::SET_FAVORITE,
            commands::LIST_PROCESSES,
            commands::GET_DISKS,
            commands::GET_VOLUMES,
            commands::GET_SYSTEM_DETAILS,
            commands::GET_REPORT,
            commands::EXPORT_SENSOR_CSV,
            commands::GET_SETTINGS,
            commands::SET_SETTINGS,
            commands::LIST_SENSORS,
            commands::GET_HISTORY,
            commands::SET_SAMPLE_INTERVAL,
            events::SENSORS_TICK,
            events::ALERTS_STATE,
            events::ALERTS_EVENT,
            events::TOAST
        );
        fs::write(dir.join("names.js"), names).unwrap();
    }

    fn read_all(dir: &Path) -> Vec<(String, String)> {
        let mut files: Vec<(String, String)> = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "ts" || x == "js"))
            .map(|e| {
                let text = fs::read_to_string(e.path()).unwrap().replace("\r\n", "\n");
                (e.file_name().to_string_lossy().to_string(), text)
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn generated_typescript_matches_the_committed_files() {
        let tmp = std::env::temp_dir().join(format!("argus-api-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        generate(&tmp);

        let committed = committed_dir();
        if std::env::var("ARGUS_BLESS").is_ok_and(|v| v == "1") {
            let _ = fs::remove_dir_all(&committed);
            fs::create_dir_all(&committed).unwrap();
            for (name, text) in read_all(&tmp) {
                fs::write(committed.join(name), text).unwrap();
            }
        }
        let expected = read_all(&tmp);
        let actual = read_all(&committed);
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(
            expected, actual,
            "ui/generated is out of date; run `ARGUS_BLESS=1 cargo test -p argus-api`"
        );
    }

    #[test]
    fn payloads_round_trip_through_json() {
        let batch = TickBatch {
            readings: vec![
                Reading::new("cpu.load", 1, Availability::ok(12.5)),
                Reading::new("cpu.temp", 1, Availability::unavailable("sem sensor")),
            ],
        };
        let json = serde_json::to_string(&batch).unwrap();
        assert_eq!(serde_json::from_str::<TickBatch>(&json).unwrap(), batch);

        let meta = SensorMeta {
            id: "cpu.load".into(),
            group: "CPU".into(),
            name: "Carga".into(),
            unit: Unit::Percent,
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains(r#""unit":"percent""#));

        let field: Field<u32> = Field::unavailable("x");
        assert!(serde_json::to_string(&field)
            .unwrap()
            .contains("unavailable"));
    }
}
