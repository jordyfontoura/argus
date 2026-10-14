use crate::gpu::GpuCollector;
use argus_core::{Field, StaticInfo};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Facts about this machine that do not change while the app runs.
pub fn static_info() -> StaticInfo {
    let sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let os = match (System::name(), System::os_version()) {
        (Some(name), Some(version)) => Field::known(format!("{name} {version}")),
        (Some(name), None) => Field::known(name),
        _ => Field::unavailable("o Windows não informou a versão"),
    };
    let model = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|b| !b.is_empty());
    let total = sys.total_memory();

    let names = GpuCollector::new().device_names();
    let gpus = if names.is_empty() {
        vec![Field::unavailable(
            "o nome da placa de vídeo exige o driver da NVIDIA (NVML)",
        )]
    } else {
        names.into_iter().map(Field::known).collect()
    };

    StaticInfo {
        host: Field::from_option(
            System::host_name(),
            "o Windows não informou o nome do computador",
        ),
        os,
        cpu_model: Field::from_option(model, "o Windows não informou o modelo do processador"),
        cpu_physical_cores: Field::from_option(
            System::physical_core_count().map(|n| n as u32),
            "o Windows não informou os núcleos físicos",
        ),
        cpu_threads: Field::known(sys.cpus().len() as u32),
        memory_total_bytes: if total > 0 {
            Field::known(total as f64)
        } else {
            Field::unavailable("o Windows não informou a memória")
        },
        gpus,
    }
}

#[cfg(test)]
mod tests {
    use super::static_info;

    #[test]
    fn reports_this_machine_without_inventing_values() {
        let info = static_info();
        assert!(info.cpu_threads.value.unwrap_or(0) >= 1);
        assert!(info.memory_total_bytes.value.unwrap_or(0.0) > 0.0);
        assert!(!info.gpus.is_empty());
        // every field is either known or explained
        assert!(info.cpu_model.value.is_some() || info.cpu_model.unavailable.is_some());
    }
}
