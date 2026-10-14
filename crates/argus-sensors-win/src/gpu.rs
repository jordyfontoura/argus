use crate::nvml::Nvml;
use crate::pdh::{Counter, Pdh};
use argus_core::{
    Availability, CollectError, Collector, Field, GpuDetails, Reading, SensorMeta, Unit,
};

/// Set to `1` to skip NVML and use the generic Windows counters (also how the fallback is tested).
pub const DISABLE_NVML_ENV: &str = "ARGUS_DISABLE_NVML";

const NO_NVML: &str = "exige o driver da NVIDIA (NVML), que não está disponível";
const NO_HOTSPOT: &str = "o NVML não expõe a temperatura do hot spot";

/// GPU readings: NVIDIA through NVML when available, otherwise the generic Windows GPU counters
/// (load and video memory only) and an explicit reason for everything else.
pub struct GpuCollector {
    nvml: Result<Nvml, String>,
    generic: GenericGpu,
}

impl GpuCollector {
    pub fn new() -> Self {
        let nvml = if std::env::var(DISABLE_NVML_ENV).is_ok_and(|v| v == "1") {
            Err(format!("desativado por {DISABLE_NVML_ENV}=1"))
        } else {
            Nvml::load()
        };
        // The generic counters are only needed when NVML is not driving the GPU.
        let generic = GenericGpu::new(nvml.is_err());
        Self { nvml, generic }
    }

    /// Why NVML is not in use, when it is not.
    pub fn nvml_status(&self) -> Result<(), &str> {
        self.nvml.as_ref().map(|_| ()).map_err(String::as_str)
    }

    /// Names reported by NVML (empty without NVML).
    pub fn device_names(&self) -> Vec<String> {
        let Ok(nvml) = &self.nvml else {
            return Vec::new();
        };
        (0..nvml.device_count)
            .filter_map(|i| nvml.device_name(i).ok())
            .collect()
    }

    fn device_count(&self) -> u32 {
        self.nvml.as_ref().map_or(1, |n| n.device_count)
    }
}

impl Default for GpuCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn result(value: Result<f64, String>) -> Availability {
    match value {
        Ok(v) => Availability::ok(v),
        Err(reason) => Availability::unavailable(reason),
    }
}

impl Collector for GpuCollector {
    fn name(&self) -> &str {
        "gpu"
    }

    fn sensors(&self) -> Vec<SensorMeta> {
        let mut out = Vec::new();
        for i in 0..self.device_count() {
            let mut meta = |key: &str, name: &str, unit| {
                out.push(SensorMeta {
                    id: format!("gpu.{i}.{key}").into(),
                    group: "GPU".to_string(),
                    name: name.to_string(),
                    unit,
                });
            };
            meta("load", "Carga", Unit::Percent);
            meta("mem_load", "Controlador de memória", Unit::Percent);
            meta("vram_used", "VRAM em uso", Unit::Bytes);
            meta("vram_total", "VRAM total", Unit::Bytes);
            meta("vram_pct", "VRAM em uso (%)", Unit::Percent);
            meta("temp", "Temperatura", Unit::Celsius);
            meta("hotspot", "Temperatura do hot spot", Unit::Celsius);
            meta("clock_core", "Clock do núcleo", Unit::Megahertz);
            meta("clock_mem", "Clock da memória", Unit::Megahertz);
            meta("fan", "Ventoinha", Unit::Percent);
            meta("power", "Potência", Unit::Watt);
            meta("power_limit", "Limite de potência", Unit::Watt);
            meta("limit.power", "Limitador: potência", Unit::Count);
            meta("limit.thermal", "Limitador: temperatura", Unit::Count);
            meta(
                "limit.hw_slowdown",
                "Limitador: redução por hardware",
                Unit::Count,
            );
            meta("limit.idle", "Placa ociosa", Unit::Count);
        }
        out
    }

    fn collect(&mut self, now_ms: i64) -> Result<Vec<Reading>, CollectError> {
        let mut out = Vec::new();
        let mut push = |i: u32, key: &str, value: Availability| {
            out.push(Reading::new(format!("gpu.{i}.{key}"), now_ms, value));
        };
        match &self.nvml {
            Ok(nvml) => {
                for i in 0..nvml.device_count {
                    let util = nvml.utilization(i);
                    push(i, "load", result(util.clone().map(|u| u.0)));
                    push(i, "mem_load", result(util.map(|u| u.1)));
                    let mem = nvml.memory(i);
                    push(i, "vram_used", result(mem.clone().map(|m| m.0)));
                    push(i, "vram_total", result(mem.clone().map(|m| m.1)));
                    push(i, "vram_pct", result(mem.map(|m| m.0 / m.1 * 100.0)));
                    push(i, "temp", result(nvml.temperature(i)));
                    push(i, "hotspot", Availability::unavailable(NO_HOTSPOT));
                    push(i, "clock_core", result(nvml.clock(i, 0)));
                    push(i, "clock_mem", result(nvml.clock(i, 2)));
                    push(i, "fan", result(nvml.fan_percent(i)));
                    push(i, "power", result(nvml.power_watts(i)));
                    push(i, "power_limit", result(nvml.power_limit_watts(i)));
                    let flag = |on: bool| Availability::ok(if on { 1.0 } else { 0.0 });
                    match nvml.limiters(i) {
                        Ok(l) => {
                            push(i, "limit.power", flag(l.power));
                            push(i, "limit.thermal", flag(l.thermal));
                            push(i, "limit.hw_slowdown", flag(l.hw_slowdown));
                            push(i, "limit.idle", flag(l.idle));
                        }
                        Err(reason) => {
                            for key in [
                                "limit.power",
                                "limit.thermal",
                                "limit.hw_slowdown",
                                "limit.idle",
                            ] {
                                push(i, key, Availability::unavailable(reason.clone()));
                            }
                        }
                    }
                }
            }
            Err(why) => {
                let (load, vram) = self.generic.read();
                push(0, "load", load);
                push(0, "vram_used", vram);
                let reason = format!("{NO_NVML} ({why})");
                for key in [
                    "mem_load",
                    "vram_total",
                    "vram_pct",
                    "temp",
                    "hotspot",
                    "clock_core",
                    "clock_mem",
                    "fan",
                    "power",
                    "power_limit",
                    "limit.power",
                    "limit.thermal",
                    "limit.hw_slowdown",
                    "limit.idle",
                ] {
                    push(0, key, Availability::unavailable(reason.clone()));
                }
            }
        }
        Ok(out)
    }
}

/// Load and dedicated video memory from the generic Windows GPU counters, which work for any
/// vendor but know nothing about temperature, clocks, fans or power.
struct GenericGpu {
    active: bool,
    pdh: Result<Pdh, String>,
    engines: Result<Counter, String>,
    memory: Result<Counter, String>,
    cycles: u32,
}

/// Counter instances come and go with processes, so the query is rebuilt now and then.
const REBUILD_EVERY: u32 = 15;

impl GenericGpu {
    fn new(active: bool) -> Self {
        let mut this = Self {
            active,
            pdh: Err("não usado".to_string()),
            engines: Err("não usado".to_string()),
            memory: Err("não usado".to_string()),
            cycles: 0,
        };
        if active {
            this.rebuild();
        }
        this
    }

    fn rebuild(&mut self) {
        self.pdh = Pdh::new();
        if let Ok(pdh) = self.pdh.as_mut() {
            self.engines = pdh.add(r"\GPU Engine(*)\Utilization Percentage");
            self.memory = pdh.add(r"\GPU Adapter Memory(*)\Dedicated Usage");
            // Rate counters need two samples.
            let _ = pdh.collect();
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = pdh.collect();
        }
    }

    fn read(&mut self) -> (Availability, Availability) {
        if !self.active {
            let off = Availability::unavailable("não usado");
            return (off.clone(), off);
        }
        self.cycles += 1;
        if self.cycles.is_multiple_of(REBUILD_EVERY) {
            self.rebuild();
        } else if let Ok(pdh) = &self.pdh {
            let _ = pdh.collect();
        }
        let (Ok(pdh), Ok(engines), Ok(memory)) = (&self.pdh, &self.engines, &self.memory) else {
            let why = "os contadores de GPU do Windows não existem nesta máquina";
            return (
                Availability::unavailable(why),
                Availability::unavailable(why),
            );
        };
        let load = match pdh.array(*engines) {
            Ok(items) => {
                let busy: f64 = items
                    .iter()
                    .filter(|(name, _)| name.contains("engtype_3D"))
                    .map(|(_, v)| *v)
                    .sum();
                Availability::ok(busy.clamp(0.0, 100.0))
            }
            Err(_) => Availability::unavailable("o Windows ainda não mediu a carga da GPU"),
        };
        let vram = match pdh.array(*memory) {
            Ok(items) => Availability::ok(items.iter().map(|(_, v)| *v).sum()),
            Err(_) => Availability::unavailable("o Windows não informou a memória de vídeo"),
        };
        (load, vram)
    }
}

const NVML_ONLY: &str = "exige o driver da NVIDIA (NVML)";

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiVideo {
    name: Option<String>,
    driver_version: Option<String>,
}

/// Static facts of every video adapter. NVIDIA boards are described through NVML; any other
/// adapter only gets what WMI knows (name and driver), the rest is unavailable with a reason.
pub fn gpu_details() -> Vec<GpuDetails> {
    if !std::env::var(DISABLE_NVML_ENV).is_ok_and(|v| v == "1") {
        if let Ok(nvml) = Nvml::load() {
            let driver = nvml.driver();
            return (0..nvml.device_count)
                .map(|i| {
                    let field = |r: Result<String, String>| match r {
                        Ok(v) => Field::known(v),
                        Err(why) => Field::unavailable(why),
                    };
                    GpuDetails {
                        name: field(nvml.device_name(i)),
                        architecture: field(nvml.architecture_name(i)),
                        shader_units: match nvml.shader_units(i) {
                            Ok(v) => Field::known(v),
                            Err(why) => Field::unavailable(why),
                        },
                        vram_bytes: match nvml.memory(i) {
                            Ok((_, total)) => Field::known(total),
                            Err(why) => Field::unavailable(why),
                        },
                        memory_bus_bits: match nvml.memory_bus_bits(i) {
                            Ok(v) => Field::known(v),
                            Err(why) => Field::unavailable(why),
                        },
                        bios: field(nvml.bios(i)),
                        driver: field(driver.clone()),
                        pcie_link: field(nvml.pcie_link(i)),
                    }
                })
                .collect();
        }
    }
    let video: Vec<WmiVideo> = wmi::WMIConnection::new()
        .and_then(|c| c.raw_query("SELECT Name, DriverVersion FROM Win32_VideoController"))
        .unwrap_or_default();
    if video.is_empty() {
        return vec![GpuDetails {
            name: Field::unavailable("o Windows não listou placas de vídeo"),
            architecture: Field::unavailable(NVML_ONLY),
            shader_units: Field::unavailable(NVML_ONLY),
            vram_bytes: Field::unavailable(NVML_ONLY),
            memory_bus_bits: Field::unavailable(NVML_ONLY),
            bios: Field::unavailable(NVML_ONLY),
            driver: Field::unavailable(NVML_ONLY),
            pcie_link: Field::unavailable(NVML_ONLY),
        }];
    }
    video
        .into_iter()
        .map(|v| GpuDetails {
            name: Field::from_option(v.name, "o Windows não informou o nome"),
            architecture: Field::unavailable(NVML_ONLY),
            shader_units: Field::unavailable(NVML_ONLY),
            vram_bytes: Field::unavailable(NVML_ONLY),
            memory_bus_bits: Field::unavailable(NVML_ONLY),
            bios: Field::unavailable(NVML_ONLY),
            driver: Field::from_option(v.driver_version, "o Windows não informou o driver"),
            pcie_link: Field::unavailable(NVML_ONLY),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gpu_field_is_known_or_explained() {
        let gpus = gpu_details();
        assert!(!gpus.is_empty());
        for g in gpus {
            for (name, known, why) in [
                ("name", g.name.value.is_some(), g.name.unavailable.is_some()),
                (
                    "driver",
                    g.driver.value.is_some(),
                    g.driver.unavailable.is_some(),
                ),
                ("bios", g.bios.value.is_some(), g.bios.unavailable.is_some()),
                (
                    "vram",
                    g.vram_bytes.value.is_some(),
                    g.vram_bytes.unavailable.is_some(),
                ),
            ] {
                assert!(known ^ why, "{name} must be either a value or a reason");
            }
        }
    }

    #[test]
    fn collector_reports_every_declared_sensor() {
        let mut c = GpuCollector::new();
        let declared: Vec<String> = c.sensors().iter().map(|m| m.id.to_string()).collect();
        let got: Vec<String> = c
            .collect(1)
            .unwrap()
            .iter()
            .map(|r| r.sensor.to_string())
            .collect();
        for id in &declared {
            assert!(got.contains(id), "no reading for declared sensor {id}");
        }
    }
}
