# Contributing to Argus

Bug reports, hardware observations, documentation fixes and pull requests are
welcome. For a larger change, open an issue describing the problem and proposed
behavior before writing code. English and Portuguese reports are both fine.

## Set up a working copy

Follow the Windows prerequisites and setup commands in the [README](README.md#run-from-source).
Fork the repository, clone your fork, and create a branch from `main`.
The frontend lives in `ui/`; the Rust workspace is under `crates/` and `src-tauri/`.
The optional C# CPU helper lives in `tools/cpu-temperature/`.

Run `cargo tauri dev` from the repository root. Argus allows one native instance:
if an installed copy is already running in the tray, a second launch brings that
window forward. Exit it through the tray's **Sair** action before testing a new
native build.

Settings, rules and history normally live in `%APPDATA%\Argus`. To keep development
data separate, set `ARGUS_DATA_DIR` in the same PowerShell session before launching:

```powershell
$env:ARGUS_DATA_DIR = Join-Path (Get-Location).Path '.scratch/dev-data'
cargo tauri dev
```

This changes the app's data directory, not the system-wide CPU service or driver.
Do not activate or remove the backend as part of an ordinary UI or documentation test.

## Check a change

Run these from the repository root with Rust and Node.js available:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
node --test "ui/**/*.test.mjs"
cargo test --workspace
cargo build --workspace --release
```

These commands build with the existing
third-party downloads/cache described in the README. Use Node 24 to match the
verified environment. For a UI change, also inspect the desktop app at 1280 × 800
and the minimum supported window size, 800 × 600. The browser demo can check
layout, but cannot prove native collection, persistence or Windows actions.

The [GitHub workflow](.github/workflows/ci.yml) runs those same five checks on
Windows with Rust stable and Node 24. Local checks passing does not establish
that a hosted workflow has run.

## Report a hardware problem

Include the Argus version or commit, Windows version/build, CPU/GPU model, driver
version where relevant, and whether the optional CPU backend is activated. Give
the sensor identifier, expected behavior and steps to reproduce the problem.
These CLI commands can help narrow it down:

```powershell
cargo run -p argus-probe -- snapshot --json
cargo run -p argus-probe -- watch cpu.temp 8
cargo run -p argus-probe -- details
```

Unavailable readings may be a hardware or API limit rather than a defect. Include
the reason Argus displays. Do not disable Windows security protections to make a
reading work. Redact hostnames, IP/MAC addresses, serial numbers, usernames,
personal paths and process names before sharing reports, logs or screenshots.
For vulnerabilities, follow [SECURITY.md](SECURITY.md).

## Send a pull request

Keep changes focused. Describe what triggers the problem, what changes for the
user, and how you verified it. Add tests for changed behavior that can regress;
do not add tests that only repeat prose or decorative markup. Never weaken an
existing test or check to get a pass.

Use small commits with Conventional Commit messages, such as
`fix: explain an unavailable sensor` or `docs: clarify Windows setup`. Keep
generated files and third-party notices consistent with their sources. New
dependencies need their license and source origin recorded.

Contribution code is covered by [MIT](LICENSE);
third-party components retain their own licenses.

## Before making the repository public

The documentation is prepared for source publication. Maintainers still need to:

- Review source files and Git history for credentials, personal data and material
  they cannot redistribute. `.gitignore` prevents some future additions; it does
  not remove data already committed.
- Verify GitHub CI on the intended public setup.
- Establish a private vulnerability-reporting channel and update `SECURITY.md`.
- Review GitHub visibility and then explicitly publish the source.

Installer publication needs a separate review of the activation trust boundary in
[SECURITY.md](SECURITY.md), a trusted bootstrap and
the third-party distribution notices. Opening the source repository does not make
the current unsigned installer ready for release.
