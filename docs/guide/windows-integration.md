---
description: The File Explorer context menu and the Windows installer.
group: Desktop
order: 10
keywords: windows explorer context menu right click installer inno setup msix shell
---

# Windows integration

On Windows, Arca plugs into File Explorer so you can open and extract archives without touching a terminal.

## Explorer context menu

`windows/` holds the Explorer extension, in both flavours:

| Menu | Built on |
| --- | --- |
| Modern Windows 11 menu | `IExplorerCommand` plus a sparse MSIX package |
| Classic menu (Show more options) | `IContextMenu` plus registry keys |

Both target Windows 11. The `.7z` integration needs Windows runtime verification
separately from Linux shell checks.

## Progress, not silence

Right-clicking an archive opens the Arca window with a progress bar rather than running the command line with no console. An extraction that fails, or that finds a file already there, says so instead of doing nothing.

## Installer

The installer is produced with Inno Setup from `windows/arca.iss`. Download `arca-setup-<version>-x86_64.exe` from the [latest release](https://github.com/beyondhumane/arca/releases/latest), or read [windows/README.md](https://github.com/beyondhumane/arca/tree/main/windows) for building it yourself.

The installer registers `.7z` alongside ZIP/TAR/gzip for Open With and default-app
selection. Explorer recognizes `.7z` for Open/Extract and strips its extension
for the destination folder. Add to archive creates a new archive containing the
selection; it does not mutate the selected 7z. The creation dialog offers 7z.

## Why it lives outside the workspace

`windows/arca-shell` needs COM, and therefore `unsafe`, and only compiles on Windows. It’s excluded from the Cargo workspace so that `cargo build` keeps working on Linux and macOS.
