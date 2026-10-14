#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod alerts_api;
mod autostart;
mod backend;

use actions::{ActionRouter, TauriNotifier, TauriScreen, WinSound};
use alerts_api::AlertsApi;
use argus_alerts::{Actions, PlannedRule, PresetInfo, Rule, Sensitivity, SimResult};
use argus_api::{
    events, AlertsState, ApiError, AppSettings, BackgroundState, HistoryPage, HistoryQuery,
    SensorHistory, SensorInfo, SessionRange, TickBatch,
};
use argus_core::{
    CpuDetails, GpuDetails, MemoryDetails, NetworkAdapter, SensorId, StaticInfo, SystemClock,
};
use argus_runtime::{ActionDispatcher, AlertService, Engine, Runtime, SessionStats};
use argus_store::{ConfigStore, HistoryStore};
use backend::{Backend, Facts, MAX_WINDOW_MS};
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;

const COLLECTOR_TIMEOUT: Duration = Duration::from_secs(3);

#[tauri::command]
async fn activate_cpu_measurements(app: AppHandle) -> Result<String, ApiError> {
    let bundle = app
        .path()
        .resource_dir()
        .map_err(|e| ApiError::new("activation_resources", e.to_string()))?
        .join("cpu-temperature");
    tauri::async_runtime::spawn_blocking(move || {
        argus_sensors_win::activate_cpu_measurements(&bundle)
    })
    .await
    .map_err(|e| ApiError::new("activation_failed", e.to_string()))?
    .map_err(|e| ApiError::new("activation_failed", e))
}

// Command names live in `argus_api::commands`; a test in that crate checks the generated
// TypeScript. These handlers must keep the same names.
#[tauri::command]
fn get_static_info(state: State<'_, Backend>) -> StaticInfo {
    state.static_info()
}

#[tauri::command]
fn get_cpu_details(state: State<'_, Backend>) -> CpuDetails {
    state.cpu_details()
}

#[tauri::command]
fn get_gpu_details(state: State<'_, Backend>) -> Vec<GpuDetails> {
    state.gpu_details()
}

#[tauri::command]
fn get_memory_details(state: State<'_, Backend>) -> MemoryDetails {
    state.memory_details()
}

#[tauri::command]
fn get_network_adapters(state: State<'_, Backend>) -> Vec<NetworkAdapter> {
    state.network_adapters()
}

#[tauri::command]
fn list_sensors(state: State<'_, Backend>) -> Vec<SensorInfo> {
    state.list_sensors()
}

#[tauri::command]
fn get_history(
    state: State<'_, Backend>,
    sensor_ids: Vec<SensorId>,
    window_ms: i64,
) -> Result<Vec<SensorHistory>, ApiError> {
    state.history(&sensor_ids, window_ms)
}

#[tauri::command]
fn set_sample_interval(state: State<'_, Backend>, interval_ms: u64) -> Result<(), ApiError> {
    state.set_sample_interval(interval_ms)
}

#[tauri::command]
fn list_rules(state: State<'_, Backend>) -> Vec<Rule> {
    state.alerts().list_rules()
}

#[tauri::command]
fn save_rule(state: State<'_, Backend>, rule: Rule) -> Result<Rule, ApiError> {
    state.alerts().save_rule(rule)
}

#[tauri::command]
fn delete_rule(state: State<'_, Backend>, id: u64) -> Result<(), ApiError> {
    state.alerts().delete_rule(id)
}

#[tauri::command]
fn acknowledge_rule(state: State<'_, Backend>, id: u64) -> bool {
    state.alerts().acknowledge_rule(id)
}

#[tauri::command]
fn simulate_rule(state: State<'_, Backend>, rule: Rule) -> Result<SimResult, ApiError> {
    state.alerts().simulate_rule(rule)
}

#[tauri::command]
fn set_mute(state: State<'_, Backend>, seconds: u32) -> Result<AlertsState, ApiError> {
    state.alerts().set_mute(seconds)
}

#[tauri::command]
fn get_alerts_state(state: State<'_, Backend>) -> AlertsState {
    state.alerts().state()
}

#[tauri::command]
fn set_critical_bypass(state: State<'_, Backend>, on: bool) -> Result<AlertsState, ApiError> {
    state.alerts().set_critical_bypass(on)
}

#[tauri::command]
fn list_presets(state: State<'_, Backend>) -> Vec<PresetInfo> {
    state.alerts().list_presets()
}

#[tauri::command]
fn preview_preset(
    state: State<'_, Backend>,
    preset: String,
    sensitivity: Sensitivity,
    actions: Option<Actions>,
) -> Result<Vec<PlannedRule>, ApiError> {
    state
        .alerts()
        .preview_preset(&state.machine_profile(), &preset, sensitivity, actions)
}

#[tauri::command]
fn apply_preset(
    state: State<'_, Backend>,
    preset: String,
    sensitivity: Sensitivity,
    actions: Option<Actions>,
    chosen: Vec<String>,
    keep_own: bool,
) -> Result<Vec<Rule>, ApiError> {
    state.alerts().apply_preset(
        &state.machine_profile(),
        &preset,
        sensitivity,
        actions,
        &chosen,
        keep_own,
    )
}

#[tauri::command]
fn query_history(state: State<'_, Backend>, query: HistoryQuery) -> Result<HistoryPage, ApiError> {
    state.alerts().query_history(&query)
}

/// Saves the filtered history as CSV where the user chooses (system save dialog). Returns the
/// path, or `None` when the dialog was cancelled. `ARGUS_EXPORT_PATH` skips the dialog (tests).
#[tauri::command]
async fn export_history(
    app: AppHandle,
    state: State<'_, Backend>,
    query: HistoryQuery,
) -> Result<Option<String>, ApiError> {
    let csv = state.alerts().history_csv(&query)?;
    let path = match std::env::var_os("ARGUS_EXPORT_PATH") {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let picked = app
                .dialog()
                .file()
                .add_filter("CSV", &["csv"])
                .set_file_name("argus-historico.csv")
                .blocking_save_file();
            match picked {
                None => return Ok(None),
                Some(file) => file.into_path().map_err(|e| {
                    ApiError::new("export_failed", format!("caminho inválido: {e}"))
                })?,
            }
        }
    };
    std::fs::write(&path, csv).map_err(|e| {
        ApiError::new(
            "export_failed",
            format!("não foi possível gravar o arquivo: {e}"),
        )
    })?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn clear_history(state: State<'_, Backend>) -> Result<u64, ApiError> {
    state.alerts().clear_history()
}

#[tauri::command]
fn set_history_retention(state: State<'_, Backend>, days: u32) -> Result<HistoryPage, ApiError> {
    state.alerts().set_history_retention(days)
}

fn background_state(state: &Backend) -> BackgroundState {
    BackgroundState {
        autostart: autostart::is_enabled(autostart::VALUE_NAME),
        minimize_to_tray: state.alerts().minimize_to_tray(),
    }
}

#[tauri::command]
fn get_background_state(state: State<'_, Backend>) -> BackgroundState {
    background_state(&state)
}

#[tauri::command]
fn set_background(
    state: State<'_, Backend>,
    autostart: Option<bool>,
    minimize_to_tray: Option<bool>,
) -> Result<BackgroundState, ApiError> {
    if let Some(on) = autostart {
        let command = autostart::run_command().map_err(|e| ApiError::new("autostart_failed", e))?;
        autostart::set_enabled(autostart::VALUE_NAME, on, &command)
            .map_err(|e| ApiError::new("autostart_failed", e))?;
    }
    state
        .alerts()
        .save_background(autostart, minimize_to_tray)?;
    Ok(background_state(&state))
}

/// Brings the main window back (from the tray, a second launch or a tray click).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir o Argus", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "Silenciar por 1 h", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &mute, &quit])?;
    let mut builder = TrayIconBuilder::new()
        .tooltip("Argus: monitorando em segundo plano")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "mute" => {
                let backend = app.state::<Backend>();
                if let Ok(state) = backend.alerts().set_mute(3600) {
                    let _ = app.emit(events::ALERTS_STATE, state);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[tauri::command]
fn get_session_ranges(state: State<'_, Backend>) -> Vec<SessionRange> {
    state.session_ranges()
}

#[tauri::command]
fn reset_session_ranges(state: State<'_, Backend>) {
    state.reset_session_ranges();
}

#[tauri::command]
fn get_favorites(state: State<'_, Backend>) -> Vec<String> {
    state.alerts().favorites()
}

#[tauri::command]
fn set_favorite(
    state: State<'_, Backend>,
    sensor: String,
    on: bool,
) -> Result<Vec<String>, ApiError> {
    state.alerts().set_favorite(&sensor, on)
}

#[tauri::command]
fn list_processes(state: State<'_, Backend>) -> Vec<argus_core::ProcessRow> {
    state.list_processes()
}

/// Physical disks with their health, read on demand (the temperature changes).
#[tauri::command]
async fn get_disks() -> Vec<argus_core::DiskInfo> {
    // The storage queries can take a moment, so they run off the main thread.
    argus_sensors_win::physical_disks()
}

#[tauri::command]
async fn get_volumes() -> Vec<argus_core::VolumeInfo> {
    argus_sensors_win::volumes()
}

#[tauri::command]
async fn get_system_details() -> argus_core::SystemDetails {
    argus_sensors_win::system_details()
}

/// The text of the system report. The UI copies it to the clipboard.
#[tauri::command]
async fn get_report(state: State<'_, Backend>) -> Result<String, ApiError> {
    let system = argus_sensors_win::system_details();
    let disks = argus_sensors_win::physical_disks();
    let volumes = argus_sensors_win::volumes();
    Ok(state.report(&system, &disks, &volumes))
}

/// Saves the session's sensor history as CSV (system save dialog; `ARGUS_EXPORT_PATH` skips it).
#[tauri::command]
async fn export_sensor_csv(
    app: AppHandle,
    state: State<'_, Backend>,
) -> Result<Option<String>, ApiError> {
    let csv = state.sensor_csv();
    let path = match std::env::var_os("ARGUS_EXPORT_PATH") {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let picked = app
                .dialog()
                .file()
                .add_filter("CSV", &["csv"])
                .set_file_name("argus-sensores.csv")
                .blocking_save_file();
            match picked {
                None => return Ok(None),
                Some(file) => file.into_path().map_err(|e| {
                    ApiError::new("export_failed", format!("caminho inválido: {e}"))
                })?,
            }
        }
    };
    std::fs::write(&path, csv).map_err(|e| {
        ApiError::new(
            "export_failed",
            format!("não foi possível gravar o arquivo: {e}"),
        )
    })?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn get_settings(state: State<'_, Backend>) -> AppSettings {
    state.alerts().settings()
}

#[tauri::command]
fn set_settings(state: State<'_, Backend>, settings: AppSettings) -> Result<AppSettings, ApiError> {
    state.apply_settings(settings)
}

/// The real collectors, or a scripted scenario when `ARGUS_SENSORS=fake:<name>` is set (used to
/// trigger alerts on demand in end-to-end runs).
fn collectors() -> Vec<Box<dyn argus_core::Collector>> {
    if let Ok(spec) = std::env::var("ARGUS_SENSORS") {
        match spec
            .strip_prefix("fake:")
            .and_then(argus_sensors_fake::scenario)
        {
            Some(fake) => {
                eprintln!("argus: usando o cenário falso {spec}");
                return fake;
            }
            None => eprintln!(
                "argus: ARGUS_SENSORS={spec} não é um cenário conhecido; usando o hardware"
            ),
        }
    }
    argus_sensors_win::default_collectors()
}

fn open_alerts(dispatcher: Arc<dyn ActionDispatcher>) -> AlertsApi {
    let dir = argus_store::default_dir();
    let store = ConfigStore::new(&dir);
    let (config, report) = store.load();
    if let Some(reason) = &report.reason {
        eprintln!(
            "argus: configuração ignorada ({reason}); backup: {:?}",
            report.set_aside
        );
    }
    let history = HistoryStore::open(&dir.join("history.sqlite")).unwrap_or_else(|e| {
        eprintln!("argus: histórico em disco indisponível ({e}); usando memória");
        HistoryStore::open_in_memory().expect("in-memory history")
    });
    let bypass = config.settings.critical_bypasses_mute;
    let service = AlertService::new(config.rules.clone(), history, dispatcher);
    service.set_critical_bypasses_mute(bypass);
    let now = argus_core::Clock::now_ms(&SystemClock);
    let _ = service.apply_retention(now, config.settings.history_retention_days);
    AlertsApi::new(service, store, config)
}

fn main() {
    let engine = Engine::new(
        collectors(),
        Arc::new(SystemClock),
        COLLECTOR_TIMEOUT,
        MAX_WINDOW_MS,
    );
    let ticks = engine.subscribe();
    let screen = Arc::new(TauriScreen::default());
    let notifier = Arc::new(TauriNotifier::default());
    let router = ActionRouter {
        screen: screen.clone(),
        sound: Arc::new(WinSound),
        notifier: notifier.clone(),
    };
    let alerts = open_alerts(Arc::new(router));
    // Alerts are driven by the runtime's own cycle, not by any window.
    alerts.service().attach(&engine);
    let session = SessionStats::new();
    session.attach(&engine);
    let alert_events = alerts.service().subscribe();
    // The interval the user chose last time (a first run has the 1 s default).
    let saved_interval = Duration::from_millis(alerts.settings().sample_interval_ms);
    let backend = Backend::new(
        Runtime::start(engine, saved_interval),
        Facts {
            info: argus_sensors_win::static_info(),
            cpu: argus_sensors_win::cpu_details(),
            gpus: argus_sensors_win::gpu_details(),
            memory: argus_sensors_win::memory_details(),
        },
        alerts,
        session,
    );

    tauri::Builder::default()
        // A second launch (for example at sign-in plus a double click) only brings the first one
        // back; two runtimes would write to the same history.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            activate_cpu_measurements,
            get_static_info,
            get_cpu_details,
            get_gpu_details,
            get_memory_details,
            get_network_adapters,
            list_sensors,
            get_history,
            set_sample_interval,
            list_rules,
            save_rule,
            delete_rule,
            acknowledge_rule,
            simulate_rule,
            set_mute,
            get_alerts_state,
            set_critical_bypass,
            list_presets,
            preview_preset,
            apply_preset,
            query_history,
            export_history,
            clear_history,
            set_history_retention,
            get_background_state,
            set_background,
            get_session_ranges,
            reset_session_ranges,
            get_favorites,
            set_favorite,
            list_processes,
            get_disks,
            get_volumes,
            get_system_details,
            get_report,
            export_sensor_csv,
            get_settings,
            set_settings
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // With the option on, closing only hides the window: collection and alerts live
                // in the runtime and keep going from the tray.
                if window
                    .app_handle()
                    .state::<Backend>()
                    .alerts()
                    .minimize_to_tray()
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            setup_tray(app)?;
            if std::env::args().any(|a| a == "--minimized") {
                eprintln!("argus: iniciado em segundo plano (--minimized)");
            } else {
                show_main_window(app.handle());
            }
            let _ = screen.handle.set(app.handle().clone());
            let _ = notifier.handle.set(app.handle().clone());
            let handle = app.handle().clone();
            // The runtime keeps collecting whether or not a window is listening.
            std::thread::Builder::new()
                .name("argus-ticks".to_string())
                .spawn(move || {
                    for batch in ticks {
                        let payload = TickBatch {
                            readings: batch.as_ref().clone(),
                        };
                        let _ = handle.emit(events::SENSORS_TICK, payload);
                        let state = handle.state::<Backend>().alerts().state();
                        let _ = handle.emit(events::ALERTS_STATE, state);
                    }
                })?;
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("argus-alert-events".to_string())
                .spawn(move || {
                    for event in alert_events {
                        let _ = handle.emit(events::ALERTS_EVENT, event);
                    }
                })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Argus");
}

#[cfg(test)]
mod contract {
    use super::*;
    use argus_api::commands;

    /// The handler names registered above are the ones the UI is generated against.
    #[test]
    fn command_names_match_the_contract() {
        assert_eq!(
            commands::ACTIVATE_CPU_MEASUREMENTS,
            "activate_cpu_measurements"
        );
        assert_eq!(commands::GET_STATIC_INFO, "get_static_info");
        assert_eq!(commands::GET_CPU_DETAILS, "get_cpu_details");
        assert_eq!(commands::GET_GPU_DETAILS, "get_gpu_details");
        assert_eq!(commands::GET_MEMORY_DETAILS, "get_memory_details");
        assert_eq!(commands::GET_NETWORK_ADAPTERS, "get_network_adapters");
        assert_eq!(commands::LIST_SENSORS, "list_sensors");
        assert_eq!(commands::GET_HISTORY, "get_history");
        assert_eq!(commands::SET_SAMPLE_INTERVAL, "set_sample_interval");
        assert_eq!(commands::LIST_RULES, "list_rules");
        assert_eq!(commands::SAVE_RULE, "save_rule");
        assert_eq!(commands::DELETE_RULE, "delete_rule");
        assert_eq!(commands::ACKNOWLEDGE_RULE, "acknowledge_rule");
        assert_eq!(commands::SIMULATE_RULE, "simulate_rule");
        assert_eq!(commands::SET_MUTE, "set_mute");
        assert_eq!(commands::GET_ALERTS_STATE, "get_alerts_state");
        assert_eq!(commands::SET_CRITICAL_BYPASS, "set_critical_bypass");
        assert_eq!(commands::LIST_PRESETS, "list_presets");
        assert_eq!(commands::PREVIEW_PRESET, "preview_preset");
        assert_eq!(commands::APPLY_PRESET, "apply_preset");
        assert_eq!(commands::QUERY_HISTORY, "query_history");
        assert_eq!(commands::EXPORT_HISTORY, "export_history");
        assert_eq!(commands::CLEAR_HISTORY, "clear_history");
        assert_eq!(commands::SET_HISTORY_RETENTION, "set_history_retention");
        assert_eq!(commands::GET_BACKGROUND_STATE, "get_background_state");
        assert_eq!(commands::SET_BACKGROUND, "set_background");
        assert_eq!(commands::GET_SESSION_RANGES, "get_session_ranges");
        assert_eq!(commands::RESET_SESSION_RANGES, "reset_session_ranges");
        assert_eq!(commands::GET_FAVORITES, "get_favorites");
        assert_eq!(commands::SET_FAVORITE, "set_favorite");
        assert_eq!(commands::LIST_PROCESSES, "list_processes");
        assert_eq!(commands::GET_DISKS, "get_disks");
        assert_eq!(commands::GET_VOLUMES, "get_volumes");
        assert_eq!(commands::GET_SYSTEM_DETAILS, "get_system_details");
        assert_eq!(commands::GET_REPORT, "get_report");
        assert_eq!(commands::EXPORT_SENSOR_CSV, "export_sensor_csv");
        assert_eq!(commands::GET_SETTINGS, "get_settings");
        assert_eq!(commands::SET_SETTINGS, "set_settings");
        assert_eq!(events::SENSORS_TICK, "sensors:tick");
        assert_eq!(events::ALERTS_STATE, "alerts:state");
        assert_eq!(events::ALERTS_EVENT, "alerts:event");
        assert_eq!(events::TOAST, "toast");
    }
}
