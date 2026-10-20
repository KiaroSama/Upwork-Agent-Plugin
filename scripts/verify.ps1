param(
    [string[]]$CargoArgs = @('test', '--locked', '--all-targets', '--', '--test-threads=1'),
    [ValidateRange(1, 86400)][int]$WallSeconds = 900,
    [ValidateRange(1, 86400)][int]$IdleSeconds = 180
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$env:TEMP = $env:TMP = $env:TMPDIR = $work
$env:CARGO_HOME = Join-Path $root '.cargo-cache'
$env:CARGO_TARGET_DIR = Join-Path $root 'target'
$env:XDG_CACHE_HOME = Join-Path $root '.ci-cache'
New-Item -ItemType Directory -Force -Path $env:CARGO_HOME, $env:XDG_CACHE_HOME | Out-Null
$env:CARGO_BUILD_JOBS = '2'
$info = [Diagnostics.ProcessStartInfo]::new()
$info.FileName = 'cargo'
$info.WorkingDirectory = $root
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.RedirectStandardInput = $true
foreach ($arg in $CargoArgs) { $info.ArgumentList.Add($arg) }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $info
$start = [DateTime]::UtcNow
$last = $start
$expired = $false
$started = $false
$code = 1
$receiptId = [Guid]::NewGuid().ToString('N')
$logPath = Join-Path $work ('verification_' + $start.ToString('yyyy-MM-dd_HH-mm-ss') + '_UTC_' + $receiptId + '.log')
function Event([string]$level, [string]$message) {
    $line = '[{0} UTC] [{1}] [verification] {2}' -f [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'), $level, $message
    try { [IO.File]::AppendAllText($logPath, "$line`n", [Text.UTF8Encoding]::new($false)) }
    catch { [Console]::Error.WriteLine('Verification file logging unavailable; safe lifecycle diagnostics follow.'); [Console]::Error.WriteLine($line) }
}
$receiptDirectory = Join-Path $work 'verification'
New-Item -ItemType Directory -Force -Path $receiptDirectory | Out-Null
$stage = if ($CargoArgs.Count -and $CargoArgs[0] -in @('fmt', 'clippy', 'nextest', 'test', 'build')) { $CargoArgs[0] } else { 'cargo' }
$releaseBuild = ($CargoArgs -join '|') -eq 'build|--release|--locked'
$buildInput = $null
$outcome = 'interrupted'
try {
    Event INFO "Verification started stage=$stage wall_seconds=$WallSeconds idle_seconds=$IdleSeconds."
    if ($releaseBuild) {
        $receiptPath = Join-Path $work 'build-provenance.json'
        if (Test-Path -LiteralPath $receiptPath) { Remove-Item -LiteralPath $receiptPath -Force }
        $buildInput = & (Join-Path $PSScriptRoot 'build-receipt.ps1') -Capture
    }
    if (-not $process.Start()) { throw 'Cannot start Cargo.' }
    $started = $true
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadLineAsync()
    $stderr = $process.StandardError.ReadLineAsync()
    while (-not $process.HasExited -or $null -ne $stdout -or $null -ne $stderr) {
        foreach ($name in @('stdout', 'stderr')) {
            $task = Get-Variable -Name $name -ValueOnly
            if ($null -ne $task -and $task.IsCompleted) {
                $line = $task.GetAwaiter().GetResult()
                if ($null -eq $line) { Set-Variable -Name $name -Value $null }
                else {
                    Write-Host $line
                    $last = [DateTime]::UtcNow
                    $stream = if ($name -eq 'stdout') { $process.StandardOutput } else { $process.StandardError }
                    Set-Variable -Name $name -Value ($stream.ReadLineAsync())
                }
            }
        }
        if (([DateTime]::UtcNow - $start).TotalSeconds -gt $WallSeconds -or ([DateTime]::UtcNow - $last).TotalSeconds -gt $IdleSeconds) {
            $expired = $true
            if (-not $process.HasExited) { $process.Kill($true) }
            break
        }
        # Also yield while inherited output handles drain after the parent exits.
        if (-not $process.HasExited) { $null = $process.WaitForExit(100) }
        elseif ($null -ne $stdout -or $null -ne $stderr) {
            $pending = @(@($stdout, $stderr) | Where-Object { $null -ne $_ })
            if ($pending.Count) { $null = [Threading.Tasks.Task]::WaitAny([Threading.Tasks.Task[]]$pending, 100) }
        }
    }
    if (-not $process.WaitForExit(5000)) { throw 'Cargo could not be reaped within the shutdown bound.' }
    $code = if ($expired) { 1 } else { $process.ExitCode }
    if ($code -eq 0 -and $releaseBuild) {
        $binary = if ($IsWindows) { 'upwork-connector.exe' } else { 'upwork-connector' }
        try { & (Join-Path $PSScriptRoot 'build-receipt.ps1') -Binary (Join-Path $root "target/release/$binary") -Expected $buildInput }
        catch { $code = 1; throw }
    }
    $outcome = if ($expired) { 'timeout' } elseif ($code -eq 0) { 'success' } else { 'failure' }
    if ($expired) { Write-Error 'Cargo exceeded its wall/idle bound; owned process tree terminated.' }
} catch {
    if ($code -eq 0) { $code = 1 }
    if (-not $expired) { $outcome = if ($started) { 'failure' } else { 'startup_failure' } }
    throw
} finally {
    try {
        if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Cargo could not be reaped.' } }
        $end = [DateTime]::UtcNow
        $identity = [Text.Encoding]::UTF8.GetBytes(($CargoArgs | ConvertTo-Json -Compress))
        $digest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($identity)).ToLowerInvariant()
        $receipt = @{ run_id = $receiptId; stage = $stage; command_sha256 = $digest; pid = $(if ($started) { $process.Id } else { $null }); start = $start.ToString('o'); end = $end.ToString('o'); elapsed = ($end - $start).TotalSeconds; wall_seconds = $WallSeconds; idle_seconds = $IdleSeconds; started = $started; expired = $expired; outcome = $outcome; exitCode = $code; reaped = (-not $started -or $process.HasExited) }
        $receipt | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $receiptDirectory "$stage-$receiptId.json") -Encoding utf8NoBOM
        if ($releaseBuild -and $code -ne 0 -and (Test-Path -LiteralPath (Join-Path $work 'build-provenance.json'))) {
            Remove-Item -LiteralPath (Join-Path $work 'build-provenance.json') -Force
        }
        Event $(if ($code -eq 0) { 'INFO' } else { 'ERROR' }) "Verification stopped stage=$stage outcome=$outcome exit_code=$code reaped=$($receipt.reaped)."
    } catch {
        [Console]::Error.WriteLine('Verification receipt or process cleanup failed; no complete verification is claimed.')
        if ($code -eq 0) { $code = 1 }
    } finally { $process.Dispose() }
}
exit $code
