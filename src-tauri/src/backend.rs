//! Command logic, free of Tauri types so it can be tested with fake collectors.

use crate::alerts_api::AlertsApi;
use argus_api::{ApiError, SamplePoint, SensorHistory, SensorInfo};
use argus_core::{
    CpuDetails, GpuDetails, MemoryDetails, NetworkAdapter, SensorId, StaticInfo, ThresholdRegistry,
};
use argus_runtime::{Runtime, SessionStats};
use std::sync::Arc;
use std::time::Duration;

/// Longest history the UI may ask for (the largest window it offers is 15 min).
pub const MAX_WINDOW_MS: i64 = 20 * 60_000;
const MIN_INTERVAL_MS: u64 = 250;
const MAX_INTERVAL_MS: u64 = 10_000;

/// Facts that do not change while the app runs, read once at startup.
pub struct Facts {
    pub info: StaticInfo,
    pub cpu: CpuDetails,
    pub gpus: Vec<GpuDetails>,
    pub memory: MemoryDetails,
}

pub struct Backend {
    runtime: Runtime,
    facts: Facts,
    thresholds: ThresholdRegistry,
    alerts: AlertsApi,
    session: Arc<SessionStats>,
    processes: std::sync::Mutex<argus_sensors_win::ProcessSampler>,
}

impl Backend {
    pub fn new(
        runtime: Runtime,
        facts: Facts,
        alerts: AlertsApi,
        session: Arc<SessionStats>,
    ) -> Self {
        Self {
            runtime,
            facts,
            thresholds: ThresholdRegistry::with_defaults(),
            alerts,
            session,
            processes: std::sync::Mutex::new(argus_sensors_win::ProcessSampler::new()),
        }
    }

    /// Samples the processes now. Only the open Processes screen asks for this, so it costs
    /// nothing while another screen (or the tray) is showing.
    pub fn list_processes(&self) -> Vec<argus_core::ProcessRow> {
        self.processes.lock().expect("processes lock").sample()
    }

    /// The plain-text system report, from the startup facts plus what is read now.
    pub fn report(
        &self,
        system: &argus_core::SystemDetails,
        disks: &[argus_core::DiskInfo],
        volumes: &[argus_core::VolumeInfo],
    ) -> String {
        let now = argus_core::Clock::now_ms(&argus_core::SystemClock);
        let generated_at = format!("{} UTC", argus_store::format_utc(now));
        argus_core::build_report(&argus_core::ReportInput {
            generated_at: &generated_at,
            app_version: env!("CARGO_PKG_VERSION"),
            system,
            info: &self.facts.info,
            cpu: &self.facts.cpu,
            gpus: &self.facts.gpus,
            memory: &self.facts.memory,
            disks,
            volumes,
        })
    }

    /// Every sensor sample kept in memory (the last ~20 minutes), as CSV.
    pub fn sensor_csv(&self) -> String {
        use argus_core::Availability;
        let mut rows = Vec::new();
        for (meta, samples) in self.runtime.engine().export_series() {
            let unit = argus_core::Unit::label(meta.unit).to_string();
            for s in samples {
                let (value, note) = match s.value {
                    Availability::Ok { value } => (Some(value), String::new()),
                    Availability::Unavailable { reason } => {
                        (None, format!("indisponível: {reason}"))
                    }
                    Availability::NoResponse { .. } => (None, "sem resposta".to_string()),
                };
                rows.push(argus_store::SensorRow {
                    ts_ms: s.ts_ms,
                    sensor: meta.id.to_string(),
                    name: format!("{} {}", meta.group, meta.name),
                    unit: unit.clone(),
                    value,
                    note,
                });
            }
        }
        rows.sort_by(|a, b| a.ts_ms.cmp(&b.ts_ms).then_with(|| a.sensor.cmp(&b.sensor)));
        argus_store::sensors_csv(&rows)
    }

    /// Saves the settings and applies the ones that act on the running app (collection interval).
    pub fn apply_settings(
        &self,
        new: argus_api::AppSettings,
    ) -> Result<argus_api::AppSettings, ApiError> {
        let saved = self.alerts.update_settings(new)?;
        self.runtime
            .set_interval(Duration::from_millis(saved.sample_interval_ms));
        Ok(saved)
    }

    pub fn session_ranges(&self) -> Vec<argus_api::SessionRange> {
        self.session
            .snapshot()
            .into_iter()
            .map(|(sensor, min, max)| argus_api::SessionRange { sensor, min, max })
            .collect()
    }

    pub fn reset_session_ranges(&self) {
        self.session.reset();
    }

    pub fn alerts(&self) -> &AlertsApi {
        &self.alerts
    }

    /// What this machine can be protected on: readable sensors, why the others are not, and the
    /// CPU's Tjmax when the model is known.
    pub fn machine_profile(&self) -> argus_alerts::MachineProfile {
        let engine = self.runtime.engine();
        let mut profile = argus_alerts::MachineProfile {
            thresholds: self.thresholds.clone(),
            tjmax: self
                .facts
                .cpu
                .model
                .value
                .as_deref()
                .and_then(argus_alerts::tjmax_for),
            ..Default::default()
        };
        for meta in engine.sensors() {
            match engine.latest(&meta.id).map(|s| s.value) {
                Some(argus_core::Availability::Ok { .. }) => {
                    profile.readable.insert(meta.id.to_string());
                }
                Some(argus_core::Availability::Unavailable { reason }) => {
                    profile
                        .unreadable_reasons
                        .insert(meta.id.to_string(), reason);
                }
                _ => {}
            }
        }
        profile
    }

    pub fn static_info(&self) -> StaticInfo {
        self.facts.info.clone()
    }

    pub fn cpu_details(&self) -> CpuDetails {
        self.facts.cpu.clone()
    }

    pub fn gpu_details(&self) -> Vec<GpuDetails> {
        self.facts.gpus.clone()
    }

    pub fn memory_details(&self) -> MemoryDetails {
        self.facts.memory.clone()
    }

    /// Read on demand: adapters come and go (Wi-Fi off, VPN up).
    pub fn network_adapters(&self) -> Vec<NetworkAdapter> {
        argus_sensors_win::network_adapters()
    }

    pub fn list_sensors(&self) -> Vec<SensorInfo> {
        self.runtime
            .engine()
            .sensors()
            .into_iter()
            .map(|meta| SensorInfo {
                threshold: self.thresholds.get(&meta.id),
                meta,
            })
            .collect()
    }

    pub fn history(
        &self,
        sensor_ids: &[SensorId],
        window_ms: i64,
    ) -> Result<Vec<SensorHistory>, ApiError> {
        if !(1..=MAX_WINDOW_MS).contains(&window_ms) {
            return Err(ApiError::new(
                "invalid_window",
                format!("a janela deve ter entre 1 ms e {MAX_WINDOW_MS} ms"),
            ));
        }
        let known = self.list_sensors();
        let engine = self.runtime.engine();
        sensor_ids
            .iter()
            .map(|id| {
                if !known.iter().any(|m| &m.meta.id == id) {
                    return Err(ApiError::new(
                        "unknown_sensor",
                        format!("sensor desconhecido: {id}"),
                    ));
                }
                Ok(SensorHistory {
                    sensor: id.clone(),
                    samples: engine
                        .window(id, window_ms)
                        .into_iter()
                        .map(|s| SamplePoint {
                            ts_ms: s.ts_ms,
                            value: s.value,
                        })
                        .collect(),
                })
            })
            .collect()
    }

    pub fn set_sample_interval(&self, interval_ms: u64) -> Result<(), ApiError> {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval_ms) {
            return Err(ApiError::new(
                "invalid_interval",
                format!("o intervalo deve ficar entre {MIN_INTERVAL_MS} e {MAX_INTERVAL_MS} ms"),
            ));
        }
        self.runtime
            .set_interval(Duration::from_millis(interval_ms));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::{Availability, FakeClock, Field};
    use argus_runtime::Engine;
    use argus_sensors_fake::{Behavior, FakeCollector};
    use std::sync::Arc;

    fn info() -> StaticInfo {
        StaticInfo {
            host: Field::known("PC".into()),
            os: Field::known("Windows".into()),
            cpu_model: Field::unavailable("x"),
            cpu_physical_cores: Field::known(4),
            cpu_threads: Field::known(8),
            memory_total_bytes: Field::known(1.0),
            gpus: vec![],
        }
    }

    fn cpu_details() -> CpuDetails {
        fn na<T>() -> Field<T> {
            Field::unavailable("x")
        }
        CpuDetails {
            model: Field::known("CPU".into()),
            vendor: na(),
            codename: na(),
            socket: na(),
            process_node: na(),
            physical_cores: Field::known(4),
            threads: Field::known(8),
            signature: na(),
            base_clock_mhz: na(),
            tdp_watts: na(),
            tjmax_celsius: na(),
            multiplier: na(),
            caches: vec![],
            instruction_sets: vec![],
        }
    }

    fn memory_details() -> MemoryDetails {
        MemoryDetails {
            total_slots: Field::known(2),
            modules: vec![],
            timings: Field::unavailable("spd"),
            command_rate: Field::unavailable("spd"),
            profile: Field::unavailable("spd"),
            channels: Field::unavailable("x"),
        }
    }

    fn test_alerts() -> AlertsApi {
        use argus_runtime::{AlertService, RecordingDispatcher};
        use argus_store::{ConfigStore, HistoryStore};
        let dir = std::env::temp_dir().join(format!("argus-backend-test-{}", std::process::id()));
        let store = ConfigStore::new(&dir);
        let (config, _) = store.load();
        let service = AlertService::new(
            config.rules.clone(),
            HistoryStore::open_in_memory().unwrap(),
            Arc::new(RecordingDispatcher::default()),
        );
        AlertsApi::new(service, store, config)
    }

    fn backend() -> (Backend, FakeClock) {
        let (cpu, _) = FakeCollector::new("cpu", &["cpu.load"], Behavior::Value(10.0));
        let clock = FakeClock::new(100_000);
        let engine = Engine::new(
            vec![Box::new(cpu)],
            Arc::new(clock.clone()),
            Duration::from_millis(500),
            MAX_WINDOW_MS,
        );
        engine.tick();
        clock.advance_ms(1_000);
        engine.tick();
        (
            Backend::new(
                Runtime::start(engine, Duration::from_secs(3600)),
                Facts {
                    info: info(),
                    cpu: cpu_details(),
                    gpus: vec![],
                    memory: memory_details(),
                },
                test_alerts(),
                SessionStats::new(),
            ),
            clock,
        )
    }

    #[test]
    fn history_returns_the_window_by_time() {
        let (b, _clock) = backend();
        let id = SensorId::new("cpu.load");
        let out = b.history(std::slice::from_ref(&id), 60_000).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].samples.len() >= 2);
        assert!(out[0]
            .samples
            .iter()
            .all(|s| s.value == Availability::ok(10.0)));
    }

    #[test]
    fn history_rejects_bad_requests_with_an_error_not_a_panic() {
        let (b, _clock) = backend();
        let id = SensorId::new("cpu.load");
        assert_eq!(
            b.history(std::slice::from_ref(&id), 0).unwrap_err().code,
            "invalid_window"
        );
        assert_eq!(
            b.history(&[SensorId::new("nope")], 1_000).unwrap_err().code,
            "unknown_sensor"
        );
    }

    #[test]
    fn sample_interval_is_validated() {
        let (b, _clock) = backend();
        assert!(b.set_sample_interval(1_000).is_ok());
        assert_eq!(
            b.set_sample_interval(5).unwrap_err().code,
            "invalid_interval"
        );
    }

    #[test]
    fn static_info_and_catalog_are_exposed() {
        let (b, _clock) = backend();
        assert_eq!(b.static_info().cpu_threads.value, Some(8));
        assert_eq!(b.cpu_details().physical_cores.value, Some(4));
        assert!(b.gpu_details().is_empty());
        assert_eq!(b.memory_details().total_slots.value, Some(2));
        assert_eq!(b.list_sensors().len(), 1);
    }
}
