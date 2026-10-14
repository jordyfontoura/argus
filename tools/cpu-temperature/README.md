# CPU temperature backend

`cargo build -p argus-probe` builds the CPU-only .NET Framework helper and verifies the pinned
NuGet packages and official PawnIO installer. The first build downloads those inputs into
`target/cpu-temperature-downloads`; later builds use the verified cache. Runtime uses no network.

## Installed app (0.2.0)

`cargo tauri build` prepares `target/cpu-temperature-bundle` through the app's Cargo build and
ships it as `cpu-temperature` beside the installed executable. This includes the helper, all
pinned DLLs, unchanged official installer, manifest, pins, activation/removal scripts, notices
and full license texts. The installed app resolves its Tauri resource directory; it does not
use Cargo's development OUT_DIR. No .NET SDK or runtime downloads are needed for activation.

In **Configurações → Medidores da CPU**, **Ativar / atualizar leitura** requests Windows UAC
explicitly. Activation runs on a worker so the window and other collectors remain responsive.
The result appears beside the button; failure gives a retry action and log location. Nothing
requests elevation merely because the app opened, including when a current service is installed.
The existing sensor subscribers start/recover the activated service without elevation.

Uninstall offers optional removal of only `ArgusCpuTemperature` and its protected folder.
Declining elevation leaves those files/service and shows how to retry. The shared PawnIO driver
and user configuration/history are always kept. Upgrades skip that removal action. Activation
also removes validated legacy Argus measurement resources. Unrelated services and tasks
are left untouched.

## Activate on a development machine

Build, then run the dry run from the repository root:

```powershell
cargo build -p argus-probe
$bundle = & target/debug/argus-probe.exe cpu-temp-bundle
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/cpu-temperature/activate.ps1 -Bundle $bundle -DryRun
```

Activate once with Windows administrator consent:

```powershell
$script = Join-Path (Get-Location).Path 'tools/cpu-temperature/activate.ps1'
Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList ('-NoProfile -ExecutionPolicy Bypass -File "' + $script + '" -Bundle "' + $bundle + '"')
```

Activation copies the helper and DLLs into `%ProgramFiles%\Argus\CpuTemperature`, checks their
destination hashes, and restricts writes to SYSTEM and administrators. It registers
`ArgusCpuTemperature` under LocalSystem with manual startup and a quoted executable path.
Interactive users receive only query-status, start, and stop rights. Installed PawnIO 2.0.0 or
newer is reused. Otherwise activation verifies the official installer's hash and signature in
the protected directory before running it. An older driver is left untouched.

The normal CLI and app start the service without elevation. The service stops after 30 seconds
without readers. `activation.log` goes into `%TEMP%/ArgusCpuTemperature` by default; `-EvidenceDir`
can choose another log directory.
PawnIO remains shared with other programs.

## Readings and failure states

```powershell
cargo run -p argus-probe -- snapshot --json
cargo run -p argus-probe -- watch cpu.temp 8
$env:ARGUS_DISABLE_LOWLEVEL = '1'
cargo run -p argus-probe -- snapshot --json
Remove-Item Env:ARGUS_DISABLE_LOWLEVEL
```

`cpu.temp`, `cpu.vcore` and the package-power fallback subscribe to
`\\.\pipe\argus-cpu-temperature-v1`. The helper creates the pipe outbound only and publishes
`unix_ms,temp_status,celsius,power_status,watts,vcore_status,volts` each second. It accepts no commands,
hardware requests, filenames, pipe names, or start arguments. Argus checks the pipe server PID
against the running SCM service PID before reading each available batch.

CPU collection never waits for helper I/O. A bounded background subscriber keeps only the latest
measurement batch; the collector consumes it once with its measured timestamp. Each metric is
validated independently, so an absent or invalid voltage never invalidates a real temperature.
It rejects nonfinite values, temperatures outside 1..150 °C, power outside 0.01..2000 W, voltage
outside 0.1..2.5 V, future/repeated timestamps, and measurements older than
`max(5 seconds, 3 × collection interval)`. Errors have a Portuguese explanation and an action.
ACPI thermal zones are no longer registered or presented as CPU temperature.

The helper checks device access before and after each update. Driver recovery recreates the LHM
computer and driver handles. It owns `Global\Access_PCI` on the same thread during the update;
after a 750 ms timeout it reports busy and publishes no retained value. The mapping prefers
package/Tdie to Tctl. Real hardware verification so far covers only the Ryzen 5 3500X's
`Core (Tctl/Tdie)`; other matching CPUs depend on the library's support.

Sensor selection uses CPU hardware type and sensor meaning, without processor model strings or
sensor indexes. Package power matches `CPU Package`/`Package`; Vcore matches `Core (SVI2 TFN)`/
`Core (SVI3 TFN)` rail telemetry. Per-core VID is a requested voltage and is never used as Vcore.
Working Windows/PDH package power remains preferred; LHM fills it only when unavailable. NVML,
SMART and the other ordinary collectors remain in use. During helper upgrades, the subscriber
also accepts the former temperature-only line and reports that the new meters need an updated helper.
Unsupported physical voltage remains unavailable with a specific reason, including on CPUs where
the library only offers VID. CPU fan, multiplier and model metadata have no new source here.

Versions, hashes, licenses and source origins are in `pins.psd1` and
[the third-party notices](THIRD-PARTY-NOTICES.txt).

## Distribution limit

The current local bundle is unsigned. Activation elevates PowerShell over scripts and a
manifest from a user-writable directory. Matching hashes detect corruption but cannot
authenticate files replaced together with that manifest. A separately trusted activation
bootstrap and another security review are required before distributing installers to other
users or machines. Keep Windows protections enabled. See [SECURITY.md](../../SECURITY.md).

## Checks

`cargo test -p argus-sensors-win` covers the Collector seam with injected PDH thermal-zone data,
measurement timestamps and invalid/future/stale values. Cargo builds the helper as part of
the native crates. Test values do not establish
hardware accuracy; use a real CLI reading for that.
