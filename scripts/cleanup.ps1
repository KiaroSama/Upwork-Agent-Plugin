param([string]$ProjectRoot = (Split-Path -Parent $PSScriptRoot), [switch]$Apply)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
if ((Get-Item -LiteralPath $root -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked installation root refused.' }
if (-not (Test-Path -LiteralPath (Join-Path $root 'bin') -PathType Container)) { throw 'Installation bin directory is missing.' }
if ((Get-Item -LiteralPath (Join-Path $root 'bin') -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked installation bin directory refused.' }
$logDirectory = Join-Path $root 'logs'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$log = Join-Path $logDirectory ('upwork-cleanup_' + [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss') + '_UTC_' + [Guid]::NewGuid().ToString('N') + '.log')
function Event([string]$level, [string]$message) {
    try { [IO.File]::AppendAllText($log, ('[{0} UTC] [{1}] [upwork-cleanup] {2}' -f [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'), $level, $message) + "`n", [Text.UTF8Encoding]::new($false)) }
    catch { [Console]::Error.WriteLine('Cleanup file logging unavailable.') }
}
Event INFO 'Cleanup started; account data excluded.'
try {
$names = @('upwork-connector', 'upwork-connector.exe', 'upwork-connector.previous', 'upwork-connector.previous.exe', 'cargo', 'rustc', 'rustup', 'node', 'npm')
if (@(Get-Process -ErrorAction Stop | Where-Object ProcessName -in $names).Count) { throw 'A connector or build/runtime process is active. Close it before cache cleanup.' }
$allowed = @('target', '.cargo-cache', '.ci-cache', '.ci-work', '.runtime', 'node_modules', '.cache/npm', 'bin/upwork-connector.previous.exe', 'bin/upwork-connector.previous')
function Check-Path([string]$path) {
    $item = Get-Item -LiteralPath $path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked cleanup path refused.' }
    if ($item.Name -in @('secrets.md', '.ignoreme', 'explain-AI.md', '.state', 'config', 'logs', '.ai', '.claude', '.codex', '.agents') -or $item.Name -match '^\.env($|\.)') { throw 'Protected content found inside a cleanup target; nothing was deleted.' }
    if ($item.PSIsContainer) {
        foreach ($child in Get-ChildItem -LiteralPath $path -Force) { Check-Path $child.FullName }
    }
}
$candidates = @()
foreach ($relative in $allowed) {
    $path = Join-Path $root $relative
    $parent = Split-Path -Parent $path
    if ((Test-Path -LiteralPath $parent) -and ((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Linked cleanup parent refused.' }
    if (Test-Path -LiteralPath $path) { Check-Path $path; $candidates += $relative }
}
Event INFO 'Cleanup preflight passed; account data excluded.'
    foreach ($relative in $candidates) {
        if ($Apply) {
            if (@(Get-Process -ErrorAction Stop | Where-Object ProcessName -in $names).Count) { throw 'A process started during cleanup; remaining targets preserved.' }
            $path = Join-Path $root $relative
            Check-Path $path
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }
    Event INFO "Cleanup completed apply=$([bool]$Apply) targets=$($candidates.Count)."
    @{ apply = [bool]$Apply; targets = $candidates; count = $candidates.Count } | ConvertTo-Json -Depth 3
} catch {
    Event ERROR 'Cleanup failed; inspect filesystem permissions or active processes.'
    throw
}
