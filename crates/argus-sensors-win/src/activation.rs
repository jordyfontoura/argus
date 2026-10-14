//! Explicit user activation only. Measurements never call this elevation path.
use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub fn activate_cpu_measurements(bundle: &Path) -> Result<String, String> {
    let canonical = bundle
        .canonicalize()
        .map_err(|_| "Arquivos de ativação ausentes; reinstale o Argus.".to_string())?;
    // PowerShell 5/.NET Framework expects a normal DOS/UNC path, rather than Rust's verbatim prefix.
    let canonical_text = canonical.to_string_lossy();
    let bundle = if let Some(unc) = canonical_text.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{unc}"))
    } else {
        std::path::PathBuf::from(
            canonical_text
                .strip_prefix(r"\\?\")
                .unwrap_or(&canonical_text),
        )
    };
    let script = bundle.join("activate.ps1");
    if !script.is_file() || !bundle.join("manifest.json").is_file() {
        return Err("Pacote de ativação incompleto; reinstale o Argus.".into());
    }
    let bundle_text = bundle.to_string_lossy();
    // Windows cannot represent these in a filename; reject before argument serialization.
    if bundle_text.contains(['"', '\r', '\n']) {
        return Err("Caminho de instalação inválido; reinstale o Argus.".into());
    }
    let mut system_dir = [0u16; 32768];
    // SAFETY: writable buffer with its exact capacity; returns a length, not a retained pointer.
    let len = unsafe { GetSystemDirectoryW(Some(&mut system_dir)) } as usize;
    if len == 0 || len >= system_dir.len() {
        return Err("Windows PowerShell indisponível.".into());
    }
    let exe = std::path::PathBuf::from(String::from_utf16_lossy(&system_dir[..len]))
        .join("WindowsPowerShell/v1.0/powershell.exe");
    let exe: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    let parameters: Vec<u16> = format!(
        "-NoProfile -ExecutionPolicy Bypass -File \"{}\" -Bundle \"{}\"",
        script.display(),
        bundle.display()
    )
    .encode_utf16()
    .chain([0])
    .collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: zero-terminated strings remain alive for ShellExecuteExW; returned handle is owned.
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| format!("Ativação cancelada ou recusada pelo Windows ({e}). Tente novamente nas Configurações."))?;
        if info.hProcess.is_invalid() {
            return Err("O Windows não retornou o processo de ativação.".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1;
        let result = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        result.map_err(|e| format!("Não foi possível conferir a ativação: {e}"))?;
        if code != 0 {
            return Err(format!("Ativação falhou (código {code}). Confira activation.log em %TEMP%\\ArgusCpuTemperature e a Segurança do Windows; tente novamente ou reinstale o Argus."));
        }
    }
    Ok("Leitura da CPU ativada. Aguarde alguns segundos pelos novos valores; PawnIO compartilhado foi preservado.".into())
}
