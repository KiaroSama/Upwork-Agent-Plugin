param()
$ErrorActionPreference = 'Stop'
function Write-BuildEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [native-build] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-BuildEvent ERROR 'Rust 1.99 or newer is required. No global installation is performed.'
        exit 1
    }
    # Release optimization/linking can legitimately be silent for more than three
    # minutes. Keep the total wall bound and all ordinary verification defaults.
    Write-BuildEvent INFO 'Native release build started.'
    Write-BuildEvent DEBUG 'Release verification bounds: wall_seconds=900 idle_seconds=600.'
    & (Join-Path $PSScriptRoot 'scripts/verify.ps1') -CargoArgs @('build', '--release', '--locked') -WallSeconds 900 -IdleSeconds 600
    if ($LASTEXITCODE -ne 0) {
        $code = $LASTEXITCODE
        Write-BuildEvent WARNING 'Release verification failed; the existing installed binary is preserved.'
        Write-BuildEvent ERROR "Native release build failed exit_code=$code."
        exit $code
    }
    $binary = if ($IsWindows) { 'upwork-connector.exe' } else { 'upwork-connector' }
    New-Item -ItemType Directory -Force -Path (Join-Path $PSScriptRoot 'bin') | Out-Null
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "target/release/$binary") -Destination (Join-Path $PSScriptRoot "bin/$binary") -Force
    if ($IsWindows) { & (Join-Path $PSScriptRoot 'scripts/protect-state.ps1') -ProjectRoot $PSScriptRoot }
    Write-BuildEvent INFO 'Native connector built. Run run.ps1, or bin/upwork-connector desktop-config.'
} catch {
    Write-BuildEvent ERROR 'Native build or publication failed; no successful build is claimed.'
    throw
}
