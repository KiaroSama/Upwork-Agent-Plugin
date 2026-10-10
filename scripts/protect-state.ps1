param([Parameter(Mandatory)][string]$ProjectRoot)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $ProjectRoot).Path
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
foreach ($name in @('config', '.state', 'logs')) {
    $path = Join-Path $root $name
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    $acl = Get-Acl $path
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.Access)) { $acl.RemoveAccessRuleSpecific($rule) }
    foreach ($sid in @($user, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    Set-Acl -Path $path -AclObject $acl
}
