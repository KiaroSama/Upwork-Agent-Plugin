$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Permission-root fixture requires Windows ACL facilities.' }
$root = Split-Path -Parent $PSScriptRoot
$owned = Join-Path $root ('.ci-work/permission-root-' + [Guid]::NewGuid().ToString('N'))
$literal = Join-Path $owned 'install [abc] space'
$distractor = Join-Path $owned 'install a space'
[IO.Directory]::CreateDirectory($literal) | Out-Null
[IO.Directory]::CreateDirectory($distractor) | Out-Null
$sentinel = Join-Path $distractor 'keep'
[IO.File]::WriteAllText($sentinel, 'preserve', [Text.UTF8Encoding]::new($false))
$before = (Get-Acl -LiteralPath $distractor).Sddl
try {
    foreach ($fixture in @($literal, (Join-Path $owned 'ordinary install space'))) {
        [IO.Directory]::CreateDirectory($fixture) | Out-Null
        & (Join-Path $PSScriptRoot 'protect-state.ps1') -ProjectRoot $fixture
        foreach ($name in @('config', '.state', 'logs', '.cache')) {
            $path = Join-Path $fixture $name
            if (-not [IO.Directory]::Exists($path)) { throw 'Literal permission destination missing.' }
            $acl = Get-Acl -LiteralPath $path
            if (-not $acl.AreAccessRulesProtected) { throw 'Permission inheritance was not restricted.' }
            $rules = @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]))
            $owner = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
            if ($rules.Count -ne 2 -or @($rules | Where-Object { $_.IdentityReference.Value -notin @($owner, 'S-1-5-18') }).Count) { throw 'Unexpected permission principals.' }
        }
    }
    if ((Get-Acl -LiteralPath $distractor).Sddl -ne $before -or [IO.File]::ReadAllText($sentinel, [Text.Encoding]::UTF8) -ne 'preserve') { throw 'Distractor changed.' }
    if (@(Get-ChildItem -LiteralPath $distractor -Force).Count -ne 1) { throw 'Wildcard resolution touched distractor.' }
    @{ passed=$true; bracket_root=$true; ordinary_space_root=$true; distractor_unchanged=$true } | ConvertTo-Json |
        Set-Content -LiteralPath (Join-Path $root '.ci-work/permission-root.json') -Encoding utf8NoBOM
    Write-Host 'Literal bracket-root ACL and distractor preservation passed.'
} finally { Remove-Item -LiteralPath $owned -Recurse -Force }
