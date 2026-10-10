param(
    [string[]]$CargoArgs = @('test', '--locked', '--all-targets', '--', '--test-threads=1'),
    [int]$WallSeconds = 900,
    [int]$IdleSeconds = 180
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$env:TEMP = $env:TMP = $env:TMPDIR = $work
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
try {
    if (-not $process.Start()) { throw 'Cannot start Cargo.' }
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
        if (-not $process.HasExited) { $null = $process.WaitForExit(100) }
    }
    $process.WaitForExit()
    $code = if ($expired) { 1 } else { $process.ExitCode }
    @{ pid = $process.Id; start = $start.ToString('o'); elapsed = ([DateTime]::UtcNow - $start).TotalSeconds; expired = $expired; exitCode = $code } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'rust-verification.json') -Encoding utf8
    if ($expired) { Write-Error 'Cargo exceeded its wall/idle bound; owned process tree terminated.' }
} finally {
    if ($process.Id -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    $process.Dispose()
}
exit $code
