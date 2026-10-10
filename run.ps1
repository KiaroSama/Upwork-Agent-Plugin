param([switch]$Status, [switch]$Preview)
$ErrorActionPreference = 'Stop'
$binary = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'upwork-connector.exe' } else { 'upwork-connector' }
$connector = Join-Path $PSScriptRoot "bin/$binary"
if (-not (Test-Path -LiteralPath $connector)) {
    Write-Error 'Native connector is missing. Download and extract the complete Windows release archive; source builds are developer-only.'
    exit 1
}
$command = if ($Status) { 'status' } elseif ($Preview) { 'preview' } else { 'menu' }
& $connector --root $PSScriptRoot $command
exit $LASTEXITCODE
