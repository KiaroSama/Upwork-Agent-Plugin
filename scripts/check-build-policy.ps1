param(
    [string]$BuildScript = (Join-Path (Split-Path -Parent $PSScriptRoot) 'build.ps1'),
    [string]$Output = (Join-Path (Split-Path -Parent $PSScriptRoot) '.ci-work/build-policy.json')
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
New-Item -ItemType Directory -Force -Path $work | Out-Null
function Write-PolicyEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [build-policy] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
$rows = [Collections.Generic.List[object]]::new()
$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
$binary = if ($IsWindows) { 'upwork-connector.exe' } else { 'upwork-connector' }
# All execution is inside owned disposable roots. The stub captures the actual
# build.ps1 invocation; it neither compiles code nor touches the real installation.
$stub = @'
param([string[]]$CargoArgs, [int]$WallSeconds = 900, [int]$IdleSeconds = 180)
$ErrorActionPreference = 'Stop'
function Write-FixtureEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [verification-fixture] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
$root = Split-Path -Parent $PSScriptRoot
@{ args = $CargoArgs; wall = $WallSeconds; idle = $IdleSeconds } | ConvertTo-Json | Set-Content (Join-Path $root 'invocation.json') -Encoding utf8
$mode = (Get-Content -LiteralPath (Join-Path $root 'mode.txt') -Raw).Trim()
Write-FixtureEvent DEBUG 'Captured build verification arguments without running Cargo.'
if ($mode -eq 'verification_failure') { Write-FixtureEvent WARNING 'Injected verification failure.'; exit 23 }
if ($mode -eq 'success') {
    $binary = if ($IsWindows) { 'upwork-connector.exe' } else { 'upwork-connector' }
    $directory = Join-Path $root 'target/release'
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    [IO.File]::WriteAllText((Join-Path $directory $binary), 'synthetic-new-binary')
}
Write-FixtureEvent INFO 'Synthetic verification completed.'
exit 0
'@
$protect = @'
param([string]$ProjectRoot)
function Write-FixtureEvent {
    param([ValidateSet('INFO', 'WARNING', 'ERROR', 'DEBUG')][string]$Level, [string]$Message)
    [Console]::Error.WriteLine(('[{0}] [{1}] [permission-fixture] {2}' -f [DateTime]::UtcNow.ToString('o'), $Level, $Message))
}
Write-FixtureEvent INFO 'Synthetic permissions hook; no ACL changes.'
exit 0
'@
try {
    Write-PolicyEvent INFO 'Checking release-only timeout policy and failed-build publication boundaries.'
    $tokens = $null; $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'scripts/verify.ps1'), [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count) { throw 'Invalid verification wrapper syntax.' }
    $defaults = @{}
    foreach ($parameter in $ast.ParamBlock.Parameters) {
        if ($parameter.Name.VariablePath.UserPath -in @('WallSeconds', 'IdleSeconds')) {
            $defaults[$parameter.Name.VariablePath.UserPath] = $parameter.DefaultValue.SafeGetValue()
        }
    }
    if ($defaults.WallSeconds -ne 900 -or $defaults.IdleSeconds -ne 180) { throw 'Ordinary verification limits changed.' }
    foreach ($mode in @('success', 'verification_failure', 'missing_artifact')) {
        $caseRoot = Join-Path $work ('build-policy-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Force -Path (Join-Path $caseRoot 'scripts'), (Join-Path $caseRoot 'bin') | Out-Null
        Copy-Item -LiteralPath $BuildScript -Destination (Join-Path $caseRoot 'build.ps1')
        [IO.File]::WriteAllText((Join-Path $caseRoot 'scripts/verify.ps1'), $stub)
        [IO.File]::WriteAllText((Join-Path $caseRoot 'scripts/protect-state.ps1'), $protect)
        [IO.File]::WriteAllText((Join-Path $caseRoot 'mode.txt'), $mode)
        $installed = Join-Path $caseRoot "bin/$binary"
        [IO.File]::WriteAllText($installed, 'synthetic-old-binary')
        $info = [Diagnostics.ProcessStartInfo]::new()
        $info.FileName = $pwsh
        $info.UseShellExecute = $false
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        $info.RedirectStandardInput = $true
        $info.WorkingDirectory = $caseRoot
        foreach ($argument in @('-NoProfile', '-NonInteractive', '-File', (Join-Path $caseRoot 'build.ps1'))) { $info.ArgumentList.Add($argument) }
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $info
        $started = $false
        try {
            $started = $process.Start()
            if (-not $started) { throw 'Cannot start owned build fixture.' }
            $process.StandardInput.Close()
            $out = $process.StandardOutput.ReadToEndAsync()
            $err = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(15000)) { $process.Kill($true); $process.WaitForExit(); throw 'Build fixture deadline exceeded.' }
            $text = $out.GetAwaiter().GetResult() + $err.GetAwaiter().GetResult()
            $invocation = Get-Content -LiteralPath (Join-Path $caseRoot 'invocation.json') -Raw | ConvertFrom-Json
            $published = [IO.File]::ReadAllText($installed)
            $argsOk = ($invocation.args -join '|') -eq 'build|--release|--locked'
            $budgetOk = $invocation.wall -eq 900 -and $invocation.idle -eq 600
            $passed = switch ($mode) {
                'success' { $argsOk -and $budgetOk -and $process.ExitCode -eq 0 -and $published -eq 'synthetic-new-binary' -and $text.Contains('Native connector built.') }
                'verification_failure' { $argsOk -and $process.ExitCode -eq 23 -and $published -eq 'synthetic-old-binary' -and -not $text.Contains('Native connector built.') }
                'missing_artifact' { $argsOk -and $process.ExitCode -ne 0 -and $published -eq 'synthetic-old-binary' -and -not $text.Contains('Native connector built.') }
            }
            $rows.Add(@{ case = $mode; passed = [bool]$passed; wall = $invocation.wall; idle = $invocation.idle; exitCode = $process.ExitCode; reaped = $process.HasExited })
            Write-PolicyEvent $(if ($passed) { 'INFO' } else { 'ERROR' }) "Case=$mode passed=$passed."
        } finally {
            if ($started -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
            $process.Dispose()
            Remove-Item -LiteralPath $caseRoot -Recurse -Force
        }
    }
    $passedCount = @($rows | Where-Object { $_.passed }).Count
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Output) | Out-Null
    @{ cases = $rows.Count; passed = $passedCount; general_wall = $defaults.WallSeconds; general_idle = $defaults.IdleSeconds; source_sha256 = (Get-FileHash -LiteralPath $BuildScript -Algorithm SHA256).Hash.ToLowerInvariant(); results = @($rows.ToArray()) } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Output -Encoding utf8
    Write-PolicyEvent DEBUG 'Ordinary verification defaults remain wall=900 and idle=180.'
    Write-PolicyEvent WARNING 'Synthetic launch tests validate policy and publication, not compilation performance.'
    if ($passedCount -ne $rows.Count) { exit 1 }
    Write-PolicyEvent INFO 'All release build policy contracts passed.'
} catch {
    Write-PolicyEvent ERROR 'Build policy fixture failed; no unexecuted case is counted.'
    throw
}
