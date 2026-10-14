# Security

## Reporting a vulnerability

A private reporting channel has not been established for this repository.
Inspection on 2026-10-13 found a private repository and did not confirm an enabled
GitHub private-reporting facility. There is no published security email address.

Until a private channel is established, open an issue asking the maintainer for
a private contact, without exploit details, sensitive logs or a working exploit.
If you cannot access the repository's issues, withhold those details until a
private channel is published. Once connected privately, include the affected
version or commit, reproduction steps, impact and relevant configuration.

Ordinary bugs and unsupported sensor readings belong in GitHub issues. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the hardware details that help investigate them.

## Current trust boundary

Argus 0.2.0 opens without administrator rights. Optional CPU measurements use a
LocalSystem service and the signed PawnIO driver. Activating that backend requests
Windows administrator consent. The helper lives under a protected Program Files
directory; the app checks that the reading pipe belongs to the running service.

The current local activation path elevates PowerShell over scripts and a manifest
from the unsigned, user-writable bundle. Its hashes cannot authenticate files
replaced together with the manifest. UAC names Windows PowerShell, not Argus.
This is an accepted local limitation, not a resolved distribution requirement.
A separately trusted bootstrap and a fresh review are required before distributing
installers to other users or machines. The
[CPU backend guide](tools/cpu-temperature/README.md) describes its build and local use.

Do not disable Secure Boot, memory integrity, driver blocking or other Windows
protections to use Argus. If Windows blocks the backend, keep those protections
enabled and report the displayed error. Uninstalling Argus leaves the shared
PawnIO driver in place because other programs may use it.

There is no published security support schedule or response-time commitment.
Reports should identify the exact build, including whether the CPU backend was
activated. Remove credentials, machine identifiers, serial numbers, addresses and
personal paths from material shared publicly.
