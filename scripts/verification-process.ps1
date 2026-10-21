# The startup gate keeps Cargo behind established OS lifetime ownership.
function Initialize-VerificationJobType {
    if ('UpworkVerification.Job' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace UpworkVerification {
    public sealed class Job : IDisposable {
        [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
            public long ProcessTime, JobTime;
            public uint Flags;
            public UIntPtr MinWorking, MaxWorking;
            public uint ActiveLimit;
            public UIntPtr Affinity;
            public uint Priority, Scheduling;
        }
        [StructLayout(LayoutKind.Sequential)] struct IoCounters {
            public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes;
        }
        [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
            public BasicLimits Basic;
            public IoCounters Io;
            public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
        }
        [StructLayout(LayoutKind.Sequential)] struct Accounting {
            public long User, Kernel, PeriodUser, PeriodKernel;
            public uint Faults, Total, Active, Terminated;
        }
        [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimits value, uint size);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting value, uint size, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool QueryInformationJobObject(IntPtr job, int kind, IntPtr value, uint size, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool IsProcessInJob(IntPtr process, IntPtr job, out bool member);
        [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateJobObject(IntPtr job, uint code);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        IntPtr handle;
        static void Check(bool success) { if (!success) throw new Win32Exception(Marshal.GetLastWin32Error()); }
        public Job() {
            handle = CreateJobObject(IntPtr.Zero, null);
            if (handle == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                var limits = new ExtendedLimits();
                limits.Basic.Flags = 0x2000;
                Check(SetInformationJobObject(handle, 9, ref limits, (uint)Marshal.SizeOf<ExtendedLimits>()));
            } catch { Dispose(); throw; }
        }
        public void Assign(IntPtr process) { Check(AssignProcessToJobObject(handle, process)); }
        public bool Contains(IntPtr process) { bool member; Check(IsProcessInJob(process, handle, out member)); return member; }
        public uint Active {
            get {
                Accounting value;
                Check(QueryInformationJobObject(handle, 1, out value, (uint)Marshal.SizeOf<Accounting>(), IntPtr.Zero));
                return value.Active;
            }
        }
        public uint[] Members() {
            for (int capacity = 32; capacity <= 4096; capacity *= 2) {
                int size = checked(8 + capacity * IntPtr.Size);
                IntPtr buffer = Marshal.AllocHGlobal(size);
                try {
                    if (!QueryInformationJobObject(handle, 3, buffer, (uint)size, IntPtr.Zero)) {
                        int error = Marshal.GetLastWin32Error();
                        if (error == 234) continue;
                        throw new Win32Exception(error);
                    }
                    int assigned = Marshal.ReadInt32(buffer, 0), count = Marshal.ReadInt32(buffer, 4);
                    if (count < 0 || count > capacity || count > assigned) throw new InvalidOperationException("Invalid Job membership.");
                    if (count < assigned) continue;
                    var result = new uint[count];
                    for (int i = 0; i < count; i++) result[i] = checked((uint)Marshal.ReadIntPtr(buffer, 8 + i * IntPtr.Size).ToInt64());
                    return result;
                } finally { Marshal.FreeHGlobal(buffer); }
            }
            throw new InvalidOperationException("Job membership exceeded the bounded query.");
        }
        public void Terminate() { Check(TerminateJobObject(handle, 1)); }
        public void Dispose() { if (handle != IntPtr.Zero) { CloseHandle(handle); handle = IntPtr.Zero; } }
    }
}
'@
}

function New-VerificationOwner([string]$Directory, [string[]]$Arguments) {
    $gate = Join-Path $Directory 'start'
    $command = Join-Path $Directory 'command.json'
    [IO.File]::WriteAllText($command, (ConvertTo-Json -InputObject @($Arguments) -Compress), [Text.UTF8Encoding]::new($false))
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.WorkingDirectory = Split-Path -Parent $PSScriptRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $pwsh = Join-Path $PSHOME $(if ($IsWindows) { 'pwsh.exe' } else { 'pwsh' })
    $driver = @($pwsh, '-NoProfile', '-NonInteractive', '-File', (Join-Path $PSScriptRoot 'verification-command.ps1'), '-Gate', $gate, '-CommandFile', $command)
    $job = $null
    $groupFile = Join-Path $Directory 'group'
    if ($IsWindows) {
        Initialize-VerificationJobType
        $job = [UpworkVerification.Job]::new()
        $info.FileName = $driver[0]
        foreach ($argument in $driver[1..($driver.Count - 1)]) { $info.ArgumentList.Add($argument) }
    } else {
        $info.FileName = (Get-Command bash -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        # Monitor mode gives the asynchronous driver its own group. No pipelines.
        $launch = 'set -m; case $- in *m*) ;; *) exit 1;; esac; marker=$1; shift; "$@" & child=$!; printf "%s\n" "$child" > "$marker.pending"; mv -- "$marker.pending" "$marker"; wait "$child"; exit $?'
        foreach ($argument in @('--noprofile', '--norc', '-c', $launch, 'verification', $groupFile) + $driver) { $info.ArgumentList.Add($argument) }
    }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $compilerCommand = $Arguments.Count -gt 0 -and $Arguments[0] -in @('clippy', 'build', 'nextest')
    @{ Process = $process; Job = $job; Group = $null; GroupFile = $groupFile; Gate = $gate; Directory = $Directory; Started = $false; Ps = $null; Kill = $null; CompilerCommand = $compilerCommand; CompilerAuxiliaries = 0 }
}

function Start-VerificationOwner($Owner) {
    $process = $Owner.Process
    if (-not $process.Start()) { throw 'Cannot start verification driver.' }
    $Owner.Started = $true
    $process.StandardInput.Close()
    if ($IsWindows) {
        $Owner.Job.Assign($process.Handle)
    } else {
        $Owner.Ps = (Get-Command ps -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        $Owner.Kill = (Get-Command kill -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        $watch = [Diagnostics.Stopwatch]::StartNew()
        while (-not [IO.File]::Exists($Owner.GroupFile)) {
            if ($process.HasExited -or $watch.Elapsed.TotalSeconds -gt 5) { throw 'Cannot establish verification process group.' }
            Start-Sleep -Milliseconds 10
        }
        $group = [int][IO.File]::ReadAllText($Owner.GroupFile, [Text.Encoding]::UTF8).Trim()
        $actual = (& $Owner.Ps -o pgid= -p $group).Trim()
        $ours = (& $Owner.Ps -o pgid= -p $PID).Trim()
        if ($LASTEXITCODE -ne 0 -or $group -le 1 -or $actual -ne [string]$group -or $actual -eq $ours) { throw 'Invalid isolated verification process group.' }
        $Owner.Group = $group
    }
    [IO.File]::WriteAllText($Owner.Gate, 'start', [Text.UTF8Encoding]::new($false))
}

function Test-VerificationMembers($Owner) {
    if ($IsWindows) { return $Owner.Job.Active -gt 0 }
    if ($null -eq $Owner.Group) { return $false }
    $rows = @(& $Owner.Ps -eo pid=,pgid=,stat=)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect owned verification group.' }
    # Reap the direct child separately; init owns exited orphan zombies on Unix.
    return @($rows | Where-Object { $_ -match '^\s*\d+\s+(\d+)\s+(\S+)' -and [int]$Matches[1] -eq $Owner.Group -and -not $Matches[2].StartsWith('Z') }).Count -gt 0
}

function Test-VerificationCompilerAuxiliaryPath([string]$Path) {
    if (-not $IsWindows) { return $false }
    foreach ($base in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ([string]::IsNullOrWhiteSpace($base)) { continue }
        $prefix = [regex]::Escape((Join-Path $base 'Microsoft Visual Studio') + '\')
        if ($Path -notmatch ('^' + $prefix + '[^\\]+\\[^\\]+\\VC\\Tools\\MSVC\\[0-9.]+\\bin\\Host(?:x64|arm64)\\(?:x64|arm64)\\vctip\.exe$')) { continue }
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        while ($null -ne $item) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { return $false }
            $item = if ($item -is [IO.FileInfo]) { $item.Directory } else { $item.Parent }
        }
        return $true
    }
    return $false
}

function Stop-VerificationCompilerAuxiliaries($Owner) {
    if (-not $IsWindows -or -not $Owner.CompilerCommand) { return $false }
    $count = 0
    foreach ($memberId in $Owner.Job.Members()) {
        $member = $null
        try {
            $member = [Diagnostics.Process]::GetProcessById($memberId)
            $handle = $member.Handle
            if (-not $Owner.Job.Contains($handle)) { return $false }
            if ($member.HasExited) { continue }
            $path = $member.MainModule.FileName
            if (-not (Test-VerificationCompilerAuxiliaryPath $path)) { return $false }
            $signature = Get-AuthenticodeSignature -LiteralPath $path -ErrorAction Stop
            if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false) -cne 'Microsoft Corporation') { return $false }
            $count++
        } catch { return $false }
        finally { if ($null -ne $member) { $member.Dispose() } }
    }
    if ($count -eq 0) { return $false }
    Stop-VerificationOwner $Owner
    $Owner.CompilerAuxiliaries += $count
    Write-Host "Settled Microsoft-signed MSVC telemetry auxiliaries count=$count; owned Job verified empty."
    return $true
}

function Wait-VerificationEmpty($Owner) {
    # The root exit signal can precede Job accounting or a child's final teardown.
    # Observe natural completion briefly; a still-running descendant is a failure.
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while (Test-VerificationMembers $Owner) {
        if ($watch.Elapsed.TotalSeconds -ge 1) {
            if (Stop-VerificationCompilerAuxiliaries $Owner) { return $true }
            if ($IsWindows) {
                try {
                    foreach ($memberId in $Owner.Job.Members()) {
                        $member = $null
                        try {
                            $member = [Diagnostics.Process]::GetProcessById($memberId)
                            Write-Host "Owned verification member pid=$memberId name=$($member.ProcessName)."
                        } catch { Write-Host "Owned verification member pid=$memberId identity unavailable." }
                        finally { if ($null -ne $member) { $member.Dispose() } }
                    }
                } catch { Write-Host 'Owned verification membership diagnostics unavailable; cleanup remains required.' }
            }
            return $false
        }
        Start-Sleep -Milliseconds 20
    }
    return $true
}

function Stop-VerificationOwner($Owner) {
    if ($Owner.Started) {
        if ($IsWindows) { $Owner.Job.Terminate() }
        elseif ($null -ne $Owner.Group) {
            & $Owner.Kill -KILL -- "-$($Owner.Group)" 2>$null
            # ESRCH is normal after all members have exited; inspection below is authoritative.
        }
        if (-not $Owner.Process.HasExited) {
            $Owner.Process.Kill($true)
        }
        if (-not $Owner.Process.WaitForExit(5000)) { throw 'Verification driver could not be reaped.' }
        $watch = [Diagnostics.Stopwatch]::StartNew()
        while (Test-VerificationMembers $Owner) {
            if ($watch.Elapsed.TotalSeconds -ge 5) { throw 'Verification descendants did not terminate.' }
            Start-Sleep -Milliseconds 20
        }
    }
}
