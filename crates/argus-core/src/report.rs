//! The plain-text system report ("Copiar relatório"). Pure, so it is tested without hardware.
//! Anything that could not be read appears as `indisponível (reason)`, never as an empty line.

use crate::{
    CpuDetails, DiskInfo, Field, GpuDetails, MemoryDetails, StaticInfo, SystemDetails, VolumeInfo,
};
use std::fmt::Write;

/// Everything the report is made of.
pub struct ReportInput<'a> {
    /// When the report was made, already formatted (the core has no clock or time zone).
    pub generated_at: &'a str,
    pub app_version: &'a str,
    pub system: &'a SystemDetails,
    pub info: &'a StaticInfo,
    pub cpu: &'a CpuDetails,
    pub gpus: &'a [GpuDetails],
    pub memory: &'a MemoryDetails,
    pub disks: &'a [DiskInfo],
    pub volumes: &'a [VolumeInfo],
}

fn show<T>(f: &Field<T>, fmt: impl Fn(&T) -> String) -> String {
    match (&f.value, &f.unavailable) {
        (Some(v), _) => fmt(v),
        (None, Some(why)) => format!("indisponível ({why})"),
        (None, None) => "indisponível".to_string(),
    }
}

fn text(f: &Field<String>) -> String {
    show(f, Clone::clone)
}

fn gb(bytes: f64) -> String {
    let g = bytes / 1e9;
    let s = if g >= 1000.0 {
        format!("{:.2} TB", g / 1000.0)
    } else {
        format!("{g:.1} GB")
    };
    s.replace('.', ",")
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if d > 0 {
        format!("{d} d {h:02}:{m:02}")
    } else {
        format!("{h:02}:{m:02}")
    }
}

pub fn build_report(r: &ReportInput<'_>) -> String {
    let mut o = String::new();
    let line = |o: &mut String, label: &str, value: String| {
        let _ = writeln!(o, "  {label}: {value}");
    };
    let _ = writeln!(o, "Argus {} - relatório do sistema", r.app_version);
    let _ = writeln!(o, "Gerado em {}", r.generated_at);

    let _ = writeln!(o, "\nSistema");
    line(&mut o, "Computador", text(&r.info.host));
    line(&mut o, "Sistema", text(&r.system.os_name));
    line(
        &mut o,
        "Versão",
        format!(
            "{} (build {})",
            text(&r.system.os_version),
            text(&r.system.os_build)
        ),
    );
    line(&mut o, "Arquitetura", text(&r.system.os_arch));
    line(
        &mut o,
        "Tempo ligado",
        show(&r.system.uptime_secs, |s| duration(*s)),
    );

    let _ = writeln!(o, "\nPlaca-mãe e BIOS");
    line(&mut o, "Fabricante", text(&r.system.board_vendor));
    line(&mut o, "Modelo", text(&r.system.board_model));
    line(&mut o, "Chipset", text(&r.system.chipset));
    line(
        &mut o,
        "BIOS",
        format!(
            "{} {} ({})",
            text(&r.system.bios_vendor),
            text(&r.system.bios_version),
            text(&r.system.bios_date)
        ),
    );

    let _ = writeln!(o, "\nProcessador");
    line(&mut o, "Modelo", text(&r.cpu.model));
    line(
        &mut o,
        "Núcleos / Threads",
        format!(
            "{} / {}",
            show(&r.cpu.physical_cores, u32::to_string),
            show(&r.cpu.threads, u32::to_string)
        ),
    );
    line(&mut o, "Soquete", text(&r.cpu.socket));
    line(
        &mut o,
        "Clock base",
        show(&r.cpu.base_clock_mhz, |v| format!("{v:.0} MHz")),
    );
    line(
        &mut o,
        "Família / Modelo / Stepping",
        text(&r.cpu.signature),
    );

    let _ = writeln!(o, "\nMemória");
    line(
        &mut o,
        "Total",
        show(&r.info.memory_total_bytes, |v| gb(*v)),
    );
    line(&mut o, "Slots", show(&r.memory.total_slots, u32::to_string));
    for m in &r.memory.modules {
        let speed = show(&m.configured_mts, |v| format!("{v} MT/s"));
        let _ = writeln!(
            o,
            "  {} {}: {} {} {} {} a {}",
            m.slot,
            m.bank,
            gb(m.capacity_bytes),
            text(&m.kind),
            text(&m.manufacturer),
            text(&m.part_number),
            speed
        );
    }

    let _ = writeln!(o, "\nPlaca de vídeo");
    if r.gpus.is_empty() {
        let _ = writeln!(o, "  indisponível (o Windows não listou placas de vídeo)");
    }
    for g in r.gpus {
        let _ = writeln!(
            o,
            "  {} - driver {} - VRAM {}",
            text(&g.name),
            text(&g.driver),
            show(&g.vram_bytes, |v| gb(*v))
        );
    }

    let _ = writeln!(o, "\nArmazenamento");
    for d in r.disks {
        let _ = writeln!(
            o,
            "  Disco {}: {} - {} {} - {} - firmware {} - saúde {}",
            d.index,
            d.name,
            text(&d.media),
            text(&d.bus),
            gb(d.size_bytes),
            text(&d.firmware),
            text(&d.health)
        );
    }
    if r.disks.is_empty() {
        let _ = writeln!(o, "  indisponível (o Windows não listou discos físicos)");
    }
    for v in r.volumes {
        let _ = writeln!(
            o,
            "  Volume {}: {} {} - livre {} de {}",
            v.mount,
            v.file_system,
            v.kind,
            gb(v.free_bytes),
            gb(v.total_bytes)
        );
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryModule;

    fn k<T>(v: T) -> Field<T> {
        Field::known(v)
    }
    fn na<T>(why: &str) -> Field<T> {
        Field::unavailable(why)
    }

    fn sample() -> (
        SystemDetails,
        StaticInfo,
        CpuDetails,
        Vec<GpuDetails>,
        MemoryDetails,
        Vec<DiskInfo>,
        Vec<VolumeInfo>,
    ) {
        let system = SystemDetails {
            board_vendor: k("Gigabyte".to_string()),
            board_model: k("B450 AORUS M".to_string()),
            chipset: na("o Windows não informa o chipset"),
            bios_vendor: k("AMI".to_string()),
            bios_version: k("F60c".to_string()),
            bios_date: k("28/10/2020".to_string()),
            os_name: k("Windows 11 Home".to_string()),
            os_version: k("10.0.26200".to_string()),
            os_build: k("26200".to_string()),
            os_arch: k("64 bits".to_string()),
            uptime_secs: k(2 * 86_400 + 4 * 3600 + 17 * 60),
        };
        let info = StaticInfo {
            host: k("PC".to_string()),
            os: k("Windows".to_string()),
            cpu_model: k("CPU".to_string()),
            cpu_physical_cores: k(6),
            cpu_threads: k(6),
            memory_total_bytes: k(17_102_983_168.0),
            gpus: vec![],
        };
        let cpu = CpuDetails {
            model: k("AMD Ryzen 5 3500X".to_string()),
            vendor: k("AuthenticAMD".to_string()),
            codename: na("exigiria tabela"),
            socket: k("AM4".to_string()),
            process_node: na("exigiria tabela"),
            physical_cores: k(6),
            threads: k(6),
            signature: k("17h / 71h / 0".to_string()),
            base_clock_mhz: k(3593.0),
            tdp_watts: na("x"),
            tjmax_celsius: na("x"),
            multiplier: na("x"),
            caches: vec![],
            instruction_sets: vec![],
        };
        let gpus = vec![GpuDetails {
            name: k("NVIDIA GeForce RTX 3060".to_string()),
            architecture: k("Ampere".to_string()),
            shader_units: k(3584),
            vram_bytes: k(12_884_901_888.0),
            memory_bus_bits: k(192),
            bios: k("94.06".to_string()),
            driver: k("616.92".to_string()),
            pcie_link: na("x"),
        }];
        let memory = MemoryDetails {
            total_slots: k(4),
            modules: vec![MemoryModule {
                slot: "DIMM 0".into(),
                bank: "P0 CHANNEL A".into(),
                manufacturer: k("Kingston".to_string()),
                part_number: k("KHX3200C16D4/8GX".to_string()),
                serial: na("x"),
                kind: k("DDR4".to_string()),
                form_factor: k("DIMM".to_string()),
                capacity_bytes: 8_589_934_592.0,
                rated_mts: k(2400),
                configured_mts: k(2400),
                voltage_mv: k(1200),
            }],
            timings: na("SPD"),
            command_rate: na("SPD"),
            profile: na("SPD"),
            channels: na("x"),
        };
        let disks = vec![DiskInfo {
            index: 0,
            name: "MSI M370 2TB".into(),
            media: k("SSD".to_string()),
            bus: k("NVMe".to_string()),
            size_bytes: 2_048_408_248_320.0,
            firmware: k("EDFMD1.0".to_string()),
            serial: na("x"),
            health: k("Saudável".to_string()),
            temperature_c: na("x"),
            smart: na("x"),
            reliability: na("x"),
        }];
        let volumes = vec![VolumeInfo {
            mount: "C:\\".into(),
            label: String::new(),
            file_system: "NTFS".into(),
            kind: "SSD".into(),
            removable: false,
            total_bytes: 998_926_970_880.0,
            free_bytes: 578_223_083_520.0,
        }];
        (system, info, cpu, gpus, memory, disks, volumes)
    }

    fn report_of(
        parts: &(
            SystemDetails,
            StaticInfo,
            CpuDetails,
            Vec<GpuDetails>,
            MemoryDetails,
            Vec<DiskInfo>,
            Vec<VolumeInfo>,
        ),
    ) -> String {
        build_report(&ReportInput {
            generated_at: "2026-10-12 19:00",
            app_version: "0.1.0",
            system: &parts.0,
            info: &parts.1,
            cpu: &parts.2,
            gpus: &parts.3,
            memory: &parts.4,
            disks: &parts.5,
            volumes: &parts.6,
        })
    }

    #[test]
    fn the_report_lists_every_section_with_real_values() {
        let r = report_of(&sample());
        for expected in [
            "Argus 0.1.0 - relatório do sistema",
            "Gerado em 2026-10-12 19:00",
            "Modelo: B450 AORUS M",
            "BIOS: AMI F60c (28/10/2020)",
            "Tempo ligado: 2 d 04:17",
            "Modelo: AMD Ryzen 5 3500X",
            "Total: 17,1 GB",
            "DIMM 0 P0 CHANNEL A: 8,6 GB DDR4 Kingston KHX3200C16D4/8GX a 2400 MT/s",
            "NVIDIA GeForce RTX 3060 - driver 616.92 - VRAM 12,9 GB",
            "Disco 0: MSI M370 2TB - SSD NVMe - 2,05 TB - firmware EDFMD1.0 - saúde Saudável",
            "Volume C:\\: NTFS SSD - livre 578,2 GB de 998,9 GB",
        ] {
            assert!(r.contains(expected), "missing {expected:?} in:\n{r}");
        }
    }

    #[test]
    fn what_could_not_be_read_says_so_with_the_reason_instead_of_being_empty_or_invented() {
        let r = report_of(&sample());
        assert!(
            r.contains("Chipset: indisponível (o Windows não informa o chipset)"),
            "{r}"
        );
        let mut parts = sample();
        parts.4.modules.clear();
        parts.3.clear();
        parts.5.clear();
        parts.0.uptime_secs = na("sem acesso");
        let r = report_of(&parts);
        assert!(r.contains("Tempo ligado: indisponível (sem acesso)"));
        assert!(r.contains("indisponível (o Windows não listou placas de vídeo)"));
        assert!(r.contains("indisponível (o Windows não listou discos físicos)"));
        assert!(!r.contains("None") && !r.contains("null"));
    }

    #[test]
    fn short_uptimes_have_no_day_part() {
        assert_eq!(duration(3 * 3600 + 5 * 60), "03:05");
        assert_eq!(duration(86_400), "1 d 00:00");
    }
}
