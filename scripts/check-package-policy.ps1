param([Parameter(Mandatory)][ValidateSet('windows-x64', 'linux-x64', 'macos-arm64', 'macos-x64')][string]$Platform)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
$files = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'release-files.json') -Raw -Encoding utf8 | ConvertFrom-Json
$binaryName = if ($Platform -eq 'windows-x64') { 'upwork-connector.exe' } else { 'upwork-connector' }
$rows = @()
$cases = @('wrong_source', 'wrong_tree', 'wrong_digest', 'dirty_source', 'untracked_source', 'no_git', 'archive_failure', 'extraction_failure', 'promotion_failure', 'late_smoke', 'success', 'existing_output')
if ($Platform -eq 'linux-x64') { $cases += 'skills_failure' }
foreach ($case in $cases) {
    $sandbox = Join-Path $work ('package-policy-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $sandbox | Out-Null
    try {
        $inventory = @($files.common) + @($files.windows) + @($files.unix) + @('Cargo.toml', 'scripts/package.ps1', 'scripts/release-files.json', 'docs/skills-package.md', 'scripts/build-receipt.ps1')
        foreach ($file in ($inventory | Sort-Object -Unique)) {
            $target = Join-Path $sandbox $file
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
            Copy-Item -LiteralPath (Join-Path $root $file) -Destination $target
        }
        if ($case -in @('archive_failure', 'extraction_failure', 'promotion_failure', 'skills_failure')) {
            # Inject only into this copied fixture, never add production fault switches.
            $script = Join-Path $sandbox 'scripts/package.ps1'
            $text = [IO.File]::ReadAllText($script, [Text.Encoding]::UTF8)
            $marker = switch ($case) {
                archive_failure { if ($Platform -eq 'windows-x64') { '[IO.Compression.ZipFile]::ExtractToDirectory($archive, $verify)' } else { 'New-Item -ItemType Directory -Path $verify | Out-Null' } }
                extraction_failure { '$actual = @(Get-ChildItem -LiteralPath $verify -Recurse -Force -File' }
                promotion_failure { '$promoted.Add($final)' }
                skills_failure { '$zip = [IO.Compression.ZipFile]::OpenRead($skillArchive)' }
            }
            if ([regex]::Matches($text, [regex]::Escape($marker)).Count -ne 1) { throw 'Package fixture seam changed.' }
            $fault = "throw 'Synthetic package stage failure: $case'; "
            $replacement = if ($case -eq 'promotion_failure') { "$marker; $fault" } else { "$fault$marker" }
            [IO.File]::WriteAllText($script, $text.Replace($marker, $replacement), [Text.UTF8Encoding]::new($false))
        }
        New-Item -ItemType Directory -Force -Path (Join-Path $sandbox 'bin') | Out-Null
        $binary = Join-Path $sandbox "bin/$binaryName"
        Copy-Item -LiteralPath (Join-Path $root "bin/$binaryName") -Destination $binary
        if ($case -eq 'late_smoke') { [IO.File]::WriteAllText($binary, 'not-an-executable', [Text.UTF8Encoding]::new($false)) }
        [IO.File]::WriteAllText((Join-Path $sandbox '.gitignore'), "/bin/`n/.ci-work/`n/dist/`n", [Text.UTF8Encoding]::new($false))
        & git -C $sandbox init --quiet
        if ($LASTEXITCODE) { throw 'Cannot initialize package fixture.' }
        & git -C $sandbox -c user.name=Fixture -c user.email=Kiaro.Sama.Dev@gmail.com add --all
        if ($LASTEXITCODE) { throw 'Cannot stage package fixture.' }
        & git -C $sandbox -c user.name=Fixture -c user.email=Kiaro.Sama.Dev@gmail.com -c core.hooksPath=empty-hooks commit --quiet -m 'test: package fixture' -m 'Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>'
        if ($LASTEXITCODE) { throw 'Cannot commit package fixture.' }
        $source = (& git -C $sandbox rev-parse HEAD).Trim()
        $identity = & (Join-Path $sandbox 'scripts/build-receipt.ps1') -Capture
        & (Join-Path $sandbox 'scripts/build-receipt.ps1') -Binary $binary -Expected $identity
        if ($case -eq 'no_git') { Remove-Item -LiteralPath (Join-Path $sandbox '.git') -Recurse -Force }
        if ($case -eq 'wrong_source') { $source = '0' * 40 }
        if ($case -eq 'wrong_tree') {
            $receipt = Join-Path $sandbox '.ci-work/build-provenance.json'
            $value = Get-Content -LiteralPath $receipt -Raw -Encoding utf8 | ConvertFrom-Json
            $value.source_tree = '0' * 40
            $value | ConvertTo-Json | Set-Content -LiteralPath $receipt -Encoding utf8NoBOM
        }
        if ($case -eq 'dirty_source') { [IO.File]::AppendAllText((Join-Path $sandbox 'README.md'), "`nChanged input`n", [Text.UTF8Encoding]::new($false)) }
        if ($case -eq 'untracked_source') { [IO.File]::WriteAllText((Join-Path $sandbox 'unexpected-input.txt'), 'untracked fixture', [Text.UTF8Encoding]::new($false)) }
        if ($case -eq 'wrong_digest') { [IO.File]::AppendAllText($binary, 'changed', [Text.UTF8Encoding]::new($false)) }
        $dist = Join-Path $sandbox 'dist'
        New-Item -ItemType Directory -Force -Path $dist | Out-Null
        $sentinel = Join-Path $dist 'unrelated.keep'
        [IO.File]::WriteAllText($sentinel, 'preserve', [Text.UTF8Encoding]::new($false))
        if ($case -eq 'existing_output') {
            $version = (Select-String -LiteralPath (Join-Path $sandbox 'Cargo.toml') -Pattern '^version = "([0-9.]+)"$').Matches[0].Groups[1].Value
            $extension = if ($Platform -eq 'windows-x64') { 'zip' } else { 'tar.gz' }
            $existing = Join-Path $dist "upwork-agent-plugin-$version-$Platform.$extension"
            [IO.File]::WriteAllText($existing, 'preexisting', [Text.UTF8Encoding]::new($false))
        }
        $info = [Diagnostics.ProcessStartInfo]::new($pwsh)
        $info.UseShellExecute = $false; $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
        foreach ($arg in @('-NoProfile', '-NonInteractive', '-File', (Join-Path $sandbox 'scripts/package.ps1'), '-Platform', $Platform, '-SourceSha', $source)) { $info.ArgumentList.Add($arg) }
        $process = [Diagnostics.Process]::new(); $process.StartInfo = $info
        $started = $false
        try {
            $started = $process.Start()
            if (-not $started) { throw 'Cannot start package fixture.' }
            $process.StandardInput.Close()
            $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(30000)) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Package fixture could not be reaped.' }; throw 'Package fixture deadline exceeded.' }
            $outcomeText = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
            $expectedSuccess = $case -eq 'success'
            if (($process.ExitCode -eq 0) -ne $expectedSuccess) { throw "Unexpected package outcome: $case" }
            $reason = switch ($case) {
                wrong_source { 'Package source does not match the current Git checkout.' }
                wrong_tree { 'Build provenance does not match package inputs.' }
                wrong_digest { 'Build provenance does not match package inputs.' }
                dirty_source { 'Package source has modified tracked inputs.' }
                untracked_source { 'Package source has untracked, non-ignored inputs.' }
                no_git { 'Packaging requires this project root to be the Git checkout root.' }
                existing_output { 'Package output already exists; refusing to overwrite.' }
                { $_ -in @('archive_failure', 'extraction_failure', 'promotion_failure', 'skills_failure') } { "Synthetic package stage failure: $case" }
            }
            if ($reason -and -not $outcomeText.Contains($reason)) { throw "Package failed for an unrelated reason: $case" }
            if ([IO.File]::ReadAllText($sentinel, [Text.Encoding]::UTF8) -ne 'preserve') { throw 'Unrelated output changed.' }
            $products = @(Get-ChildItem -LiteralPath $dist -Force | Where-Object Name -ne 'unrelated.keep')
            if (-not $expectedSuccess -and $case -ne 'existing_output' -and $products.Count) { throw 'Failed package left final or pending output.' }
            if ($case -eq 'existing_output' -and ([IO.File]::ReadAllText($existing, [Text.Encoding]::UTF8) -ne 'preexisting' -or $products.Count -ne 1)) { throw 'Preexisting output was changed.' }
            if ($case -eq 'late_smoke') {
                $packageLogs = @(Get-ChildItem -LiteralPath (Join-Path $sandbox '.ci-work') -Filter 'upwork-package_*.log')
                if ($packageLogs.Count -ne 1 -or -not ([IO.File]::ReadAllText($packageLogs[0].FullName, [Text.Encoding]::UTF8)).Contains('Packaging or archive smoke failed.')) { throw 'Late smoke failure was not recorded.' }
                # Retry the same destination after repairing only the intentionally broken executable.
                Copy-Item -LiteralPath (Join-Path $root "bin/$binaryName") -Destination $binary -Force
                & (Join-Path $sandbox 'scripts/build-receipt.ps1') -Binary $binary -Expected $identity
                $retry = [Diagnostics.Process]::new(); $retry.StartInfo = $info
                $retryStarted = $false
                try {
                    $retryStarted = $retry.Start()
                    if (-not $retryStarted) { throw 'Cannot start corrected package retry.' }
                    $retry.StandardInput.Close()
                    $out = $retry.StandardOutput.ReadToEndAsync(); $err = $retry.StandardError.ReadToEndAsync()
                    if (-not $retry.WaitForExit(30000)) { throw 'Corrected package retry exceeded its deadline.' }
                    $null = $out.GetAwaiter().GetResult(); $null = $err.GetAwaiter().GetResult()
                    if ($retry.ExitCode -ne 0) { throw 'Corrected package retry failed after a late smoke rejection.' }
                } finally {
                    try {
                        if ($retryStarted -and -not $retry.HasExited) { $retry.Kill($true); if (-not $retry.WaitForExit(5000)) { throw 'Package retry could not be reaped.' } }
                    } finally { $retry.Dispose() }
                }
            }
            $rows += @{ case=$case; passed=$true; reaped=$process.HasExited }
        } finally {
            if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Package fixture could not be reaped.' } }
            $process.Dispose()
        }
    } finally { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}
$rows | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'package-policy.json') -Encoding utf8NoBOM
Write-Host 'Package source/digest/smoke/promotion/no-overwrite controls passed.'
