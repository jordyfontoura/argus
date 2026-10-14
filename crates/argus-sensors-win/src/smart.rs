//! Disk health without administrator rights, through `IOCTL_STORAGE_QUERY_PROPERTY` on a handle
//! opened with no access mask (the only kind a standard user can get).
//!
//! On the verified hardware, the NVMe health log and drive temperature answer to a standard user;
//! SATA/ATA SMART and
//! the storage reliability counters do not, so those are reported as unavailable with the reason.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;
use windows::Win32::System::IO::DeviceIoControl;

const PROPERTY_DEVICE_PROTOCOL: i32 = 50; // StorageDeviceProtocolSpecificProperty
const PROPERTY_DEVICE_TEMPERATURE: i32 = 52; // StorageDeviceTemperatureProperty
const QUERY_STANDARD: i32 = 0;
const PROTOCOL_NVME: u32 = 3;
const NVME_DATA_LOG_PAGE: u32 = 2;
const NVME_LOG_HEALTH: u32 = 2;
const HEALTH_LOG_LEN: usize = 512;
/// Offset of the protocol block inside the query (right after PropertyId and QueryType).
const QUERY_HEADER: usize = 8;
const PROTOCOL_BLOCK: usize = 40;

/// The NVMe SMART / health information log, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct NvmeHealth {
    pub critical_warning: u8,
    pub temperature_c: f64,
    pub available_spare_pct: u8,
    pub spare_threshold_pct: u8,
    pub percentage_used: u8,
    pub data_read_bytes: f64,
    pub data_written_bytes: f64,
    pub power_cycles: u64,
    pub power_on_hours: u64,
    pub unsafe_shutdowns: u64,
    pub media_errors: u64,
}

fn u128_le(bytes: &[u8]) -> f64 {
    bytes
        .iter()
        .rev()
        .fold(0f64, |acc, b| acc * 256.0 + f64::from(*b))
}

fn u64_le(bytes: &[u8]) -> u64 {
    // Counters beyond u64 do not occur in practice; saturate instead of wrapping.
    if bytes[8..16].iter().any(|b| *b != 0) {
        return u64::MAX;
    }
    u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"))
}

/// Decodes the 512 bytes of NVMe log page 2. Pure, so it is unit-tested with fixtures.
pub fn parse_nvme_health(log: &[u8]) -> Option<NvmeHealth> {
    if log.len() < 192 {
        return None;
    }
    let kelvin = u16::from_le_bytes([log[1], log[2]]);
    // One data unit is 1000 blocks of 512 bytes.
    let unit = 512_000.0;
    Some(NvmeHealth {
        critical_warning: log[0],
        temperature_c: f64::from(kelvin) - 273.15,
        available_spare_pct: log[3],
        spare_threshold_pct: log[4],
        percentage_used: log[5],
        data_read_bytes: u128_le(&log[32..48]) * unit,
        data_written_bytes: u128_le(&log[48..64]) * unit,
        power_cycles: u64_le(&log[112..128]),
        power_on_hours: u64_le(&log[128..144]),
        unsafe_shutdowns: u64_le(&log[144..160]),
        media_errors: u64_le(&log[160..176]),
    })
}

fn explain(code: i32) -> String {
    match code as u32 {
        0x8007_0005 => "o Windows só entrega este dado a um administrador".to_string(),
        0x8007_0001 | 0x8007_0032 => "este disco não oferece essa leitura pelo Windows".to_string(),
        0x8007_0002 | 0x8007_0003 => "o disco não está acessível".to_string(),
        other => format!("o Windows recusou a leitura (0x{other:08X})"),
    }
}

/// Opens `\\.\PhysicalDriveN` with no access rights, enough to ask a storage query.
fn open(index: u32) -> Result<HANDLE, String> {
    let path: Vec<u16> = format!("\\\\.\\PhysicalDrive{index}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `path` is NUL terminated and lives for the call; no access is requested.
    unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| explain(e.code().0))
}

fn query(handle: HANDLE, input: &mut [u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![0u8; input.len().max(256)];
    let mut returned = 0u32;
    // SAFETY: both buffers are valid for the sizes passed and outlive the call.
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(input.as_ptr().cast()),
            input.len() as u32,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|e| explain(e.code().0))?;
    out.truncate(returned as usize);
    Ok(out)
}

/// NVMe health log of physical drive `index`.
pub fn nvme_health(index: u32) -> Result<NvmeHealth, String> {
    let handle = open(index)?;
    let mut input = vec![0u8; QUERY_HEADER + PROTOCOL_BLOCK + HEALTH_LOG_LEN];
    input[0..4].copy_from_slice(&PROPERTY_DEVICE_PROTOCOL.to_le_bytes());
    input[4..8].copy_from_slice(&QUERY_STANDARD.to_le_bytes());
    let p = &mut input[QUERY_HEADER..QUERY_HEADER + PROTOCOL_BLOCK];
    p[0..4].copy_from_slice(&PROTOCOL_NVME.to_le_bytes());
    p[4..8].copy_from_slice(&NVME_DATA_LOG_PAGE.to_le_bytes());
    p[8..12].copy_from_slice(&NVME_LOG_HEALTH.to_le_bytes());
    p[16..20].copy_from_slice(&(PROTOCOL_BLOCK as u32).to_le_bytes());
    p[20..24].copy_from_slice(&(HEALTH_LOG_LEN as u32).to_le_bytes());
    let result = query(handle, &mut input);
    // SAFETY: the handle was opened above and is closed exactly once.
    unsafe {
        let _ = CloseHandle(handle);
    }
    let out = result?;
    // Descriptor: Version, Size, then the protocol block whose offset says where the log starts.
    if out.len() < 8 + PROTOCOL_BLOCK {
        return Err("o disco respondeu sem dados de saúde".to_string());
    }
    let offset = u32::from_le_bytes(out[8 + 16..8 + 20].try_into().expect("4 bytes")) as usize;
    let start = 8 + offset;
    let log = out
        .get(start..)
        .ok_or_else(|| "o disco respondeu com um formato inesperado".to_string())?;
    parse_nvme_health(log).ok_or_else(|| "o disco respondeu com poucos dados de saúde".to_string())
}

/// Composite temperature in °C from the generic temperature property (works for any bus that
/// reports it, no administrator needed).
pub fn drive_temperature(index: u32) -> Result<f64, String> {
    let handle = open(index)?;
    let mut input = vec![0u8; 12];
    input[0..4].copy_from_slice(&PROPERTY_DEVICE_TEMPERATURE.to_le_bytes());
    input[4..8].copy_from_slice(&QUERY_STANDARD.to_le_bytes());
    let result = query(handle, &mut input);
    // SAFETY: the handle was opened above and is closed exactly once.
    unsafe {
        let _ = CloseHandle(handle);
    }
    let out = result?;
    // Descriptor: Version(4) Size(4) CriticalTemp(2) WarningTemp(2) InfoCount(2) Reserved(6),
    // then entries of Index(2) Temperature(2) OverThreshold(2) UnderThreshold(2) Flags/Reserved.
    if out.len() < 24 {
        return Err("o disco não informou a temperatura".to_string());
    }
    let count = u16::from_le_bytes([out[12], out[13]]);
    if count == 0 {
        return Err("o disco não informou a temperatura".to_string());
    }
    let celsius = i16::from_le_bytes([out[20], out[21]]);
    Ok(f64::from(celsius))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(f: impl Fn(&mut [u8; 512])) -> [u8; 512] {
        let mut l = [0u8; 512];
        f(&mut l);
        l
    }

    #[test]
    fn decodes_a_health_log() {
        let l = log(|l| {
            l[0] = 0x04;
            l[1..3].copy_from_slice(&(273u16 + 41).to_le_bytes()); // 41 °C in Kelvin
            l[3] = 100;
            l[4] = 10;
            l[5] = 7;
            l[32..40].copy_from_slice(&2_000_000u64.to_le_bytes()); // data units read
            l[48..56].copy_from_slice(&1_000u64.to_le_bytes());
            l[112..120].copy_from_slice(&321u64.to_le_bytes());
            l[128..136].copy_from_slice(&1234u64.to_le_bytes());
            l[144..152].copy_from_slice(&9u64.to_le_bytes());
            l[160..168].copy_from_slice(&2u64.to_le_bytes());
        });
        let h = parse_nvme_health(&l).unwrap();
        assert_eq!(h.critical_warning, 4);
        assert!((h.temperature_c - 40.85).abs() < 1e-9);
        assert_eq!(
            (
                h.available_spare_pct,
                h.spare_threshold_pct,
                h.percentage_used
            ),
            (100, 10, 7)
        );
        assert_eq!(h.data_read_bytes, 2_000_000.0 * 512_000.0);
        assert_eq!(h.data_written_bytes, 1_000.0 * 512_000.0);
        assert_eq!(
            (
                h.power_cycles,
                h.power_on_hours,
                h.unsafe_shutdowns,
                h.media_errors
            ),
            (321, 1234, 9, 2)
        );
    }

    #[test]
    fn a_short_or_empty_log_is_rejected_instead_of_read_out_of_bounds() {
        assert!(parse_nvme_health(&[]).is_none());
        assert!(parse_nvme_health(&[0u8; 100]).is_none());
        assert!(parse_nvme_health(&[0u8; 192]).is_some());
    }

    #[test]
    fn huge_counters_saturate_instead_of_wrapping() {
        let l = log(|l| l[128..144].fill(0xFF));
        assert_eq!(parse_nvme_health(&l).unwrap().power_on_hours, u64::MAX);
    }

    #[test]
    fn errors_are_explained_in_plain_language() {
        assert!(explain(0x8007_0005u32 as i32).contains("administrador"));
        assert!(explain(0x1234_5678).contains("0x12345678"));
    }

    /// Spike: what does this machine answer to a standard user? Printed, not asserted, because the
    /// answer depends on the hardware (NVMe works, SATA usually does not).
    #[test]
    fn spike_report_for_physical_drive_zero() {
        println!("nvme_health(0)      = {:?}", nvme_health(0));
        println!("drive_temperature(0) = {:?}", drive_temperature(0));
    }
}
