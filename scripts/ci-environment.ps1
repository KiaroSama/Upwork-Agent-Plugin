param([switch]$ToolchainPath)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:GITHUB_ENV -or -not $env:GITHUB_PATH) { throw 'This environment setup runs only in GitHub CI.' }
if ($ToolchainPath) {
    $bin = Join-Path $root '.ci-cache/rustup/toolchains'
    $cargoName = if ($IsWindows) { 'cargo.exe' } else { 'cargo' }
    $cargo = @(Get-ChildItem -LiteralPath $bin -Directory | ForEach-Object {
        $candidate = Join-Path $_.FullName "bin/$cargoName"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { Get-Item -LiteralPath $candidate }
    })
    if ($cargo.Count -ne 1) { throw 'Expected one project-contained CI Cargo toolchain.' }
    Split-Path -Parent $cargo[0].FullName | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
    return
}
$values = @{
    TEMP = Join-Path $root '.ci-work'
    TMP = Join-Path $root '.ci-work'
    TMPDIR = Join-Path $root '.ci-work'
    CARGO_HOME = Join-Path $root '.cargo-cache'
    CARGO_TARGET_DIR = Join-Path $root 'target'
    RUSTUP_HOME = Join-Path $root '.ci-cache/rustup'
    XDG_CACHE_HOME = Join-Path $root '.ci-cache'
    HOME = Join-Path $root '.ci-cache/home'
    USERPROFILE = Join-Path $root '.ci-cache/home'
}
foreach ($entry in $values.GetEnumerator()) {
    New-Item -ItemType Directory -Force -Path $entry.Value | Out-Null
    $relative = [IO.Path]::GetRelativePath($root, $entry.Value)
    if ($relative.StartsWith('..') -or [IO.Path]::IsPathRooted($relative)) { throw 'CI cache escaped project root.' }
    "$($entry.Key)=$($entry.Value)" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
Write-Host 'CI temp, Cargo, toolchain and installer caches are confined to this checkout.'
