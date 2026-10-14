# Called by the per-user uninstaller before bundled scripts are removed.
$ErrorActionPreference = 'Stop'
try {
    $script = Join-Path $PSScriptRoot 'remove.ps1'
    $powerShell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $p = Start-Process -FilePath $powerShell -Verb RunAs -WindowStyle Hidden -PassThru -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"' + $script + '"'))
    $p.WaitForExit()
    exit $p.ExitCode
} catch { exit 1 }
