param([switch]$Status, [switch]$Preview)
$ErrorActionPreference = 'Stop'
$binary = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'upwork-connector.exe' } else { 'upwork-connector' }
$connector = Join-Path $PSScriptRoot "bin/$binary"
if (-not (Test-Path -LiteralPath $connector)) {
    Write-Error 'Native connector is missing. Download and extract the complete Windows release archive; source builds are developer-only.'
    exit 1
}
$env:TEMP = $env:TMP = $env:TMPDIR = Join-Path $PSScriptRoot '.cache/tmp'
$env:XDG_CACHE_HOME = Join-Path $PSScriptRoot '.cache'
New-Item -ItemType Directory -Force -Path $env:TEMP | Out-Null
$command = if ($Status) { 'status' } elseif ($Preview) { 'preview' } else { 'menu' }
& $connector --root $PSScriptRoot $command
exit $LASTEXITCODE
