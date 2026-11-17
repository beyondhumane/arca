# Competitive benchmark: Arca against WinRAR, 7-Zip, NanaZip and xz on
# Windows, in one unattended command.
#
#   cargo build --release -p arca-cli
#   powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\compare.ps1
#
# It builds deterministic corpora once into -Work, then times CREATE, EXTRACT
# and TEST for ZIP, 7z, .tar.xz and RAR at three levels (fast, normal, best)
# with every installed tool that handles the format. Each group of tools runs
# interleaved, A B C then C B A, -Runs times; every run records wall time, CPU
# time (user + kernel of the whole process tree, through a job object), the
# peak working set and the output size. Every archive is checked with its own
# tool, with a second tool where one is installed, and against the corpus with
# `arca diff`; every extraction is compared with the corpus. A failure marks
# its row and the script goes on.
#
# Results go to -Work\results: samples.csv (every run), results.csv (one line
# per row, also the resume state), results.json and results.md (tables ready
# for docs/guide/benchmarks.md, with the machine, the tool versions and the
# exact command of every row). Rerunning the same command skips the rows
# already measured. Missing tools are skipped. -Quick is a dry run on tiny
# corpora, one run each, in its own work folder: never publish its numbers.
#
# Nothing outside -Work is written, apart from the corpus generators that
# cargo builds into this repository's target folder. Antivirus and caches are
# left alone: runs are warm and the machine's state is reported with them.
param(
    [string]$Arca = "",
    [string]$Work = "",
    [int]$Runs = 3,
    [int]$Warmup = 0,
    [string[]]$Formats = @("zip", "7z", "txz", "rar"),
    [string[]]$Corpora = @(),
    [string[]]$Levels = @("fast", "normal", "best"),
    [switch]$SingleThread,
    [switch]$Quick,
    [switch]$Force,
    [switch]$KeepCorpora,
    [string]$SourceCommit = "4b1d67b273deebfebd1dbe1b69ace1068e6c9af4",
    [string]$SevenZip = "",
    [string]$NanaZip = "",
    [string]$WinRar = "",
    [string]$Rar = "",
    [string]$Xz = "",
    [int]$TimeoutMinutes = 20
)

$ErrorActionPreference = "Stop"
$Inv = [Globalization.CultureInfo]::InvariantCulture
[Threading.Thread]::CurrentThread.CurrentCulture = $Inv
[Threading.Thread]::CurrentThread.CurrentUICulture = $Inv
$Utf8 = New-Object Text.UTF8Encoding $false
$Started = Get-Date

# powershell -File passes "a,b" as one string, not as an array.
function Split-List($v) { @($v | ForEach-Object { "$_" -split "," } | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" }) }
$Formats = Split-List $Formats
$Corpora = Split-List $Corpora
$Levels = Split-List $Levels
if ($Quick -and -not $PSBoundParameters.ContainsKey("Runs")) { $Runs = 1 }
if ($Runs -lt 1) { throw "-Runs must be at least 1" }

$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ($Work -eq "") {
    $leaf = "arca-compare"
    if ($Quick) { $leaf = "arca-compare-quick" }
    $Work = Join-Path $env:TEMP $leaf
}
$Work = [IO.Path]::GetFullPath($Work).TrimEnd("\")
if ([IO.Path]::GetPathRoot($Work).TrimEnd("\") -eq $Work) { throw "-Work cannot be a drive root: $Work" }
$Marker = Join-Path $Work ".arca-compare"
if (Test-Path -LiteralPath $Work) {
    $any = Get-ChildItem -LiteralPath $Work -Force | Select-Object -First 1
    if ($any -and -not (Test-Path -LiteralPath $Marker)) {
        throw "$Work is not empty and was not made by this script; pass an empty or new -Work folder"
    }
}
New-Item -ItemType Directory -Force $Work | Out-Null
[IO.File]::WriteAllText($Marker, "Work folder of benchmarks\compare.ps1. Everything here can be deleted.`r`n", $Utf8)

$CorpusRoot = Join-Path $Work "c"
$TarDir = Join-Path $Work "tar"
$OutDir = Join-Path $Work "out"
$KeepDir = Join-Path $Work "keep"
$XDir = Join-Path $Work "x"
$ResDir = Join-Path $Work "results"
foreach ($d in @($CorpusRoot, $TarDir, $OutDir, $KeepDir, $XDir, $ResDir)) { New-Item -ItemType Directory -Force $d | Out-Null }
$LogFile = Join-Path $ResDir "compare.log"
$SamplesCsv = Join-Path $ResDir "samples.csv"
$ResultsCsv = Join-Path $ResDir "results.csv"
$TimeoutMs = $TimeoutMinutes * 60 * 1000
$AllThreads = [Environment]::ProcessorCount

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;

namespace ArcaCompare {
public class RunResult {
    public int ExitCode; public bool TimedOut; public double Wall; public double User; public double Kernel;
    public long PeakWorkingSet; public long PeakPrivate; public int Processes; public string StdOut = ""; public string StdErr = "";
}

public static class Runner {
    [StructLayout(LayoutKind.Sequential)] struct IoCounters { public ulong A, B, C, D, E, F; }
    [StructLayout(LayoutKind.Sequential)] struct BasicLimit {
        public long PerProcessUserTimeLimit; public long PerJobUserTimeLimit; public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize; public UIntPtr MaximumWorkingSetSize; public uint ActiveProcessLimit;
        public UIntPtr Affinity; public uint PriorityClass; public uint SchedulingClass;
    }
    [StructLayout(LayoutKind.Sequential)] struct ExtendedLimit {
        public BasicLimit Basic; public IoCounters Io; public UIntPtr ProcessMemoryLimit; public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed; public UIntPtr PeakJobMemoryUsed;
    }
    [StructLayout(LayoutKind.Sequential)] struct Accounting {
        public long TotalUserTime; public long TotalKernelTime; public long ThisPeriodTotalUserTime; public long ThisPeriodTotalKernelTime;
        public uint TotalPageFaultCount; public uint TotalProcesses; public uint ActiveProcesses; public uint TotalTerminatedProcesses;
    }
    [StructLayout(LayoutKind.Sequential)] struct MemoryCounters {
        public uint cb; public uint PageFaultCount; public UIntPtr PeakWorkingSetSize; public UIntPtr WorkingSetSize;
        public UIntPtr QuotaPeakPagedPoolUsage; public UIntPtr QuotaPagedPoolUsage; public UIntPtr QuotaPeakNonPagedPoolUsage;
        public UIntPtr QuotaNonPagedPoolUsage; public UIntPtr PagefileUsage; public UIntPtr PeakPagefileUsage;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll")] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll")] static extern bool TerminateJobObject(IntPtr job, uint code);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref ExtendedLimit info, int length);
    [DllImport("kernel32.dll")] static extern bool QueryInformationJobObject(IntPtr job, int infoClass, out Accounting info, int length, IntPtr returned);
    [DllImport("kernel32.dll", EntryPoint = "QueryInformationJobObject")] static extern bool QueryLimits(IntPtr job, int infoClass, out ExtendedLimit info, int length, IntPtr returned);
    [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr process, out long creation, out long exit, out long kernel, out long user);
    [DllImport("psapi.dll")] static extern bool GetProcessMemoryInfo(IntPtr process, out MemoryCounters counters, int size);

    public static RunResult Run(string exe, string arguments, string cwd, int timeoutMs) {
        ProcessStartInfo info = new ProcessStartInfo(exe, arguments);
        info.WorkingDirectory = cwd;
        info.UseShellExecute = false;
        info.CreateNoWindow = false;
        info.RedirectStandardInput = true;
        info.RedirectStandardOutput = true;
        info.RedirectStandardError = true;
        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        if (job != IntPtr.Zero) {
            ExtendedLimit limit = new ExtendedLimit();
            limit.Basic.LimitFlags = 0x2000;
            SetInformationJobObject(job, 9, ref limit, Marshal.SizeOf(typeof(ExtendedLimit)));
        }
        RunResult r = new RunResult();
        Stopwatch clock = Stopwatch.StartNew();
        Process p = Process.Start(info);
        bool inJob = job != IntPtr.Zero && AssignProcessToJobObject(job, p.Handle);
        try { p.StandardInput.Close(); } catch (IOException) { }
        Task<string> stdout = p.StandardOutput.ReadToEndAsync();
        Task<string> stderr = p.StandardError.ReadToEndAsync();
        if (!p.WaitForExit(timeoutMs)) {
            r.TimedOut = true;
            if (inJob) { TerminateJobObject(job, 1); } else { try { p.Kill(); } catch (Exception) { } }
            p.WaitForExit();
        }
        clock.Stop();
        r.Wall = clock.Elapsed.TotalSeconds;
        r.ExitCode = p.ExitCode;
        r.Processes = 1;
        long created, exited, kernel, user;
        if (GetProcessTimes(p.Handle, out created, out exited, out kernel, out user)) { r.User = user / 1e7; r.Kernel = kernel / 1e7; }
        MemoryCounters m;
        if (GetProcessMemoryInfo(p.Handle, out m, Marshal.SizeOf(typeof(MemoryCounters)))) { r.PeakWorkingSet = (long)m.PeakWorkingSetSize.ToUInt64(); }
        if (inJob) {
            Accounting a;
            if (QueryInformationJobObject(job, 1, out a, Marshal.SizeOf(typeof(Accounting)), IntPtr.Zero)) {
                r.Processes = (int)a.TotalProcesses;
                r.User = Math.Max(r.User, a.TotalUserTime / 1e7);
                r.Kernel = Math.Max(r.Kernel, a.TotalKernelTime / 1e7);
            }
            ExtendedLimit e;
            if (QueryLimits(job, 9, out e, Marshal.SizeOf(typeof(ExtendedLimit)), IntPtr.Zero)) { r.PeakPrivate = (long)e.PeakProcessMemoryUsed.ToUInt64(); }
        }
        if (job != IntPtr.Zero) { CloseHandle(job); }
        if (stdout.Wait(10000)) { r.StdOut = stdout.Result; }
        if (stderr.Wait(10000)) { r.StdErr = stderr.Result; }
        p.Dispose();
        return r;
    }
}

public static class Tree {
    public static string Hex(byte[] bytes) {
        StringBuilder s = new StringBuilder(bytes.Length * 2);
        foreach (byte b in bytes) { s.Append(b.ToString("x2")); }
        return s.ToString();
    }
    public static string HashText(string text) {
        using (SHA256 sha = SHA256.Create()) { return Hex(sha.ComputeHash(Encoding.UTF8.GetBytes(text))); }
    }
    public static string HashFile(string path) {
        using (SHA256 sha = SHA256.Create())
        using (FileStream f = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, 1 << 16)) {
            return Hex(sha.ComputeHash(f));
        }
    }
    // One line per file, "path<TAB>size<TAB>sha256", in ordinal path order.
    public static string[] Manifest(string root) {
        string[] files = Directory.GetFiles(root, "*", SearchOption.AllDirectories);
        string[] names = new string[files.Length];
        for (int i = 0; i < files.Length; i++) { names[i] = files[i].Substring(root.Length).TrimStart('\\').Replace('\\', '/'); }
        Array.Sort(names, files, StringComparer.Ordinal);
        string[] lines = new string[files.Length];
        Parallel.For(0, files.Length, i => {
            long length = new FileInfo(files[i]).Length;
            lines[i] = names[i] + "\t" + length + "\t" + HashFile(files[i]);
        });
        return lines;
    }
    public static long[] Count(string root) {
        long files = 0, bytes = 0;
        if (!Directory.Exists(root)) { return new long[] { 0, 0 }; }
        foreach (string f in Directory.GetFiles(root, "*", SearchOption.AllDirectories)) { files++; bytes += new FileInfo(f).Length; }
        return new long[] { files, bytes };
    }
}
}
"@

# ---------------------------------------------------------------- helpers

function F([string]$format) { [string]::Format($Inv, $format, [object[]]$args) }

function Log([string]$msg) {
    $line = "{0} {1}" -f (Get-Date).ToString("yyyy-MM-dd HH:mm:ss", $Inv), $msg
    Write-Host $line
    [IO.File]::AppendAllText($LogFile, $line + "`r`n", $Utf8)
}

function Test-Inside([string]$path) {
    $full = [IO.Path]::GetFullPath($path)
    return $full.StartsWith($Work + "\", [StringComparison]::OrdinalIgnoreCase)
}

# Every delete goes through here, and nothing outside -Work can be deleted.
function Remove-Safe([string]$path) {
    $full = [IO.Path]::GetFullPath($path)
    if (-not (Test-Inside $full)) { throw "refusing to delete $full, which is outside $Work" }
    if (-not (Test-Path -LiteralPath $full)) { return }
    if ((Get-Item -LiteralPath $full -Force) -is [IO.DirectoryInfo]) {
        & cmd.exe /c rd /s /q $full | Out-Null
        if (Test-Path -LiteralPath $full) { Remove-Item -LiteralPath $full -Recurse -Force }
    } else {
        Remove-Item -LiteralPath $full -Force
    }
}

function Link-Or-Copy([string]$target, [string]$path) {
    if (-not (Test-Inside $path)) { throw "refusing to write $path, which is outside $Work" }
    Remove-Safe $path
    try { New-Item -ItemType HardLink -Path $path -Target $target | Out-Null }
    catch { Copy-Item -LiteralPath $target -Destination $path }
}

function Q([string]$s) { '"' + $s + '"' }

function Csv-Field($v) {
    $s = "$v"
    if ($s -match '[",\r\n]') { return '"' + $s.Replace('"', '""') + '"' }
    return $s
}

function Append-Csv([string]$path, [string[]]$columns, $row) {
    if (-not (Test-Path -LiteralPath $path)) { [IO.File]::WriteAllText($path, ($columns -join ",") + "`r`n", $Utf8) }
    $line = ($columns | ForEach-Object { Csv-Field $row[$_] }) -join ","
    [IO.File]::AppendAllText($path, $line + "`r`n", $Utf8)
}

function Median([double[]]$v) {
    $s = [double[]]($v | Sort-Object)
    $n = $s.Count
    if ($n % 2 -eq 1) { return $s[($n - 1) / 2] }
    return ($s[$n / 2 - 1] + $s[$n / 2]) / 2
}

function Get-Load {
    try { return [double](Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average }
    catch { return -1 }
}

function Invoke-Tool([string]$exe, [string]$arguments, [string]$cwd) {
    return [ArcaCompare.Runner]::Run($exe, $arguments, $cwd, $TimeoutMs)
}

function Short-Error($r) {
    $text = ($r.StdErr + " " + $r.StdOut).Trim() -replace "\s+", " "
    if ($text.Length -gt 300) { $text = $text.Substring(0, 300) + "..." }
    if ($r.TimedOut) { return "timed out after $TimeoutMinutes min" }
    return "exit $($r.ExitCode): $text"
}

# ---------------------------------------------------------------- tools

function Find-Exe([string[]]$candidates, [string[]]$names) {
    foreach ($c in $candidates) {
        if ($c -and (Test-Path -LiteralPath $c -PathType Leaf)) { return [IO.Path]::GetFullPath($c) }
    }
    foreach ($n in $names) {
        $cmd = Get-Command $n -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($cmd) { return $cmd.Source }
    }
    return $null
}

function First-Line([string]$text, [string]$pattern) {
    foreach ($l in ($text -split "`r?`n")) {
        $t = $l.Trim()
        if ($t -ne "" -and $t -match $pattern) { return $t }
    }
    return ""
}

$Tools = [ordered]@{}
$ToolNotes = @()
function Add-Tool([string]$key, [string]$name, [string]$short, [string]$exe, [string]$given, [string]$what) {
    if (-not $exe) {
        $note = "$name not found ($what); its rows are skipped"
        if ($given) { $note = "$name not found at $given; its rows are skipped" }
        $script:ToolNotes += $note
        return
    }
    $version = ""
    try {
        switch ($key) {
            "arca" { $version = (Invoke-Tool $exe "--version" $Work).StdOut.Trim() }
            "xz" { $version = First-Line (Invoke-Tool $exe "--version" $Work).StdOut "xz" }
            "winrar" { $version = "WinRAR " + (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion }
            "rar" { $version = First-Line (Invoke-Tool $exe "" $Work).StdOut "RAR" }
            default { $r = Invoke-Tool $exe "" $Work; $version = First-Line ($r.StdOut + "`n" + $r.StdErr) "Zip|zip" }
        }
    } catch { $version = "unknown ($($_.Exception.Message))" }
    $hash = ""
    try { $hash = [ArcaCompare.Tree]::HashFile($exe) } catch { $hash = "unreadable (app alias?)" }
    $script:Tools[$key] = [ordered]@{ key = $key; name = $name; short = $short; exe = $exe; version = $version; sha256 = $hash }
}

function Has([string]$key) { return $Tools.Contains($key) }

# ---------------------------------------------------------------- formats

$Ext = @{ zip = ".zip"; "7z" = ".7z"; txz = ".tar.xz"; rar = ".rar" }
$FormatTitle = @{ zip = "ZIP"; "7z" = "7z"; txz = ".tar.xz"; rar = "RAR" }
$CreateTools = @{ zip = @("arca", "7zip", "nanazip", "winrar"); "7z" = @("arca", "7zip", "nanazip"); txz = @("arca", "xz", "7zip", "nanazip"); rar = @("arca", "rar") }
$ReadTools = @{ zip = @("arca", "7zip", "nanazip", "winrar"); "7z" = @("arca", "7zip", "nanazip"); txz = @("arca", "xz", "7zip", "nanazip"); rar = @("arca", "rar", "7zip", "nanazip") }
# The other producer whose normal archive every reader also extracts.
$RefProducer = @{ zip = @("7zip", "nanazip", "winrar"); "7z" = @("7zip", "nanazip"); txz = @("xz", "7zip", "nanazip"); rar = @("rar") }
# The third-party tool that checks an archive Arca wrote.
$SecondCheck = @{ zip = @("7zip", "nanazip", "winrar"); "7z" = @("7zip", "nanazip"); txz = @("xz", "7zip", "nanazip"); rar = @("rar", "7zip", "nanazip") }
$LevelFlag = @{
    arca = @{ fast = "1"; normal = "6"; best = "9" }
    "7zip" = @{ fast = "1"; normal = "5"; best = "9" }
    nanazip = @{ fast = "1"; normal = "5"; best = "9" }
    winrar = @{ fast = "1"; normal = "3"; best = "5" }
    rar = @{ fast = "1"; normal = "3"; best = "5" }
    xz = @{ fast = "1"; normal = "6"; best = "9" }
}
$LevelOrder = @("fast", "normal", "best")
foreach ($l in $Levels) { if ($LevelOrder -notcontains $l) { throw "unknown level '$l': use fast, normal, best" } }
foreach ($f in $Formats) { if (-not $Ext.ContainsKey($f)) { throw "unknown format '$f': use zip, 7z, txz, rar" } }

# The corpora each format runs on by default; -Corpora overrides it. The
# whole run has to fit in about an hour, and LZMA at `best` is the slow part:
# Arca's 7z runs it on one thread, and a .tar.xz under 192 MiB is one xz
# block, one thread, for Arca and xz alike. So the 50,000 small files only
# run as ZIP, 7z leaves out the incompressible corpus, and .tar.xz keeps to
# source and text, what it is mostly used for.
$Matrix = @{ zip = @("small", "source", "text", "mixed", "compressed"); "7z" = @("source", "text", "mixed"); txz = @("source", "text"); rar = @("source", "text", "mixed", "compressed") }
$CorpusOrder = @("small", "source", "text", "mixed", "compressed")
foreach ($c in $Corpora) { if ($CorpusOrder -notcontains $c) { throw "unknown corpus '$c': use $($CorpusOrder -join ', ')" } }

function Corpora-For([string]$format) {
    if ($Corpora.Count -gt 0) { return @($CorpusOrder | Where-Object { $Corpora -contains $_ }) }
    return @($Matrix[$format])
}

# ---------------------------------------------------------------- corpora

$CorpusDefs = [ordered]@{
    small = @{ desc = "50,000 files of 0 to 16 KiB in 100 folders, 3 parts prose-like text to 1 part random bytes"; quickDesc = "500 files of 0 to 16 KiB"
        example = "small_files"; args = "50000 16384 mixed"; quickArgs = "500 16384 mixed" }
    source = @{ desc = "this repository at the pinned commit (git archive), real source, documentation, fixtures and icons"; quickDesc = "the docs folder of the pinned commit" }
    text = @{ desc = "4 large prose-like text files of up to 8 MiB"; quickDesc = "2 text files of up to 1 MiB"
        example = "small_files"; args = "4 8388608 text"; quickArgs = "2 1048576 text" }
    mixed = @{ desc = "x86-like executables, PCM audio and a bitmap (the exe and media kinds of the RAR corpus)"; quickDesc = "the exe and media kinds at scale 0.02"
        example = "rar-corpus"; args = "1 exe media"; quickArgs = "0.02 exe media" }
    compressed = @{ desc = "random bytes standing in for data that is already compressed (the random kind of the RAR corpus)"; quickDesc = "the random kind at scale 0.1"
        example = "rar-corpus"; args = "2 random"; quickArgs = "0.1 random" }
}
$ExamplePackage = @{ small_files = "arca-bench"; "rar-corpus" = "arca-rar" }
$CorpusInfo = [ordered]@{}
$BuiltExamples = @{}

function Get-Example([string]$name) {
    $exe = Join-Path $Repo "target\release\examples\$name.exe"
    if ($BuiltExamples.ContainsKey($name)) { return $exe }
    $cargo = Find-Exe @() @("cargo.exe")
    if ($cargo) {
        Log "building the corpus generator $name with cargo"
        $r = Invoke-Tool $cargo "build --release -p $($ExamplePackage[$name]) --example $name" $Repo
        if ($r.ExitCode -ne 0) { throw "cargo build of $name failed: $(Short-Error $r)" }
    } elseif (-not (Test-Path -LiteralPath $exe)) {
        throw "cargo is not on PATH and $exe does not exist: the corpora cannot be generated"
    }
    $BuiltExamples[$name] = $true
    return $exe
}

function Corpus-Command([string]$name, [string]$dir) {
    $def = $CorpusDefs[$name]
    if ($name -eq "source") {
        $spec = ""
        if ($Quick) { $spec = " -- docs" }
        return "git archive --format=tar -o source.tar $SourceCommit$spec; tar -xf source.tar -C $dir"
    }
    $a = $def.args
    if ($Quick) { $a = $def.quickArgs }
    return "cargo run --release -p $($ExamplePackage[$def.example]) --example $($def.example) -- $dir $a"
}

function Build-Corpus([string]$name) {
    $def = $CorpusDefs[$name]
    $parent = Join-Path $CorpusRoot $name
    $dir = Join-Path $parent $name
    $infoFile = Join-Path $CorpusRoot "$name.json"
    if ((Test-Path -LiteralPath $dir) -and (Test-Path -LiteralPath $infoFile)) {
        $saved = Get-Content -LiteralPath $infoFile -Raw | ConvertFrom-Json
        $lines = [ArcaCompare.Tree]::Manifest($dir)
        $content = [ArcaCompare.Tree]::HashText(($lines -join "`n") + "`n")
        if ($content -eq $saved.contentHash) {
            Log "corpus $name is ready: $($saved.files) files, $(F '{0:N0}' ([long]$saved.bytes)) bytes (read once to warm the cache)"
            $script:CorpusInfo[$name] = $saved
            return
        }
        Log "corpus $name changed on disk; building it again"
    }
    $tmp = Join-Path $CorpusRoot "$name.tmp"
    Remove-Safe $tmp
    Remove-Safe $parent
    New-Item -ItemType Directory -Force $tmp | Out-Null
    $tmpDir = Join-Path $tmp $name
    Log "building corpus $name"
    if ($name -eq "source") {
        $git = Find-Exe @() @("git.exe")
        $tar = Find-Exe @("$env:SystemRoot\System32\tar.exe") @("tar.exe")
        if (-not $git -or -not $tar) { throw "the source corpus needs git and tar on PATH" }
        $tarFile = Join-Path $tmp "source.tar"
        $spec = ""
        if ($Quick) { $spec = " -- docs" }
        $r = Invoke-Tool $git "archive --format=tar -o $(Q $tarFile) $SourceCommit$spec" $Repo
        if ($r.ExitCode -ne 0) { throw "git archive $SourceCommit failed: $(Short-Error $r)" }
        New-Item -ItemType Directory -Force $tmpDir | Out-Null
        $r = Invoke-Tool $tar "-xf $(Q $tarFile) -C $(Q $tmpDir)" $tmp
        if ($r.ExitCode -ne 0) { throw "tar -xf failed: $(Short-Error $r)" }
        Remove-Safe $tarFile
    } else {
        $exe = Get-Example $def.example
        $a = $def.args
        if ($Quick) { $a = $def.quickArgs }
        $r = Invoke-Tool $exe "$(Q $tmpDir) $a" $tmp
        if ($r.ExitCode -ne 0) { throw "$($def.example) failed: $(Short-Error $r)" }
    }
    Rename-Item -LiteralPath $tmp -NewName $name
    $lines = [ArcaCompare.Tree]::Manifest($dir)
    $bytes = [long]0
    $list = New-Object Text.StringBuilder
    foreach ($l in $lines) {
        $p = $l.Split("`t")
        $bytes += [long]$p[1]
        [void]$list.Append($p[0]).Append("`t").Append($p[1]).Append("`n")
    }
    $desc = $def.desc
    if ($Quick) { $desc = $def.quickDesc }
    $info = [ordered]@{
        name = $name; description = $desc; files = $lines.Count; bytes = $bytes
        listHash = [ArcaCompare.Tree]::HashText($list.ToString())
        contentHash = [ArcaCompare.Tree]::HashText(($lines -join "`n") + "`n")
        command = Corpus-Command $name "<dir>"
    }
    [IO.File]::WriteAllText($infoFile, ($info | ConvertTo-Json -Depth 4), $Utf8)
    [IO.File]::WriteAllText((Join-Path $CorpusRoot "$name.manifest.txt"), ($lines -join "`n") + "`n", $Utf8)
    Log "corpus ${name}: $($info.files) files, $(F '{0:N0}' $bytes) bytes, list hash $($info.listHash.Substring(0, 16))"
    $script:CorpusInfo[$name] = (($info | ConvertTo-Json -Depth 4) | ConvertFrom-Json)
}

# The tar that xz and 7-Zip compress for .tar.xz; Arca builds its own.
function Ensure-Tar([string]$name) {
    $tarFile = Join-Path $TarDir "$name.tar"
    if (Test-Path -LiteralPath $tarFile) { return $tarFile }
    $tar = Find-Exe @("$env:SystemRoot\System32\tar.exe") @("tar.exe")
    $partial = "$tarFile.partial"
    Remove-Safe $partial
    if ($tar) {
        $r = Invoke-Tool $tar "-cf $(Q $partial) $name" (Join-Path $CorpusRoot $name)
    } else {
        $r = Invoke-Tool $Tools.arca.exe "create $(Q ($partial + '.tar')) $name" (Join-Path $CorpusRoot $name)
        if ($r.ExitCode -eq 0) { Move-Item -LiteralPath ($partial + ".tar") -Destination $partial }
    }
    if ($r.ExitCode -ne 0) { throw "could not build $tarFile : $(Short-Error $r)" }
    Move-Item -LiteralPath $partial -Destination $tarFile
    Log "tar for ${name}: $(F '{0:N0}' (Get-Item -LiteralPath $tarFile).Length) bytes"
    return $tarFile
}

# ---------------------------------------------------------------- commands

function Thread-Flag([string]$tool, [int]$t) {
    switch ($tool) {
        "arca" { return "-j $t" }
        "xz" { return "-T$t" }
        "winrar" { return "-mt$([Math]::Min($t, 64))" }
        "rar" { return "-mt$([Math]::Min($t, 64))" }
        default { return "-mmt=$t" }
    }
}

function New-Cmd([string]$tool, [string]$exe, [string]$cmdArgs, [string]$cwd) {
    return @{ tool = $tool; exe = $exe; args = $cmdArgs; cwd = $cwd }
}

# A command that tests an archive with the given tool.
function Test-Cmd([string]$tool, [string]$format, [string]$file, [int]$t) {
    if (-not (Has $tool)) { return $null }
    $exe = $Tools[$tool].exe
    $tf = Thread-Flag $tool $t
    switch ($tool) {
        "arca" { return New-Cmd $tool $exe "test $(Q $file)" $Work }
        "winrar" { if ($format -ne "zip") { return $null }; return New-Cmd $tool $exe "t -ibck -inul -y $tf $(Q $file)" $Work }
        "rar" { if ($format -ne "rar") { return $null }; return New-Cmd $tool $exe "t -idq -y $tf $(Q $file)" $Work }
        "xz" { if ($format -ne "txz") { return $null }; return New-Cmd $tool $exe "-t $tf $(Q $file)" $Work }
        default { return New-Cmd $tool $exe "t $(Q $file) -bd -bso0 -bsp0 -y $tf" $Work }
    }
}

function Create-Cmd([string]$tool, [string]$format, [string]$corpus, [string]$level, [int]$t, [string]$out) {
    $exe = $Tools[$tool].exe
    $n = $LevelFlag[$tool][$level]
    $tf = Thread-Flag $tool $t
    $cwd = Join-Path $CorpusRoot $corpus
    $c = $null
    switch ($tool) {
        "arca" { $c = New-Cmd $tool $exe "create $(Q $out) $corpus -l $n $tf" $cwd }
        "winrar" { $c = New-Cmd $tool $exe "a -afzip -m$n $tf -r -ibck -inul -y $(Q $out) $corpus" $cwd }
        "rar" { $c = New-Cmd $tool $exe "a -m$n $tf -r -idq -y $(Q $out) $corpus" $cwd }
        "xz" {
            $link = $out.Substring(0, $out.Length - 3)
            $c = New-Cmd $tool $exe "-$n -k -f $tf $(Split-Path -Leaf $link)" (Split-Path -Parent $out)
            $c.link = $link
        }
        default {
            if ($format -eq "txz") {
                $c = New-Cmd $tool $exe "a -txz -mx$n $tf -bd -bso0 -bsp0 -y $(Q $out) $(Q (Join-Path $TarDir "$corpus.tar"))" $cwd
            } elseif ($format -eq "7z") {
                $c = New-Cmd $tool $exe "a -t7z -m0=lzma2 -mx$n $tf -bd -bso0 -bsp0 -y $(Q $out) $corpus" $cwd
            } else {
                $c = New-Cmd $tool $exe "a -tzip -mx$n $tf -bd -bso0 -bsp0 -y $(Q $out) $corpus" $cwd
            }
        }
    }
    if ($format -eq "txz" -and $tool -ne "arca") { $c.tar = Ensure-Tar $corpus }
    $c.out = $out
    return $c
}

# Every reader extracts into $XDir\<tool>; tar.xz readers other than Arca
# write the decompressed .tar there, not the files.
function Extract-Cmd([string]$tool, [string]$format, [string]$file, [int]$t) {
    if (-not (Has $tool)) { return $null }
    $exe = $Tools[$tool].exe
    $tf = Thread-Flag $tool $t
    $dest = Join-Path $XDir $tool
    $leaf = Split-Path -Leaf $file
    $c = $null
    switch ($tool) {
        "arca" { $c = New-Cmd $tool $exe "extract $(Q $file) -o $(Q $dest) $tf" $Work; $c.kind = "dir" }
        "winrar" { if ($format -ne "zip") { return $null }; $c = New-Cmd $tool $exe "x -ibck -inul -y $tf $(Q $file)" $dest; $c.kind = "dir"; $c.mkdir = $true }
        "rar" { if ($format -ne "rar") { return $null }; $c = New-Cmd $tool $exe "x -idq -y $tf $(Q $file)" $dest; $c.kind = "dir"; $c.mkdir = $true }
        "xz" {
            if ($format -ne "txz") { return $null }
            $c = New-Cmd $tool $exe "-d -k -f $tf $leaf" $dest
            $c.kind = "tar"; $c.mkdir = $true; $c.link = Join-Path $dest $leaf; $c.linkTarget = $file
        }
        default {
            $c = New-Cmd $tool $exe "x $(Q $file) -o$(Q $dest) -y -bd -bso0 -bsp0 $tf" $Work
            if ($format -eq "txz") { $c.kind = "tar" } else { $c.kind = "dir" }
        }
    }
    $c.dest = $dest
    if ($c.kind -eq "tar") { $c.tarOut = Join-Path $dest ($leaf.Substring(0, $leaf.Length - 3)) }
    return $c
}

function Diff-Check([string]$corpus, [string]$target) {
    $r = Invoke-Tool $Tools.arca.exe "diff $(Q (Join-Path $CorpusRoot $corpus)) $(Q $target)" $Work
    if ($r.ExitCode -eq 0) { return "" }
    return "arca diff against the corpus: $(Short-Error $r)"
}

function Exe-Hash([string]$tool) { return $Tools[$tool].sha256 }

function Fingerprint($cmd, [string]$corpus, [string]$inputHash) {
    $text = "{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}" -f (Exe-Hash $cmd.tool), $cmd.exe, $cmd.args, $cmd.cwd, $CorpusInfo[$corpus].contentHash, $inputHash, $Runs, $Warmup
    return [ArcaCompare.Tree]::HashText($text).Substring(0, 24)
}

# ---------------------------------------------------------------- inputs

function Out-Path([string]$format, [string]$corpus, [string]$tool, [string]$level, [int]$t) {
    $suffix = ""
    if ($t -eq 1) { $suffix = "-t1" }
    return Join-Path $OutDir "$format-$corpus-$tool-$level$suffix$($Ext[$format])"
}

function Prepare-Create($cmd) {
    Remove-Safe $cmd.out
    if ($cmd.link) { Link-Or-Copy $cmd.tar $cmd.link }
}

function Finish-Create($cmd) {
    if ($cmd.link) { Remove-Safe $cmd.link }
}

# The normal-level archive that a producer writes, kept for the readers. It
# is made by the create rows; when those were skipped it is made here,
# untimed.
function Ensure-Input([string]$format, [string]$corpus, [string]$producer) {
    $keep = Join-Path $KeepDir "$format-$corpus-$producer$($Ext[$format])"
    $side = "$keep.json"
    $cmd = Create-Cmd $producer $format $corpus "normal" $AllThreads (Out-Path $format $corpus $producer "normal" $AllThreads)
    $fp = Fingerprint $cmd $corpus ""
    if ((Test-Path -LiteralPath $keep) -and (Test-Path -LiteralPath $side)) {
        $saved = Get-Content -LiteralPath $side -Raw | ConvertFrom-Json
        if ($saved.fingerprint -eq $fp) { return $saved }
    }
    Log "making the $producer $format archive of $corpus for the readers (untimed)"
    Prepare-Create $cmd
    $r = Invoke-Tool $cmd.exe $cmd.args $cmd.cwd
    Finish-Create $cmd
    if ($r.ExitCode -ne 0 -or $r.TimedOut -or -not (Test-Path -LiteralPath $cmd.out)) {
        Log "  could not make it: $(Short-Error $r)"
        return $null
    }
    return Keep-Input $format $corpus $producer $cmd $fp
}

function Keep-Input([string]$format, [string]$corpus, [string]$producer, $cmd, [string]$fp) {
    $keep = Join-Path $KeepDir "$format-$corpus-$producer$($Ext[$format])"
    Remove-Safe $keep
    Move-Item -LiteralPath $cmd.out -Destination $keep
    $info = [ordered]@{
        file = $keep; producer = $producer; level = $LevelFlag[$producer]["normal"]; fingerprint = $fp
        sha256 = [ArcaCompare.Tree]::HashFile($keep); bytes = (Get-Item -LiteralPath $keep).Length
        command = "$($Tools[$producer].short) $($cmd.args)"
    }
    [IO.File]::WriteAllText("$keep.json", ($info | ConvertTo-Json -Depth 4), $Utf8)
    return (($info | ConvertTo-Json -Depth 4) | ConvertFrom-Json)
}

# ---------------------------------------------------------------- results

$ResultColumns = @("key", "fingerprint", "status", "op", "format", "corpus", "level", "threads", "tool", "tool_name", "input", "input_sha256",
    "runs", "wall_median", "wall_min", "wall_max", "cpu_median", "cpu_min", "cpu_max", "user_median", "kernel_median",
    "peak_ws_mib_median", "peak_ws_mib_max", "peak_private_mib_median", "processes", "out_bytes", "corpus_bytes", "ratio", "load_pct",
    "verify", "exe", "args", "cwd", "finished")
$SampleColumns = @("key", "fingerprint", "op", "format", "corpus", "level", "threads", "tool", "input", "run", "position", "ok",
    "wall_s", "user_s", "kernel_s", "peak_ws_bytes", "peak_private_bytes", "processes", "out_bytes", "exit", "note", "time")

$Done = @{}
if (Test-Path -LiteralPath $ResultsCsv) {
    foreach ($row in (Import-Csv -LiteralPath $ResultsCsv)) {
        if ($row.status -eq "ok") { $Done[$row.key] = $row.fingerprint } elseif ($Done.ContainsKey($row.key)) { $Done.Remove($row.key) }
    }
}

function Stat([double[]]$v) {
    $m = Median $v
    return @{ median = $m; min = ($v | Measure-Object -Minimum).Minimum; max = ($v | Measure-Object -Maximum).Maximum }
}

function Write-Result($row, [string]$status, [string]$verify, [double]$load) {
    $s = $row.samples
    $cmd = $row.cmd
    $res = @{
        key = $row.key; fingerprint = $row.fp; status = $status; op = $row.op; format = $row.format; corpus = $row.corpus
        level = $row.level; threads = $row.threads; tool = $cmd.tool; tool_name = $Tools[$cmd.tool].name; input = $row.input
        input_sha256 = $row.inputHash; runs = $s.Count; corpus_bytes = $CorpusInfo[$row.corpus].bytes; load_pct = $load
        verify = $verify; exe = $cmd.exe; args = $cmd.args; cwd = $cmd.cwd; finished = (Get-Date).ToString("s", $Inv)
    }
    if ($s.Count -gt 0) {
        $w = Stat ([double[]]($s | ForEach-Object { $_.wall }))
        $c = Stat ([double[]]($s | ForEach-Object { $_.user + $_.kernel }))
        $m = Stat ([double[]]($s | ForEach-Object { $_.peak / 1MB }))
        $res.wall_median = F "{0:F3}" $w.median; $res.wall_min = F "{0:F3}" $w.min; $res.wall_max = F "{0:F3}" $w.max
        $res.cpu_median = F "{0:F3}" $c.median; $res.cpu_min = F "{0:F3}" $c.min; $res.cpu_max = F "{0:F3}" $c.max
        $res.user_median = F "{0:F3}" (Median ([double[]]($s | ForEach-Object { $_.user })))
        $res.kernel_median = F "{0:F3}" (Median ([double[]]($s | ForEach-Object { $_.kernel })))
        $res.peak_ws_mib_median = F "{0:F1}" $m.median; $res.peak_ws_mib_max = F "{0:F1}" $m.max
        $res.peak_private_mib_median = F "{0:F1}" (Median ([double[]]($s | ForEach-Object { $_.private / 1MB })))
        $res.processes = ($s | ForEach-Object { $_.procs } | Measure-Object -Maximum).Maximum
        if ($row.op -eq "create") {
            $sizes = @($s | ForEach-Object { $_.bytes } | Sort-Object -Unique)
            $res.out_bytes = $sizes[-1]
            if ($sizes.Count -gt 1) { $res.verify = "size varies between runs: $($sizes -join ' '); $verify" }
            $res.ratio = F "{0:F4}" ($sizes[-1] / [double]$CorpusInfo[$row.corpus].bytes)
        }
    }
    Append-Csv $ResultsCsv $ResultColumns $res
    if ($status -eq "ok") { $script:Done[$row.key] = $row.fp }
}

# ---------------------------------------------------------------- running

# A reader's row is identified by the command that made its archive, so a
# finished row is skipped without making that archive again.
function New-Row([string]$op, $g, $cmd, [string]$inputFp) {
    $key = "{0}|{1}|{2}|{3}|t{4}|{5}|{6}" -f $op, $g.format, $g.corpus, $g.level, $g.threads, $cmd.tool, $g.input
    return @{ key = $key; fp = (Fingerprint $cmd $g.corpus $inputFp); op = $op; format = $g.format; corpus = $g.corpus
        level = $g.level; threads = $g.threads; input = $g.input; inputHash = ""; cmd = $cmd; samples = @(); failed = "" }
}

function Prepare-Row($row) {
    $cmd = $row.cmd
    if ($row.op -eq "create") { Prepare-Create $cmd }
    elseif ($row.op -eq "extract") {
        Remove-Safe $cmd.dest
        if ($cmd.mkdir) { New-Item -ItemType Directory -Force $cmd.dest | Out-Null }
        if ($cmd.link) { Link-Or-Copy $cmd.linkTarget $cmd.link }
    }
}

# Checks one extraction outside the timed part: in full against the corpus
# on the first run, by file count and bytes on the others.
function Check-Extract($row, [bool]$full) {
    $cmd = $row.cmd
    $info = $CorpusInfo[$row.corpus]
    if ($cmd.kind -eq "tar") {
        if (-not (Test-Path -LiteralPath $cmd.tarOut)) { return "no $($cmd.tarOut) after extraction" }
        if ($full) { return Diff-Check $row.corpus $cmd.tarOut }
        return ""
    }
    if ($full) { return Diff-Check $row.corpus $cmd.dest }
    $n = [ArcaCompare.Tree]::Count((Join-Path $cmd.dest $row.corpus))
    if ($n[0] -ne [long]$info.files -or $n[1] -ne [long]$info.bytes) { return "extracted $($n[0]) files, $($n[1]) bytes; expected $($info.files), $($info.bytes)" }
    return ""
}

function Run-Once($row, [int]$run, [int]$position, [bool]$record) {
    $cmd = $row.cmd
    Prepare-Row $row
    $r = Invoke-Tool $cmd.exe $cmd.args $cmd.cwd
    $note = ""
    $bytes = ""
    if ($r.TimedOut -or $r.ExitCode -ne 0) { $note = Short-Error $r }
    elseif ($row.op -eq "create") {
        if (Test-Path -LiteralPath $cmd.out) { $bytes = (Get-Item -LiteralPath $cmd.out).Length } else { $note = "no archive at $($cmd.out)" }
    } elseif ($row.op -eq "extract") {
        $note = Check-Extract $row ($row.samples.Count -eq 0)
        Remove-Safe $cmd.dest
    }
    if ($cmd.link -and $row.op -eq "create") { Finish-Create $cmd }
    $ok = ($note -eq "")
    if ($record) {
        Append-Csv $SamplesCsv $SampleColumns @{ key = $row.key; fingerprint = $row.fp; op = $row.op; format = $row.format; corpus = $row.corpus
            level = $row.level; threads = $row.threads; tool = $cmd.tool; input = $row.input; run = $run; position = $position; ok = $ok
            wall_s = F "{0:F4}" $r.Wall; user_s = F "{0:F4}" $r.User; kernel_s = F "{0:F4}" $r.Kernel; peak_ws_bytes = $r.PeakWorkingSet
            peak_private_bytes = $r.PeakPrivate; processes = $r.Processes; out_bytes = $bytes; exit = $r.ExitCode; note = $note
            time = (Get-Date).ToString("s", $Inv) }
    }
    if (-not $ok) { $row.failed = $note; Log "    $($Tools[$cmd.tool].name) FAILED: $note"; return }
    if ($record) {
        $row.samples += @{ wall = $r.Wall; user = $r.User; kernel = $r.Kernel; peak = [double]$r.PeakWorkingSet; private = [double]$r.PeakPrivate; procs = $r.Processes; bytes = $bytes }
        Log (F "    {0,-18} run {1}: {2,8:F3} s wall {3,8:F2} s cpu {4,6:N0} MiB {5}" $Tools[$cmd.tool].name $run $r.Wall ($r.User + $r.Kernel) ($r.PeakWorkingSet / 1MB) $bytes)
    }
}

# Checks a created archive with its own tool, a second tool and the corpus.
function Verify-Created($row) {
    $cmd = $row.cmd
    $file = $cmd.out
    $parts = @()
    $bad = $false
    $checks = @()
    $own = Test-Cmd $cmd.tool $row.format $file $AllThreads
    if ($own) { $checks += , @("own: $($Tools[$cmd.tool].short) test", $own) }
    $second = $null
    if ($cmd.tool -eq "arca") {
        foreach ($t in $SecondCheck[$row.format]) { $second = Test-Cmd $t $row.format $file $AllThreads; if ($second) { break } }
    } else { $second = Test-Cmd "arca" $row.format $file $AllThreads }
    if ($second) { $checks += , @("second: $($Tools[$second.tool].short) test", $second) }
    foreach ($c in $checks) {
        $r = Invoke-Tool $c[1].exe $c[1].args $c[1].cwd
        if ($r.ExitCode -eq 0 -and -not $r.TimedOut) { $parts += "$($c[0]) ok" } else { $parts += "$($c[0]) FAILED ($(Short-Error $r))"; $bad = $true }
    }
    if (-not $second) { $parts += "second: n/a (no other tool installed)" }
    $d = Diff-Check $row.corpus $file
    if ($d -eq "") { $parts += "content: arca diff ok" } else { $parts += "content: $d"; $bad = $true }
    return @{ text = ($parts -join "; "); bad = $bad }
}

function Invoke-Group($g, [int]$index, [int]$total) {
    $label = "{0} {1} {2}" -f $g.op, $FormatTitle[$g.format], $g.corpus
    if ($g.op -eq "create") { $label += " $($g.level)" } else { $label += " (archive by $($Tools[$g.input].name))" }
    $label += " threads=$($g.threads)"
    $inputFp = ""
    $keep = ""
    if ($g.op -ne "create") {
        $producerCmd = Create-Cmd $g.input $g.format $g.corpus "normal" $AllThreads (Out-Path $g.format $g.corpus $g.input "normal" $AllThreads)
        $inputFp = Fingerprint $producerCmd $g.corpus ""
        $keep = Join-Path $KeepDir "$($g.format)-$($g.corpus)-$($g.input)$($Ext[$g.format])"
    }
    $rows = @()
    foreach ($tool in $g.tools) {
        if (-not (Has $tool)) { continue }
        $cmd = $null
        if ($g.op -eq "create") { $cmd = Create-Cmd $tool $g.format $g.corpus $g.level $g.threads (Out-Path $g.format $g.corpus $tool $g.level $g.threads) }
        elseif ($g.op -eq "extract") { $cmd = Extract-Cmd $tool $g.format $keep $g.threads }
        else { $cmd = Test-Cmd $tool $g.format $keep $g.threads }
        if (-not $cmd) { continue }
        $row = New-Row $g.op $g $cmd $inputFp
        if ($Done.ContainsKey($row.key) -and $Done[$row.key] -eq $row.fp) { continue }
        $rows += $row
    }
    if ($rows.Count -eq 0) { Log "[$index/$total] $label already measured"; return }
    if ($g.op -ne "create") {
        $inputInfo = Ensure-Input $g.format $g.corpus $g.input
        if (-not $inputInfo) { Log "[$index/$total] $label skipped: its archive could not be made"; return }
        foreach ($row in $rows) { $row.inputHash = $inputInfo.sha256 }
    }
    $script:GroupRan = $true
    $load = Get-Load
    Log "[$index/$total] $label : $(($rows | ForEach-Object { $Tools[$_.cmd.tool].name }) -join ', ') (load $load%)"
    for ($w = 0; $w -lt $Warmup; $w++) { foreach ($row in $rows) { if (-not $row.failed) { Run-Once $row 0 0 $false } } }
    for ($i = 0; $i -lt $Runs; $i++) {
        $order = @($rows)
        if ($i % 2 -eq 1) { [array]::Reverse($order) }
        $pos = 0
        foreach ($row in $order) { $pos++; if (-not $row.failed) { Run-Once $row ($i + 1) $pos $true } }
    }
    foreach ($row in $rows) {
        if ($row.failed) { Write-Result $row "failed" $row.failed $load; continue }
        $verify = "every run checked against the corpus"
        if ($row.op -eq "create") {
            $v = Verify-Created $row
            $verify = $v.text
            if ($v.bad) { Write-Result $row "failed" $verify $load; Log "    $($Tools[$row.cmd.tool].name) verification FAILED: $verify"; Remove-Safe $row.cmd.out; continue }
            if ($g.level -eq "normal" -and $g.threads -eq $AllThreads -and (@($InputProducers[$g.format]) -contains $row.cmd.tool)) {
                $fp = Fingerprint $row.cmd $g.corpus ""
                [void](Keep-Input $g.format $g.corpus $row.cmd.tool $row.cmd $fp)
            } else { Remove-Safe $row.cmd.out }
        } elseif ($row.op -eq "extract") {
            if ($row.cmd.kind -eq "tar") { $verify = "first run's .tar compared with the corpus (arca diff); exit status on the others" }
            else { $verify = "first run compared with the corpus (arca diff); file count and bytes on the others" }
        } else { $verify = "exit status" }
        Write-Result $row "ok" $verify $load
    }
}

# ---------------------------------------------------------------- machine

function Get-Machine {
    $m = [ordered]@{}
    try {
        $cpu = @(Get-CimInstance Win32_Processor)
        $m.cpu = ($cpu | ForEach-Object { $_.Name.Trim() } | Select-Object -Unique) -join "; "
        $m.cores = ($cpu | Measure-Object -Property NumberOfCores -Sum).Sum
        $m.logical_processors = ($cpu | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum
        $m.max_clock_mhz = ($cpu | Measure-Object -Property MaxClockSpeed -Maximum).Maximum
    } catch { $m.cpu = "unknown ($($_.Exception.Message))" }
    try { $m.ram_gib = F "{0:F1}" ((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB) } catch { $m.ram_gib = "unknown" }
    try {
        $os = Get-CimInstance Win32_OperatingSystem
        $m.os = "$($os.Caption) $($os.Version) build $($os.BuildNumber)"
    } catch { $m.os = "unknown" }
    $drive = $Work.Substring(0, 1)
    try {
        $vol = Get-Volume -DriveLetter $drive
        $m.work_filesystem = $vol.FileSystem
    } catch { $m.work_filesystem = "unknown ($($_.Exception.Message))" }
    try {
        $disk = Get-Partition -DriveLetter $drive | Get-Disk
        $pd = Get-PhysicalDisk | Where-Object { "$($_.DeviceId)" -eq "$($disk.Number)" } | Select-Object -First 1
        if ($pd) { $m.work_disk = "$($pd.FriendlyName) ($($pd.MediaType), $($pd.BusType))" } else { $m.work_disk = "$($disk.FriendlyName)" }
    } catch { $m.work_disk = "unknown ($($_.Exception.Message))" }
    try {
        $mp = Get-MpComputerStatus
        $m.defender_realtime = "$($mp.RealTimeProtectionEnabled)"
        $m.defender_mode = "$($mp.AMRunningMode)"
    } catch { $m.defender_realtime = "unknown ($($_.Exception.Message))" }
    try { $m.power_plan = ((& powercfg.exe /getactivescheme) -join " ").Trim() } catch { $m.power_plan = "unknown" }
    try {
        $git = Find-Exe @() @("git.exe")
        $head = (Invoke-Tool $git "rev-parse HEAD" $Repo).StdOut.Trim()
        $dirty = (Invoke-Tool $git "status --porcelain --untracked-files=no" $Repo).StdOut.Trim()
        $m.arca_commit = $head
        if ($dirty) { $m.arca_commit += " (with uncommitted changes)" }
    } catch { $m.arca_commit = "unknown" }
    $m.powershell = "$($PSVersionTable.PSVersion)"
    return $m
}

function Test-Quiet([int]$seconds) {
    $before = @{}
    foreach ($p in (Get-Process)) { try { $before[$p.Id] = $p.TotalProcessorTime.TotalSeconds } catch {} }
    $samples = @()
    for ($i = 0; $i -lt $seconds; $i++) { $samples += Get-Load; Start-Sleep -Seconds 1 }
    $busy = @()
    foreach ($p in (Get-Process)) {
        try {
            $d = $p.TotalProcessorTime.TotalSeconds
            if ($before.ContainsKey($p.Id)) { $d -= $before[$p.Id] }
            if ($d -gt 0.2) { $busy += [pscustomobject]@{ name = $p.ProcessName; id = $p.Id; cpu = $d } }
        } catch {}
    }
    $avg = ($samples | Measure-Object -Average).Average
    $top = @($busy | Sort-Object cpu -Descending | Select-Object -First 5 | ForEach-Object { F "{0} ({1}) {2:F1} s" $_.name $_.id $_.cpu })
    return @{ avg = $avg; top = $top }
}

# ---------------------------------------------------------------- reports

$Rank = @{}
$i = 0; foreach ($x in @("create", "extract", "test")) { $Rank["op:$x"] = $i++ }
$i = 0; foreach ($x in @("zip", "7z", "txz", "rar")) { $Rank["format:$x"] = $i++ }
$i = 0; foreach ($x in $CorpusOrder) { $Rank["corpus:$x"] = $i++ }
$i = 0; foreach ($x in $LevelOrder) { $Rank["level:$x"] = $i++ }
$i = 0; foreach ($x in @("arca", "7zip", "nanazip", "winrar", "rar", "xz")) { $Rank["tool:$x"] = $i++ }

function Latest-Results {
    if (-not (Test-Path -LiteralPath $ResultsCsv)) { return @() }
    $last = [ordered]@{}
    foreach ($r in (Import-Csv -LiteralPath $ResultsCsv)) { $last[$r.key] = $r }
    $rows = @($last.Values)
    $rows = @($rows | Sort-Object { -[int]$_.threads }, { $Rank["format:" + $_.format] }, { $Rank["op:" + $_.op] }, { $Rank["corpus:" + $_.corpus] }, { $Rank["level:" + $_.level] }, { $Rank["tool:" + $_.input] }, { $Rank["tool:" + $_.tool] })
    return $rows
}

function Dash { return [string][char]0x2013 }

function Secs([string]$med, [string]$lo, [string]$hi, [int]$n) {
    $a = [double]::Parse($med, $Inv)
    $fmt = "{0:F3}"
    if ($a -ge 10) { $fmt = "{0:F2}" }
    $s = (F $fmt $a) + " s"
    if ($n -gt 1) { $s += " [" + (F $fmt ([double]::Parse($lo, $Inv))) + (Dash) + (F $fmt ([double]::Parse($hi, $Inv))) + "]" }
    return $s
}

function Show-Cmd($r) {
    $short = $Tools[$r.tool].short
    if (-not $short) { $short = $r.tool }
    return Redact "$short $($r.args)"
}

# The report is meant for the docs, so it names no user folder; results.csv
# and results.json keep the absolute paths.
function Redact([string]$s) {
    $s = $s -ireplace [regex]::Escape($Work), "<work>"
    $s = $s -ireplace [regex]::Escape($Repo), "<repo>"
    if ($env:USERPROFILE) { $s = $s -ireplace [regex]::Escape($env:USERPROFILE), "%USERPROFILE%" }
    return $s
}

function Level-Text([string]$tool, [string]$level) {
    $n = $LevelFlag[$tool][$level]
    switch ($tool) {
        "arca" { return "-l $n" }
        "xz" { return "-$n" }
        "winrar" { return "-m$n" }
        "rar" { return "-m$n" }
        default { return "-mx$n" }
    }
}


function Write-Reports {
    $rows = Latest-Results
    $json = [ordered]@{
        generated = (Get-Date).ToString("s", $Inv); quick = [bool]$Quick; runs = $Runs; warmup = $Warmup; threads = $AllThreads
        command = $ReproCommand; machine = $Machine; tools = @($Tools.Values); tool_notes = $ToolNotes
        corpora = @($CorpusInfo.Values); rows = $rows
    }
    [IO.File]::WriteAllText((Join-Path $ResDir "results.json"), ($json | ConvertTo-Json -Depth 8), $Utf8)

    $md = New-Object Text.StringBuilder
    $nl = "`n"
    [void]$md.Append("# Arca against other archivers$nl$nl")
    if ($Quick) { [void]$md.Append("> [!WARNING]$nl> Dry run (``-Quick``): tiny corpora and one run each. These numbers prove the script works and must not be published.$nl$nl") }
    [void]$md.Append("Generated $((Get-Date).ToString('yyyy-MM-dd HH:mm', $Inv)) by ``benchmarks/compare.ps1``. Reproduce with:$nl$nl``````powershell$nl")
    [void]$md.Append("cargo build --release -p arca-cli$nl$(Redact $ReproCommand)$nl``````$nl$nl")
    [void]$md.Append("## Machine$nl$nl| | |$nl| --- | --- |$nl")
    foreach ($k in $Machine.Keys) { [void]$md.Append("| $k | $($Machine[$k]) |$nl") }
    [void]$md.Append("| threads given to every tool | $AllThreads |$nl$nl")
    [void]$md.Append("## Tools$nl$nl| Tool | Version | Executable | SHA-256 |$nl| --- | --- | --- | --- |$nl")
    foreach ($t in $Tools.Values) {
        $h = $t.sha256
        if ($h.Length -ge 16) { $h = $h.Substring(0, 16) }
        [void]$md.Append("| $($t.name) | $($t.version) | ``$(Redact $t.exe)`` | ``$h`` |$nl")
    }
    foreach ($n in $ToolNotes) { [void]$md.Append("$nl- $(Redact $n)") }
    [void]$md.Append("$nl$nl## Corpora$nl$nl| Corpus | What | Files | Bytes | File list SHA-256 | Made with |$nl| --- | --- | --- | --- | --- | --- |$nl")
    foreach ($c in $CorpusInfo.Values) {
        [void]$md.Append("| ``$($c.name)`` | $($c.description) | $(F '{0:N0}' ([long]$c.files)) | $(F '{0:N0}' ([long]$c.bytes)) | ``$($c.listHash.Substring(0, 16))`` | ``$($c.command)`` |$nl")
    }
    [void]$md.Append("$nl## Method$nl$nl")
    [void]$md.Append("- Each group of tools runs interleaved, A B C then C B A, $Runs time(s), after $Warmup warmup run(s); every figure is the median with the range in brackets.$nl")
    [void]$md.Append("- Wall time includes process start. CPU is user plus kernel time of the whole process tree. Peak memory is the peak working set of the tool's process.$nl")
    [void]$md.Append("- Warm cache: every corpus is read in full before the first run. Outputs are deleted between runs, outside the timed part. Defender was left as reported above.$nl")
    [void]$md.Append("- Every archive is tested by its own tool and by a second one where installed, and compared with the corpus by ``arca diff``. Every extraction is compared with the corpus.$nl")
    [void]$md.Append("- Creation commands run in ``<work>\c\<corpus>``, on the folder named after the corpus. For .tar.xz, Arca reads the folder and writes the TAR itself, while xz and 7-Zip compress a TAR made beforehand, untimed, by ``tar -cf``; and only Arca extracts the files, the others write the ``.tar``.$nl")
    [void]$md.Append("- Arca's 7z (LZMA2) and RAR writers use every core their memory budget allows whatever ``-j`` says, and its 7z at ``best`` runs on one thread.$nl")
    $failed = @($rows | Where-Object { $_.status -ne "ok" })
    foreach ($thr in @($rows | ForEach-Object { [int]$_.threads } | Sort-Object -Unique -Descending)) {
        foreach ($fmt in @("zip", "7z", "txz", "rar")) {
            $set = @($rows | Where-Object { $_.format -eq $fmt -and [int]$_.threads -eq $thr })
            if ($set.Count -eq 0) { continue }
            $thrText = "$thr threads"
            if ($thr -eq 1) { $thrText = "1 thread" }
            [void]$md.Append("$nl## $($FormatTitle[$fmt]), $thrText$nl")
            foreach ($op in @("create", "extract", "test")) {
                $part = @($set | Where-Object { $_.op -eq $op })
                if ($part.Count -eq 0) { continue }
                $title = @{ create = "Creating"; extract = "Extracting"; test = "Testing" }[$op]
                [void]$md.Append("$nl### $title$nl$nl")
                if ($op -eq "create") { [void]$md.Append("| Corpus | Level | Tool | Wall | CPU | Peak memory | Size | Ratio | Command |$nl| --- | --- | --- | --- | --- | --- | --- | --- | --- |$nl") }
                else { [void]$md.Append("| Corpus | Archive by | Tool | Wall | CPU | Peak memory | Command |$nl| --- | --- | --- | --- | --- | --- | --- |$nl") }
                foreach ($r in $part) {
                    $tool = $r.tool_name
                    if ($thr -eq 1 -and $r.tool -eq "arca" -and $op -eq "create" -and ($fmt -eq "7z" -or $fmt -eq "rar")) { $tool += " (ignores -j)" }
                    if ($r.status -eq "ok") {
                        $wall = Secs $r.wall_median $r.wall_min $r.wall_max ([int]$r.runs)
                        $cpu = (F "{0:F2}" ([double]::Parse($r.cpu_median, $Inv))) + " s"
                        $mem = (F "{0:N0}" ([double]::Parse($r.peak_ws_mib_median, $Inv))) + " MiB"
                    } else { $wall = "failed"; $cpu = ""; $mem = "" }
                    $c = Show-Cmd $r
                    if ($op -eq "create") {
                        $size = ""; $ratio = ""
                        if ($r.out_bytes) { $size = F "{0:N0}" ([long]$r.out_bytes); $ratio = (F "{0:F2}" (100 * [double]::Parse($r.ratio, $Inv))) + "%" }
                        [void]$md.Append("| ``$($r.corpus)`` | $($r.level) | $tool | $wall | $cpu | $mem | $size | $ratio | ``$c`` |$nl")
                    } else {
                        $by = $Tools[$r.input].name
                        if (-not $by) { $by = $r.input }
                        $by += " " + (Level-Text $r.input "normal")
                        [void]$md.Append("| ``$($r.corpus)`` | $by | $tool | $wall | $cpu | $mem | ``$c`` |$nl")
                    }
                }
            }
        }
    }
    if ($failed.Count -gt 0) {
        [void]$md.Append("$nl## Failed rows$nl$nl")
        foreach ($r in $failed) { [void]$md.Append("- ``$($r.key)``: $(Redact $r.verify)$nl") }
    }
    [void]$md.Append("$nl## Working folders$nl$nl``<work>`` is the work folder (``-Work``, by default ``%TEMP%\arca-compare``) and ``<repo>`` this repository. Creation commands run in ``<work>\c\<corpus>`` (xz in ``<work>\out``, on a hard link to ``<work>\tar\<corpus>.tar``); readers of RAR and ZIP made by WinRAR, and xz, run in their output folder ``<work>\x\<tool>``; the rest in ``<work>``. results.csv and results.json hold every command with its absolute paths.$nl")
    [IO.File]::WriteAllText((Join-Path $ResDir "results.md"), $md.ToString(), $Utf8)
}

# ---------------------------------------------------------------- main

$parts = @("powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks\compare.ps1")
foreach ($k in $PSBoundParameters.Keys) {
    $v = $PSBoundParameters[$k]
    if ($v -is [System.Management.Automation.SwitchParameter]) { if ($v.IsPresent) { $parts += "-$k" } }
    elseif ($k -eq "Work") { $parts += "-Work <work>" }
    elseif ($v -is [array]) { $parts += "-$k $((Split-List $v) -join ',')" }
    else { $parts += "-$k `"$v`"" }
}
$ReproCommand = $parts -join " "

Log "benchmarks\compare.ps1 in $Work"
Log "command: $ReproCommand"

if ($Arca -eq "") { $Arca = Join-Path $Repo "target\release\arca.exe" }
if (-not (Test-Path -LiteralPath $Arca -PathType Leaf)) { throw "Arca not found at $Arca; build it with: cargo build --release -p arca-cli" }
$gitExe = Find-Exe @() @("git.exe")
$gitRoot = $null
if ($gitExe) { $gitRoot = Split-Path -Parent (Split-Path -Parent $gitExe) }
Add-Tool "arca" "Arca" "arca" ([IO.Path]::GetFullPath($Arca)) "" ""
Add-Tool "7zip" "7-Zip" "7z" (Find-Exe @($SevenZip, "$env:ProgramFiles\7-Zip\7z.exe", "${env:ProgramFiles(x86)}\7-Zip\7z.exe") @("7z.exe", "7zz.exe")) $SevenZip "7z.exe in Program Files or on PATH"
Add-Tool "nanazip" "NanaZip" "NanaZipC" (Find-Exe @($NanaZip, "$env:LOCALAPPDATA\Microsoft\WindowsApps\NanaZipC.exe") @("NanaZipC.exe")) $NanaZip "NanaZipC.exe app alias or on PATH"
Add-Tool "winrar" "WinRAR" "WinRAR" (Find-Exe @($WinRar, "$env:ProgramFiles\WinRAR\WinRAR.exe") @("WinRAR.exe")) $WinRar "WinRAR.exe in Program Files"
Add-Tool "rar" "WinRAR (Rar.exe)" "Rar" (Find-Exe @($Rar, "$env:ProgramFiles\WinRAR\Rar.exe") @("Rar.exe")) $Rar "Rar.exe in Program Files"
$xzCandidates = @($Xz)
if ($gitRoot) { $xzCandidates += @((Join-Path $gitRoot "mingw64\bin\xz.exe"), (Join-Path $gitRoot "usr\bin\xz.exe")) }
Add-Tool "xz" "xz" "xz" (Find-Exe $xzCandidates @("xz.exe")) $Xz "xz.exe on PATH or in Git for Windows"
foreach ($t in $Tools.Values) { Log "tool $($t.name): $($t.version) at $($t.exe)" }
foreach ($n in $ToolNotes) { Log "note: $n" }

$InputProducers = @{}
foreach ($f in $Formats) {
    $list = @("arca")
    foreach ($p in $RefProducer[$f]) { if (Has $p) { $list += $p; break } }
    $InputProducers[$f] = $list
}

$Machine = Get-Machine
foreach ($k in $Machine.Keys) { Log "machine $k : $($Machine[$k])" }

if (-not $Force) {
    Log "checking that the machine is quiet (10 s)"
    $q = Test-Quiet 10
    Log (F "average processor load {0:F0}%" $q.avg)
    if ($q.avg -gt 15) {
        Log "busiest processes: $($q.top -join ', ')"
        if (-not $Quick) { throw "the machine is busy (more than 15% load); close what is running or pass -Force" }
        Log "warning: busy machine; going on because this is a dry run"
    }
}

$needed = @()
foreach ($f in $Formats) { foreach ($c in (Corpora-For $f)) { if ($needed -notcontains $c) { $needed += $c } } }
$needed = @($CorpusOrder | Where-Object { $needed -contains $_ })
foreach ($c in $needed) { Build-Corpus $c }
[IO.File]::WriteAllText((Join-Path $ResDir "corpora.json"), (@($CorpusInfo.Values) | ConvertTo-Json -Depth 4), $Utf8)

$threadSets = @($AllThreads)
if ($SingleThread -and $AllThreads -ne 1) { $threadSets += 1 }
$groups = @()
foreach ($t in $threadSets) {
    foreach ($f in $Formats) {
        foreach ($c in (Corpora-For $f)) {
            foreach ($l in @($LevelOrder | Where-Object { $Levels -contains $_ })) {
                $groups += @{ op = "create"; format = $f; corpus = $c; level = $l; threads = $t; tools = $CreateTools[$f]; input = "" }
            }
            foreach ($p in $InputProducers[$f]) {
                $groups += @{ op = "extract"; format = $f; corpus = $c; level = "normal"; threads = $t; tools = $ReadTools[$f]; input = $p }
            }
            if ($t -ne 1) {
                foreach ($p in $InputProducers[$f]) {
                    $groups += @{ op = "test"; format = $f; corpus = $c; level = "normal"; threads = $t; tools = $ReadTools[$f]; input = $p }
                }
            }
        }
    }
}

# New corpus files keep the antivirus busy for a while, so the load gets
# up to five minutes to settle before the measuring starts.
if (-not $Force -and -not $Quick) {
    $q = Test-Quiet 10
    for ($try = 1; $q.avg -gt 15 -and $try -le 5; $try++) {
        Log (F "load is {0:F0}% ({1}); waiting 50 s for it to settle" $q.avg ($q.top -join ", "))
        Start-Sleep -Seconds 50
        $q = Test-Quiet 10
    }
    if ($q.avg -gt 15) { Log "busiest processes: $($q.top -join ', ')"; throw (F "the machine is busy ({0:F0}% load); rerun later or pass -Force; the corpora are kept" $q.avg) }
    Log (F "load {0:F0}%; measuring" $q.avg)
}

# A rough guess of each group's seconds, only to spread the time left over
# the groups in proportion: what the first groups took scales the rest.
$CostPerMiB = @{ "zip:fast" = 0.01; "zip:normal" = 0.02; "zip:best" = 0.06; "7z:fast" = 0.03; "7z:normal" = 0.3; "7z:best" = 1.2
    "txz:fast" = 0.03; "txz:normal" = 0.3; "txz:best" = 1.2; "rar:fast" = 0.2; "rar:normal" = 0.3; "rar:best" = 0.5 }
function Group-Cost($g) {
    $mib = [double]$CorpusInfo[$g.corpus].bytes / 1MB
    $files = [double]$CorpusInfo[$g.corpus].files
    $n = @($g.tools | Where-Object { Has $_ }).Count
    if ($g.op -eq "create") { $per = $mib * $CostPerMiB["$($g.format):$($g.level)"] + 3 }
    elseif ($g.op -eq "extract") { $per = $mib * 0.01 + $files * 0.0005 }
    else { $per = $mib * 0.005 }
    return $n * ($Runs + $Warmup) * $per + 2
}
$costs = @($groups | ForEach-Object { Group-Cost $_ })
$costLeft = ($costs | Measure-Object -Sum).Sum
$costDone = 0.0
$measured = 0.0
for ($gi = 0; $gi -lt $groups.Count; $gi++) {
    $t0 = Get-Date
    $ran = $false
    $script:GroupRan = $false
    try { Invoke-Group $groups[$gi] ($gi + 1) $groups.Count; $ran = $script:GroupRan }
    catch { Log "group $($gi + 1) stopped by an error: $($_.Exception.Message)"; $ran = $true }
    try { Write-Reports } catch { Log "could not write the reports: $($_.Exception.Message)" }
    $costLeft -= $costs[$gi]
    if ($ran) {
        $costDone += $costs[$gi]
        $measured += ((Get-Date) - $t0).TotalMinutes
        Log (F "  {0:F1} min measuring so far, roughly {1:F0} min left" $measured ($measured / $costDone * $costLeft))
    }
}

$all = Latest-Results
$bad = @($all | Where-Object { $_.status -ne "ok" })
Log "finished: $($all.Count) rows, $($bad.Count) failed, $(F '{0:F1}' ((Get-Date) - $Started).TotalMinutes) min in all"
foreach ($d in @($OutDir, $XDir, $KeepDir, $TarDir)) { Remove-Safe $d }
if (-not $KeepCorpora) { Remove-Safe $CorpusRoot; Log "removed the corpora; pass -KeepCorpora to keep them" }
Log "results: $(Join-Path $ResDir 'results.md'), results.csv, results.json, samples.csv"
