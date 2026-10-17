param([switch]$Status, [switch]$Preview)
$ErrorActionPreference = 'Stop'
function Write-LauncherEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [launcher] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
# Propagate the connector's native status instead of converting it to a PowerShell exception.
$PSNativeCommandUseErrorActionPreference = $false
$binary = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'upwork-connector.exe' } else { 'upwork-connector' }
$connector = Join-Path $PSScriptRoot "bin/$binary"
if (-not (Test-Path -LiteralPath $connector)) {
    Write-LauncherEvent ERROR 'Native connector is missing. Extract the complete release archive for this platform.'
    exit 1
}
$env:TEMP = $env:TMP = $env:TMPDIR = Join-Path $PSScriptRoot '.cache/tmp'
$env:XDG_CACHE_HOME = Join-Path $PSScriptRoot '.cache'
# The native entry point validates cache links and the command before creating directories.
$command = if ($Status) { 'status' } elseif ($Preview) { 'preview' } else { 'menu' }
Write-LauncherEvent INFO 'Starting the native connector.'
Write-LauncherEvent DEBUG 'Native startup owns cache validation and directory creation.'
& $connector --root $PSScriptRoot $command
$code = $LASTEXITCODE
if ($code -ne 0) {
    Write-LauncherEvent WARNING 'The native connector rejected startup or the command; no fallback was attempted.'
}
exit $code
