param(
    [string]$Binary,
    [switch]$Capture,
    [object]$Expected
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $checkout = @(& git rev-parse --show-toplevel)
    if ($LASTEXITCODE -ne 0 -or $checkout.Count -ne 1 -or [IO.Path]::GetFullPath($checkout[0].Trim()) -ne [IO.Path]::GetFullPath($root)) {
        throw 'Build provenance requires this project root to be the Git checkout root.'
    }
    $sourceOutput = @(& git rev-parse HEAD)
    if ($LASTEXITCODE -ne 0 -or $sourceOutput.Count -ne 1) { throw 'Build provenance requires a Git checkout.' }
    $source = $sourceOutput[0].Trim()
    $treeOutput = @(& git rev-parse 'HEAD^{tree}')
    if ($LASTEXITCODE -ne 0 -or $treeOutput.Count -ne 1) { throw 'Cannot resolve source tree.' }
    $tree = $treeOutput[0].Trim()
    & git diff --quiet HEAD --
    if ($LASTEXITCODE -ne 0) { throw 'Build provenance requires unchanged tracked inputs.' }
    $untracked = @(& git ls-files --others --exclude-standard)
    if ($LASTEXITCODE -ne 0 -or $untracked.Count) { throw 'Build provenance refuses untracked, non-ignored inputs.' }
    $compiler = @(& rustc -vV)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve compiler identity.' }
    $hostLine = @($compiler | Where-Object { $_ -like 'host: *' })
    if ($hostLine.Count -ne 1) { throw 'Cannot resolve compiler target.' }
    $version = (Select-String -LiteralPath (Join-Path $root 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$').Matches[0].Groups[1].Value
    $identity = [ordered]@{ source_sha = $source; source_tree = $tree; target = $hostLine[0].Substring(6); version = $version; compiler = ($compiler -join "`n") }
    if ($Capture) { return [pscustomobject]$identity }
    if (-not $Binary -or $null -eq $Expected) { throw 'Build receipt requires the input identity captured before compilation.' }
    foreach ($key in $identity.Keys) {
        if ($Expected.$key -cne $identity[$key]) { throw 'Build inputs changed during compilation; provenance refused.' }
    }
    $identity['binary_sha256'] = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
    $identity['transformation'] = 'unsigned-build'
    $work = Join-Path $root '.ci-work'
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    $identity | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'build-provenance.json') -Encoding utf8NoBOM
} finally { Pop-Location }
