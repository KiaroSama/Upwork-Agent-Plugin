param([switch]$Status, [switch]$Preview)
$ErrorActionPreference = 'Stop'
$binary = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'upwork-connector.exe' } else { 'upwork-connector' }
$connector = Join-Path $PSScriptRoot "bin/$binary"
if (-not (Test-Path -LiteralPath $connector)) {
    Write-Error 'Native connector is missing. Run build.ps1 first; no dependencies are installed automatically.'
    exit 1
}
$command = if ($Status) { 'status' } elseif ($Preview) { 'preview' } else { 'menu' }
& $connector --root $PSScriptRoot $command
exit $LASTEXITCODE
