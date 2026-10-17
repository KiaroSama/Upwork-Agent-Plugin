param(
    [string]$BinaryPath,
    [string]$LauncherRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$OutputPath,
    [switch]$ExpectBaseline
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Write-TestEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [launcher-boundaries] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
$root = [IO.Path]::GetFullPath($LauncherRoot)
$windows = $IsWindows
$name = if ($windows) { 'upwork-connector.exe' } else { 'upwork-connector' }
if (-not $BinaryPath) { $BinaryPath = Join-Path $root "bin/$name" }
$BinaryPath = (Resolve-Path -LiteralPath $BinaryPath).Path
$work = Join-Path $root '.ci-work'
[IO.Directory]::CreateDirectory($work) | Out-Null
if (-not $OutputPath) { $OutputPath = Join-Path $work 'launcher-boundaries.json' }
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($OutputPath)) | Out-Null
$owned = Join-Path $work ('launcher boundary ' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($owned) | Out-Null
$results = [Collections.Generic.List[object]]::new()
$links = [Collections.Generic.List[string]]::new()

function Invoke-OwnedProcess {
    param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory)
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.WorkingDirectory = $WorkingDirectory
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.RedirectStandardInput = $true
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $started = $false
    try {
        $started = $process.Start()
        if (-not $started) { throw 'Owned launcher process did not start.' }
        $process.StandardInput.Close()
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $timedOut = -not $process.WaitForExit(15000)
        if ($timedOut) { $process.Kill($true) }
        if (-not $process.WaitForExit(5000)) { throw 'Owned process could not be reaped.' }
        $out = $stdout.GetAwaiter().GetResult()
        $err = $stderr.GetAwaiter().GetResult()
        return [pscustomobject]@{
            ExitCode = $process.ExitCode
            TimedOut = $timedOut
            Reaped = $process.HasExited
            Stdout = $out
            Stderr = $err
        }
    } finally {
        if ($started -and -not $process.HasExited) {
            $process.Kill($true)
            $process.WaitForExit(5000) | Out-Null
        }
        $process.Dispose()
    }
}

function New-OwnedLink {
    param([string]$Path, [string]$Target)
    # A directory junction does not require Windows symbolic-link privilege.
    $kind = if ($windows) { 'Junction' } else { 'SymbolicLink' }
    New-Item -ItemType $kind -Path $Path -Target $Target | Out-Null
    $links.Add($Path)
    if (-not ([IO.File]::GetAttributes($Path) -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'The link fixture is not a real link/reparse point.'
    }
}

try {
    Write-TestEvent INFO 'Testing actual launchers with a real native connector in disposable roots.'
    $engines = [Collections.Generic.List[object]]::new()
    $engines.Add(@{ Name = 'pwsh'; Executable = (Get-Process -Id $PID).Path; Shell = $false })
    if ($windows) {
        $engines.Add(@{ Name = 'windows-powershell'; Executable = (Get-Command powershell.exe -ErrorAction Stop).Source; Shell = $false })
    } else {
        $engines.Add(@{ Name = 'sh'; Executable = (Get-Command sh -ErrorAction Stop).Source; Shell = $true })
    }
    foreach ($engine in $engines) {
        $cases = @('fresh', 'existing', 'literal-path', 'linked-cache', 'linked-temp', 'cache-file', 'missing-binary')
        if ($ExpectBaseline) { $cases = @($cases | Where-Object { $_ -ne 'literal-path' }) }
        if ($engine.Shell) { $cases += @('invalid-command', 'native-exit-code') }
        foreach ($case in $cases) {
            $suffix = if ($case -eq 'literal-path') { ' [literal]' } else { '' }
            $fixtureRoot = Join-Path $owned ($engine.Name + '-' + $case + $suffix)
            $outside = Join-Path $owned ($engine.Name + '-' + $case + '-outside')
            [IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'bin')) | Out-Null
            [IO.Directory]::CreateDirectory($outside) | Out-Null
            [IO.File]::WriteAllText((Join-Path $outside 'sentinel'), 'preserved-fixture')
            [IO.File]::Copy((Join-Path $root 'run.ps1'), (Join-Path $fixtureRoot 'run.ps1'))
            [IO.File]::Copy((Join-Path $root 'run.sh'), (Join-Path $fixtureRoot 'run.sh'))
            $native = Join-Path $fixtureRoot "bin/$name"
            if ($case -ne 'missing-binary') {
                [IO.File]::Copy($BinaryPath, $native)
                if (-not $windows) {
                    & chmod 700 -- $native
                    if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare the owned native executable.' }
                }
            }
            $cache = Join-Path $fixtureRoot '.cache'
            $temp = Join-Path $cache 'tmp'
            switch ($case) {
                'existing' {
                    [IO.Directory]::CreateDirectory($temp) | Out-Null
                    [IO.File]::WriteAllText((Join-Path $temp 'keep.txt'), 'keep-fixture')
                }
                'linked-cache' { New-OwnedLink $cache $outside }
                'linked-temp' {
                    [IO.Directory]::CreateDirectory($cache) | Out-Null
                    New-OwnedLink $temp $outside
                }
                'cache-file' { [IO.File]::WriteAllText($cache, 'cache-file-fixture') }
                'native-exit-code' {
                    [IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'config')) | Out-Null
                    [IO.File]::WriteAllText((Join-Path $fixtureRoot 'config/connections.json'), '{"version":1,"mode":"direct","selected":null,"profiles":[]}')
                }
            }
            $before = @([IO.Directory]::GetFileSystemEntries($outside) | ForEach-Object { [IO.Path]::GetFileName($_) } | Sort-Object) -join '|'
            if ($engine.Shell) {
                $arguments = @((Join-Path $fixtureRoot 'run.sh'), 'preview')
                if ($case -eq 'invalid-command') { $arguments = @((Join-Path $fixtureRoot 'run.sh'), 'direct', '--help') }
                if ($case -eq 'native-exit-code') { $arguments = @((Join-Path $fixtureRoot 'run.sh'), 'bridge') }
            } else {
                $arguments = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $fixtureRoot 'run.ps1'), '-Preview')
            }
            $receipt = Invoke-OwnedProcess $engine.Executable $arguments $outside
            $after = @([IO.Directory]::GetFileSystemEntries($outside) | ForEach-Object { [IO.Path]::GetFileName($_) } | Sort-Object) -join '|'
            $outsideUnchanged = $before -ceq $after -and [IO.File]::ReadAllText((Join-Path $outside 'sentinel')) -ceq 'preserved-fixture'
            $positive = $case -in @('fresh', 'existing', 'literal-path')
            $passed = -not $receipt.TimedOut -and $receipt.Reaped -and $outsideUnchanged
            $passed = $passed -and (($receipt.ExitCode -eq 0) -eq $positive)
            if ($positive) { $passed = $passed -and [IO.Directory]::Exists($temp) }
            if ($case -eq 'existing') { $passed = $passed -and [IO.File]::ReadAllText((Join-Path $temp 'keep.txt')) -ceq 'keep-fixture' }
            if ($case -eq 'cache-file') { $passed = $passed -and [IO.File]::ReadAllText($cache) -ceq 'cache-file-fixture' }
            $cacheAbsent = -not [IO.Directory]::Exists($cache) -and -not [IO.File]::Exists($cache)
            if ($case -in @('missing-binary', 'invalid-command')) { $passed = $passed -and $cacheAbsent }
            if ($case -eq 'native-exit-code') { $passed = $passed -and $receipt.ExitCode -eq 3 }
            $result = [pscustomobject]@{
                engine = $engine.Name; case = $case; exit_code = $receipt.ExitCode
                timed_out = $receipt.TimedOut; reaped = $receipt.Reaped
                outside_unchanged = $outsideUnchanged; cache_absent = $cacheAbsent
                passed = [bool]$passed
            }
            $results.Add($result)
            Write-TestEvent INFO ("Engine={0} case={1} passed={2} external_state_unchanged={3}" -f $engine.Name, $case, $passed, $outsideUnchanged)
        }
    }
    # The native positive control rejects a linked cache without any wrapper.
    $fixtureRoot = Join-Path $owned 'native-control'
    $outside = Join-Path $owned 'native-control-outside'
    [IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null
    [IO.Directory]::CreateDirectory($outside) | Out-Null
    New-OwnedLink (Join-Path $fixtureRoot '.cache') $outside
    $receipt = Invoke-OwnedProcess $BinaryPath @('--root', $fixtureRoot, 'preview') $outside
    $unchanged = @([IO.Directory]::GetFileSystemEntries($outside)).Count -eq 0
    $results.Add([pscustomobject]@{
        engine = 'native'; case = 'linked-cache'; exit_code = $receipt.ExitCode
        timed_out = $receipt.TimedOut; reaped = $receipt.Reaped
        outside_unchanged = $unchanged; cache_absent = $false
        passed = $receipt.ExitCode -ne 0 -and -not $receipt.TimedOut -and $receipt.Reaped -and $unchanged
    })
    $failures = @($results | Where-Object { -not $_.passed })
    $summary = [ordered]@{
        cases = $results.Count; passed = $results.Count - $failures.Count
        expected_baseline = [bool]$ExpectBaseline; results = $results.ToArray()
    }
    [IO.File]::WriteAllText($OutputPath, ($summary | ConvertTo-Json -Depth 6) + "`n", [Text.UTF8Encoding]::new($false))
    Write-TestEvent DEBUG 'The receipt contains fixed case names, exit codes and booleans only.'
    if ($ExpectBaseline) {
        $expected = @($results | Where-Object { ($_.engine -ne 'native' -and $_.case -eq 'linked-cache') -or $_.case -eq 'invalid-command' })
        if ($failures.Count -ne $expected.Count -or $expected.Count -lt 2) { throw 'Baseline failed outside the expected launcher defects.' }
        foreach ($item in $expected) {
            if ($item.passed -or $item.exit_code -eq 0 -or $item.timed_out -or -not $item.reaped) { throw 'Baseline rejection did not reproduce as expected.' }
            if ($item.case -eq 'linked-cache' -and $item.outside_unchanged) { throw 'Baseline did not produce the expected external directory.' }
            if ($item.case -eq 'invalid-command' -and $item.cache_absent) { throw 'Baseline invalid invocation had no startup write.' }
        }
        Write-TestEvent WARNING 'Baseline violations reproduced; they are not successful corrected contracts.'
    } elseif ($failures.Count) {
        throw 'A launcher boundary contract failed.'
    }
    Write-TestEvent INFO ("Completed {0} real-process cases." -f $results.Count)
} catch {
    Write-TestEvent ERROR 'Launcher verification failed; no unperformed case is certified.'
    throw
} finally {
    # Remove each owned link itself before recursively removing disposable roots.
    foreach ($link in $links) {
        if ([IO.Directory]::Exists($link)) { [IO.Directory]::Delete($link) }
    }
    if ([IO.Directory]::Exists($owned)) { [IO.Directory]::Delete($owned, $true) }
}
