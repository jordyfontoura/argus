use crate::cpu_temperature::CpuTemperature;
use crate::pdh::{Counter, Pdh};
use argus_core::{Availability, CollectError, Collector, Reading, SensorMeta, Unit};
use sysinfo::{CpuRefreshKind, RefreshKind, System, MINIMUM_CPU_UPDATE_INTERVAL};

pub struct CpuCollector {
    sys: System,
    pdh: Result<Pdh, String>,
    frequency: Result<Counter, String>,
    performance: Result<Counter, String>,
    temperature: CpuTemperature,
    power: Result<Counter, String>,
    limit_flags: Result<Counter, String>,
    max_freq_pct: Result<Counter, String>,
}

impl CpuCollector {
    pub fn new() -> Self {
        Self::with_temperature(CpuTemperature::new())
    }

    fn with_temperature(temperature: CpuTemperature) -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
        );
        // Usage is a delta between two refreshes, so prime it before the first real reading.
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_cpu_all();

        let mut pdh = Pdh::new();
        let na = || Err("PDH indisponível".to_string());
        let (mut frequency, mut performance) = (na(), na());
        let (mut power, mut limit_flags, mut max_freq_pct) = (na(), na(), na());
        if let Ok(p) = pdh.as_mut() {
            frequency = p.add(r"\Processor Information(_Total)\Processor Frequency");
            performance = p.add(r"\Processor Information(_Total)\% Processor Performance");
            // Windows exposes the RAPL package counter as a power meter on CPUs that report it.
            power = p
                .add(r"\Power Meter(*)\Power")
                .or_else(|_| p.add(r"\Energy Meter(*)\Power"));
            limit_flags = p.add(r"\Processor Information(_Total)\Performance Limit Flags");
            max_freq_pct = p.add(r"\Processor Information(_Total)\% of Maximum Frequency");
            let _ = p.collect();
        }
        Self {
            sys,
            pdh,
            frequency,
            performance,
            temperature,
            power,
            limit_flags,
            max_freq_pct,
        }
    }

    fn thread_count(&self) -> usize {
        self.sys.cpus().len()
    }

    fn clock_mhz(&self) -> Availability {
        let (Ok(pdh), Ok(freq), Ok(perf)) = (&self.pdh, &self.frequency, &self.performance) else {
            return self.sysinfo_clock();
        };
        match (pdh.value(*freq), pdh.value(*perf)) {
            (Ok(base), Ok(percent)) if base > 0.0 => Availability::ok(base * percent / 100.0),
            _ => self.sysinfo_clock(),
        }
    }

    fn sysinfo_clock(&self) -> Availability {
        let cpus = self.sys.cpus();
        let total: u64 = cpus.iter().map(|c| c.frequency()).sum();
        if cpus.is_empty() || total == 0 {
            Availability::unavailable("o Windows não informou a frequência")
        } else {
            Availability::ok(total as f64 / cpus.len() as f64)
        }
    }

    fn pdh_value(&self, counter: &Result<Counter, String>, missing: &str) -> Availability {
        let (Ok(pdh), Ok(counter)) = (&self.pdh, counter) else {
            return Availability::unavailable(missing);
        };
        match pdh.value(*counter) {
            Ok(v) => Availability::ok(v),
            Err(_) => Availability::unavailable(missing),
        }
    }

    /// Package power in watts from the RAPL counter, when the platform reports it.
    fn package_power(&self) -> Availability {
        const MISSING: &str = "o Windows não expõe o consumo do pacote neste processador";
        let (Ok(pdh), Ok(counter)) = (&self.pdh, &self.power) else {
            return Availability::unavailable(MISSING);
        };
        pdh.array(*counter)
            .unwrap_or_default()
            .iter()
            .find(|(name, mw)| {
                name.to_ascii_lowercase().contains("pkg") && mw.is_finite() && *mw > 0.0
            })
            // The meter reports milliwatts.
            .map_or_else(
                || Availability::unavailable(MISSING),
                |(_, mw)| Availability::ok(mw / 1000.0),
            )
    }
}

impl Default for CpuCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for CpuCollector {
    fn name(&self) -> &str {
        "cpu"
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        let meta = |id: String, name: String, unit| SensorMeta {
            id: id.as_str().into(),
            group: "CPU".to_string(),
            name,
            unit,
        };
        let mut out = vec![
            meta("cpu.load".into(), "Carga total".into(), Unit::Percent),
            meta("cpu.clock".into(), "Clock médio".into(), Unit::Megahertz),
            meta("cpu.temp".into(), "Temperatura".into(), Unit::Celsius),
            meta("cpu.power".into(), "Potência do pacote".into(), Unit::Watt),
            meta("cpu.vcore".into(), "Tensão do núcleo".into(), Unit::Volt),
            meta(
                "cpu.freq_pct".into(),
                "Frequência em relação ao máximo".into(),
                Unit::Percent,
            ),
            meta(
                "cpu.limit_flags".into(),
                "Flags de limite de desempenho".into(),
                Unit::Count,
            ),
        ];
        for i in 0..self.thread_count() {
            out.push(meta(
                format!("cpu.thread.{i}.load"),
                format!("Thread {i}"),
                Unit::Percent,
            ));
        }
        out
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        self.sys.refresh_cpu_all();
        if let Ok(pdh) = &self.pdh {
            let _ = pdh.collect();
        }
        let helper = self.temperature.readings(now_ms);
        let windows_power = self.package_power();
        let mut out = vec![
            Reading::new(
                "cpu.load",
                now_ms,
                Availability::ok(f64::from(self.sys.global_cpu_usage())),
            ),
            Reading::new("cpu.clock", now_ms, self.clock_mhz()),
        ];
        if let Some([temperature, helper_power, vcore]) = helper {
            out.push(temperature);
            out.push(if windows_power.value().is_some() {
                Reading::new("cpu.power", now_ms, windows_power)
            } else {
                helper_power
            });
            out.push(vcore);
        } else if windows_power.value().is_some() {
            out.push(Reading::new("cpu.power", now_ms, windows_power));
        }
        out.extend([
            Reading::new(
                "cpu.freq_pct",
                now_ms,
                self.pdh_value(
                    &self.max_freq_pct,
                    "o Windows não informou a frequência relativa",
                ),
            ),
            Reading::new(
                "cpu.limit_flags",
                now_ms,
                self.pdh_value(
                    &self.limit_flags,
                    "o Windows não informa limites de desempenho",
                ),
            ),
        ]);
        for (i, cpu) in self.sys.cpus().iter().enumerate() {
            out.push(Reading::new(
                format!("cpu.thread.{i}.load"),
                now_ms,
                Availability::ok(f64::from(cpu.cpu_usage())),
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdh_thermal_zone_alone_is_not_cpu_temperature() {
        let mut collector = CpuCollector::with_temperature(CpuTemperature::disabled());
        let pdh = collector.pdh.as_mut().unwrap();
        // Replace the external PDH wildcard data, not Argus's temperature selection.
        let counter = pdh
            .add(r"\Processor Information(*)\% Processor Time")
            .unwrap();
        pdh.array_fixture = Some(vec![("ACPI\\ThermalZone\\TZ00".into(), 330.15)]);
        collector.power = Ok(counter);
        let readings = collector.collect(10_000).unwrap();
        assert!(readings
            .iter()
            .find(|r| r.sensor.as_str() == "cpu.temp")
            .unwrap()
            .value
            .value()
            .is_none());
        assert!(readings
            .iter()
            .find(|r| r.sensor.as_str() == "cpu.load")
            .unwrap()
            .value
            .value()
            .is_some());
        // A functioning Windows package meter stays available without low-level access.
        collector.pdh.as_mut().unwrap().array_fixture = Some(vec![("RAPL_PKG".into(), 45_500.0)]);
        let readings = collector.collect(11_000).unwrap();
        let power = readings
            .iter()
            .find(|r| r.sensor.as_str() == "cpu.power")
            .unwrap();
        assert_eq!(power.value.value(), Some(45.5));
        assert_eq!(power.ts_ms, 11_000);
        assert!(readings
            .iter()
            .find(|r| r.sensor.as_str() == "cpu.vcore")
            .unwrap()
            .value
            .value()
            .is_none());
    }
}
