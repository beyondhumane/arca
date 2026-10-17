# Custom installer (`arca-setup`)

Status: **stages 1 and 2 written, stage 2 half tested.** The new installer exists and can be built with `windows\build-setup.ps1`, but releases keep publishing the Inno Setup one (`windows/arca.iss`) until stage 4 replaces it.

## What exists today

`arca-setup` is a workspace crate with a 720 × 420 egui window (eframe over OpenGL) that follows the "Monolito fluido" proposal: welcome, customize, installing, done and error, in Spanish and English depending on the system language, with Sora and Inter embedded and all artwork in vector form. Behind it sits an engine that ports what `arca.iss` did.

| Screen | What it does |
| --- | --- |
| Welcome | Horizontal logo, "Install ARCA" and "Customize" (or the confirmation, when uninstalling) |
| Customize | Three toggles (context menu, associations, PATH) and the install folder |
| Installing | Bar, percentage and four steps with their state |
| Done | "Start using ARCA" and a link to the release notes |
| Error | The reason and "Retry" |

The engine, in `arca-setup/src/engine`, works on Windows, per user, without administrator rights:

- Extracts the files from a ZIP embedded in the executable, using `arca-zip`. If a file is in use (Arca open, the Explorer DLL loaded) it moves it aside as `name.old-N` and writes the new one; the next sweep deletes the moved files.
- Writes the HKCU registry: classic menu, associations, capabilities, PATH and the Installed apps entry. It uses Inno's key, so it adopts a previous installation instead of duplicating it.
- Registers the modern menu with `Add-AppxPackage`, creates the Start menu shortcut and notifies the system of the PATH and association changes (`arca-notify`).
- Copies itself as `unins000.exe`. `--uninstall` relaunches from `%TEMP%` so it can delete its own folder, and deletes its copy when done.
- On update and in silent mode it keeps whatever the user had enabled, instead of ticking everything again.

### What is tested and what is not

Tested without touching the registry (`--files-only`, with a temporary folder): install, update with a locked file, the sweep, uninstall without leaving a copy in `%TEMP%`, and a failure that returns exit code 1 and is logged in `%TEMP%\arca-setup.log`. The pure functions (PATH, PowerShell base64, manifest, command-line options, reading the embedded ZIP with truncated files) have unit tests.

**Not tested end to end**: the real HKCU writes, the MSIX package registration, the Explorer restart and the silent update launched from Arca itself. The pending cycle is to install and uninstall, comparing the registry and PATH before and after.

### Command-line options

The Inno options the updater uses (`/VERYSILENT`, `/SILENT`, `/update=1`, `/NOCANCEL`, `/NORESTART`, `/NORESTARTAPPLICATIONS`), plus `/DIR=`, `--uninstall`, `--lang=es|en` and `--files-only`, which only copies files and is meant for testing. With `--preview`, `--screen=welcome|options|installing|done|failed` or `--percent=N` the window simulates progress without installing anything:

```sh
cargo run -p arca-setup -- --screen=installing --percent=68 --lang=es
```

### How to build it

```powershell
.\windows\build-setup.ps1        # release, with LTO
.\windows\build-setup.ps1 -Fast  # without LTO, for testing
```

It produces `dist\arca-setup-<version>-x86_64.exe`. A plain `cargo build` builds an empty installer on purpose: the ZIP with the binaries comes in through the `ARCA_PAYLOAD` variable.

## Why not stay with Inno

The Inno Setup wizard cannot draw these screens: vector background, steps with state, custom typography and clean scaling at any DPI. The header of `windows/arca.iss` warned that writing it by hand in Rust meant redoing, worse, what Inno already does well. That is still true for the part nobody sees, which is why stage 2 is the delicate one.

## Contract the new installer must honour

Arca updates itself and takes the installer's shape for granted. Changing it without honouring these five points breaks updates for everyone who already has it installed.

1. **File name**: `arca-setup-<version>-x86_64.exe`, uploaded to the `github.com/beyondhumane/arca` release. The new-version notice looks for that pattern (`arca-gui/src/controller/actions.rs`) and checks the sum in `SHA256SUMS.txt`.
2. **Arguments**: `/VERYSILENT /NOCANCEL /NORESTART /NORESTARTAPPLICATIONS /update=1` (`install_update` in `arca-gui/src/archive_ops/mod.rs`). Silent mode opens no window.
3. **Flicker-free update**: with `/update=1` Explorer is not restarted. The extension DLL is replaced on the next start and the executables are copied regardless.
4. **Reopening Arca**: with `/update=1` the installer itself launches `arca-gui.exe` when done; Arca closes itself so it can be replaced.
5. **`unins000.exe` next to `arca-gui.exe`**: `installed_by_setup()` uses it to know whether the copy can update itself. Either the new uninstaller keeps that name, or that check changes in the same commit.

It must also **adopt an existing Inno installation**: same `AppId` (`{7C4E0E4A-6C0D-4C21-9E0B-2B5D0F1A9C77}`), same `%LOCALAPPDATA%\Programs\Arca` folder and the uninstall key Inno left, so an update does not leave two entries in Installed apps.

## What `arca.iss` does and must be ported

| Function | Where it lives in `arca.iss` |
| --- | --- |
| Copy `arca.exe`, `arca-gui.exe`, `arca_shell.dll`, manifest, license and `Assets` | `[Files]` |
| Per-user install, no administrator, no UAC | `PrivilegesRequired=lowest` |
| Classic context menu (CLSID in HKCU and two handlers) | `[Registry]`, `shellmenu` task |
| Windows 11 modern menu (sparse MSIX package) | `RegisterModernMenu`, `StampManifestVersion` |
| `.zip`, `.tar`, `.gz`, `.tgz` associations with ProgID, `OpenWithProgids` and `Capabilities` | `[Registry]`, `fileassoc` task |
| Add `arca` to and remove it from the user PATH | `AddToPath`, `RemoveFromPath` |
| Restart Explorer to release `arca_shell.dll` | `RestartExplorer`, `QuietUpgrade` |
| Installed apps entry and uninstaller | `Uninstall*` |
| Delete any `UserChoice` pointing to Arca on uninstall | `ForgetDefaults` |
| Start menu shortcut | `[Icons]` |

## Stages

- [x] **1. Design and flow.** Window, brand, backgrounds, typography, languages, keyboard. No effects on the system.
- [x] **2. Engine.** `Choices` go in, progress events come out; the payload travels as a ZIP inside the executable and is extracted with `arca-zip`. Written and tested with `--files-only`.
- [ ] **3. Test it for real.** Full install and uninstall cycle comparing HKCU and PATH before and after, the silent update Arca launches over an Inno installation, and the modern menu with developer mode enabled.
- [ ] **4. Release.** With stage 3 done, replace the Inno step in `.github/workflows/release.yml`, check `--version` of the silent installer and test a real update from a previous version.
- [ ] **5. Retire `arca.iss`** once stage 4 has shipped in a release without incidents.

## Decisions already made

- **egui, not another stack.** It is the one `arca-gui` uses since the move back from GPUI (see `migration-to-egui.md`); both crates share eframe and the CI cache.
- **SVG through `egui_extras` (`resvg`).** The symbol, the logo and the backgrounds are vector. Each SVG carries an intrinsic size of about 1.5 times its on-screen size, so the texture `egui_extras` rasterizes is scaled down rather than up and the edges stay smooth.
- **Backgrounds without blur filters.** Each `feGaussianBlur` forced a whole canvas to be rasterized, and the screen took seconds to paint the background.
- **Sora and Inter embedded.** Sora SemiBold (headings, buttons, emphasis) and Inter Regular (body), both under the OFL, which ships in `arca-setup/assets/fonts`. If they fail to load, the installer falls back to the system font.
- **The tagline is outlined**, extracted from Sora's glyphs, so the logo looks the same without the font installed.
