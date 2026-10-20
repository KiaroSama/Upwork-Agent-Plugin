param(
    [Parameter(Mandatory)][ValidateSet('windows-x64', 'linux-x64', 'macos-arm64', 'macos-x64')][string]$Platform,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceSha
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
$destination = Join-Path $work ("package-" + [Guid]::NewGuid().ToString('N'))
$verify = Join-Path $work ("package smoke " + [Guid]::NewGuid().ToString('N'))
$output = Join-Path $root 'dist'
$pending = Join-Path $output ('.pending-' + [Guid]::NewGuid().ToString('N'))
$promoted = [Collections.Generic.List[string]]::new()
New-Item -ItemType Directory -Force -Path $work, $output, $pending | Out-Null
$env:TEMP = $env:TMP = $env:TMPDIR = $work
$log = Join-Path $work ("upwork-package_" + [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss') + '_UTC_' + [Guid]::NewGuid().ToString('N') + '.log')
function Event([string]$level, [string]$message) {
    $line = '[{0} UTC] [{1}] [upwork-package] {2}' -f [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'), $level, $message
    try { [IO.File]::AppendAllText($log, "$line`n", [Text.UTF8Encoding]::new($false)) }
    catch { [Console]::Error.WriteLine('Packaging log unavailable.') }
}
function Smoke([string]$binary, [string[]]$arguments, [int]$expected) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $binary
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.WorkingDirectory = $work
    foreach ($key in @('TEMP', 'TMP', 'TMPDIR')) { $info.Environment[$key] = Join-Path $verify '.cache/tmp' }
    $info.Environment['XDG_CACHE_HOME'] = Join-Path $verify '.cache'
    foreach ($arg in $arguments) { $info.ArgumentList.Add($arg) }
    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $info
    $started = $false
    try {
        $started = $child.Start()
        if (-not $started) { throw 'Cannot start packaged binary.' }
        $child.StandardInput.Close()
        $stdout = $child.StandardOutput.ReadToEndAsync()
        $stderr = $child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(5000)) { $child.Kill($true); if (-not $child.WaitForExit(5000)) { throw 'Packaged binary could not be reaped.' }; throw 'Packaged binary smoke timeout.' }
        if ($child.ExitCode -ne $expected) { throw 'Packaged binary returned an unexpected exit code.' }
        $stderr.GetAwaiter().GetResult() | Out-Null
        return $stdout.GetAwaiter().GetResult()
    } finally {
        if ($started -and -not $child.HasExited) { $child.Kill($true); if (-not $child.WaitForExit(5000)) { throw 'Packaged binary could not be reaped.' } }
        $child.Dispose()
    }
}
try {
    Event INFO 'Packaging started.'
    $checkout = @(& git -C $root rev-parse --show-toplevel)
    if ($LASTEXITCODE -ne 0 -or $checkout.Count -ne 1 -or [IO.Path]::GetFullPath($checkout[0].Trim()) -ne [IO.Path]::GetFullPath($root)) {
        throw 'Packaging requires this project root to be the Git checkout root.'
    }
    $targets = @{'windows-x64'='x86_64-pc-windows-msvc'; 'linux-x64'='x86_64-unknown-linux-gnu'; 'macos-arm64'='aarch64-apple-darwin'; 'macos-x64'='x86_64-apple-darwin'}
    $hostTarget = @(& rustc -vV) | Where-Object { $_ -like 'host: *' }
    if ($LASTEXITCODE -ne 0 -or $hostTarget -ne "host: $($targets[$Platform])") { throw 'Runner architecture does not match the requested archive.' }
    $version = (Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$').Matches[0].Groups[1].Value
    $files = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'release-files.json') -Raw -Encoding utf8 | ConvertFrom-Json
    $windows = $Platform -eq 'windows-x64'
    $allowed = @($files.common) + $(if ($windows) { @($files.windows) } else { @($files.unix) })
    $binaryName = if ($windows) { 'upwork-connector.exe' } else { 'upwork-connector' }
    $allowed += "bin/$binaryName"
    foreach ($file in $allowed) {
        if ($file -match '(^|/)(\.\.|\.state|\.ai|secrets\.md|\.mcp\.json|logs|target|node_modules)(/|$)' -or [IO.Path]::IsPathRooted($file)) { throw 'Invalid release allowlist.' }
        $source = Join-Path $root $file
        if (-not (Test-Path -LiteralPath $source -PathType Leaf) -or (Get-Item -LiteralPath $source).LinkType) { throw "Release source missing or linked: $file" }
        $target = Join-Path $destination $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
        Copy-Item -LiteralPath $source -Destination $target
    }
    if ($Platform -eq 'linux-x64') {
        $skillArchive = Join-Path $pending "upwork-selection-plugin-$version.zip"
        if (Test-Path -LiteralPath $skillArchive) { throw 'Skills archive already exists.' }
        $skillStage = Join-Path $destination 'skills-only-stage'
        try {
            foreach ($file in $files.common) {
                $target = Join-Path $skillStage $file
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
                Copy-Item -LiteralPath (Join-Path $root $file) -Destination $target
            }
            Copy-Item -LiteralPath (Join-Path $root 'docs/skills-package.md') -Destination (Join-Path $skillStage 'README.md') -Force
            [IO.Compression.ZipFile]::CreateFromDirectory($skillStage, $skillArchive)
            $zip = [IO.Compression.ZipFile]::OpenRead($skillArchive)
            try {
                $members = @($zip.Entries | ForEach-Object { $_.FullName })
                if (Compare-Object (@($files.common) | Sort-Object) ($members | Sort-Object)) { throw 'Skills archive inventory mismatch.' }
                $landing = [IO.StreamReader]::new($zip.GetEntry('README.md').Open(), [Text.Encoding]::UTF8)
                try { $landingText = $landing.ReadToEnd() } finally { $landing.Dispose() }
                if (-not $landingText.StartsWith('# Upwork skills-only package')) { throw 'Skills archive landing page mismatch.' }
                foreach ($match in [regex]::Matches($landingText, '\]\(([^)#]+)(?:#[^)]*)?\)')) {
                    $link = $match.Groups[1].Value
                    if ($link -notmatch '^https://' -and $link -notin $members) { throw 'Skills landing page references an absent resource.' }
                }
            } finally { $zip.Dispose() }
            $hash = (Get-FileHash -LiteralPath $skillArchive -Algorithm SHA256).Hash.ToLowerInvariant()
            "$hash  $([IO.Path]::GetFileName($skillArchive))" | Set-Content -LiteralPath "$skillArchive.sha256" -Encoding utf8NoBOM
        } finally {
            if (Test-Path -LiteralPath $skillStage) { Remove-Item -LiteralPath $skillStage -Recurse -Force }
        }
    }
    $head = (& git -C $root rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -ne $SourceSha) { throw 'Package source does not match the current Git checkout.' }
    $tree = (& git -C $root rev-parse 'HEAD^{tree}').Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve package source tree.' }
    & git -C $root diff --quiet HEAD --
    if ($LASTEXITCODE -ne 0) { throw 'Package source has modified tracked inputs.' }
    $untracked = @(& git -C $root ls-files --others --exclude-standard)
    if ($LASTEXITCODE -ne 0 -or $untracked.Count) { throw 'Package source has untracked, non-ignored inputs.' }
    $build = Get-Content -LiteralPath (Join-Path $work 'build-provenance.json') -Raw -Encoding utf8 | ConvertFrom-Json
    $binaryHash = (Get-FileHash -LiteralPath (Join-Path $destination "bin/$binaryName") -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($build.source_sha -ne $head -or $build.source_tree -ne $tree -or $build.target -ne $targets[$Platform] -or $build.version -ne $version -or $build.binary_sha256 -ne $binaryHash -or $build.transformation -ne 'unsigned-build') { throw 'Build provenance does not match package inputs.' }
    $metadata = @{version=$version; source_sha=$SourceSha; source_tree=$tree; binary_sha256=$binaryHash; target=$targets[$Platform]; platform=$Platform; transformation='unsigned-build'}
    $metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'release.json') -Encoding utf8NoBOM
    if (-not $windows) {
        & chmod 700 (Join-Path $destination "bin/$binaryName") (Join-Path $destination 'run.sh')
        if ($LASTEXITCODE -ne 0) { throw 'Cannot preserve executable permissions.' }
    }
    $extension = if ($windows) { 'zip' } else { 'tar.gz' }
    $archive = Join-Path $pending "upwork-agent-plugin-$version-$Platform.$extension"
    if (Test-Path -LiteralPath $archive) { throw 'Archive already exists; refusing to overwrite.' }
    if ($windows) {
        [IO.Compression.ZipFile]::CreateFromDirectory($destination, $archive)
        [IO.Compression.ZipFile]::ExtractToDirectory($archive, $verify)
    } else {
        & tar -czf $archive -C $destination .
        if ($LASTEXITCODE -ne 0) { throw 'Archive creation failed.' }
        New-Item -ItemType Directory -Path $verify | Out-Null
        & tar -xzf $archive -C $verify
        if ($LASTEXITCODE -ne 0) { throw 'Archive extraction failed.' }
    }
    $actual = @(Get-ChildItem -LiteralPath $verify -Recurse -Force -File | ForEach-Object { [IO.Path]::GetRelativePath($verify, $_.FullName).Replace('\', '/') })
    if (Compare-Object ($allowed + @('release.json') | Sort-Object) ($actual | Sort-Object)) { throw 'Extracted archive differs from the release allowlist.' }
    $binary = Join-Path $verify "bin/$binaryName"
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -cne $binaryHash) { throw 'Extracted executable digest differs from verified build provenance.' }
    $extractedMetadata = Get-Content -LiteralPath (Join-Path $verify 'release.json') -Raw -Encoding utf8 | ConvertFrom-Json
    foreach ($key in $metadata.Keys) {
        if ($extractedMetadata.$key -cne $metadata[$key]) { throw 'Extracted release metadata differs from verified package inputs.' }
    }
    $entry = (Smoke $binary @('desktop-config') 0 | ConvertFrom-Json).mcpServers.'Upwork-Agent-mcp'
    if ($entry.args.Count -ne 1 -or $entry.args[0] -ne 'bridge' -or $entry.command.Replace('\\?\', '') -ne $binary) { throw 'Packaged host configuration requires a launcher or wrong executable.' }
    foreach ($key in @('TEMP', 'TMP', 'TMPDIR')) {
        if ($entry.env.$key.Replace('\\?\', '') -ne (Join-Path $verify '.cache/tmp')) { throw 'Packaged host temporary path escaped the installation.' }
    }
    if ($entry.env.XDG_CACHE_HOME.Replace('\\?\', '') -ne (Join-Path $verify '.cache') -or -not (Test-Path -LiteralPath (Join-Path $verify '.cache/tmp') -PathType Container)) { throw 'Packaged cache root mismatch.' }
    if ((Smoke $binary @('--version') 0).Trim() -ne "upwork-connector $version") { throw 'Packaged version mismatch.' }
    $status = Smoke $binary @('status') 0 | ConvertFrom-Json
    if ($status.mode -ne 'blocked' -or $null -ne $status.selected -or $status.profiles.Count -ne 0) { throw 'Archive does not start with clean blocked state.' }
    if ((Smoke $binary @('bridge') 1).Length -ne 0) { throw 'Blocked bridge polluted protocol stdout.' }
    $settings = Smoke $binary @('local-settings') 0 | ConvertFrom-Json
    if ($settings.url -ne 'socks5h://127.0.0.1:1595') { throw 'Packaged local proxy default mismatch.' }
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding utf8NoBOM
    # Multi-file promotion is not crash-atomic. A failed live invocation rolls back only its own moves.
    $products = @(Get-ChildItem -LiteralPath $pending -File)
    foreach ($product in $products) {
        if (Test-Path -LiteralPath (Join-Path $output $product.Name)) { throw 'Package output already exists; refusing to overwrite.' }
    }
    foreach ($product in $products) {
        $final = Join-Path $output $product.Name
        [IO.File]::Move($product.FullName, $final)
        $promoted.Add($final)
    }
    Event INFO 'Archive allowlist, version, blocked mode, settings and checksum verified.'
    Write-Host "Packaged and verified $([IO.Path]::GetFileName($archive))."
} catch {
    foreach ($owned in $promoted) { if (Test-Path -LiteralPath $owned) { Remove-Item -LiteralPath $owned -Force } }
    Event ERROR 'Packaging or archive smoke failed.'
    throw
} finally {
    foreach ($path in @($destination, $verify, $pending)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force } }
    Event INFO 'Packaging scratch cleaned.'
}
