param([string]$Gate, [string]$CommandFile)
$ErrorActionPreference = 'Stop'
$deadline = [Diagnostics.Stopwatch]::StartNew()
while (-not [IO.File]::Exists($Gate)) {
    if ($deadline.Elapsed.TotalSeconds -ge 10) { throw 'Verification ownership gate timed out.' }
    Start-Sleep -Milliseconds 10
}
$command = @([IO.File]::ReadAllText($CommandFile, [Text.Encoding]::UTF8) | ConvertFrom-Json)
& cargo @command
exit $LASTEXITCODE
