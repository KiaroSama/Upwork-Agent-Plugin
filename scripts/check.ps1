$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:CARGO_HOME = Join-Path $root '.cargo-cache'
$env:CARGO_TARGET_DIR = Join-Path $root 'target'
$env:TEMP = $env:TMP = $env:TMPDIR = Join-Path $root '.ci-work'
New-Item -ItemType Directory -Force -Path $env:CARGO_HOME, $env:TEMP | Out-Null
function Get-SourceLineCount([string]$Text) {
    if ($Text.Length -eq 0) { return 0 }
    return ([regex]::Matches($Text, "`n")).Count + [int](-not $Text.EndsWith("`n"))
}
foreach ($count in @(799, 800, 801)) {
    foreach ($newline in @("`n", "`r`n")) {
        foreach ($trailing in @($true, $false)) {
            $sample = ('x' + $newline) * $count
            if (-not $trailing) { $sample = $sample.Substring(0, $sample.Length - $newline.Length) }
            if ((Get-SourceLineCount $sample) -ne $count -or (Get-SourceLineCount ([char]0xfeff + $sample)) -ne $count) { throw 'Source line-count boundary regression.' }
        }
    }
}
Push-Location $root
try {
    $utf8 = [Text.UTF8Encoding]::new($false, $true)
    $files = @(& git ls-files)
    if (-not $files.Count) { throw 'No tracked publication files.' }
    foreach ($file in $files) {
        $bytes = [IO.File]::ReadAllBytes((Join-Path $root $file))
        $text = $utf8.GetString($bytes)
        if ($text.Contains([char]0)) { throw "Unexpected binary source file: $file" }
        $lines = Get-SourceLineCount $text
        if ([IO.Path]::GetExtension($file) -in @('.rs', '.ps1', '.py', '.sh') -and $lines -gt 800) { throw "Source file exceeds 800 lines: $file" }
        if ($file.EndsWith('.ps1')) {
            $tokens = $null; $errors = $null
            [Management.Automation.Language.Parser]::ParseFile((Join-Path $root $file), [ref]$tokens, [ref]$errors) | Out-Null
            if ($errors.Count) { throw "Invalid PowerShell syntax: $file" }
        }
    }
    $tests = @(Get-ChildItem -LiteralPath (Join-Path $root 'tests') -Filter '*.rs' -File)
    if ($tests.Count -lt 3) { throw 'Rust regression suites missing.' }
    $metadata = & cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $targets = @($metadata.packages[0].targets | Where-Object { $_.kind -contains 'test' })
    $workflow = [IO.File]::ReadAllText((Join-Path $root '.github/workflows/ci.yml'), $utf8)
    foreach ($policy in @('check-unix-build', 'check-permission-root', 'check-package-policy', 'check-verification-receipts', 'check-signed-stage')) {
        if (-not $workflow.Contains("./scripts/$policy.ps1")) { throw "Policy fixture omitted from CI: $policy" }
    }
    foreach ($test in $tests) {
        if ($test.FullName -notin $targets.src_path) { throw "Test suite omitted from Cargo: $($test.Name)" }
    }
    Write-Host "Validated UTF-8, source size, PowerShell syntax and $($tests.Count) Cargo test suites."
} finally { Pop-Location }
