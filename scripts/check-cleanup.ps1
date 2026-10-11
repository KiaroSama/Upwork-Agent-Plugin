$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$sandbox = Join-Path $root ('.ci-work/cleanup smoke ' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $sandbox 'bin'), (Join-Path $sandbox 'target'), (Join-Path $sandbox '.state') -Force | Out-Null
$target = Join-Path $sandbox 'target/generated.txt'
$session = Join-Path $sandbox '.state/session.json'
[IO.File]::WriteAllText($target, 'generated', [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($session, 'preserve', [Text.UTF8Encoding]::new($false))
function Run([bool]$apply, [int]$expected) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = (Get-Process -Id $PID).Path
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($arg in @('-NoProfile', '-File', (Join-Path $root 'scripts/cleanup.ps1'), '-ProjectRoot', $sandbox)) { $info.ArgumentList.Add($arg) }
    if ($apply) { $info.ArgumentList.Add('-Apply') }
    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $info
    try {
        if (-not $child.Start()) { throw 'Cleanup fixture start failed.' }
        $out = $child.StandardOutput.ReadToEndAsync()
        $err = $child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(10000)) { $child.Kill($true); $child.WaitForExit(); throw 'Cleanup fixture timeout.' }
        $err.GetAwaiter().GetResult() | Out-Null
        if (($expected -eq 0) -ne ($child.ExitCode -eq 0)) { throw 'Cleanup fixture returned an unexpected status.' }
        return $out.GetAwaiter().GetResult()
    } finally {
        if ($child.Id -and -not $child.HasExited) { $child.Kill($true); $child.WaitForExit() }
        $child.Dispose()
    }
}
try {
    $report = Run $false 0 | ConvertFrom-Json
    if ($report.apply -or $report.count -ne 1 -or -not (Test-Path -LiteralPath $target)) { throw 'Cleanup dry-run removed data or missed target.' }
    $protected = Join-Path $sandbox 'target/secrets.md'
    [IO.File]::WriteAllText($protected, 'fixture-only', [Text.UTF8Encoding]::new($false))
    Run $true 1 | Out-Null
    if (-not (Test-Path -LiteralPath $target)) { throw 'Protected preflight partially removed files.' }
    Remove-Item -LiteralPath $protected
    Run $true 0 | Out-Null
    if (Test-Path -LiteralPath (Join-Path $sandbox 'target')) { throw 'Cleanup did not remove disposable cache.' }
    if ([IO.File]::ReadAllText($session, [Text.Encoding]::UTF8) -ne 'preserve') { throw 'Cleanup touched account state.' }
    $logs = @(Get-ChildItem -LiteralPath (Join-Path $sandbox 'logs') -Filter '*.log')
    if ($logs.Count -ne 3) { throw 'Cleanup log per-run contract failed.' }
    Write-Host 'Cleanup dry-run, protected refusal, cache removal, state preservation and logging passed.'
} finally {
    if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}
