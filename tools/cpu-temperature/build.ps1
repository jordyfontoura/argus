# Cargo invokes this build; all inputs are verified before extraction/compilation.
param([Parameter(Mandatory=$true)][string]$OutDir)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$pins = Import-PowerShellDataFile -LiteralPath (Join-Path $PSScriptRoot 'pins.psd1')
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$cache = Join-Path $root 'target\cpu-temperature-downloads'
New-Item -ItemType Directory -Path $cache,$OutDir -Force | Out-Null
function Fetch([string]$Url, [string]$Name, [string]$Hash) {
    $file = Join-Path $cache $Name
    if (-not (Test-Path -LiteralPath $file)) { Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $file }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Hash) { throw "Hash incorreto: $Name" }
    return $file
}
foreach ($p in $pins.NuGet) {
    $id = $p.Id.ToLowerInvariant()
    $pkg = Fetch "https://api.nuget.org/v3-flatcontainer/$id/$($p.Version)/$id.$($p.Version).nupkg" "$id.$($p.Version).nupkg" $p.Sha256
    $zip = [IO.Compression.ZipFile]::OpenRead($pkg)
    try {
        $entry = $zip.GetEntry($p.Extract)
        if ($null -eq $entry) { throw "Entrada ausente: $($p.Extract)" }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $OutDir ([IO.Path]::GetFileName($p.Extract))), $true)
    } finally { $zip.Dispose() }
}
$setup = Fetch $pins.PawnIO.Url 'PawnIO_setup.exe' $pins.PawnIO.Sha256
Copy-Item -LiteralPath $setup -Destination $OutDir -Force
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $csc /nologo /target:exe /platform:x64 /optimize+ /warnaserror+ "/out:$(Join-Path $OutDir 'ArgusCpuTemperature.exe')" /r:System.dll /r:System.Core.dll /r:System.ServiceProcess.dll "/r:$(Join-Path $OutDir 'LibreHardwareMonitorLib.dll')" (Join-Path $PSScriptRoot 'Helper.cs')
if ($LASTEXITCODE -ne 0) { throw "csc: $LASTEXITCODE" }
$manifest = @{}
Get-ChildItem -LiteralPath $OutDir -File | Where-Object { $_.Extension -in '.exe','.dll' } | ForEach-Object { $manifest[$_.Name] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
$manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutDir 'manifest.json') -Encoding UTF8
foreach ($name in 'activate.ps1','remove.ps1','uninstall.ps1','pins.psd1','THIRD-PARTY-NOTICES.txt') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $OutDir -Force
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'licenses') -Destination $OutDir -Recurse -Force
Write-Host "CPU temperature bundle: $OutDir"
