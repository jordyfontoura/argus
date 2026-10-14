//! Physical disks (identity from the storage subsystem, health from the NVMe log when the
//! Windows build lets a standard user read it) and mounted volumes.

use crate::smart::{self, NvmeHealth};
use argus_core::{DiskInfo, Field, SmartAttribute, VolumeInfo};
use serde::Deserialize;
use sysinfo::Disks;

const NEEDS_ADMIN_SATA: &str = "o Windows só entrega o SMART de discos SATA/ATA a um administrador";
const RELIABILITY_NEEDS_ADMIN: &str =
    "os contadores de confiabilidade do Windows exigem administrador";

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiPhysicalDisk {
    device_id: Option<String>,
    friendly_name: Option<String>,
    media_type: Option<u32>,
    bus_type: Option<u32>,
    health_status: Option<u32>,
    size: Option<u64>,
    firmware_version: Option<String>,
    serial_number: Option<String>,
}

fn clean(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().trim_end_matches('.').to_string())
        .filter(|t| !t.is_empty())
}

fn media_name(code: u32) -> Option<&'static str> {
    Some(match code {
        3 => "HDD",
        4 => "SSD",
        5 => "SCM",
        _ => return None,
    })
}

pub(crate) const BUS_NVME: u32 = 17;

pub(crate) fn bus_name(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        7 => "USB",
        8 => "RAID",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        16 => "Storage Spaces",
        17 => "NVMe",
        _ => return None,
    })
}

fn health_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0 => "Saudável",
        1 => "Atenção",
        2 => "Com problemas",
        _ => return None,
    })
}

fn level(text: &str) -> Option<String> {
    Some(text.to_string())
}

/// Human-readable attributes with a verdict where the standard defines one.
pub(crate) fn nvme_attributes(h: &NvmeHealth) -> Vec<SmartAttribute> {
    let attr = |name: &str, value: String, lvl: Option<&str>| SmartAttribute {
        name: name.to_string(),
        value,
        level: lvl.and_then(level),
    };
    let tb = |bytes: f64| format!("{} TB", format!("{:.1}", bytes / 1e12).replace('.', ","));
    let mut out = Vec::new();
    let warning = if h.critical_warning == 0 {
        "ok"
    } else {
        "crit"
    };
    out.push(attr(
        "Aviso crítico",
        if h.critical_warning == 0 {
            "nenhum".to_string()
        } else {
            format!("0x{:02X}", h.critical_warning)
        },
        Some(warning),
    ));
    let temp = if h.temperature_c >= 70.0 {
        "crit"
    } else if h.temperature_c >= 60.0 {
        "warn"
    } else {
        "ok"
    };
    out.push(attr(
        "Temperatura",
        format!("{:.0} °C", h.temperature_c),
        Some(temp),
    ));
    let used = if h.percentage_used >= 90 {
        "crit"
    } else if h.percentage_used >= 70 {
        "warn"
    } else {
        "ok"
    };
    out.push(attr(
        "Vida útil consumida",
        format!("{} %", h.percentage_used),
        Some(used),
    ));
    let spare = if h.available_spare_pct < h.spare_threshold_pct {
        "crit"
    } else if h.available_spare_pct < 20 {
        "warn"
    } else {
        "ok"
    };
    out.push(attr(
        "Reserva disponível",
        format!(
            "{} % (limite {} %)",
            h.available_spare_pct, h.spare_threshold_pct
        ),
        Some(spare),
    ));
    out.push(attr("Dados lidos", tb(h.data_read_bytes), None));
    out.push(attr("Dados gravados", tb(h.data_written_bytes), None));
    out.push(attr("Horas ligado", h.power_on_hours.to_string(), None));
    out.push(attr("Ciclos de energia", h.power_cycles.to_string(), None));
    out.push(attr(
        "Desligamentos inesperados",
        h.unsafe_shutdowns.to_string(),
        None,
    ));
    out.push(attr(
        "Erros de mídia",
        h.media_errors.to_string(),
        Some(if h.media_errors == 0 { "ok" } else { "warn" }),
    ));
    out
}

fn disk_from(w: WmiPhysicalDisk) -> Option<DiskInfo> {
    let index: u32 = w.device_id.as_deref()?.trim().parse().ok()?;
    let bus = w.bus_type;
    let (temperature_c, smart) = match bus {
        Some(BUS_NVME) => match smart::nvme_health(index) {
            Ok(h) => (
                Field::known(h.temperature_c),
                Field::known(nvme_attributes(&h)),
            ),
            Err(why) => (
                smart::drive_temperature(index)
                    .map_or_else(|_| Field::unavailable(why.clone()), Field::known),
                Field::unavailable(why),
            ),
        },
        Some(_) => (
            smart::drive_temperature(index)
                .map_or_else(|_| Field::unavailable(NEEDS_ADMIN_SATA), Field::known),
            Field::unavailable(NEEDS_ADMIN_SATA),
        ),
        None => (
            Field::unavailable("o Windows não informou o tipo de conexão do disco"),
            Field::unavailable("o Windows não informou o tipo de conexão do disco"),
        ),
    };
    Some(DiskInfo {
        index,
        name: clean(w.friendly_name).unwrap_or_else(|| format!("Disco {index}")),
        media: Field::from_option(
            w.media_type.and_then(media_name).map(str::to_string),
            "o Windows não informou o tipo (SSD/HDD)",
        ),
        bus: Field::from_option(
            bus.and_then(bus_name).map(str::to_string),
            "o Windows não informou a conexão",
        ),
        size_bytes: w.size.map_or(0.0, |s| s as f64),
        firmware: Field::from_option(
            clean(w.firmware_version),
            "o Windows não informou o firmware",
        ),
        serial: Field::from_option(
            clean(w.serial_number),
            "o Windows não informou o número de série",
        ),
        health: Field::from_option(
            w.health_status.and_then(health_name).map(str::to_string),
            "o Windows não informou a saúde geral",
        ),
        temperature_c,
        smart,
        reliability: Field::unavailable(RELIABILITY_NEEDS_ADMIN),
    })
}

/// Every physical disk the storage subsystem lists, sorted by index.
pub fn physical_disks() -> Vec<DiskInfo> {
    let rows: Vec<WmiPhysicalDisk> = wmi::WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage")
        .and_then(|c| {
            c.raw_query(
                "SELECT DeviceId, FriendlyName, MediaType, BusType, HealthStatus, Size, FirmwareVersion, SerialNumber FROM MSFT_PhysicalDisk",
            )
        })
        .unwrap_or_default();
    let mut disks: Vec<DiskInfo> = rows.into_iter().filter_map(disk_from).collect();
    disks.sort_by_key(|d| d.index);
    disks
}

/// Mounted volumes with capacity and file system.
pub fn volumes() -> Vec<VolumeInfo> {
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|d| {
            let total = d.total_space();
            VolumeInfo {
                mount: d.mount_point().to_string_lossy().to_string(),
                label: d.name().to_string_lossy().to_string(),
                file_system: d.file_system().to_string_lossy().to_string(),
                kind: format!("{}", d.kind()),
                removable: d.is_removable(),
                total_bytes: total as f64,
                free_bytes: d.available_space() as f64,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> NvmeHealth {
        NvmeHealth {
            critical_warning: 0,
            temperature_c: 45.0,
            available_spare_pct: 100,
            spare_threshold_pct: 10,
            percentage_used: 5,
            data_read_bytes: 2e12,
            data_written_bytes: 1.5e12,
            power_cycles: 100,
            power_on_hours: 1000,
            unsafe_shutdowns: 3,
            media_errors: 0,
        }
    }

    fn lvl(attrs: &[SmartAttribute], name: &str) -> Option<String> {
        attrs
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.level.clone())
    }

    #[test]
    fn a_healthy_nvme_has_no_warnings() {
        let attrs = nvme_attributes(&healthy());
        assert!(attrs
            .iter()
            .filter_map(|a| a.level.as_deref())
            .all(|l| l == "ok"));
        assert_eq!(
            attrs
                .iter()
                .find(|a| a.name == "Dados lidos")
                .unwrap()
                .value,
            "2,0 TB"
        );
    }

    #[test]
    fn worn_hot_or_failing_drives_are_flagged() {
        let mut h = healthy();
        h.percentage_used = 85;
        h.temperature_c = 65.0;
        assert_eq!(
            lvl(&nvme_attributes(&h), "Vida útil consumida").as_deref(),
            Some("warn")
        );
        assert_eq!(
            lvl(&nvme_attributes(&h), "Temperatura").as_deref(),
            Some("warn")
        );
        h.percentage_used = 95;
        h.temperature_c = 75.0;
        h.available_spare_pct = 5;
        h.critical_warning = 0x04;
        h.media_errors = 2;
        let a = nvme_attributes(&h);
        assert_eq!(lvl(&a, "Vida útil consumida").as_deref(), Some("crit"));
        assert_eq!(lvl(&a, "Temperatura").as_deref(), Some("crit"));
        assert_eq!(lvl(&a, "Reserva disponível").as_deref(), Some("crit"));
        assert_eq!(lvl(&a, "Aviso crítico").as_deref(), Some("crit"));
        assert_eq!(lvl(&a, "Erros de mídia").as_deref(), Some("warn"));
    }

    #[test]
    fn codes_map_to_names_and_unknown_ones_are_not_invented() {
        assert_eq!(bus_name(17), Some("NVMe"));
        assert_eq!(bus_name(11), Some("SATA"));
        assert_eq!(bus_name(999), None);
        assert_eq!(media_name(4), Some("SSD"));
        assert_eq!(media_name(0), None);
        assert_eq!(health_name(0), Some("Saudável"));
        assert_eq!(health_name(7), None);
    }

    #[test]
    fn a_sata_disk_reports_unavailable_with_the_reason_instead_of_made_up_data() {
        let d = disk_from(WmiPhysicalDisk {
            device_id: Some("99".into()),
            friendly_name: Some("Test SATA".into()),
            media_type: Some(3),
            bus_type: Some(11),
            health_status: Some(0),
            size: Some(1_000_000_000_000),
            firmware_version: Some("1.0".into()),
            serial_number: Some("ABC.".into()),
        })
        .unwrap();
        assert!(d.smart.value.is_none());
        assert!(d
            .smart
            .unavailable
            .as_deref()
            .unwrap()
            .contains("administrador"));
        assert_eq!(
            d.serial.value.as_deref(),
            Some("ABC"),
            "trailing dot of the Windows serial is removed"
        );
        assert_eq!(d.media.value.as_deref(), Some("HDD"));
        assert!(d.reliability.unavailable.is_some());
    }

    #[test]
    fn lists_the_disks_and_volumes_of_this_machine() {
        let disks = physical_disks();
        assert!(!disks.is_empty(), "this machine has a disk");
        assert!(disks
            .iter()
            .all(|d| d.size_bytes > 0.0 && !d.name.is_empty()));
        // Every field is a value or a reason, never empty.
        for d in &disks {
            assert!(d.health.value.is_some() ^ d.health.unavailable.is_some());
            assert!(d.smart.value.is_some() ^ d.smart.unavailable.is_some());
        }
        let vols = volumes();
        assert!(vols.iter().any(|v| v.mount.starts_with("C:")));
        assert!(vols.iter().all(|v| v.total_bytes >= v.free_bytes));
    }
}
