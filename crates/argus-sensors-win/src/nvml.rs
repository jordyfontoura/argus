//! NVIDIA Management Library, loaded at run time. There is no link-time dependency: a machine
//! without an NVIDIA driver simply fails to load `nvml.dll` and the caller falls back.

use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::path::Path;

type Device = *mut c_void;
type Status = c_int;

const SUCCESS: Status = 0;
const NOT_SUPPORTED: Status = 3;
const NO_PERMISSION: Status = 4;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct MemoryInfo {
    total: u64,
    free: u64,
    used: u64,
}

/// Bits of `nvmlDeviceGetCurrentClocksThrottleReasons` grouped the way the UI shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Limiters {
    pub idle: bool,
    pub power: bool,
    pub thermal: bool,
    pub hw_slowdown: bool,
}

impl Limiters {
    pub(crate) fn from_mask(mask: u64) -> Self {
        Self {
            idle: mask & 0x1 != 0,
            power: mask & (0x4 | 0x80) != 0,
            thermal: mask & (0x20 | 0x40) != 0,
            hw_slowdown: mask & 0x8 != 0,
        }
    }
}

fn explain(status: Status) -> String {
    match status {
        NOT_SUPPORTED => "o driver da NVIDIA não oferece esta leitura nesta placa".to_string(),
        NO_PERMISSION => "o Windows negou a leitura sem administrador".to_string(),
        other => format!("o NVML respondeu com o erro {other}"),
    }
}

type InitFn = unsafe extern "C" fn() -> Status;
type CountFn = unsafe extern "C" fn(*mut c_uint) -> Status;
type HandleFn = unsafe extern "C" fn(c_uint, *mut Device) -> Status;
type NameFn = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> Status;
type UtilFn = unsafe extern "C" fn(Device, *mut Utilization) -> Status;
type MemFn = unsafe extern "C" fn(Device, *mut MemoryInfo) -> Status;
type TempFn = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> Status;
type ClockFn = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> Status;
type UintFn = unsafe extern "C" fn(Device, *mut c_uint) -> Status;
type MaskFn = unsafe extern "C" fn(Device, *mut u64) -> Status;
type TextFn = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> Status;
type SystemTextFn = unsafe extern "C" fn(*mut c_char, c_uint) -> Status;

/// A loaded and initialized NVML with the entry points Argus uses.
pub(crate) struct Nvml {
    // Keeps the DLL mapped for as long as the function pointers below are alive.
    _lib: Library,
    shutdown: InitFn,
    handle: HandleFn,
    name: NameFn,
    utilization: UtilFn,
    memory: MemFn,
    temperature: TempFn,
    clock: ClockFn,
    fan: Option<UintFn>,
    power: Option<UintFn>,
    power_limit: Option<UintFn>,
    throttle: Option<MaskFn>,
    driver_version: Option<SystemTextFn>,
    vbios: Option<TextFn>,
    pcie_gen: Option<UintFn>,
    pcie_width: Option<UintFn>,
    pcie_gen_max: Option<UintFn>,
    pcie_width_max: Option<UintFn>,
    bus_width: Option<UintFn>,
    architecture: Option<UintFn>,
    gpu_cores: Option<UintFn>,
    pub(crate) device_count: u32,
}

// SAFETY: NVML is documented as thread safe; the struct is moved to one worker thread anyway.
unsafe impl Send for Nvml {}

/// Resolves a symbol or reports which one is missing.
macro_rules! symbol {
    ($lib:expr, $name:literal, $ty:ty) => {
        // SAFETY: the symbol name and signature follow nvml.h.
        unsafe { $lib.get::<$ty>($name) }.map(|s| *s).map_err(|_| {
            format!(
                "a biblioteca da NVIDIA não tem {}",
                String::from_utf8_lossy(&$name[..$name.len() - 1])
            )
        })
    };
}

macro_rules! optional {
    ($lib:expr, $name:literal, $ty:ty) => {
        // SAFETY: the symbol name and signature follow nvml.h.
        unsafe { $lib.get::<$ty>($name) }.ok().map(|s| *s)
    };
}

impl Nvml {
    /// Loads `nvml.dll` from the standard places.
    pub(crate) fn load() -> Result<Self, String> {
        let mut last = "nvml.dll não encontrada (sem driver NVIDIA)".to_string();
        for candidate in [
            "nvml.dll",
            r"C:\Program Files\NVIDIA Corporation\NVSMI\nvml.dll",
        ] {
            match Self::load_from(Path::new(candidate)) {
                Ok(nvml) => return Ok(nvml),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    pub(crate) fn load_from(path: &Path) -> Result<Self, String> {
        // SAFETY: loading a DLL runs its initialization; nvml.dll is the vendor library.
        let lib = unsafe { Library::new(path) }
            .map_err(|_| format!("não foi possível carregar {}", path.display()))?;
        let init = symbol!(lib, b"nvmlInit_v2\0", InitFn)?;
        let count = symbol!(lib, b"nvmlDeviceGetCount_v2\0", CountFn)?;
        let shutdown = symbol!(lib, b"nvmlShutdown\0", InitFn)?;
        let handle = symbol!(lib, b"nvmlDeviceGetHandleByIndex_v2\0", HandleFn)?;
        let name = symbol!(lib, b"nvmlDeviceGetName\0", NameFn)?;
        let utilization = symbol!(lib, b"nvmlDeviceGetUtilizationRates\0", UtilFn)?;
        let memory = symbol!(lib, b"nvmlDeviceGetMemoryInfo\0", MemFn)?;
        let temperature = symbol!(lib, b"nvmlDeviceGetTemperature\0", TempFn)?;
        let clock = symbol!(lib, b"nvmlDeviceGetClockInfo\0", ClockFn)?;
        let fan = optional!(lib, b"nvmlDeviceGetFanSpeed\0", UintFn);
        let power = optional!(lib, b"nvmlDeviceGetPowerUsage\0", UintFn);
        let power_limit = optional!(lib, b"nvmlDeviceGetEnforcedPowerLimit\0", UintFn);
        let throttle = optional!(lib, b"nvmlDeviceGetCurrentClocksThrottleReasons\0", MaskFn);
        let driver_version = optional!(lib, b"nvmlSystemGetDriverVersion\0", SystemTextFn);
        let vbios = optional!(lib, b"nvmlDeviceGetVbiosVersion\0", TextFn);
        let pcie_gen = optional!(lib, b"nvmlDeviceGetCurrPcieLinkGeneration\0", UintFn);
        let pcie_width = optional!(lib, b"nvmlDeviceGetCurrPcieLinkWidth\0", UintFn);
        let pcie_gen_max = optional!(lib, b"nvmlDeviceGetMaxPcieLinkGeneration\0", UintFn);
        let pcie_width_max = optional!(lib, b"nvmlDeviceGetMaxPcieLinkWidth\0", UintFn);
        let bus_width = optional!(lib, b"nvmlDeviceGetMemoryBusWidth\0", UintFn);
        let architecture = optional!(lib, b"nvmlDeviceGetArchitecture\0", UintFn);
        let gpu_cores = optional!(lib, b"nvmlDeviceGetNumGpuCores\0", UintFn);

        // SAFETY: plain calls into the initialized library, with valid out pointers.
        let status = unsafe { init() };
        if status != SUCCESS {
            return Err(format!("o NVML não iniciou ({})", explain(status)));
        }
        let mut device_count = 0;
        // SAFETY: see above.
        let status = unsafe { count(&mut device_count) };
        if status != SUCCESS || device_count == 0 {
            // SAFETY: init succeeded just above.
            unsafe { shutdown() };
            return Err("nenhuma placa NVIDIA encontrada".to_string());
        }
        Ok(Self {
            _lib: lib,
            shutdown,
            handle,
            name,
            utilization,
            memory,
            temperature,
            clock,
            fan,
            power,
            power_limit,
            throttle,
            driver_version,
            vbios,
            pcie_gen,
            pcie_width,
            pcie_gen_max,
            pcie_width_max,
            bus_width,
            architecture,
            gpu_cores,
            device_count,
        })
    }

    fn device(&self, index: u32) -> Result<Device, String> {
        let mut device: Device = std::ptr::null_mut();
        // SAFETY: valid out pointer; `index` is below the device count.
        let status = unsafe { (self.handle)(index, &mut device) };
        if status == SUCCESS {
            Ok(device)
        } else {
            Err(explain(status))
        }
    }

    pub(crate) fn device_name(&self, index: u32) -> Result<String, String> {
        let device = self.device(index)?;
        let mut buf = [0 as c_char; 96];
        // SAFETY: the buffer length is passed along and NVML NUL terminates the name.
        let status = unsafe { (self.name)(device, buf.as_mut_ptr(), buf.len() as c_uint) };
        if status != SUCCESS {
            return Err(explain(status));
        }
        let bytes: Vec<u8> = buf
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// `(core %, memory controller %)`.
    pub(crate) fn utilization(&self, index: u32) -> Result<(f64, f64), String> {
        let device = self.device(index)?;
        let mut u = Utilization::default();
        // SAFETY: valid out pointer.
        let status = unsafe { (self.utilization)(device, &mut u) };
        if status == SUCCESS {
            Ok((f64::from(u.gpu), f64::from(u.memory)))
        } else {
            Err(explain(status))
        }
    }

    /// `(used bytes, total bytes)` of dedicated video memory.
    pub(crate) fn memory(&self, index: u32) -> Result<(f64, f64), String> {
        let device = self.device(index)?;
        let mut m = MemoryInfo::default();
        // SAFETY: valid out pointer.
        let status = unsafe { (self.memory)(device, &mut m) };
        if status == SUCCESS {
            Ok((m.used as f64, m.total as f64))
        } else {
            Err(explain(status))
        }
    }

    /// GPU core temperature in °C.
    pub(crate) fn temperature(&self, index: u32) -> Result<f64, String> {
        let device = self.device(index)?;
        let mut t = 0;
        // SAFETY: sensor type 0 is NVML_TEMPERATURE_GPU; valid out pointer.
        let status = unsafe { (self.temperature)(device, 0, &mut t) };
        if status == SUCCESS {
            Ok(f64::from(t))
        } else {
            Err(explain(status))
        }
    }

    /// Clock in MHz: 0 is graphics, 2 is memory.
    pub(crate) fn clock(&self, index: u32, kind: u32) -> Result<f64, String> {
        let device = self.device(index)?;
        let mut mhz = 0;
        // SAFETY: valid out pointer.
        let status = unsafe { (self.clock)(device, kind, &mut mhz) };
        if status == SUCCESS {
            Ok(f64::from(mhz))
        } else {
            Err(explain(status))
        }
    }

    fn uint(&self, f: Option<UintFn>, index: u32, scale: f64) -> Result<f64, String> {
        let f = f.ok_or_else(|| "esta versão do driver não tem esta leitura".to_string())?;
        let device = self.device(index)?;
        let mut v = 0;
        // SAFETY: valid out pointer.
        let status = unsafe { f(device, &mut v) };
        if status == SUCCESS {
            Ok(f64::from(v) * scale)
        } else {
            Err(explain(status))
        }
    }

    /// Fan speed in percent of its maximum.
    pub(crate) fn fan_percent(&self, index: u32) -> Result<f64, String> {
        self.uint(self.fan, index, 1.0)
    }

    /// Board power draw in watts.
    pub(crate) fn power_watts(&self, index: u32) -> Result<f64, String> {
        self.uint(self.power, index, 0.001)
    }

    /// Enforced power limit in watts.
    pub(crate) fn power_limit_watts(&self, index: u32) -> Result<f64, String> {
        self.uint(self.power_limit, index, 0.001)
    }

    /// A raw unsigned reading from an optional entry point (not scaled).
    fn plain(&self, f: Option<UintFn>, index: u32) -> Result<u32, String> {
        self.uint(f, index, 1.0).map(|v| v as u32)
    }

    pub(crate) fn pcie_link(&self, index: u32) -> Result<String, String> {
        let (gen, width) = (
            self.plain(self.pcie_gen, index)?,
            self.plain(self.pcie_width, index)?,
        );
        let max = (
            self.plain(self.pcie_gen_max, index).ok(),
            self.plain(self.pcie_width_max, index).ok(),
        );
        Ok(match max {
            (Some(mg), Some(mw)) => format!("PCIe {gen}.0 x{width} (máx. PCIe {mg}.0 x{mw})"),
            _ => format!("PCIe {gen}.0 x{width}"),
        })
    }

    pub(crate) fn memory_bus_bits(&self, index: u32) -> Result<u32, String> {
        self.plain(self.bus_width, index)
    }

    pub(crate) fn shader_units(&self, index: u32) -> Result<u32, String> {
        self.plain(self.gpu_cores, index)
    }

    pub(crate) fn architecture_name(&self, index: u32) -> Result<String, String> {
        let id = self.plain(self.architecture, index)?;
        Ok(match id {
            2 => "Kepler",
            3 => "Maxwell",
            4 => "Pascal",
            5 => "Volta",
            6 => "Turing",
            7 => "Ampere",
            8 => "Ada Lovelace",
            9 => "Hopper",
            10 => "Blackwell",
            other => return Err(format!("arquitetura desconhecida do NVML ({other})")),
        }
        .to_string())
    }

    pub(crate) fn bios(&self, index: u32) -> Result<String, String> {
        let f = self
            .vbios
            .ok_or_else(|| "esta versão do driver não informa a BIOS".to_string())?;
        let device = self.device(index)?;
        let mut buf = [0 as c_char; 96];
        // SAFETY: the buffer length is passed along and NVML NUL terminates the text.
        let status = unsafe { f(device, buf.as_mut_ptr(), buf.len() as c_uint) };
        if status != SUCCESS {
            return Err(explain(status));
        }
        Ok(text(&buf))
    }

    pub(crate) fn driver(&self) -> Result<String, String> {
        let f = self
            .driver_version
            .ok_or_else(|| "esta versão do driver não informa a versão".to_string())?;
        let mut buf = [0 as c_char; 96];
        // SAFETY: the buffer length is passed along and NVML NUL terminates the text.
        let status = unsafe { f(buf.as_mut_ptr(), buf.len() as c_uint) };
        if status != SUCCESS {
            return Err(explain(status));
        }
        Ok(text(&buf))
    }

    pub(crate) fn limiters(&self, index: u32) -> Result<Limiters, String> {
        let f = self
            .throttle
            .ok_or_else(|| "esta versão do driver não informa os limitadores".to_string())?;
        let device = self.device(index)?;
        let mut mask = 0u64;
        // SAFETY: valid out pointer.
        let status = unsafe { f(device, &mut mask) };
        if status == SUCCESS {
            Ok(Limiters::from_mask(mask))
        } else {
            Err(explain(status))
        }
    }
}

/// The NUL terminated text NVML wrote into `buf`.
fn text(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: init succeeded in `load_from`; the library is still mapped (`_lib` drops after).
        unsafe {
            (self.shutdown)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_library_is_an_error_not_a_panic() {
        let err = Nvml::load_from(Path::new(r"C:\does\not\exist\nvml.dll"))
            .err()
            .expect("must fail");
        assert!(err.contains("não foi possível carregar"), "{err}");
    }

    #[test]
    fn throttle_mask_is_grouped() {
        assert_eq!(Limiters::from_mask(0), Limiters::default());
        let l = Limiters::from_mask(0x4 | 0x40);
        assert!(l.power && l.thermal && !l.hw_slowdown && !l.idle);
        assert!(Limiters::from_mask(0x8).hw_slowdown);
        assert!(Limiters::from_mask(0x1).idle);
        assert!(Limiters::from_mask(0x80).power);
    }
}
