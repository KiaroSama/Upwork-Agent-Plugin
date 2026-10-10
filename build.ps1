param()
$ErrorActionPreference = 'Stop'
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Write-Error 'Rust 1.99 or newer is required to build. No global installation is performed.'; exit 1 }
& (Join-Path $PSScriptRoot 'scripts/verify.ps1') -CargoArgs @('build', '--release', '--locked')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$binary = if ($IsWindows) { 'upwork-connector.exe' } else { 'upwork-connector' }
New-Item -ItemType Directory -Force -Path (Join-Path $PSScriptRoot 'bin') | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot "target/release/$binary") -Destination (Join-Path $PSScriptRoot "bin/$binary") -Force
if ($IsWindows) { & (Join-Path $PSScriptRoot 'scripts/protect-state.ps1') -ProjectRoot $PSScriptRoot }
Write-Host 'Native connector built. Run run.ps1, or bin/upwork-connector desktop-config.'
