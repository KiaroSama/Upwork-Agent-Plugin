$ErrorActionPreference = 'Stop'
if ($IsWindows) { throw 'Unix build policy requires native Unix process groups.' }
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
$rows = @()
$signalTool = (Get-Command kill -CommandType Application -ErrorAction Stop).Source
$processTool = (Get-Command ps -CommandType Application -ErrorAction Stop).Source
# This harness owns the real build.sh; only Cargo is replaced, never a compiler invocation.
foreach ($case in @('success', 'failure', 'quiet', 'progress', 'cancel')) {
    $sandbox = Join-Path $work ('unix build [' + [Guid]::NewGuid().ToString('N') + ']')
    [IO.Directory]::CreateDirectory($sandbox) | Out-Null
    $process = $null; $started = $false
    try {
        foreach ($directory in @('bin', 'stub', '.ci-work', 'empty-hooks')) { [IO.Directory]::CreateDirectory((Join-Path $sandbox $directory)) | Out-Null }
        Copy-Item -LiteralPath (Join-Path $root 'build.sh') -Destination (Join-Path $sandbox 'build.sh')
        Copy-Item -LiteralPath (Join-Path $root 'Cargo.toml') -Destination (Join-Path $sandbox 'Cargo.toml')
        [IO.File]::WriteAllText((Join-Path $sandbox '.gitignore'), "/bin/`n/stub/`n/.ci-work/`n/.cargo-cache/`n/.ci-cache/`n/logs/`n/target/`n/empty-hooks/`n", [Text.UTF8Encoding]::new($false))
        $stub = @'
#!/bin/sh
set -eu
[ "$*" = 'build --release --locked' ] || exit 27
printf 'STARTED\n' > .ci-work/fixture-ready
case "$FIXTURE_BUILD_CASE" in
success) mkdir -p target/release; printf 'synthetic-new-binary' > target/release/upwork-connector; exit 0;;
failure) exit 23;;
quiet) exec sleep 60;;
progress) while :; do printf 'fixture progress\n'; sleep 1; done;;
cancel) sh -c 'sleep 60 & leaf=$!; printf "%s\n" "$leaf" > .ci-work/fixture-leaf; wait "$leaf"' & wait "$!";;
esac
'@
        [IO.File]::WriteAllText((Join-Path $sandbox 'stub/cargo'), $stub.Replace("`r`n", "`n") + "`n", [Text.UTF8Encoding]::new($false))
        & chmod 700 (Join-Path $sandbox 'stub/cargo')
        if ($LASTEXITCODE) { throw 'Cannot prepare Unix Cargo stub.' }
        $installed = Join-Path $sandbox 'bin/upwork-connector'
        [IO.File]::WriteAllText($installed, 'synthetic-old-binary', [Text.UTF8Encoding]::new($false))
        & git -C $sandbox init --quiet
        if ($LASTEXITCODE) { throw 'Cannot initialize isolated source fixture.' }
        & git -C $sandbox -c core.hooksPath=empty-hooks add --all
        if ($LASTEXITCODE) { throw 'Cannot stage isolated source fixture.' }
        & git -C $sandbox -c core.hooksPath=empty-hooks -c user.name=Fixture -c user.email=Kiaro.Sama.Dev@gmail.com commit --quiet -m 'test: source build fixture' -m 'Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>'
        if ($LASTEXITCODE) { throw 'Cannot record isolated source fixture.' }
        $info = [Diagnostics.ProcessStartInfo]::new('sh')
        $info.ArgumentList.Add((Join-Path $sandbox 'build.sh'))
        $info.WorkingDirectory = $work
        $info.UseShellExecute = $false; $info.CreateNoWindow = $true
        $info.RedirectStandardInput = $true; $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
        $info.Environment['PATH'] = (Join-Path $sandbox 'stub') + [IO.Path]::PathSeparator + $env:PATH
        $info.Environment['FIXTURE_BUILD_CASE'] = $case
        $info.Environment['UPWORK_BUILD_WALL_SECONDS'] = '6'
        $info.Environment['UPWORK_BUILD_IDLE_SECONDS'] = if ($case -eq 'quiet') { '2' } else { '5' }
        $process = [Diagnostics.Process]::new(); $process.StartInfo = $info
        $started = $process.Start()
        if (-not $started) { throw 'Cannot start Unix build fixture.' }
        $process.StandardInput.Close()
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        if ($case -eq 'cancel') {
            $deadline = [DateTime]::UtcNow.AddSeconds(5)
            while (-not [IO.File]::Exists((Join-Path $sandbox '.ci-work/fixture-leaf'))) {
                if ($process.HasExited -or [DateTime]::UtcNow -ge $deadline) { throw 'Cancellation fixture did not reach its child gate.' }
                $null = $process.WaitForExit(50)
            }
            & $signalTool -TERM $process.Id
            if ($LASTEXITCODE) { throw 'Cannot signal the owned Unix build.' }
        }
        if (-not $process.WaitForExit(15000)) { throw 'Unix build fixture exceeded its outer bound.' }
        $out = $stdout.GetAwaiter().GetResult(); $err = $stderr.GetAwaiter().GetResult()
        $expected = switch ($case) { success { 0 } failure { 23 } quiet { 124 } progress { 124 } cancel { 143 } }
        if ($process.ExitCode -ne $expected) {
            # These streams contain only this isolated fixture's synthetic build diagnostics.
            [Console]::Error.WriteLine($err)
            foreach ($file in @(Get-ChildItem -LiteralPath (Join-Path $sandbox 'logs') -Filter '*.log' -ErrorAction SilentlyContinue)) {
                [Console]::Error.WriteLine([IO.File]::ReadAllText($file.FullName, [Text.Encoding]::UTF8))
            }
            throw "Unexpected Unix build exit for $case`: $($process.ExitCode)"
        }
        $bytes = [IO.File]::ReadAllText($installed, [Text.Encoding]::UTF8)
        if ($bytes -ne $(if ($case -eq 'success') { 'synthetic-new-binary' } else { 'synthetic-old-binary' })) { throw 'Unix build publication boundary failed.' }
        if (($out.Contains('Native connector built.')) -ne ($case -eq 'success')) { throw 'Unix build reported an incorrect completion.' }
        $logs = @(Get-ChildItem -LiteralPath (Join-Path $sandbox 'logs') -Filter '*.log')
        if ($logs.Count -ne 1) { throw 'Unix build log missing or duplicated.' }
        $text = [IO.File]::ReadAllText($logs[0].FullName, [Text.Encoding]::UTF8)
        if (-not $text.Contains("Native build stopped exit_code=$expected")) { throw 'Unix build lifecycle log incomplete.' }
        if ($case -eq 'progress' -and -not $text.Contains('fixture progress')) { throw 'Build diagnostics were not captured.' }
        if ($case -in @('quiet', 'progress') -and -not $text.Contains('exceeded wall or idle bound')) { throw 'Timeout fixture failed for an unrelated cause.' }
        $groupMatch = [regex]::Match($text, 'Build process ownership pid=(\d+) pgid=(\d+)\.')
        if (-not $groupMatch.Success -or $groupMatch.Groups[1].Value -ne $groupMatch.Groups[2].Value) { throw 'Build process group was not verified.' }
        $group = [int]$groupMatch.Groups[2].Value
        $members = @(& $processTool -axo pgid=,stat= | Where-Object {
            $fields = $_.Trim() -split '\s+'
            $fields.Count -eq 2 -and $fields[0] -eq [string]$group -and $fields[1] -notlike 'Z*'
        })
        if ($members.Count) { throw 'Owned build group still has a live member.' }
        $receipt = Join-Path $sandbox '.ci-work/build-provenance.json'
        if ([IO.File]::Exists($receipt) -ne ($case -eq 'success')) { throw 'Failed Unix build left usable provenance.' }
        if ($case -eq 'success') {
            $value = Get-Content -LiteralPath $receipt -Raw -Encoding utf8 | ConvertFrom-Json
            if ($value.binary_sha256 -ne (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Unix build digest mismatch.' }
        }
        if ($case -eq 'cancel') {
            $leaf = [int][IO.File]::ReadAllText((Join-Path $sandbox '.ci-work/fixture-leaf'), [Text.Encoding]::UTF8)
            $live = @(& $processTool -axo pid=,stat= | Where-Object {
                $fields = $_.Trim() -split '\s+'
                $fields.Count -eq 2 -and $fields[0] -eq [string]$leaf -and $fields[1] -notlike 'Z*'
            })
            if ($live.Count) { throw 'Owned build descendant survived cancellation.' }
        }
        $rows += @{ case=$case; exit=$process.ExitCode; reaped=$process.HasExited; preserved=($case -ne 'success'); passed=$true }
    } finally {
        if ($null -ne $process) {
            try {
                if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Unix fixture could not be reaped.' } }
            } finally { $process.Dispose() }
        }
        Remove-Item -LiteralPath $sandbox -Recurse -Force
    }
}
$rows | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'unix-build-policy.json') -Encoding utf8NoBOM
Write-Host 'Unix build success, failure, idle/wall timeout and descendant cancellation controls passed.'
