# Times RAR 5 creation by Arca and by WinRAR's console Rar.exe on the corpus
# that `cargo run --release -p arca-rar --example rar-corpus -- <dir>` writes.
#
#   cargo build --release -p arca-cli
#   cargo run --release -p arca-rar --example rar-corpus -- $env:TEMP\rar-corpus
#   mkdir $env:TEMP\rar-corpus\repo
#   git archive --format=tar 4b1d67b | tar -x -C $env:TEMP\rar-corpus\repo
#   powershell -ExecutionPolicy Bypass -File crates\arca-rar\tests\rar-bench.ps1 `
#       -Corpus $env:TEMP\rar-corpus -Runs 5
#
# The generated kinds are synthetic. `repo` is real data: this repository at a
# pinned commit, 1,142 files of source, documentation, test fixtures and icons.
# -Kinds picks subdirectories, comma separated; "all" times the whole tree as
# one archive.
#
# Each Arca level is paired with the WinRAR method it stands in for: Arca 1, 3,
# 6, 7 and 9 are RAR 5 levels 1 to 5, as Rar.exe -m1 to -m5 are. Every run
# records wall time, the CPU time of the process and all its threads, the
# archive size, and the machine's processor load sampled just before it starts,
# since a busy machine moves wall time far more than CPU time or size. Every
# archive Arca writes is checked with `Rar.exe t`. Results go to a CSV and a
# summary of medians and ranges is printed at the end.
param(
    [Parameter(Mandatory = $true)][string]$Corpus,
    [string]$Arca = "target\release\arca.exe",
    [string]$Rar = "C:\Program Files\WinRAR\Rar.exe",
    [string]$Levels = "1,2,3,4,5",
    [string]$Kinds = "all",
    [string]$Tools = "arca,rar",
    [int]$Runs = 3,
    [string]$RarExtra = "",
    [string]$Work = (Join-Path $env:TEMP "rar-bench"),
    [string]$Csv = ""
)

$ErrorActionPreference = "Stop"
$levelList = $Levels -split "," | ForEach-Object { [int]$_ }
$kindList = $Kinds -split ","
$toolList = $Tools -split ","
$arcaLevel = @{ 1 = 1; 2 = 3; 3 = 6; 4 = 7; 5 = 9 }
New-Item -ItemType Directory -Force $Work | Out-Null
if ($Csv -eq "") { $Csv = Join-Path $Work "results.csv" }
$Arca = (Resolve-Path $Arca).Path
"tool,level,kind,run,wall_s,cpu_s,bytes,load_pct" | Out-File -Encoding ascii $Csv

function Invoke-Timed([string]$exe, [string]$arguments) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $exe
    $info.Arguments = $arguments
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $process = [System.Diagnostics.Process]::Start($info)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $clock.Stop()
    $cpu = $process.TotalProcessorTime.TotalSeconds
    if ($process.ExitCode -ne 0) {
        throw "$exe $arguments exited with $($process.ExitCode): $($stderr.Result) $($stdout.Result)"
    }
    return @{ Wall = $clock.Elapsed.TotalSeconds; Cpu = $cpu }
}

function Get-Load {
    (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average
}

$rows = @()
foreach ($kind in $kindList) {
    $source = if ($kind -eq "all") { $Corpus } else { Join-Path $Corpus $kind }
    $label = $kind
    foreach ($level in $levelList) {
        foreach ($tool in $toolList) {
            for ($run = 1; $run -le $Runs; $run++) {
                $out = Join-Path $Work "$tool-$label-$level.rar"
                Remove-Item -Force -ErrorAction SilentlyContinue $out
                $load = Get-Load
                if ($tool -eq "arca") {
                    $t = Invoke-Timed $Arca "create `"$out`" `"$source`" -l $($arcaLevel[$level])"
                } else {
                    $t = Invoke-Timed $Rar "a -m$level -ep1 -r -idq $RarExtra `"$out`" `"$source`""
                }
                $bytes = (Get-Item $out).Length
                if ($tool -eq "arca" -and $run -eq 1) {
                    Invoke-Timed $Rar "t -idq `"$out`"" | Out-Null
                }
                $row = [pscustomobject]@{
                    tool = $tool; level = $level; kind = $label; run = $run
                    wall = $t.Wall; cpu = $t.Cpu; bytes = $bytes; load = $load
                }
                $rows += $row
                ("{0},{1},{2},{3},{4:F3},{5:F3},{6},{7}" -f $tool, $level, $label, $run,
                    $t.Wall, $t.Cpu, $bytes, $load) | Out-File -Append -Encoding ascii $Csv
                Write-Host ("{0,-4} m{1} {2,-7} run {3}: {4,8:F2} s wall {5,8:F2} s cpu {6,12} bytes (load {7}%)" -f `
                        $tool, $level, $label, $run, $t.Wall, $t.Cpu, $bytes, $load)
            }
        }
    }
}

function Get-Median([double[]]$values) {
    $sorted = $values | Sort-Object
    $n = $sorted.Count
    if ($n % 2 -eq 1) { return $sorted[($n - 1) / 2] }
    return ($sorted[$n / 2 - 1] + $sorted[$n / 2]) / 2
}

Write-Host ""
Write-Host "tool level kind      wall median (range)        cpu median (range)         bytes     load"
$rows | Group-Object tool, level, kind | ForEach-Object {
    $g = $_.Group
    $w = [double[]]($g | ForEach-Object { $_.wall })
    $c = [double[]]($g | ForEach-Object { $_.cpu })
    $l = [double[]]($g | ForEach-Object { $_.load })
    Write-Host ("{0,-4} m{1}    {2,-7} {3,7:F2} ({4:F2}-{5:F2})  {6,8:F2} ({7:F2}-{8:F2})  {9,12}  {10,3:F0}%" -f `
            $g[0].tool, $g[0].level, $g[0].kind, (Get-Median $w), ($w | Measure-Object -Minimum).Minimum,
        ($w | Measure-Object -Maximum).Maximum, (Get-Median $c), ($c | Measure-Object -Minimum).Minimum,
        ($c | Measure-Object -Maximum).Maximum, $g[0].bytes, (Get-Median $l))
}
Write-Host "results: $Csv"
