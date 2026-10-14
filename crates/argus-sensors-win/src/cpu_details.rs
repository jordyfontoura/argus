//! Static processor facts: CPUID, cache topology and WMI. Anything that needs a driver or a
//! per-model table is reported as unavailable with the reason.

use argus_core::{CacheInfo, CpuDetails, Field};
use raw_cpuid::CpuId;
use serde::Deserialize;
use std::collections::BTreeMap;
use sysinfo::{CpuRefreshKind, RefreshKind, System};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationCache, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

const NEEDS_TABLE: &str = "o Windows não informa (exigiria tabela por modelo)";

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiProcessor {
    socket_designation: Option<String>,
    max_clock_speed: Option<u32>,
}

pub fn cpu_details() -> CpuDetails {
    let sys =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
    let brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .filter(|b| !b.is_empty());

    let cpuid = CpuId::new();
    let vendor = cpuid.get_vendor_info().map(|v| v.as_str().to_string());
    let signature = cpuid.get_feature_info().map(|f| {
        format!(
            "{:X}h / {:X}h / {}",
            f.family_id(),
            f.model_id(),
            f.stepping_id()
        )
    });

    let wmi: Option<WmiProcessor> = wmi::WMIConnection::new()
        .and_then(|c| c.raw_query("SELECT SocketDesignation, MaxClockSpeed FROM Win32_Processor"))
        .ok()
        .and_then(|mut v: Vec<WmiProcessor>| v.pop());
    let socket = wmi
        .as_ref()
        .and_then(|w| w.socket_designation.clone())
        .filter(|s| !s.trim().is_empty());
    let base = wmi
        .as_ref()
        .and_then(|w| w.max_clock_speed)
        .filter(|m| *m > 0)
        .map(f64::from);

    CpuDetails {
        model: Field::from_option(brand, "o Windows não informou o modelo"),
        vendor: Field::from_option(vendor, "o processador não informou o fabricante"),
        codename: Field::unavailable(NEEDS_TABLE),
        socket: Field::from_option(socket, "o Windows (WMI) não informou o soquete"),
        process_node: Field::unavailable(NEEDS_TABLE),
        physical_cores: Field::from_option(
            System::physical_core_count().map(|n| n as u32),
            "o Windows não informou os núcleos físicos",
        ),
        threads: Field::known(sys.cpus().len() as u32),
        signature: Field::from_option(signature, "o processador não expõe o CPUID"),
        base_clock_mhz: Field::from_option(base, "o Windows (WMI) não informou o clock base"),
        tdp_watts: Field::unavailable(NEEDS_TABLE),
        tjmax_celsius: Field::unavailable(NEEDS_TABLE),
        multiplier: Field::unavailable("o Argus não oferece um multiplicador único para esta CPU"),
        caches: caches(),
        instruction_sets: instruction_sets(),
    }
}

/// Instruction-set extensions from CPUID, in the order they are usually listed.
fn instruction_sets() -> Vec<String> {
    let cpuid = CpuId::new();
    let mut out = Vec::new();
    let mut add = |on: bool, name: &str| {
        if on {
            out.push(name.to_string());
        }
    };
    if let Some(f) = cpuid.get_feature_info() {
        add(f.has_mmx(), "MMX");
        add(f.has_sse(), "SSE");
        add(f.has_sse2(), "SSE2");
        add(f.has_sse3(), "SSE3");
        add(f.has_ssse3(), "SSSE3");
        add(f.has_sse41(), "SSE4.1");
        add(f.has_sse42(), "SSE4.2");
        add(f.has_aesni(), "AES");
        add(f.has_avx(), "AVX");
        add(f.has_fma(), "FMA3");
        add(f.has_vmx(), "VT-x");
    }
    if let Some(e) = cpuid.get_extended_feature_info() {
        add(e.has_avx2(), "AVX2");
        add(e.has_bmi1(), "BMI1");
        add(e.has_bmi2(), "BMI2");
        add(e.has_sha(), "SHA");
        add(e.has_avx512f(), "AVX-512");
    }
    if let Some(x) = cpuid.get_extended_processor_and_feature_identifiers() {
        add(x.has_sse4a(), "SSE4A");
        add(x.has_64bit_mode(), "x86-64");
        add(x.has_svm(), "AMD-V");
    }
    out
}

/// Cache levels from `GetLogicalProcessorInformationEx`, grouped by level and kind.
fn caches() -> Vec<CacheInfo> {
    let mut len = 0u32;
    // SAFETY: a null buffer only asks for the size; the call fails with INSUFFICIENT_BUFFER.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationCache, None, &mut len) };
    if len == 0 {
        return Vec::new();
    }
    // 8-byte aligned storage for the variable-length records.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    let ptr = buf
        .as_mut_ptr()
        .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>();
    // SAFETY: `buf` holds at least `len` bytes and is aligned for the record type.
    if unsafe { GetLogicalProcessorInformationEx(RelationCache, Some(ptr), &mut len) }.is_err() {
        return Vec::new();
    }

    // (level, kind) -> (instances, size, ways)
    let mut groups: BTreeMap<(u8, i32), (u32, u32, u8)> = BTreeMap::new();
    let base = buf.as_ptr().cast::<u8>();
    let mut offset = 0usize;
    while offset + std::mem::size_of::<u32>() * 2 <= len as usize {
        // SAFETY: `offset` stays inside the `len` bytes written by the API and every record
        // starts with Relationship and Size, which give the offset of the next one.
        let record = unsafe {
            &*base
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        if record.Size == 0 {
            break;
        }
        if record.Relationship == RelationCache {
            // SAFETY: the relationship says the Cache member of the union is the active one.
            let c = unsafe { record.Anonymous.Cache };
            let entry =
                groups
                    .entry((c.Level, c.Type.0))
                    .or_insert((0, c.CacheSize, c.Associativity));
            entry.0 += 1;
        }
        offset += record.Size as usize;
    }

    groups
        .into_iter()
        .map(|((level, kind), (instances, size, ways))| CacheInfo {
            name: match (level, kind) {
                (1, 1) => "L1 instruções".to_string(),
                (1, 2) => "L1 dados".to_string(),
                (l, _) => format!("L{l}"),
            },
            instances,
            size_bytes_each: f64::from(size),
            ways: (ways != 0 && ways != 0xFF).then_some(u32::from(ways)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_this_cpu_with_a_reason_for_what_it_cannot_know() {
        let d = cpu_details();
        assert!(d.threads.value.unwrap_or(0) >= 1);
        assert!(d.signature.value.is_some(), "x86 CPUID is always readable");
        assert!(d.multiplier.value.is_none() && d.multiplier.unavailable.is_some());
        assert!(!d.caches.is_empty(), "cache topology should be readable");
        assert!(d.instruction_sets.iter().any(|s| s == "SSE2"));
        // L1 data exists on every x86 CPU and is smaller than L3.
        let l1 = d
            .caches
            .iter()
            .find(|c| c.name == "L1 dados")
            .expect("L1 data");
        assert!(l1.size_bytes_each >= 8.0 * 1024.0);
    }
}
