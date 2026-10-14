# One explicitly elevated activation. The app and subscriber never elevate themselves.
param(
    [Parameter(Mandatory=$true)][string]$Bundle,
    [string]$EvidenceDir = '',
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
if (-not $EvidenceDir) { $EvidenceDir = Join-Path ([IO.Path]::GetTempPath()) 'ArgusCpuTemperature' }
$pins = Import-PowerShellDataFile -LiteralPath (Join-Path $PSScriptRoot 'pins.psd1')
$Bundle = [IO.Path]::GetFullPath($Bundle)
$protected = [IO.Path]::GetFullPath((Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Argus\CpuTemperature'))
$expected = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Argus\CpuTemperature'
if ($protected -ne $expected) { throw "Destino inesperado: $protected" }
New-Item -ItemType Directory -Path $EvidenceDir -Force | Out-Null
$cursor = [IO.Path]::GetFullPath($EvidenceDir)
while ($cursor) {
    if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Log redirecionado: $cursor" }
    $cursor = Split-Path -Parent $cursor
}
if (Test-Path -LiteralPath (Join-Path $EvidenceDir 'activation.log')) {
    if ((Get-Item -LiteralPath (Join-Path $EvidenceDir 'activation.log') -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Log redirecionado.' }
}
Start-Transcript -LiteralPath (Join-Path $EvidenceDir 'activation.log') -Append | Out-Null
try {
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $DryRun -and -not $admin) { throw 'Execute esta ativação uma vez como administrador.' }
    Write-Host "Activation elevated=$admin dryrun=$DryRun source=$Bundle destination=$protected"
    $manifest = Get-Content -LiteralPath (Join-Path $Bundle 'manifest.json') -Raw | ConvertFrom-Json
    $allowed = @('ArgusCpuTemperature.exe','PawnIO_setup.exe') + @($pins.NuGet | ForEach-Object { [IO.Path]::GetFileName($_.Extract) })
    $entries = @($manifest.PSObject.Properties)
    if ($entries.Count -ne $allowed.Count) { throw 'Manifesto de arquivos incompleto ou inesperado.' }
    foreach ($entry in $entries) {
        if ($entry.Name -notin $allowed -or $entry.Value -notmatch '^[0-9a-f]{64}$') { throw "Entrada inesperada: $($entry.Name)" }
        $source = Join-Path $Bundle $entry.Name
        if ((Get-Item -LiteralPath $source).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Arquivo redirecionado: $source" }
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) { throw "Hash de origem incorreto: $($entry.Name)" }
    }
    if ($manifest.'PawnIO_setup.exe' -ne $pins.PawnIO.Sha256) { throw 'Instalador PawnIO diferente da versão fixada.' }
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO'
    $installed = Test-Path -LiteralPath $key
    if ($installed) {
        $version = [version](Get-ItemProperty -LiteralPath $key).DisplayVersion
        if ($version -lt [version]'2.0.0') { throw 'PawnIO antigo: atualize pelo pawnio.eu; o Argus não substitui essa instalação.' }
        Write-Host "Reusing installed PawnIO $version; no driver change."
    }
    if ($DryRun) { Write-Host 'DRYRUN_OK: verified bundle; would protect/copy/hash-check files and register CPU-only manual LocalSystem service.'; return }
    # Reject junctions at every existing component before any elevated write.
    $cursor = $protected
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Destino redirecionado: $cursor" }
        }
        $cursor = Split-Path -Parent $cursor
    }
    $serviceName = 'ArgusCpuTemperature'
    $exe = Join-Path $protected 'ArgusCpuTemperature.exe'
    $binary = '"' + $exe + '"'
    $existing = Get-CimInstance Win32_Service -Filter "Name='ArgusCpuTemperature'"
    $previouslyInstalledPawnIO = $false
    if ($existing) {
        if ($existing.PathName -ne $binary -or $existing.StartName -ne 'LocalSystem') { throw 'O serviço existente tem outra origem; nenhuma alteração foi feita.' }
        Stop-Service -Name $serviceName -ErrorAction Stop
        (Get-Service $serviceName).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(15))
        $previousRecord = Join-Path $protected 'activation.json'
        if (Test-Path -LiteralPath $previousRecord) {
            $previouslyInstalledPawnIO = [bool](Get-Content -LiteralPath $previousRecord -Raw | ConvertFrom-Json).installedPawnIO
        }
    }
    New-Item -ItemType Directory -Path $protected -Force | Out-Null
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
    foreach ($sid in 'S-1-5-18','S-1-5-32-544') {
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')))
    }
    $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-545')), 'ReadAndExecute', 'ContainerInherit,ObjectInherit', 'None', 'Allow')))
    Set-Acl -LiteralPath $protected -AclObject $acl
    foreach ($entry in $entries) {
        $destination = Join-Path $protected $entry.Name
        if (Test-Path -LiteralPath $destination) {
            if ((Get-Item -LiteralPath $destination -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Arquivo de destino redirecionado: $destination" }
        }
        Copy-Item -LiteralPath (Join-Path $Bundle $entry.Name) -Destination $destination -Force
        # Explicitly inherit only the protected parent ACL, including on an upgrade.
        $fileAcl = New-Object Security.AccessControl.FileSecurity
        $fileAcl.SetAccessRuleProtection($false, $false)
        $fileAcl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
        Set-Acl -LiteralPath $destination -AclObject $fileAcl
        $actual = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $entry.Value) { throw "Hash de destino incorreto: $($entry.Name)" }
        Write-Host "Verified destination $actual $($entry.Name)"
    }
    $setup = Join-Path $protected 'PawnIO_setup.exe'
    if ((Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins.PawnIO.Sha256) { throw 'Hash do instalador PawnIO incorreto.' }
    $sig = Get-AuthenticodeSignature -LiteralPath $setup
    if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Thumbprint -ne $pins.PawnIO.SignerThumbprint -or $sig.SignerCertificate.Subject -notlike "*$($pins.PawnIO.SignerSubjectContains)*") { throw 'Assinatura do instalador PawnIO inválida.' }
    if (-not $installed) {
        $process = Start-Process -FilePath $setup -ArgumentList '-install','-silent' -PassThru -WindowStyle Hidden
        if (-not $process.WaitForExit(180000) -or $process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $key)) { throw 'PawnIO não foi instalado; confira o Segurança do Windows. O Argus não desativa proteções.' }
    }
    $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $protected 'manifest.json') -Encoding UTF8
    @{ installedPawnIO = ($previouslyInstalledPawnIO -or -not $installed); driverVersion = (Get-ItemProperty -LiteralPath $key).DisplayVersion; activatedUtc = [DateTime]::UtcNow.ToString('o') } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $protected 'activation.json') -Encoding UTF8
    if (-not $existing) {
        New-Service -Name $serviceName -BinaryPathName $binary -StartupType Manual -DisplayName 'Argus CPU temperature' -Description 'CPU-only outbound measurement service. No commands or hardware requests are accepted from clients.' | Out-Null
    }
    Set-Service -Name $serviceName -StartupType Manual
    # SYSTEM/admin full rights; interactive users ONLY query status, start, stop.
    & sc.exe sdset $serviceName 'D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;LCRPWP;;;IU)'
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao proteger os direitos do serviço.' }
    & sc.exe qc $serviceName
    & sc.exe sdshow $serviceName
    Get-Acl -LiteralPath $protected | Format-List Path,Owner,AccessToString
    & (Join-Path $PSScriptRoot 'remove.ps1') -SpikeOnly
    Write-Host 'ACTIVATION_DONE; app stays non-elevated; service starts on demand without UAC.'
} finally { Stop-Transcript | Out-Null }
