$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Signed-stage policy requires Windows.' }
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root '.ci-work'
$sandbox = Join-Path $work ('signed-stage-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($sandbox) | Out-Null
$pwsh = (Get-Command pwsh -ErrorAction Stop).Source
# Mock only the OS certificate/signing boundary; run the actual stage validator and metadata write.
$fixture = @'
param([string]$Script, [string]$Stage, [string]$Case)
$ErrorActionPreference = 'Stop'
$binary = Join-Path $Stage 'bin/upwork-connector.exe'
$before = [IO.File]::ReadAllBytes($binary)
$metadataPath = Join-Path $Stage 'release.json'
$original = [IO.File]::ReadAllBytes($metadataPath)
$global:fixtureSigned = $false
$thumbprint = 'A' * 40
$certificateBytes = [Text.Encoding]::UTF8.GetBytes('synthetic certificate; not a real signing identity')
$fingerprint = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($certificateBytes)).ToLowerInvariant()
function Get-Item {
    param([string]$LiteralPath)
    if ($LiteralPath.StartsWith('Cert:')) {
        return [pscustomobject]@{ RawData=$certificateBytes; HasPrivateKey=$true; Thumbprint=$thumbprint;
            NotBefore=[DateTime]::UtcNow.AddDays(-1); NotAfter=[DateTime]::UtcNow.AddDays(1) }
    }
    Microsoft.PowerShell.Management\Get-Item -LiteralPath $LiteralPath
}
function Get-AuthenticodeSignature {
    param([string]$LiteralPath)
    if (-not $global:fixtureSigned) { return [pscustomobject]@{Status='NotSigned'} }
    return [pscustomobject]@{ Status=$(if ($Case -eq 'signature_failure') {'HashMismatch'} elseif ($Case -eq 'unknown_failure') {'UnknownError'} else {'Valid'});
        StatusMessage='synthetic fixture'; SignerCertificate=[pscustomobject]@{Thumbprint=$(if ($Case -eq 'wrong_signer') {'B' * 40} else {$thumbprint})} }
}
function Set-AuthenticodeSignature {
    param([string]$LiteralPath, [object]$Certificate, [string]$HashAlgorithm, [string]$IncludeChain)
    if ($HashAlgorithm -ne 'SHA256' -or $IncludeChain -ne 'All') { throw 'Signing arguments changed.' }
    [IO.File]::AppendAllText($LiteralPath, '-synthetic-signature', [Text.UTF8Encoding]::new($false))
    $global:fixtureSigned = $true
    Get-AuthenticodeSignature -LiteralPath $LiteralPath
}
$digest = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Case -eq 'wrong_digest') { $digest = '0' * 64 }
if ($Case -eq 'wrong_certificate') { $fingerprint = '0' * 64 }
$failed = $false
try { & $Script -Stage $Stage -ExpectedUnsignedSha256 $digest -CertificateThumbprint $thumbprint -CertificateSha256 $fingerprint }
catch { $failed = $true }
if ($failed -ne ($Case -ne 'success')) { throw 'Unexpected signing policy result.' }
if ($global:fixtureSigned -ne ($Case -notin @('wrong_digest', 'wrong_certificate'))) { throw 'Signing rejection occurred at an unrelated stage.' }
$metadata = Get-Content -LiteralPath $metadataPath -Raw -Encoding utf8 | ConvertFrom-Json
if ($Case -eq 'success') {
    $final = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($metadata.transformation -ne 'windows-authenticode-sha256' -or $metadata.binary_sha256 -cne $final -or
        $metadata.unsigned_binary_sha256 -cne $digest -or $metadata.signer_certificate_sha256 -cne $fingerprint -or
        $metadata.source_sha -ne ('1' * 40) -or $metadata.source_tree -ne ('2' * 40)) { throw 'Signed metadata chain mismatch.' }
} else {
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($metadataPath)) -cne [Convert]::ToBase64String($original)) { throw 'Failed signing changed metadata.' }
    if ($Case -in @('wrong_digest', 'wrong_certificate') -and [Convert]::ToBase64String([IO.File]::ReadAllBytes($binary)) -cne [Convert]::ToBase64String($before)) { throw 'Preflight failure changed executable.' }
}
'@
$rows = @()
try {
    $helper = Join-Path $sandbox 'fixture.ps1'
    [IO.File]::WriteAllText($helper, $fixture, [Text.UTF8Encoding]::new($false))
    foreach ($case in @('wrong_digest', 'wrong_certificate', 'signature_failure', 'unknown_failure', 'wrong_signer', 'success')) {
        $stage = Join-Path $sandbox $case
        [IO.Directory]::CreateDirectory((Join-Path $stage 'bin')) | Out-Null
        $binary = Join-Path $stage 'bin/upwork-connector.exe'
        [IO.File]::WriteAllText($binary, 'unsigned-fixture', [Text.UTF8Encoding]::new($false))
        @{ platform='windows-x64'; target='x86_64-pc-windows-msvc'; version='1.0.0'; source_sha=('1'*40); source_tree=('2'*40);
            transformation='unsigned-build'; binary_sha256=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage 'release.json') -Encoding utf8NoBOM
        $info = [Diagnostics.ProcessStartInfo]::new($pwsh)
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true; $info.RedirectStandardInput=$true
        foreach ($arg in @('-NoProfile', '-NonInteractive', '-File', $helper, '-Script', (Join-Path $PSScriptRoot 'sign-package-stage.ps1'), '-Stage', $stage, '-Case', $case)) { $info.ArgumentList.Add($arg) }
        $process = [Diagnostics.Process]::new(); $process.StartInfo=$info
        $started=$false
        try {
            $started=$process.Start()
            if (-not $started) { throw 'Cannot start signed-stage fixture.' }
            $process.StandardInput.Close()
            $out=$process.StandardOutput.ReadToEndAsync(); $err=$process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(10000)) { throw 'Signed-stage fixture exceeded its deadline.' }
            $null=$out.GetAwaiter().GetResult(); $null=$err.GetAwaiter().GetResult()
            if ($process.ExitCode -ne 0) { throw "Signed-stage policy failed: $case" }
            $rows += @{case=$case; passed=$true; reaped=$true; cryptographic_signing=$false}
        } finally {
            try {
                if ($started -and -not $process.HasExited) { $process.Kill($true); if (-not $process.WaitForExit(5000)) { throw 'Signed-stage fixture could not be reaped.' } }
            } finally { $process.Dispose() }
        }
    }
    $rows | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $work 'signed-stage-policy.json') -Encoding utf8NoBOM
    Write-Host 'Signed-stage metadata and rejection policy passed; no real certificate used or cryptographic proof claimed.'
} finally { Remove-Item -LiteralPath $sandbox -Recurse -Force }
