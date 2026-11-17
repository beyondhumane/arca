# Rebuilds mszip.cab and lzx.cab with Windows' own makecab. The tests check
# their contents against the same text built in Rust (big_text in
# src/tests.rs), so the two must stay in step. Every other fixture of this
# crate is built by the tests themselves.
#
# Usage, from this folder:  powershell -File generate.ps1

$ErrorActionPreference = "Stop"
$work = Join-Path ([IO.Path]::GetTempPath()) "arca-extra-cab"
New-Item -ItemType Directory -Force $work | Out-Null
[IO.File]::WriteAllText("$work\a.txt", "hello from a cabinet`n")
$text = New-Object System.Text.StringBuilder
for ($i = 0; $i -lt 2500; $i++) {
    [void]$text.Append("line $i of the arca cabinet fixture: " + "abcdefghij".Substring($i % 10) + "`n")
}
[IO.File]::WriteAllText("$work\big.txt", $text.ToString())

foreach ($kind in @("MSZIP", "LZX")) {
    $name = $kind.ToLower()
    # makecab picks the LZX window from CompressionMemory: 16 is 64 KiB.
    $memory = if ($kind -eq "LZX") { ".Set CompressionMemory=16`r`n" } else { "" }
    $ddf = ".Set CabinetNameTemplate=$name.cab`r`n" +
        ".Set DiskDirectoryTemplate=$PSScriptRoot`r`n" +
        ".Set CompressionType=$kind`r`n" + $memory +
        ".Set Cabinet=on`r`n.Set Compress=on`r`n.Set MaxDiskSize=0`r`n" +
        ".Set RptFileName=nul`r`n.Set InfFileName=nul`r`n" +
        "`"$work\a.txt`" `"a.txt`"`r`n" +
        "`"$work\big.txt`" `"docs\big.txt`"`r`n"
    [IO.File]::WriteAllText("$work\$name.ddf", $ddf)
    & makecab.exe /F "$work\$name.ddf" | Out-Null
}
Remove-Item -Recurse -Force $work
