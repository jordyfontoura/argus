//! Nonblocking subscriber to the CPU-only LocalSystem publisher.
use argus_core::{Availability, Clock, Reading, SystemClock};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::{GetNamedPipeServerProcessId, PeekNamedPipe};
use windows::Win32::System::Services::*;

pub const DISABLE_LOWLEVEL_ENV: &str = "ARGUS_DISABLE_LOWLEVEL";
const ACTIVATE: &str = "leitura de baixo nível não ativada; ative a leitura de temperatura da CPU no Argus (uma autorização de administrador)";
const WAITING: &str =
    "aguardando nova medição da CPU; se persistir, reinicie o serviço ArgusCpuTemperature";
const METRICS: [&str; 3] = ["cpu.temp", "cpu.power", "cpu.vcore"];

#[derive(Clone)]
struct Measurement {
    ts_ms: i64,
    values: [Availability; 3],
}
struct State {
    latest: Option<Measurement>,
    reason: String,
}

pub(crate) struct CpuTemperature {
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    last_cycle_ms: Option<i64>,
    last_measured_ms: Option<i64>,
}

impl CpuTemperature {
    pub(crate) fn new() -> Self {
        if std::env::var(DISABLE_LOWLEVEL_ENV).is_ok_and(|v| v == "1") {
            return Self::disabled();
        }
        let backend = Self::with_reason(WAITING);
        let (state, stop) = (backend.state.clone(), backend.stop.clone());
        thread::Builder::new()
            .name("argus-cpu-temperature".into())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Err(reason) = subscribe(&state, &stop) {
                        if let Ok(mut shared) = state.lock() {
                            shared.latest = None;
                            shared.reason = reason;
                        }
                    }
                    for _ in 0..20 {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            })
            .expect("spawn CPU temperature subscriber");
        backend
    }

    pub(crate) fn disabled() -> Self {
        Self::with_reason(&format!("desativado por {DISABLE_LOWLEVEL_ENV}=1; remova essa variável e ative a leitura de temperatura da CPU no Argus"))
    }

    fn with_reason(reason: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                latest: None,
                reason: reason.into(),
            })),
            stop: Arc::new(AtomicBool::new(false)),
            last_cycle_ms: None,
            last_measured_ms: None,
        }
    }

    /// Publish each measurement once, retaining its timestamp. Between fresh samples, publish
    /// no update; the store keeps that measured value until a new sample or a real failure.
    pub(crate) fn readings(&mut self, now_ms: i64) -> Option<[Reading; 3]> {
        self.readings_with_clock(now_ms, &SystemClock)
    }

    fn readings_with_clock(&mut self, now_ms: i64, clock: &dyn Clock) -> Option<[Reading; 3]> {
        let unavailable = |reason: &str| {
            Some(METRICS.map(|id| Reading::new(id, now_ms, Availability::unavailable(reason))))
        };
        let interval = self
            .last_cycle_ms
            .map_or(1000, |previous| now_ms.saturating_sub(previous).max(100));
        self.last_cycle_ms = Some(now_ms);
        let max_age = 5000.max(interval.saturating_mul(3));
        let recent = |checked_ms: i64| {
            self.last_measured_ms.is_some_and(|measured_ms| {
                measured_ms <= checked_ms && checked_ms.saturating_sub(measured_ms) <= max_age
            })
        };
        let Ok(mut state) = self.state.try_lock() else {
            if recent(clock.now_ms()) {
                return None;
            }
            return unavailable(WAITING);
        };
        let Some(sample) = state.latest.take() else {
            if state.reason == WAITING && recent(clock.now_ms()) {
                return None;
            }
            return unavailable(&state.reason);
        };
        // An async measurement can arrive after the collection tick while sysinfo/PDH
        // refreshes. Validate after taking it, while retaining its measured timestamp.
        let checked_ms = clock.now_ms();
        if sample.ts_ms > checked_ms || checked_ms.saturating_sub(sample.ts_ms) > max_age {
            return unavailable("medição da CPU velha ou com horário futuro; confira o relógio e reinicie o serviço ArgusCpuTemperature");
        }
        if self
            .last_measured_ms
            .is_some_and(|previous| sample.ts_ms <= previous)
        {
            return unavailable("medição repetida; reinicie o serviço ArgusCpuTemperature");
        }
        self.last_measured_ms = Some(sample.ts_ms);
        let [temperature, power, vcore] = sample.values;
        Some([
            Reading::new("cpu.temp", sample.ts_ms, temperature),
            Reading::new("cpu.power", sample.ts_ms, power),
            Reading::new("cpu.vcore", sample.ts_ms, vcore),
        ])
    }
}

impl Drop for CpuTemperature {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn reason(status: &str) -> &'static str {
    match status {
        "driver_not_installed" => "driver PawnIO ausente; ative a leitura de temperatura da CPU no Argus",
        "driver_old" => "driver PawnIO antigo; atualize pela origem oficial pawnio.eu e reinicie o serviço ArgusCpuTemperature",
        "access_denied" => "permissão negada ao driver PawnIO; ative novamente a leitura de temperatura da CPU no Argus",
        "driver_not_running" => "driver PawnIO instalado, mas não carregado; confira o bloqueio no Segurança do Windows e reinicie o computador; o Argus não desativa proteções",
        "driver_blocked" => "Windows recusou acesso ao driver PawnIO; confira o Segurança do Windows; o Argus não desativa proteções",
        "busy" => "acesso PCI ocupado por outro monitor; feche o outro monitor e aguarde uma nova medição",
        "no_sensor" => "o backend não encontrou temperatura de pacote/Tdie/Tctl nesta CPU; confira o suporte no LibreHardwareMonitor",
        "no_value" => "o backend ainda não mediu a temperatura da CPU; aguarde ou reinicie o serviço ArgusCpuTemperature",
        "invalid" => "medição de temperatura inválida; reinicie o serviço ArgusCpuTemperature",
        _ => "o backend falhou ao medir a temperatura; reinicie o serviço ArgusCpuTemperature",
    }
}

fn parse(line: &str) -> Result<Measurement, String> {
    let fields: Vec<_> = line.trim_end_matches('\r').split(',').collect();
    // Accept the installed temperature-only helper during upgrade, with the new
    // meters explicitly absent. The fixed extended batch has three status/value pairs.
    if fields.len() != 3 && fields.len() != 7 {
        return Err(reason("invalid").into());
    }
    let ts_ms = fields[0]
        .parse::<i64>()
        .ok()
        .ok_or_else(|| reason("invalid").to_string())?;
    let values = std::array::from_fn(|i| {
        if i > 0 && fields.len() == 3 {
            return Availability::unavailable("helper antigo oferece somente temperatura; ative novamente a leitura no Argus para atualizar o serviço");
        }
        let status = fields[1 + i * 2];
        let raw = fields[2 + i * 2];
        let missing = match i {
            0 => reason("no_sensor"),
            1 => "o LibreHardwareMonitor não oferece potência do pacote nesta CPU",
            _ => {
                "o LibreHardwareMonitor não oferece Vcore físico nesta CPU; VID não é tensão medida"
            }
        };
        let invalid = match i {
            0 => reason("invalid"),
            1 => "medição de potência do pacote inválida; reinicie o serviço ArgusCpuTemperature",
            _ => "medição de Vcore inválida; reinicie o serviço ArgusCpuTemperature",
        };
        if status == "no_sensor" {
            return Availability::unavailable(missing);
        }
        if status == "no_value" {
            return Availability::unavailable("o backend ainda não mediu este sensor da CPU; aguarde ou reinicie o serviço ArgusCpuTemperature");
        }
        if status == "invalid" {
            return Availability::unavailable(invalid);
        }
        if status != "ok" {
            return Availability::unavailable(reason(status));
        }
        let bounds = match i {
            0 => 1.0..=150.0,
            1 => 0.01..=2000.0,
            _ => 0.1..=2.5,
        };
        match raw.parse::<f64>() {
            Ok(value) if value.is_finite() && bounds.contains(&value) => Availability::ok(value),
            _ => Availability::unavailable(invalid),
        }
    });
    Ok(Measurement { ts_ms, values })
}

struct Service(SC_HANDLE);
impl Drop for Service {
    fn drop(&mut self) {
        // SAFETY: owned SCM handle, closed once.
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn service() -> Result<Service, String> {
    // SAFETY: local SCM, constant NUL-terminated service name; no start arguments.
    unsafe {
        let manager = Service(
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT)
                .map_err(|_| ACTIVATE.to_string())?,
        );
        let service = Service(OpenServiceW(manager.0, w!("ArgusCpuTemperature"), SERVICE_QUERY_STATUS | SERVICE_START).map_err(|e| {
            if e.code().0 as u32 == 0x80070005 { "permissão negada ao serviço; ative novamente a leitura de temperatura da CPU no Argus".to_string() } else { ACTIVATE.to_string() }
        })?);
        let status = service_status(&service)?;
        if status.dwCurrentState == SERVICE_STOPPED {
            StartServiceW(service.0, None).map_err(|_| "não foi possível iniciar o backend; ative novamente a leitura de temperatura da CPU no Argus".to_string())?;
        }
        Ok(service)
    }
}

fn service_status(service: &Service) -> Result<SERVICE_STATUS_PROCESS, String> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    // SAFETY: correctly aligned status object exposed as bytes for the Win32 output call.
    unsafe {
        let bytes = std::slice::from_raw_parts_mut(
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast::<u8>(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
        );
        QueryServiceStatusEx(service.0, SC_STATUS_PROCESS_INFO, Some(bytes), &mut needed)
            .map_err(|_| "helper encerrado; reinicie o serviço ArgusCpuTemperature".to_string())?;
    }
    Ok(status)
}

fn authenticated(file: &File, service: &Service) -> Result<(), String> {
    let mut pid = 0;
    // SAFETY: file owns a valid pipe handle; pid is a valid output pointer.
    unsafe { GetNamedPipeServerProcessId(HANDLE(file.as_raw_handle()), &mut pid) }.map_err(
        |_| "não foi possível autenticar o backend; ative novamente a leitura no Argus".to_string(),
    )?;
    let status = service_status(service)?;
    if status.dwCurrentState != SERVICE_RUNNING || pid == 0 || pid != status.dwProcessId {
        return Err("pipe de temperatura não pertence ao serviço Argus; ative novamente a leitura de temperatura da CPU no Argus".into());
    }
    Ok(())
}

fn subscribe(state: &Mutex<State>, stop: &AtomicBool) -> Result<(), String> {
    let service = service()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut pipe = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        // SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION: a spoofed server cannot impersonate us.
        match OpenOptions::new()
            .read(true)
            .custom_flags(0x0010_0000 | 0x0001_0000)
            .open(r"\\.\pipe\argus-cpu-temperature-v1")
        {
            Ok(pipe) => break pipe,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                return Err("helper sem resposta; reinicie o serviço ArgusCpuTemperature".into())
            }
        }
    };
    authenticated(&pipe, &service)?;
    let mut pending = Vec::new();
    let mut last = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let mut available = 0;
        // SAFETY: pipe handle remains live, output points to a u32. Never read before bytes exist.
        unsafe {
            PeekNamedPipe(
                HANDLE(pipe.as_raw_handle()),
                None,
                0,
                None,
                Some(&mut available),
                None,
            )
        }
        .map_err(|_| {
            "helper encerrado; aguarde a reinicialização automática do serviço ArgusCpuTemperature"
                .to_string()
        })?;
        if available > 0 {
            authenticated(&pipe, &service)?;
            let mut buffer = [0; 512];
            let count = (available as usize).min(buffer.len());
            let n = pipe.read(&mut buffer[..count]).map_err(|_| "helper encerrado; aguarde a reinicialização automática do serviço ArgusCpuTemperature".to_string())?;
            if n == 0 {
                return Err("helper encerrado; reinicie o serviço ArgusCpuTemperature".into());
            }
            pending.extend_from_slice(&buffer[..n]);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                if end > 256 {
                    return Err(reason("invalid").into());
                }
                let text = std::str::from_utf8(&pending[..end])
                    .map_err(|_| reason("invalid").to_string())?;
                let sample = parse(text)?;
                if let Ok(mut shared) = state.lock() {
                    shared.latest = Some(sample);
                    shared.reason = WAITING.into();
                }
                pending.drain(..=end);
                last = Instant::now();
            }
            if pending.len() > 256 {
                return Err(reason("invalid").into());
            }
        }
        if last.elapsed() > Duration::from_secs(5) {
            return Err("helper sem resposta; reinicie o serviço ArgusCpuTemperature".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_core::FakeClock;
    #[test]
    fn cpu_temp_backend_accepts_measurement_collected_after_cycle_started() {
        let clock = FakeClock::new(1000);
        let cycle_ms = clock.now_ms();
        let mut backend = CpuTemperature::with_reason(WAITING);
        // The helper measures while the CPU collector refreshes sysinfo/PDH.
        clock.advance_ms(100);
        backend.state.lock().unwrap().latest = Some(parse("1100,ok,57.25,ok,45.5,ok,1.2").unwrap());
        clock.advance_ms(25);
        let samples = backend.readings_with_clock(cycle_ms, &clock).unwrap();
        for (sample, expected) in samples.iter().zip([57.25, 45.5, 1.2]) {
            assert_eq!(sample.ts_ms, 1100);
            assert_eq!(sample.value.value(), Some(expected));
        }
    }
    #[test]
    fn cpu_temp_backend_preserves_measurement_time_and_consumes_once() {
        let mut backend = CpuTemperature::with_reason(WAITING);
        backend.state.lock().unwrap().latest = Some(parse("1000,ok,57.25,ok,45.5,ok,1.2").unwrap());
        let samples = backend
            .readings_with_clock(1200, &FakeClock::new(1200))
            .unwrap();
        for (sample, expected) in samples.iter().zip([57.25, 45.5, 1.2]) {
            assert_eq!(sample.ts_ms, 1000);
            assert_eq!(sample.value.value(), Some(expected));
        }
        assert!(backend
            .readings_with_clock(1300, &FakeClock::new(1300))
            .is_none());
        let legacy = parse("2000,ok,58.25").unwrap();
        assert_eq!(legacy.values[0].value(), Some(58.25));
        assert!(legacy.values[1..].iter().all(|v| v.value().is_none()));
    }
    #[test]
    fn cpu_temp_backend_rejects_invalid_stale_and_future_measurements() {
        for line in ["1000,ok,NaN", "1000,ok,0", "1000,ok,151", "1000,ok,inf"] {
            assert!(parse(line).unwrap().values[0].value().is_none());
        }
        let mixed = parse("1000,ok,57.25,ok,NaN,no_sensor,").unwrap();
        assert_eq!(mixed.values[0].value(), Some(57.25));
        assert!(mixed.values[1..].iter().all(|v| v.value().is_none()));
        let mixed = parse("1000,invalid,,ok,45.5,ok,1.2").unwrap();
        assert!(mixed.values[0].value().is_none());
        assert_eq!(mixed.values[1].value(), Some(45.5));
        assert_eq!(mixed.values[2].value(), Some(1.2));
        let mut backend = CpuTemperature::with_reason(WAITING);
        for ts in [1, 10_001] {
            backend.state.lock().unwrap().latest = Some(Measurement {
                ts_ms: ts,
                values: std::array::from_fn(|_| Availability::ok(57.25)),
            });
            assert!(backend
                .readings_with_clock(10_000, &FakeClock::new(10_000))
                .unwrap()
                .iter()
                .all(|r| r.value.value().is_none()));
        }
    }

    #[test]
    fn cpu_temp_backend_waits_between_fresh_samples_without_a_failure_or_duplicate() {
        let clock = FakeClock::new(1000);
        let mut backend = CpuTemperature::with_reason(WAITING);
        backend.state.lock().unwrap().latest = Some(parse("1000,ok,57.25,ok,45.5,ok,1.2").unwrap());
        assert_eq!(
            backend.readings_with_clock(1000, &clock).unwrap()[0].ts_ms,
            1000
        );
        clock.advance_ms(250);
        assert!(
            backend.readings_with_clock(1250, &clock).is_none(),
            "a poll between helper samples must not publish failure or another point"
        );
        // A real disconnect remains a failure even while the preceding sample is recent.
        backend.state.lock().unwrap().reason = "helper encerrado".into();
        assert!(backend
            .readings_with_clock(1500, &clock)
            .unwrap()
            .iter()
            .all(|r| r.value.value().is_none()));
        backend.state.lock().unwrap().reason = WAITING.into();
        clock.advance_ms(5001);
        assert!(
            backend
                .readings_with_clock(2500, &clock)
                .unwrap()
                .iter()
                .all(|r| r.value.value().is_none()),
            "an expired measurement must become unavailable"
        );
    }
}
