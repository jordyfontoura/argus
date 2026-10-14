//! Real Windows collectors (sysinfo, PDH, WMI, NVML). The only crate allowed to touch the OS.
//!
//! Ordinary collectors need no elevation. CPU low-level readings subscribe to the activated service.
//! Whatever cannot be read on a given
//! machine is reported as `Availability::Unavailable` with the reason, never as zero.

mod activation;
mod adapters;
mod cpu;
mod cpu_details;
mod cpu_temperature;
mod disk;
mod disks;
mod gpu;
mod info;
mod memory;
mod memory_details;
mod network;
mod nvml;
mod pdh;
mod processes;
mod smart;
mod system;

use argus_core::Collector;

pub use activation::activate_cpu_measurements;
pub use adapters::network_adapters;
pub use cpu::CpuCollector;
pub use cpu_details::cpu_details;
pub use cpu_temperature::DISABLE_LOWLEVEL_ENV;

/// Cargo-built activation bundle. Runtime measurements never load from this writable directory.
pub const CPU_TEMPERATURE_BUNDLE: &str = env!("ARGUS_CPU_TEMP_BUNDLE");
pub use disk::DiskCollector;
pub use disks::{physical_disks, volumes};
pub use gpu::{gpu_details, GpuCollector, DISABLE_NVML_ENV};
pub use info::static_info;
pub use memory::MemoryCollector;
pub use memory_details::memory_details;
pub use network::NetworkCollector;
pub use processes::ProcessSampler;
pub use system::system_details;

/// The collectors available on this machine, one per hardware area.
pub fn default_collectors() -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(CpuCollector::new()),
        Box::new(MemoryCollector::new()),
        Box::new(DiskCollector::new()),
        Box::new(GpuCollector::new()),
        Box::new(NetworkCollector::new()),
    ]
}

/// Turns a display name into a stable sensor id fragment (`Wi-Fi 2` -> `wi_fi_2`).
pub(crate) fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slug_keeps_ids_stable_and_safe() {
        assert_eq!(slug("Wi-Fi 2"), "wi_fi_2");
        assert_eq!(slug("Ethernet (Realtek PCIe)"), "ethernet_realtek_pcie");
        assert_eq!(slug("C:\\"), "c");
    }
}
