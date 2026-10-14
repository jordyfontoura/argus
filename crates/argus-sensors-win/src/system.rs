//! Motherboard, firmware and operating system facts through WMI (no administrator needed).

use argus_core::{Field, SystemDetails};
use serde::Deserialize;
use sysinfo::System;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiBoard {
    manufacturer: Option<String>,
    product: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiBios {
    manufacturer: Option<String>,
    #[serde(rename = "SMBIOSBIOSVersion")]
    smbios_bios_version: Option<String>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiOs {
    caption: Option<String>,
    version: Option<String>,
    build_number: Option<String>,
    #[serde(rename = "OSArchitecture")]
    os_architecture: Option<String>,
}

fn clean(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| {
        let lower = t.to_ascii_lowercase();
        !t.is_empty()
            && !matches!(
                lower.as_str(),
                "default string"
                    | "to be filled by o.e.m."
                    | "system manufacturer"
                    | "system product name"
                    | "not applicable"
            )
    })
}

/// A WMI CIM date (`20201028210000.000000+000`) as `28/10/2020`.
pub(crate) fn cim_date(raw: &str) -> Option<String> {
    let digits = raw.get(..8)?;
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!(
        "{}/{}/{}",
        &digits[6..8],
        &digits[4..6],
        &digits[0..4]
    ))
}

fn first<T: for<'de> Deserialize<'de>>(query: &str) -> Option<T> {
    wmi::WMIConnection::new()
        .ok()?
        .raw_query::<T>(query)
        .ok()?
        .into_iter()
        .next()
}

pub fn system_details() -> SystemDetails {
    let board: Option<WmiBoard> = first("SELECT Manufacturer, Product FROM Win32_BaseBoard");
    let bios: Option<WmiBios> =
        first("SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS");
    let os: Option<WmiOs> =
        first("SELECT Caption, Version, BuildNumber, OSArchitecture FROM Win32_OperatingSystem");
    let no_wmi = "o Windows (WMI) não informou";
    SystemDetails {
        board_vendor: Field::from_option(
            clean(board.as_ref().and_then(|b| b.manufacturer.clone())),
            no_wmi,
        ),
        board_model: Field::from_option(
            clean(board.as_ref().and_then(|b| b.product.clone())),
            no_wmi,
        ),
        chipset: Field::unavailable("o Windows não informa o chipset"),
        bios_vendor: Field::from_option(
            clean(bios.as_ref().and_then(|b| b.manufacturer.clone())),
            no_wmi,
        ),
        bios_version: Field::from_option(
            clean(bios.as_ref().and_then(|b| b.smbios_bios_version.clone())),
            no_wmi,
        ),
        bios_date: Field::from_option(
            bios.as_ref()
                .and_then(|b| b.release_date.as_deref())
                .and_then(cim_date),
            no_wmi,
        ),
        os_name: Field::from_option(clean(os.as_ref().and_then(|o| o.caption.clone())), no_wmi),
        os_version: Field::from_option(clean(os.as_ref().and_then(|o| o.version.clone())), no_wmi),
        os_build: Field::from_option(
            clean(os.as_ref().and_then(|o| o.build_number.clone())),
            no_wmi,
        ),
        os_arch: Field::from_option(
            clean(os.as_ref().and_then(|o| o.os_architecture.clone())),
            no_wmi,
        ),
        uptime_secs: Field::known(System::uptime()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cim_dates_become_day_month_year() {
        assert_eq!(
            cim_date("20201028210000.000000+000").as_deref(),
            Some("28/10/2020")
        );
        assert_eq!(cim_date("garbage"), None);
        assert_eq!(cim_date("2020"), None);
    }

    #[test]
    fn placeholder_strings_from_cheap_firmware_are_not_shown_as_facts() {
        assert_eq!(clean(Some("To be filled by O.E.M.".into())), None);
        assert_eq!(clean(Some("  ".into())), None);
        assert_eq!(clean(Some(" ASUS ".into())).as_deref(), Some("ASUS"));
    }

    #[test]
    fn reads_this_machine_and_explains_the_chipset() {
        let s = system_details();
        assert!(s.os_name.value.is_some());
        assert!(s.uptime_secs.value.unwrap_or(0) > 0);
        assert!(s.chipset.value.is_none() && s.chipset.unavailable.is_some());
    }
}
