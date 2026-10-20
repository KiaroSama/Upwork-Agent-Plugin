param(
    [Parameter(Mandatory)][string]$Stage,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedUnsignedSha256,
    [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{40}$')][string]$CertificateThumbprint,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$CertificateSha256
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Authenticode stage signing requires Windows.' }
$work = Join-Path (Split-Path -Parent $PSScriptRoot) '.ci-work'
[IO.Directory]::CreateDirectory($work) | Out-Null
$log = Join-Path $work ('sign-stage_' + [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss') + '_UTC_' + [Guid]::NewGuid().ToString('N') + '.log')
function Event([string]$level, [string]$message) {
    $line = '[{0} UTC] [{1}] [sign-stage] {2}' -f [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss'), $level, $message
    try { [IO.File]::AppendAllText($log, "$line`n", [Text.UTF8Encoding]::new($false)) }
    catch { [Console]::Error.WriteLine('Signing log unavailable.'); [Console]::Error.WriteLine($line) }
}
try {
Event INFO 'Unsigned stage validation started.'
$directory = Get-Item -LiteralPath $Stage
if (-not $directory.PSIsContainer -or $directory.LinkType) { throw 'Signing stage must be an ordinary directory.' }
$root = $directory.FullName
$relative = [IO.Path]::GetRelativePath([IO.Path]::GetFullPath($work), $root)
if ($relative -eq '.' -or $relative -eq '..' -or $relative.StartsWith('..' + [IO.Path]::DirectorySeparatorChar) -or [IO.Path]::IsPathRooted($relative)) {
    throw 'Signing requires a disposable extracted stage inside this project development work directory.'
}
$ancestor = $directory
while ($null -ne $ancestor) {
    if ($ancestor.LinkType) { throw 'Signing refuses linked stage ancestors.' }
    $ancestor = $ancestor.Parent
}
$binary = Join-Path $root 'bin/upwork-connector.exe'
$metadataPath = Join-Path $root 'release.json'
foreach ($path in @((Join-Path $root 'bin'), $binary, $metadataPath)) {
    if ((Get-Item -LiteralPath $path).LinkType) { throw 'Signing refuses linked package inputs.' }
}
$metadata = Get-Content -LiteralPath $metadataPath -Raw -Encoding utf8 | ConvertFrom-Json
if ($metadata.platform -ne 'windows-x64' -or $metadata.target -ne 'x86_64-pc-windows-msvc' -or
    $metadata.transformation -ne 'unsigned-build' -or $metadata.binary_sha256 -cne $ExpectedUnsignedSha256 -or
    $metadata.source_sha -notmatch '^[a-f0-9]{40}$' -or $metadata.source_tree -notmatch '^[a-f0-9]{40}$' -or
    $metadata.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Signing requires validated unsigned Windows package metadata.' }
if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedUnsignedSha256) {
    throw 'Unsigned executable does not match the reviewed package digest.'
}
if ((Get-AuthenticodeSignature -LiteralPath $binary).Status -ne 'NotSigned') { throw 'Signing refuses an already transformed executable.' }
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
$fingerprint = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($certificate.RawData)).ToLowerInvariant()
$now = [DateTime]::UtcNow
if (-not $certificate.HasPrivateKey -or $certificate.Thumbprint -ine $CertificateThumbprint -or
    $fingerprint -cne $CertificateSha256 -or $certificate.NotBefore.ToUniversalTime() -gt $now -or
    $certificate.NotAfter.ToUniversalTime() -le $now) { throw 'Approved signing identity unavailable, mismatched or expired.' }
# This is an extracted disposable stage, never the installed executable or final archive.
$signature = Set-AuthenticodeSignature -LiteralPath $binary -Certificate $certificate -HashAlgorithm SHA256 -IncludeChain All
$verified = Get-AuthenticodeSignature -LiteralPath $binary
foreach ($result in @($signature, $verified)) {
    if ($null -eq $result.SignerCertificate -or $result.SignerCertificate.Thumbprint -ine $CertificateThumbprint) {
        throw 'Signed executable does not match the approved signer.'
    }
    # Retain the existing explicit self-signed trust exception, not arbitrary UnknownError.
    if ($result.Status -ne 'Valid' -and -not ($result.Status -eq 'UnknownError' -and
        $result.StatusMessage -like '*terminated in a root certificate which is not trusted*')) {
        throw 'Native signature verification failed; package publication is blocked.'
    }
}
$metadata.transformation = 'windows-authenticode-sha256'
$metadata | Add-Member -NotePropertyName unsigned_binary_sha256 -NotePropertyValue $ExpectedUnsignedSha256
$metadata.binary_sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
$metadata | Add-Member -NotePropertyName signer_certificate_sha256 -NotePropertyValue $fingerprint
$metadata | Add-Member -NotePropertyName signature_trust -NotePropertyValue $(if ($verified.Status -eq 'Valid') { 'platform-valid' } else { 'self-signed-untrusted-root' })
$pending = Join-Path $root ('.release-' + [Guid]::NewGuid().ToString('N') + '.json')
try {
    $metadata | ConvertTo-Json | Set-Content -LiteralPath $pending -Encoding utf8NoBOM
    [IO.File]::Move($pending, $metadataPath, $true)
} finally {
    if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending -Force }
}
Event INFO 'Signed stage metadata committed; final archive verification remains required.'
Write-Host 'Staged native signature verified; unsigned and final executable digests retained. Repackage and verify before publication.'
} catch {
    Event ERROR 'Stage signing failed; preserve the unsigned archive and rebuild this disposable stage before retrying.'
    throw 'Stage signing failed; publication blocked. Preserve the unsigned archive and inspect safe lifecycle diagnostics.'
}
