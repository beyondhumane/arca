---
description: A file explorer and archiver written in safe Rust, with a desktop window, a command line and one shared core.
group: Getting started
order: 1
keywords: overview what why status about
---

# Introduction

Arca is a file explorer and an archiver in one, written in Rust. Its desktop window browses your disk like any explorer and steps into ZIP, 7z, TAR, RAR and ISO archives as if they were folders; it creates ZIP, 7z, TAR and RAR archives and XZ streams, compresses with Deflate or Zstandard across every core of your machine, and encrypts with AES-256. The same core runs as a single command-line binary and as the Windows Explorer context menu.

## Why another archiver?

Archive parsers are a classic attack surface, and most of the ones in daily use are written in memory-unsafe languages. Arca’s container parsers are written in safe Rust with `#![forbid(unsafe_code)]` at crate level: a malformed archive produces an error, never memory corruption.

It is also fast. On the machine it was measured on, Arca with Zstandard compressed the Silesia corpus 13.2× faster than `zip` and 18.1× faster than 7-Zip, on the same two cores. Where it doesn’t win, the [benchmarks](benchmarks.md) say so.

## At a glance

- **Formats:** ZIP with Zip64 (Store/Deflate/Zstandard), 7z (Store/LZMA2 creation and solid reading), ustar TAR, `.tar.gz`, `.tar.xz` and standalone `.xz`. [ISO 9660 images](iso.md) are read, never written.
- **Speed:** multi-threaded compression, parallel extraction of `.zip`, and a sub-millisecond cold start.
- **Safety:** unsafe-free parsers, bounded header reads and a Zip Slip defence on every entry name.
- **Encryption:** ZIP WinZip AE-2; 7z AES-256 with optional encrypted headers. See [integrity limits](encryption.md).
- **Desktop:** `arca-gui`, a file explorer with panes or details, per-pane filters, a preview with syntax highlighting, hex and images, devices with mount, format and label, and archives that open in place. See [the desktop workspace](desktop-app.md).
- **Interfaces:** the `arca` command line, the `arca-gui` window and a Windows 11 Explorer menu.
- **License:** Apache-2.0, free for personal and commercial use.

## Project status

> [!NOTE]
> Arca covers phase F01 and part of F03 of its design (the core, ZIP and TAR, Zstandard, multi-threaded compression and a command line), plus 7z with encrypted headers, AES-256 encryption, the desktop window and Windows Explorer integration. The [roadmap](roadmap.md) lists what isn’t there yet.

## Next steps

- [Installation](installation.md): download a release or build from source.
- [Quick start](quick-start.md): create, list, test and extract in four commands.
- [CLI reference](cli-reference.md): every command, argument and default.
- [Encryption](encryption.md): how AES-256 works in Arca, and its limits.
