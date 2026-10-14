//! Verification CLI: prints what the runtime collects, so values can be compared with other tools.
//!
//! Usage: `argus-probe snapshot [--json] [--samples N]` or `argus-probe details` (static facts as JSON)
//! A rate sensor needs two samples, so the snapshot waits one second between cycles and
//! prints the last one.

use argus_core::{Availability, Clock, SensorMeta, SystemClock, Unit};
use argus_runtime::Engine;
use serde_json::json;
use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Percent => "%",
        Unit::Celsius => "°C",
        Unit::Megahertz => "MHz",
        Unit::Watt => "W",
        Unit::Volt => "V",
        Unit::Rpm => "RPM",
        Unit::Bytes => "B",
        Unit::BytesPerSecond => "B/s",
        Unit::Mbps => "Mbps",
        Unit::Milliseconds => "ms",
        Unit::Count => "",
    }
}

fn snapshot(json_output: bool, samples: usize) -> ExitCode {
    let engine = Engine::new(
        argus_sensors_win::default_collectors(),
        Arc::new(SystemClock),
        Duration::from_secs(3),
        60_000,
    );
    let mut last = Vec::new();
    for i in 0..samples {
        if i > 0 {
            std::thread::sleep(Duration::from_secs(1));
        }
        last = engine.tick();
    }
    let catalog: HashMap<String, SensorMeta> = engine
        .sensors()
        .into_iter()
        .map(|m| (m.id.to_string(), m))
        .collect();

    if json_output {
        let sensors: Vec<_> = last
            .iter()
            .map(|r| {
                let meta = catalog.get(r.sensor.as_str());
                json!({
                    "id": r.sensor,
                    "group": meta.map(|m| m.group.as_str()),
                    "name": meta.map(|m| m.name.as_str()),
                    "unit": meta.map(|m| m.unit),
                    "ts_ms": r.ts_ms,
                    "reading": r.value,
                })
            })
            .collect();
        let out = json!({ "taken_ms": SystemClock.now_ms(), "sensors": sensors });
        println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
    } else {
        for r in &last {
            let meta = catalog.get(r.sensor.as_str());
            let unit = meta.map_or("", |m| unit_label(m.unit));
            let shown = match &r.value {
                Availability::Ok { value } => format!("{value:.2} {unit}"),
                Availability::Unavailable { reason } => format!("indisponível: {reason}"),
                Availability::NoResponse { .. } => "sem resposta".to_string(),
            };
            println!("{:<28} {shown}", r.sensor);
        }
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("cpu-temp-bundle") => {
            println!("{}", argus_sensors_win::CPU_TEMPERATURE_BUNDLE);
            ExitCode::SUCCESS
        }
        Some("details") => {
            let out = json!({
                "static": argus_sensors_win::static_info(),
                "cpu": argus_sensors_win::cpu_details(),
                "gpus": argus_sensors_win::gpu_details(),
                "memory": argus_sensors_win::memory_details(),
                "network": argus_sensors_win::network_adapters(),
                "disks": argus_sensors_win::physical_disks(),
                "volumes": argus_sensors_win::volumes(),
            });
            println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
            ExitCode::SUCCESS
        }
        Some("history") => {
            // argus-probe history [--dir <data dir>] [--tests]: the alert history from the database
            // (readable while the app is running), newest first.
            let dir = args
                .iter()
                .position(|a| a == "--dir")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(argus_store::default_dir);
            let include_tests = args.iter().any(|a| a == "--tests");
            let path = dir.join("history.sqlite");
            if !path.exists() {
                eprintln!("sem histórico em {}", path.display());
                return ExitCode::from(1);
            }
            match argus_store::HistoryStore::open(&path).and_then(|h| {
                h.query(&argus_store::HistoryFilter {
                    include_tests,
                    ..Default::default()
                })
            }) {
                Ok(events) => {
                    for e in events {
                        let ev = e.event;
                        println!(
                            "{} {:<5} {:<9} {} | {}{}",
                            ev.ts_ms,
                            ev.severity.as_str(),
                            ev.kind.as_str(),
                            ev.rule_name,
                            ev.detail,
                            if ev.is_test { " [teste]" } else { "" }
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("não foi possível ler o histórico: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("watch") => {
            // argus-probe watch <sensor id> [count]: one value per second, for rate checks.
            let Some(sensor) = args.get(1) else {
                eprintln!("uso: argus-probe watch <sensor> [segundos]");
                return ExitCode::from(2);
            };
            let count: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(10);
            let engine = Engine::new(
                argus_sensors_win::default_collectors(),
                Arc::new(SystemClock),
                Duration::from_secs(3),
                60_000,
            );
            for _ in 0..count {
                let batch = engine.tick();
                if let Some(r) = batch.iter().find(|r| r.sensor.as_str() == sensor) {
                    match &r.value {
                        Availability::Ok { value } => println!("{} {value:.3}", r.ts_ms),
                        other => println!("{} {other:?}", r.ts_ms),
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            ExitCode::SUCCESS
        }
        Some("snapshot") => {
            let json_output = args.iter().any(|a| a == "--json");
            let samples = args
                .iter()
                .position(|a| a == "--samples")
                .and_then(|i| args.get(i + 1))
                .and_then(|n| n.parse().ok())
                .unwrap_or(2usize)
                .max(1);
            snapshot(json_output, samples)
        }
        _ => {
            eprintln!("uso: argus-probe snapshot [--json] [--samples N] | details | history [--dir D] [--tests] | watch <sensor> [segundos]");
            ExitCode::from(2)
        }
    }
}
