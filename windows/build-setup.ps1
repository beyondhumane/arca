# Builds the Arca installer: one arca-setup-<version>-x86_64.exe with everything
# it installs inside it.
#
# Requirements: the same as build.ps1 (Rust, the MSVC linker, the Windows SDK).
#
# Usage:  .\build-setup.ps1          release build, the one to hand out
#         .\build-setup.ps1 -Fast    no LTO, several times quicker, for trying it out
#
# The result lands in ..\dist. It has no code signature, so SmartScreen will
# warn the first time it runs.

param([switch]$Fast)

$ErrorActionPreference = "Stop"
$Root  = Split-Path -Parent $PSScriptRoot
$Stage = Join-Path $PSScriptRoot "salida-setup"
$Dist  = Join-Path $Root "dist"

# cargo writes its progress to stderr even when everything is fine, and with
# ErrorActionPreference = Stop that would abort the script. The exit code is the
# only thing that means a real failure.
function Invoke-Tool($program, $arguments) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $program @arguments
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($code -ne 0) {
        throw "$program $($arguments -join ' ') failed with code $code"
    }
}

if ($Fast) {
    $env:CARGO_PROFILE_RELEASE_LTO = "off"
    $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "16"
}

$Version = ((Get-Content (Join-Path $Root "Cargo.toml") |
    Where-Object { $_ -match '^version' } | Select-Object -First 1) -split '"')[1]

Write-Host "==> Building arca.exe and arca-gui.exe" -ForegroundColor Cyan
Push-Location $Root
Invoke-Tool "cargo" @("build", "--release", "-p", "arca-cli", "-p", "arca-gui")
Pop-Location

Write-Host "==> Building arca_shell.dll" -ForegroundColor Cyan
Invoke-Tool "cargo" @("build", "--release", "--manifest-path", (Join-Path $PSScriptRoot "arca-shell\Cargo.toml"))

Write-Host "==> Gathering the files" -ForegroundColor Cyan
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage, "$Stage\Assets" | Out-Null
Copy-Item "$Root\target\release\arca.exe" $Stage
Copy-Item "$Root\target\release\arca-gui.exe" $Stage
Copy-Item "$PSScriptRoot\arca-shell\target\release\arca_shell.dll" $Stage
Copy-Item "$PSScriptRoot\AppxManifest.xml" $Stage
Copy-Item "$Root\LICENSE" $Stage
Copy-Item "$Root\README.md" $Stage
Copy-Item "$PSScriptRoot\assets\*.png" "$Stage\Assets"

# Arca packs its own installer: the reader that opens it is the same one that
# opens every other zip.
Write-Host "==> Packing them with arca" -ForegroundColor Cyan
$Payload = Join-Path $PSScriptRoot "salida-setup.zip"
if (Test-Path $Payload) { Remove-Item -Force $Payload }
Push-Location $Stage
Invoke-Tool "$Root\target\release\arca.exe" @("create", $Payload, "arca.exe", "arca-gui.exe", "arca_shell.dll", "AppxManifest.xml", "LICENSE", "README.md", "Assets")
Pop-Location

Write-Host "==> Building the installer" -ForegroundColor Cyan
$env:ARCA_PAYLOAD = $Payload
Push-Location $Root
Invoke-Tool "cargo" @("build", "--release", "-p", "arca-setup")
Pop-Location

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Out = Join-Path $Dist "arca-setup-$Version-x86_64.exe"
Copy-Item "$Root\target\release\arca-setup.exe" $Out -Force
$Hash = (Get-FileHash $Out -Algorithm SHA256).Hash.ToLower()

Write-Host ""
Write-Host "Done: $Out" -ForegroundColor Green
Write-Host ("Size:   {0:N1} MB" -f ((Get-Item $Out).Length / 1MB))
Write-Host "SHA256: $Hash"
