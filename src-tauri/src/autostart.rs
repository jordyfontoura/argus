//! "Start with Windows" through the per-user Run key. It needs no administrator rights and only
//! touches `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

/// Name of the Run value the app uses.
pub const VALUE_NAME: &str = "Argus";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_run_key() -> Result<HKEY, String> {
    let mut key = HKEY::default();
    // SAFETY: the sub key is a static NUL terminated string and `key` is a valid out pointer.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(key)
    } else {
        Err(format!(
            "não foi possível abrir a chave de inicialização do Windows ({})",
            status.0
        ))
    }
}

pub fn is_enabled(name: &str) -> bool {
    let Ok(key) = open_run_key() else {
        return false;
    };
    let name = wide(name);
    // SAFETY: `name` is NUL terminated; asking for no data only checks that the value exists.
    let status = unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, None) };
    // SAFETY: the key was opened above and is closed exactly once.
    unsafe {
        let _ = RegCloseKey(key);
    }
    status == ERROR_SUCCESS
}

/// Turns the entry on (running `command` at sign-in) or off.
pub fn set_enabled(name: &str, on: bool, command: &str) -> Result<(), String> {
    let key = open_run_key()?;
    let wname = wide(name);
    let result = if on {
        let value = wide(command);
        let bytes: Vec<u8> = value.iter().flat_map(|u| u.to_le_bytes()).collect();
        // SAFETY: `wname` is NUL terminated and `bytes` holds the UTF-16 value with its NUL.
        let status =
            unsafe { RegSetValueExW(key, PCWSTR(wname.as_ptr()), None, REG_SZ, Some(&bytes)) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "não foi possível gravar a entrada de inicialização ({})",
                status.0
            ))
        }
    } else {
        // SAFETY: `wname` is NUL terminated. A missing value is not an error when turning off.
        let status = unsafe { RegDeleteValueW(key, PCWSTR(wname.as_ptr())) };
        if status == ERROR_SUCCESS || !is_enabled(name) {
            Ok(())
        } else {
            Err(format!(
                "não foi possível remover a entrada de inicialização ({})",
                status.0
            ))
        }
    };
    // SAFETY: the key was opened above and is closed exactly once.
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

/// The command stored in the Run key: this executable, started hidden in the tray.
pub fn run_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" --minimized", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_can_be_turned_on_and_off_without_administrator_rights() {
        // A throw-away value name so a real "Argus" entry is never touched.
        let name = format!("ArgusTest{}", std::process::id());
        assert!(!is_enabled(&name));
        set_enabled(
            &name,
            true,
            "\"C:\\Program Files\\Argus\\argus.exe\" --minimized",
        )
        .unwrap();
        assert!(is_enabled(&name));
        // Setting it again just replaces it.
        set_enabled(&name, true, "\"C:\\Argus\\argus.exe\" --minimized").unwrap();
        assert!(is_enabled(&name));
        set_enabled(&name, false, "").unwrap();
        assert!(!is_enabled(&name));
        // Turning off something that is not there is fine.
        set_enabled(&name, false, "").unwrap();
    }

    #[test]
    fn the_command_starts_this_executable_minimized() {
        let c = run_command().unwrap();
        assert!(c.starts_with('"') && c.ends_with("--minimized"));
    }
}
