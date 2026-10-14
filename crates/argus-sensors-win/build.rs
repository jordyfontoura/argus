use std::{env, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let tools = root.join("tools/cpu-temperature");
    for name in [
        "Helper.cs",
        "build.ps1",
        "pins.psd1",
        "activate.ps1",
        "remove.ps1",
        "uninstall.ps1",
        "THIRD-PARTY-NOTICES.txt",
        "licenses",
    ] {
        println!("cargo:rerun-if-changed={}", tools.join(name).display());
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let bundle = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("cpu-temperature");
    let status = Command::new("powershell.exe")
        // Restore Windows PowerShell's module search path when Cargo was launched from pwsh 7.
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(tools.join("build.ps1"))
        .arg("-OutDir")
        .arg(&bundle)
        .status()
        .expect("run pinned CPU temperature helper build");
    assert!(status.success(), "CPU temperature helper build failed");
    println!("cargo:rustc-env=ARGUS_CPU_TEMP_BUNDLE={}", bundle.display());
}
