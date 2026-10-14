# Only exact Argus-owned artifacts. Never remove PawnIO or user data.
param([switch]$SpikeOnly)
$ErrorActionPreference = 'Stop'
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'A remoção do serviço exige administrador.' }
function ValidateFolder([string]$folder) {
    $resolved = [IO.Path]::GetFullPath($folder)
    $allowed = @((Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Argus\CpuTemperature'), (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'ArgusCpuTempSpike'))
    if ($resolved -notin $allowed) { throw "Destino de remoção inesperado: $resolved" }
    $cursor = $resolved
    while ($cursor) {
        if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Destino redirecionado: $cursor" }
        $cursor = Split-Path -Parent $cursor
    }
    if (Test-Path -LiteralPath $resolved) {
        if (Get-ChildItem -LiteralPath $resolved -Force -Recurse | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw "Conteúdo redirecionado em $resolved" }
    }
    return $resolved
}
function RemoveOwned([string]$name, [string]$folder, [string]$exe) {
    $folder = ValidateFolder $folder
    $service = Get-CimInstance Win32_Service -Filter "Name='$name'"
    if ($service) {
        $expected = '"' + (Join-Path $folder $exe) + '"'
        if ($name -eq 'ArgusCpuTempSpike') { $expected += ' service --pipe argus-cpu-temp-spike' }
        if ($service.PathName -ne $expected -or $service.StartName -ne 'LocalSystem') { throw "O serviço $name tem outra origem; remoção recusada." }
        Stop-Service -Name $name -ErrorAction Stop
        (Get-Service $name).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(15))
        & sc.exe delete $name
        if ($LASTEXITCODE -ne 0) { throw "Não foi possível remover $name" }
    }
    if (Test-Path -LiteralPath $folder) { Remove-Item -LiteralPath $folder -Recurse -Force }
}
if ($SpikeOnly) {
    $folder = ValidateFolder (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'ArgusCpuTempSpike')
    $service = Get-CimInstance Win32_Service -Filter "Name='ArgusCpuTempSpike'"
    $expected = '"' + (Join-Path $folder 'bin\ArgusCpuTempSpike.exe') + '" service --pipe argus-cpu-temp-spike'
    if ($service -and ($service.PathName -ne $expected -or $service.StartName -ne 'LocalSystem')) { throw 'O serviço do spike tem outra origem; nenhuma limpeza foi feita.' }
    foreach ($name in 'ArgusCpuTempSpike-Serve','ArgusCpuTempSpike-HwinfoRef','ArgusCpuTempSpike-Cleanup') {
        $task = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
        if ($task) {
            # The historic cleanup is inspected, never executed. No commands from it are reused.
            if ($task.TaskPath -ne '\' -or -not ($task.Actions | Where-Object { ($_.Execute + ' ' + $_.Arguments).Contains($folder) })) { throw "Tarefa $name tem outra origem; remoção recusada." }
            Stop-ScheduledTask -InputObject $task
            Unregister-ScheduledTask -InputObject $task -Confirm:$false
        }
    }
    # Stop only leftover spike executables whose full path belongs to the exact folder.
    foreach ($p in Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and ([IO.Path]::GetDirectoryName($_.ExecutablePath) -in @($folder,(Join-Path $folder 'bin'),(Join-Path $folder 'hwinfo'))) }) {
        if ([IO.Path]::GetFileName($p.ExecutablePath) -notin @('ArgusCpuTempSpike.exe','HWiNFO64.exe')) { throw 'Processo desconhecido na pasta do spike; limpeza recusada.' }
        Stop-Process -Id $p.ProcessId -ErrorAction Stop
    }
    RemoveOwned 'ArgusCpuTempSpike' $folder 'bin\ArgusCpuTempSpike.exe'
} else {
    RemoveOwned 'ArgusCpuTemperature' (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Argus\CpuTemperature') 'ArgusCpuTemperature.exe'
}
Write-Host 'Argus-owned backend removed. Shared PawnIO and user data were kept.'
