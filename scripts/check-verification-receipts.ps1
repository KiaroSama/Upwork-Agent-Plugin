$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
$sandbox = Join-Path $work ('receipt-policy-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $sandbox 'scripts') | Out-Null
$originalPath = $env:PATH
$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
try {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'verify.ps1') -Destination (Join-Path $sandbox 'scripts/verify.ps1')
    $invoke = @'
param([string]$Stage, [int]$Wall = 5, [int]$Idle = 3)
& (Join-Path $PSScriptRoot 'scripts/verify.ps1') -CargoArgs @($Stage, 'private-fixture-marker') -WallSeconds $Wall -IdleSeconds $Idle
exit $LASTEXITCODE
'@
    [IO.File]::WriteAllText((Join-Path $sandbox 'invoke.ps1'), $invoke, [Text.UTF8Encoding]::new($false))
    # A real process substitutes only Cargo: the actual wrapper owns output/deadlines/receipts.
    $stub = Join-Path $sandbox 'stub'
    New-Item -ItemType Directory -Path $stub | Out-Null
    if ($IsWindows) {
        Copy-Item -LiteralPath (Join-Path $root 'bin/upwork-connector.exe') -Destination (Join-Path $stub 'cargo.exe')
    } else {
        [IO.File]::WriteAllText((Join-Path $stub 'cargo'), "#!/bin/sh`nprintf 'fixture progress\n'`nexit 0`n", [Text.UTF8Encoding]::new($false))
        & chmod 700 (Join-Path $stub 'cargo')
        if ($LASTEXITCODE) { throw 'Cannot prepare owned Cargo stub.' }
    }
    # Windows uses the built connector's invalid-command exit, never a compiler/rustup proxy.
    $expectedExit = if ($IsWindows) { 1 } else { 0 }
    $env:PATH = $stub + [IO.Path]::PathSeparator + $originalPath
    foreach ($batch in @(@('fmt', 'clippy'), @('nextest', 'test'))) {
        $children = [Collections.Generic.List[object]]::new()
        try {
            foreach ($stage in $batch) {
                $info = [Diagnostics.ProcessStartInfo]::new($pwsh)
                $info.UseShellExecute = $false; $info.CreateNoWindow = $true
                $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
                foreach ($argument in @('-NoProfile', '-NonInteractive', '-File', (Join-Path $sandbox 'invoke.ps1'), '-Stage', $stage)) { $info.ArgumentList.Add($argument) }
                $process = [Diagnostics.Process]::new(); $process.StartInfo = $info
                if (-not $process.Start()) { $process.Dispose(); throw 'Cannot start receipt fixture.' }
                $child = @{ Process = $process; Output = $null; Error = $null }
                $children.Add($child)
                $process.StandardInput.Close()
                $child.Output = $process.StandardOutput.ReadToEndAsync()
                $child.Error = $process.StandardError.ReadToEndAsync()
            }
            $deadline = [DateTime]::UtcNow.AddSeconds(15)
            foreach ($child in $children) {
                $process = $child.Process
                $remaining = [Math]::Max(1, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
                if (-not $process.WaitForExit($remaining)) { throw 'Concurrent receipt fixture deadline exceeded.' }
                if (-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($child.Output, $child.Error), 1000)) { throw 'Concurrent receipt output did not drain.' }
                $null = $child.Output.GetAwaiter().GetResult()
                $null = $child.Error.GetAwaiter().GetResult()
                if ($process.ExitCode -ne $expectedExit) { throw 'Receipt fixture returned an unexpected exit.' }
            }
        } finally {
            foreach ($child in $children) {
                $process = $child.Process
                try {
                    if (-not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Receipt fixture could not be reaped.' } }
                } finally { $process.Dispose() }
            }
        }
    }
    $receipts = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work/verification') -Filter '*.json')
    if ($receipts.Count -ne 4) { throw 'Verification records were overwritten.' }
    $stages = @()
    foreach ($file in $receipts) {
        $text = Get-Content -LiteralPath $file.FullName -Raw -Encoding utf8
        if ($text.Contains('private-fixture-marker')) { throw 'Private arguments escaped into a receipt.' }
        $value = $text | ConvertFrom-Json
        $expectedOutcome = if ($expectedExit -eq 0) { 'success' } else { 'failure' }
        if (-not $value.started -or -not $value.reaped -or $value.exitCode -ne $expectedExit -or $value.outcome -ne $expectedOutcome -or $value.expired -or $value.command_sha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid command outcome receipt.' }
        $stages += $value.stage
    }
    if (Compare-Object @('fmt', 'clippy', 'nextest', 'test') $stages) { throw 'Command stages are not attributable.' }
    $logs = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work') -Filter 'verification_*.log')
    if ($logs.Count -ne 4) { throw 'Verification lifecycle logs were overwritten.' }
    foreach ($file in $logs) {
        $text = Get-Content -LiteralPath $file.FullName -Raw -Encoding utf8
        if ($text.Contains('private-fixture-marker') -or -not $text.Contains('Verification started') -or -not $text.Contains('Verification stopped')) { throw 'Invalid or unsafe verification lifecycle log.' }
    }
    if (-not $IsWindows) {
        # exec keeps the stalled process itself under the wrapper's tree owner.
        [IO.File]::WriteAllText((Join-Path $stub 'cargo'), "#!/bin/sh`nexec sleep 60`n", [Text.UTF8Encoding]::new($false))
        $info = [Diagnostics.ProcessStartInfo]::new($pwsh)
        $info.UseShellExecute = $false; $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
        foreach ($argument in @('-NoProfile', '-NonInteractive', '-File', (Join-Path $sandbox 'invoke.ps1'), '-Stage', 'test', '-Wall', '2', '-Idle', '1')) { $info.ArgumentList.Add($argument) }
        $process = [Diagnostics.Process]::new(); $process.StartInfo = $info
        $started = $false
        try {
            $started = $process.Start()
            if (-not $started) { throw 'Cannot start timeout receipt fixture.' }
            $process.StandardInput.Close()
            $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(10000)) { throw 'Timeout receipt fixture exceeded its outer bound.' }
            if (-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($stdout, $stderr), 1000)) { throw 'Timeout receipt output did not drain.' }
            $null = $stdout.GetAwaiter().GetResult(); $null = $stderr.GetAwaiter().GetResult()
            if ($process.ExitCode -eq 0) { throw 'A timeout was reported as success.' }
        } finally {
            try {
                if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Timeout fixture could not be reaped.' } }
            } finally { $process.Dispose() }
        }
        $new = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work/verification') -Filter '*.json' | Where-Object FullName -notin $receipts.FullName)
        if ($new.Count -ne 1) { throw 'Timeout receipt missing or overwritten.' }
        $value = Get-Content -LiteralPath $new[0].FullName -Raw -Encoding utf8 | ConvertFrom-Json
        if (-not $value.expired -or -not $value.reaped -or $value.exitCode -eq 0 -or $value.outcome -ne 'timeout') { throw 'Invalid timeout receipt.' }
    }
    # An empty PATH makes Cargo startup fail without touching any installed compiler.
    [IO.File]::WriteAllText((Join-Path $sandbox 'startup.ps1'), @'
param([string]$Invoke, [string]$EmptyPath)
$env:PATH = $EmptyPath
& $Invoke -Stage test
exit $LASTEXITCODE
'@, [Text.UTF8Encoding]::new($false))
    $emptyPath = Join-Path $sandbox 'empty-path'
    [IO.Directory]::CreateDirectory($emptyPath) | Out-Null
    $previous = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work/verification') -Filter '*.json')
    $info = [Diagnostics.ProcessStartInfo]::new($pwsh)
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-File', (Join-Path $sandbox 'startup.ps1'), '-Invoke', (Join-Path $sandbox 'invoke.ps1'), '-EmptyPath', $emptyPath)) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new(); $process.StartInfo = $info
    $started = $false
    try {
        $started = $process.Start()
        if (-not $started) { throw 'Cannot start startup-failure receipt fixture.' }
        $process.StandardInput.Close()
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(10000)) { throw 'Startup-failure fixture exceeded its outer bound.' }
        if (-not [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($stdout, $stderr), 1000)) { throw 'Startup-failure output did not drain.' }
        if ($process.ExitCode -eq 0) { throw 'Cargo startup failure was reported as success.' }
        $new = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work/verification') -Filter '*.json' | Where-Object FullName -notin $previous.FullName)
        if ($new.Count -ne 1) { throw 'Startup-failure receipt missing or overwritten.' }
        $text = Get-Content -LiteralPath $new[0].FullName -Raw -Encoding utf8
        $value = $text | ConvertFrom-Json
        if ($value.started -or $null -ne $value.pid -or -not $value.reaped -or $value.expired -or $value.exitCode -eq 0 -or $value.outcome -ne 'startup_failure' -or $text.Contains('private-fixture-marker')) { throw 'Invalid startup-failure receipt.' }
    } finally {
        try {
            if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Startup-failure fixture could not be reaped.' } }
        } finally { $process.Dispose() }
    }
    Write-Host 'Concurrent command-specific receipts retained; private arguments suppressed; startup-failure and applicable timeout controls passed.'
} finally {
    $env:PATH = $originalPath
    Remove-Item -LiteralPath $sandbox -Recurse -Force
}
