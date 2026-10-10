param([Parameter(Mandatory)][string]$ProjectRoot)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $ProjectRoot).Path
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
foreach ($name in @('config', '.state', 'logs', '.cache')) {
    $path = Join-Path $root $name
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    $acl = [System.Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($user, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    [System.IO.FileSystemAclExtensions]::SetAccessControl([System.IO.DirectoryInfo]::new($path), $acl)
}
