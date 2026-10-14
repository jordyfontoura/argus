use serde::{Deserialize, Serialize};

/// A static fact about the machine that may not be readable. Exactly one of the two is set, so
/// the UI can always say why a value is missing instead of showing an empty cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Field<T> {
    pub value: Option<T>,
    pub unavailable: Option<String>,
}

impl<T> Field<T> {
    pub fn known(value: T) -> Self {
        Self {
            value: Some(value),
            unavailable: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            unavailable: Some(reason.into()),
        }
    }

    pub fn from_option(value: Option<T>, reason: &str) -> Self {
        match value {
            Some(v) => Self::known(v),
            None => Self::unavailable(reason),
        }
    }
}

/// Machine facts that do not change while the app runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StaticInfo {
    pub host: Field<String>,
    pub os: Field<String>,
    pub cpu_model: Field<String>,
    pub cpu_physical_cores: Field<u32>,
    pub cpu_threads: Field<u32>,
    pub memory_total_bytes: Field<f64>,
    /// One entry per video adapter the system reports.
    pub gpus: Vec<Field<String>>,
}

/// One level of the processor cache, summed over all cores that have it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CacheInfo {
    /// "L1 dados", "L1 instruções", "L2" or "L3".
    pub name: String,
    pub instances: u32,
    pub size_bytes_each: f64,
    pub ways: Option<u32>,
}

/// Processor facts shown on the CPU screen. What Windows does not tell without a driver is
/// `unavailable` with the reason; nothing is guessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CpuDetails {
    pub model: Field<String>,
    pub vendor: Field<String>,
    pub codename: Field<String>,
    pub socket: Field<String>,
    pub process_node: Field<String>,
    pub physical_cores: Field<u32>,
    pub threads: Field<u32>,
    /// "Family / Model / Stepping" in hexadecimal, from CPUID.
    pub signature: Field<String>,
    pub base_clock_mhz: Field<f64>,
    pub tdp_watts: Field<f64>,
    pub tjmax_celsius: Field<f64>,
    pub multiplier: Field<f64>,
    pub caches: Vec<CacheInfo>,
    /// Instruction-set extensions reported by CPUID.
    pub instruction_sets: Vec<String>,
}

/// One video adapter's static facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GpuDetails {
    pub name: Field<String>,
    pub architecture: Field<String>,
    pub shader_units: Field<u32>,
    pub vram_bytes: Field<f64>,
    pub memory_bus_bits: Field<u32>,
    pub bios: Field<String>,
    pub driver: Field<String>,
    pub pcie_link: Field<String>,
}

/// One installed memory module, as described by the firmware (SMBIOS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MemoryModule {
    /// Slot label such as "DIMM 0" and the bank/channel the firmware reports.
    pub slot: String,
    pub bank: String,
    pub manufacturer: Field<String>,
    pub part_number: Field<String>,
    pub serial: Field<String>,
    pub kind: Field<String>,
    pub form_factor: Field<String>,
    pub capacity_bytes: f64,
    /// Speed the module is rated for, in MT/s.
    pub rated_mts: Field<u32>,
    /// Speed it is running at, in MT/s.
    pub configured_mts: Field<u32>,
    pub voltage_mv: Field<u32>,
}

/// Installed memory. Timings, command rate and XMP/EXPO profiles live in the module's SPD chip,
/// which Windows does not expose without a driver, so they are reported as unavailable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MemoryDetails {
    pub total_slots: Field<u32>,
    pub modules: Vec<MemoryModule>,
    pub timings: Field<String>,
    pub command_rate: Field<String>,
    pub profile: Field<String>,
    pub channels: Field<String>,
}

/// A network adapter and its addresses. Throughput comes from the `net.*` sensors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NetworkAdapter {
    pub name: String,
    /// Id fragment of the matching sensors: `net.<key>.rx` and `net.<key>.tx`.
    pub sensor_key: String,
    pub description: Field<String>,
    pub addresses: Vec<String>,
    pub connected: Field<bool>,
    pub link_mbps: Field<f64>,
}

/// Motherboard, firmware and operating system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SystemDetails {
    pub board_vendor: Field<String>,
    pub board_model: Field<String>,
    pub chipset: Field<String>,
    pub bios_vendor: Field<String>,
    pub bios_version: Field<String>,
    pub bios_date: Field<String>,
    pub os_name: Field<String>,
    pub os_version: Field<String>,
    pub os_build: Field<String>,
    pub os_arch: Field<String>,
    pub uptime_secs: Field<u64>,
}

/// One line of a disk's health report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SmartAttribute {
    pub name: String,
    pub value: String,
    /// "ok", "warn" or "crit" when the standard defines a verdict for it.
    pub level: Option<String>,
}

/// A physical disk. Health comes from the NVMe log when Windows lets a standard user read it;
/// otherwise `smart` is unavailable with the reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiskInfo {
    pub index: u32,
    pub name: String,
    pub media: Field<String>,
    pub bus: Field<String>,
    pub size_bytes: f64,
    pub firmware: Field<String>,
    pub serial: Field<String>,
    pub health: Field<String>,
    pub temperature_c: Field<f64>,
    pub smart: Field<Vec<SmartAttribute>>,
    /// Windows' storage reliability counters (wear, errors); administrator only.
    pub reliability: Field<String>,
}

/// A mounted volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VolumeInfo {
    pub mount: String,
    pub label: String,
    pub file_system: String,
    pub kind: String,
    pub removable: bool,
    pub total_bytes: f64,
    pub free_bytes: f64,
}

/// One running process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    /// Share of the whole machine's CPU, 0-100.
    pub cpu_percent: f64,
    /// Working set in bytes.
    pub memory_bytes: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_either_known_or_explained() {
        let known = Field::known(8u32);
        assert_eq!(known.value, Some(8));
        assert!(known.unavailable.is_none());
        let missing: Field<u32> = Field::from_option(None, "sem acesso");
        assert!(missing.value.is_none());
        assert_eq!(missing.unavailable.as_deref(), Some("sem acesso"));
    }
}
