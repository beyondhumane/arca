# Times one command line for several builds of a program on Windows, in
# A B B A order after one warmup each, and prints for every build the median
# and range of the wall time, the process CPU time (user + kernel, which
# Windows keeps per process) and the peak working set.
#
#   powershell -File benchmarks\time.ps1 -Exes old.exe,new.exe `
#     -ArgLine "create C:\b\out.zip C:\b\corpus" -Runs 10 -Clean C:\b\out.zip
#
# -Clean is deleted before every run, outside the timed part. Nothing here
# turns off antivirus or flushes caches: runs are warm, and the machine's
# state, Defender included, should be reported with the numbers.
param(
  [Parameter(Mandatory = $true)][string[]]$Exes,
  [Parameter(Mandatory = $true)][string]$ArgLine,
  [int]$Runs = 10,
  [string]$Clean = ""
)
$ErrorActionPreference = "Stop"
# powershell -File passes "a.exe,b.exe" as one string, not as an array.
$Exes = @($Exes | ForEach-Object { $_ -split ',' } | Where-Object { $_ -ne "" })
$res = @{}
foreach ($e in $Exes) { $res[$e] = @{ w = @(); u = @(); k = @(); m = @() } }

function Once($exe) {
  if ($Clean -ne "" -and (Test-Path $Clean)) {
    if ((Get-Item $Clean) -is [System.IO.DirectoryInfo]) { cmd /c rd /s /q "$Clean" } else { Remove-Item -Force $Clean }
  }
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exe; $psi.Arguments = $ArgLine; $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $p = [System.Diagnostics.Process]::Start($psi)
  $stdout = $p.StandardOutput.ReadToEndAsync(); $stderr = $p.StandardError.ReadToEndAsync()
  $peak = 0
  while (-not $p.HasExited) {
    try { $p.Refresh(); $peak = [math]::Max($peak, $p.PeakWorkingSet64) } catch {}
    Start-Sleep -Milliseconds 20
  }
  $p.WaitForExit(); $sw.Stop()
  if ($p.ExitCode -ne 0) { throw "$exe failed: $($stderr.Result)" }
  return @($sw.Elapsed.TotalSeconds, $p.UserProcessorTime.TotalSeconds, $p.PrivilegedProcessorTime.TotalSeconds, $peak)
}

function Median($a) { $s = @($a | Sort-Object); $s[[int][math]::Floor($s.Count / 2)] }

foreach ($e in $Exes) { $null = Once $e }
for ($i = 0; $i -lt $Runs; $i++) {
  $order = $Exes.Clone()
  if ($i % 2 -eq 1) { [array]::Reverse($order) }
  foreach ($e in $order) {
    $r = Once $e
    $res[$e].w += $r[0]; $res[$e].u += $r[1]; $res[$e].k += $r[2]; $res[$e].m += $r[3]
  }
}
foreach ($e in $Exes) {
  $w = @($res[$e].w | Sort-Object)
  $c = @(); for ($j = 0; $j -lt $Runs; $j++) { $c += $res[$e].u[$j] + $res[$e].k[$j] }
  $cs = @($c | Sort-Object)
  "{0}: wall median {1:N3} s [{2:N3}-{3:N3}]; CPU median {4:N2} s [{5:N2}-{6:N2}] (user {7:N2}, kernel {8:N2}); peak working set {9:N0} MiB; n={10}" -f `
    (Split-Path $e -Leaf), (Median $w), $w[0], $w[-1], (Median $c), $cs[0], $cs[-1], `
    (Median $res[$e].u), (Median $res[$e].k), ((Median $res[$e].m) / 1MB), $Runs
}
