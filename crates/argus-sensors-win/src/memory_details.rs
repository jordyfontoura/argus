//! Installed memory as described by the firmware (SMBIOS through WMI), no driver needed.

use argus_core::{Field, MemoryDetails, MemoryModule};
use serde::Deserialize;

const SPD_ONLY: &str = "fica no chip SPD do módulo; o Windows não expõe sem driver";

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiModule {
    bank_label: Option<String>,
    device_locator: Option<String>,
    manufacturer: Option<String>,
    part_number: Option<String>,
    serial_number: Option<String>,
    capacity: Option<u64>,
    speed: Option<u32>,
    configured_clock_speed: Option<u32>,
    configured_voltage: Option<u32>,
    #[serde(rename = "SMBIOSMemoryType")]
    smbios_memory_type: Option<u32>,
    form_factor: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiArray {
    memory_devices: Option<u32>,
}

fn clean(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("unknown") && t != "0")
}

/// SMBIOS "Memory Device" type code to a name.
pub(crate) fn memory_kind(code: u32) -> Option<&'static str> {
    Some(match code {
        20 => "DDR",
        21 => "DDR2",
        24 => "DDR3",
        26 => "DDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => return None,
    })
}

pub(crate) fn form_factor(code: u32) -> Option<&'static str> {
    Some(match code {
        8 => "DIMM",
        12 => "SO-DIMM",
        _ => return None,
    })
}

pub fn memory_details() -> MemoryDetails {
    let conn = wmi::WMIConnection::new();
    let modules: Result<Vec<WmiModule>, String> = conn
        .as_ref()
        .map_err(|e| e.to_string())
        .and_then(|c| {
            c.raw_query(
                "SELECT BankLabel, DeviceLocator, Manufacturer, PartNumber, SerialNumber, Capacity, \
                 Speed, ConfiguredClockSpeed, ConfiguredVoltage, SMBIOSMemoryType, FormFactor \
                 FROM Win32_PhysicalMemory",
            )
            .map_err(|e| e.to_string())
        });
    let slots: Option<u32> = conn
        .as_ref()
        .ok()
        .and_then(|c| {
            c.raw_query::<WmiArray>("SELECT MemoryDevices FROM Win32_PhysicalMemoryArray")
                .ok()
        })
        .and_then(|v| {
            v.into_iter()
                .filter_map(|a| a.memory_devices)
                .reduce(|a, b| a + b)
        });

    let modules = match modules {
        Ok(list) => list
            .into_iter()
            .map(|m| {
                let kind = m
                    .smbios_memory_type
                    .and_then(memory_kind)
                    .map(str::to_string);
                MemoryModule {
                    slot: clean(m.device_locator).unwrap_or_else(|| "?".to_string()),
                    bank: clean(m.bank_label).unwrap_or_default(),
                    manufacturer: Field::from_option(
                        clean(m.manufacturer),
                        "o firmware não informou o fabricante",
                    ),
                    part_number: Field::from_option(
                        clean(m.part_number),
                        "o firmware não informou o modelo",
                    ),
                    serial: Field::from_option(
                        clean(m.serial_number),
                        "o firmware não informou o número de série",
                    ),
                    kind: Field::from_option(kind, "o firmware não informou o tipo"),
                    form_factor: Field::from_option(
                        m.form_factor.and_then(form_factor).map(str::to_string),
                        "o firmware não informou o formato",
                    ),
                    capacity_bytes: m.capacity.map_or(0.0, |c| c as f64),
                    rated_mts: Field::from_option(
                        m.speed.filter(|s| *s > 0),
                        "o firmware não informou a velocidade nominal",
                    ),
                    configured_mts: Field::from_option(
                        m.configured_clock_speed.filter(|s| *s > 0),
                        "o firmware não informou a velocidade atual",
                    ),
                    voltage_mv: Field::from_option(
                        m.configured_voltage.filter(|v| *v > 0),
                        "o firmware não informou a tensão",
                    ),
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    MemoryDetails {
        total_slots: Field::from_option(
            slots.filter(|s| *s > 0),
            "o firmware não informou a quantidade de slots",
        ),
        modules,
        timings: Field::unavailable(SPD_ONLY),
        command_rate: Field::unavailable(SPD_ONLY),
        profile: Field::unavailable(SPD_ONLY),
        channels: Field::unavailable("o Windows não informa a configuração de canais"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smbios_codes_map_to_names() {
        assert_eq!(memory_kind(26), Some("DDR4"));
        assert_eq!(memory_kind(34), Some("DDR5"));
        assert_eq!(memory_kind(0), None);
        assert_eq!(form_factor(8), Some("DIMM"));
        assert_eq!(form_factor(12), Some("SO-DIMM"));
    }

    #[test]
    fn reads_the_installed_modules_and_never_invents_spd_data() {
        let d = memory_details();
        assert!(!d.modules.is_empty(), "this machine has memory installed");
        assert!(d.modules.iter().all(|m| m.capacity_bytes > 0.0));
        // Timings and profiles need the SPD chip: always explained, never guessed.
        assert!(d.timings.value.is_none() && d.timings.unavailable.is_some());
        assert!(d.profile.value.is_none() && d.profile.unavailable.is_some());
    }
}
