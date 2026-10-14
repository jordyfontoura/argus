fn main() {
    // The UI is embedded at compile time, so a change under ui/ must rebuild the app.
    println!("cargo:rerun-if-changed=../ui");
    println!("cargo:rerun-if-changed=../tools/cpu-temperature");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let root =
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("..");
        let status = std::process::Command::new("powershell.exe")
            .env_remove("PSModulePath")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(root.join("tools/cpu-temperature/build.ps1"))
            .arg("-OutDir")
            .arg(root.join("target/cpu-temperature-bundle"))
            .status()
            .expect("build installed CPU measurement resources");
        assert!(status.success(), "CPU measurement resource build failed");
    }
    tauri_build::build()
}
