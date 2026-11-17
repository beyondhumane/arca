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

Both target Windows 11. They offer **Open with Arca**, **Extract here** and
**Extract to "name\"** for every format Arca reads, ZIP containers such as
`.jar` or `.epub` and RAR volumes such as `.r00` included, and name the
folder exactly as the window will. **Add to archive…** works on any selection.

## Progress, not silence

Right-clicking an archive opens the Arca window with a progress bar rather than running the command line with no console. An extraction that fails, or that finds a file already there, says so instead of doing nothing.

## Installer

The installer is produced with Inno Setup from `windows/arca.iss`. Download `arca-setup-<version>-x86_64.exe` from the [latest release](https://github.com/beyondhumane/arca/releases/latest), or read [windows/README.md](https://github.com/beyondhumane/arca/tree/main/windows) for building it yourself.

### File associations

With the associations option on, the installer adds Arca to **Open with** and
to **Settings > Default apps** for these extensions:

| Extensions | Formats |
| --- | --- |
| `.zip`, `.7z`, `.rar`, `.tar` | ZIP, 7z, RAR, TAR |
| `.gz`, `.tgz` | TAR.GZ |
| `.xz`, `.txz` | XZ and TAR.XZ |
| `.bz2`, `.tbz2`, `.tbz` | BZ2 and TAR.BZ2 |
| `.zst`, `.tzst` | ZST and TAR.ZST |
| `.iso` | ISO disc images, only on request |

Windows matches on the last extension, so `backup.tar.xz` is an `.xz` file to
it; Arca still reads the whole name and opens it as TAR.XZ.

Arca never makes itself the default: it registers next to whatever already
opens those files, and you choose it in **Open with** or in Settings. The new
installer, `arca-setup`, makes one exception: it takes `.zip` for your user
account so that **New > Arca ZIP archive** appears in Explorer, and hands it
back to whatever had it before when Arca is uninstalled.

Disc images are off by default, because Explorer mounts them itself. Tick
**Disc images (.iso) too** in the Inno Setup installer, start the new
installer built by `windows/build-setup.ps1` with `--iso`, or run
`windows\install.ps1 -Iso` for the development install.

ZIP-based packages and documents (`.jar`, `.apk`, `.epub`, `.cbz` and the rest
of [ZIP containers](zip-containers.md)) and `.cbr` comics are never
associated: they belong to Java, Android tools, e-book and comic readers. Arca opens
them when you ask it to, through the context menu or the window.

The list comes from `arca_core::Format`. `arca-setup` derives it at run time,
and its tests fail when `windows/arca.iss`,
`windows/install.ps1` or the Explorer extension in `windows/arca-shell`
disagree with it.

## Why it lives outside the workspace

`windows/arca-shell` needs COM, and therefore `unsafe`, and only compiles on Windows. It’s excluded from the Cargo workspace so that `cargo build` keeps working on Linux and macOS.
