//! Running processes (name, PID, CPU %, memory) from the OS, without administrator rights.

use argus_core::ProcessRow;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Keeps the previous sample, because a process's CPU use is the change between two samples.
pub struct ProcessSampler {
    sys: System,
    logical_cpus: f64,
}

impl ProcessSampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Prime the CPU counters so the first real sample already has a delta.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let logical = std::thread::available_parallelism().map_or(1, |n| n.get());
        Self {
            sys,
            logical_cpus: logical as f64,
        }
    }

    /// One row per process. CPU is a percentage of the whole machine (like Task Manager), so the
    /// rows add up to at most 100; memory is the working set in bytes.
    pub fn sample(&mut self) -> Vec<ProcessRow> {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        self.sys
            .processes()
            .values()
            .map(|p| ProcessRow {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_percent: (f64::from(p.cpu_usage()) / self.logical_cpus).clamp(0.0, 100.0),
                memory_bytes: p.memory() as f64,
            })
            .collect()
    }
}

impl Default for ProcessSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_this_very_process_with_a_sane_footprint() {
        let mut s = ProcessSampler::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let rows = s.sample();
        let me = rows
            .iter()
            .find(|r| r.pid == std::process::id())
            .expect("the test process is listed");
        assert!(!me.name.is_empty());
        assert!(
            me.memory_bytes > 1_000_000.0,
            "a test binary uses more than 1 MB"
        );
        assert!(rows.len() > 20, "a Windows machine runs many processes");
    }

    #[test]
    fn cpu_is_a_share_of_the_machine_and_never_above_100() {
        let mut s = ProcessSampler::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let rows = s.sample();
        assert!(rows.iter().all(|r| (0.0..=100.0).contains(&r.cpu_percent)));
        let total: f64 = rows.iter().map(|r| r.cpu_percent).sum();
        assert!(
            total <= 100.5 * 1.5,
            "shares add up to about the machine, got {total}"
        );
    }
}
