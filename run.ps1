param([switch]$Status, [switch]$Preview)
$ErrorActionPreference = 'Stop'
$script:InputLines = $null
$script:InputIndex = 0
if ([Console]::IsInputRedirected -and -not $Status -and -not $Preview) {
    $script:InputLines = [Console]::In.ReadToEnd() -split '\r?\n'
}
if ($PSVersionTable.PSVersion.Major -lt 7) {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if (-not $pwsh) { Write-Error 'PowerShell 7 is required. Install it only in an approved location.'; exit 1 }
    & $pwsh.Source -NoProfile -File $PSCommandPath @PSBoundParameters
    exit $LASTEXITCODE
}
$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) { Write-Error 'Node.js 24.13.1 or newer is required.'; exit 1 }
$root = $PSScriptRoot
$env:NODE_OPTIONS = ''
$env:NODE_DEBUG = ''
$env:NODE_TLS_REJECT_UNAUTHORIZED = '1'
$env:UPWORK_LOCAL_SIGN_IN = '0'
$version = & $node.Source --version
if ([version]($version.TrimStart('v')) -lt [version]'24.13.1') { Write-Error 'Node.js 24.13.1 or newer is required.'; exit 1 }
$manage = Join-Path $root 'src/manage.ts'
$signIn = Join-Path $root 'src/sign-in.ts'
$env:NODE_COMPILE_CACHE = Join-Path $root '.cache/compile'
$env:TEMP = $env:TMP = Join-Path $root '.cache/tmp'
New-Item -ItemType Directory -Force -Path $env:TEMP | Out-Null
if (-not (Test-Path (Join-Path $root 'node_modules/mcp-remote/package.json'))) {
    Write-Error 'Local dependencies are missing. Install from package-lock.json with a project-local npm cache.'
    exit 1
}
if ($Status) { & $node.Source $manage status; exit $LASTEXITCODE }
if (-not $env:NO_COLOR -and -not [Console]::IsOutputRedirected) {
    Write-Host "`e[38;2;245;248;255m" -NoNewline
}

function Write-Accent([string]$Text, [string]$Color = '117', [switch]$Inline) {
    $palette = @{
        '117' = '155;225;255'; '75' = '155;205;255'; '92' = '160;255;185'
        '120' = '160;255;185'; '221' = '255;230;160'; '227' = '255;240;165'; '91' = '255;150;160'
    }
    if ($env:NO_COLOR -or [Console]::IsOutputRedirected) { $value = $Text }
    else { $value = "`e[1;38;2;$($palette[$Color])m$Text`e[0m" }
    Write-Host $value -NoNewline:$Inline
    if (-not $env:NO_COLOR -and -not [Console]::IsOutputRedirected) { Write-Host "`e[38;2;245;248;255m" -NoNewline }
}
function Read-Answer([string]$Title, [string]$Hint = 'back=0, quit=exit', [string]$Default = '', [switch]$Secret) {
    Write-Host ''
    Write-Host $Title -NoNewline
    if ($Default) { Write-Accent " [$Default]" '92' -Inline }
    if ($Hint) { Write-Accent " {$Hint}" '221' -Inline }
    Write-Host ': ' -NoNewline
    if ([Console]::IsInputRedirected) {
        if ($script:InputIndex -ge $script:InputLines.Count) { exit 0 }
        $value = $script:InputLines[$script:InputIndex++]
    } elseif ($Secret) { $value = Read-Host -MaskInput }
    else { $value = Read-Host }
    $value = $value.Trim()
    if ($value -match '^(?i:exit)$') { exit 0 }
    if (-not $value) { return $Default }
    return $value
}
function Get-State {
    $json = & $node.Source $manage status
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read connection configuration.' }
    return ($json | ConvertFrom-Json)
}
function Show-Menu {
    $state = Get-State
    $width = 80
    if (-not [Console]::IsOutputRedirected) { try { $width = [Math]::Max(40, [Console]::WindowWidth) } catch {} }
    $title = 'Upwork Local Connector'
    Write-Host ''
    Write-Accent ((' ' * [Math]::Max(0, [int](($width - $title.Length) / 2))) + $title) '75'
    Write-Accent ('=' * $width) '75'
    Write-Host "  Connection: " -NoNewline
    Write-Accent $state.mode $(if ($state.mode -eq 'blocked') { '227' } else { '120' }) -Inline
    if ($state.selected) {
        $active = $state.profiles | Where-Object name -eq $state.selected
        Write-Host "  |  Proxy: $($active.label)" -NoNewline
    }
    Write-Host "  |  Saved proxies: $(@($state.profiles).Count)"
    Write-Host ''
    Write-Accent 'Upwork Main menu:'
    $items = @('Select proxy', 'Import proxy list from file', 'Add one proxy', 'Direct connection (no proxy)',
        'Block connections', 'Sign in to Upwork', 'Register MCP in Claude')
    for ($i = 0; $i -lt $items.Count; $i++) {
        Write-Accent "  $($i + 1)." -Inline
        Write-Host " $($items[$i])" -NoNewline
        if ($i -eq 0) { Write-Accent ' [1]' '92' -Inline }
        Write-Host ''
    }
}
if ($Preview) { Show-Menu; exit 0 }
while ($true) {
    try {
        Show-Menu
        $choice = Read-Answer 'Selection' 'quit=exit' '1'
        switch ($choice) {
            '1' {
                $state = Get-State
                $profiles = @($state.profiles)
                if (-not $profiles.Count) { Write-Accent 'No saved proxies. Import a list with option 2.' '227'; break }
                Write-Accent "`nSaved proxies:"
                for ($i = 0; $i -lt $profiles.Count; $i++) {
                    Write-Accent "  $($i + 1)." -Inline
                    Write-Host " $($profiles[$i].label)  ($($profiles[$i].url))"
                }
                $selection = Read-Answer 'Proxy number'
                if ($selection -eq '0') { break }
                $number = 0
                if (-not [int]::TryParse($selection, [ref]$number) -or $number -lt 1 -or $number -gt $profiles.Count) {
                    Write-Accent 'Enter a listed proxy number.' '91'; break
                }
                & $node.Source $manage select $profiles[$number - 1].name
            }
            '2' {
                $path = Read-Answer 'Proxy list file path' 'back=0, quit=exit' (Join-Path $root 'Proxy list.txt')
                if ($path -eq '0') { break }
                & $node.Source $manage import $path
            }
            '3' {
                $name = Read-Answer 'Profile name (letters/digits/_/-)'
                if ($name -eq '0') { break }
                $url = Read-Answer 'Proxy URL (http/https/socks5)' -Secret
                if ($url -eq '0') { break }
                @{ name = $name; url = $url } | ConvertTo-Json -Compress | & $node.Source $manage add
                $url = $null
            }
            '4' {
                $answer = Read-Answer 'Use direct connection without proxy?' 'back=0, quit=exit' 'Y'
                if ($answer -match '^(?i:y|yes)$') { & $node.Source $manage direct }
            }
            '5' { & $node.Source $manage block }
            '6' {
                Write-Accent 'A complete link will appear. Copy it into your browser on this computer.' '227'
                Write-Host 'Keep this terminal open. The helper confirms when the session is saved.'
                & $node.Source $signIn
            }
            '7' { & $node.Source $manage register }
            default { Write-Accent 'Enter a menu number from 1 to 7, or exit.' '91' }
        }
        if ($LASTEXITCODE) { Write-Accent "Operation exit code: $LASTEXITCODE" '91' }
    } catch { Write-Accent 'Menu operation failed. Check local configuration and permissions.' '91' }
}
