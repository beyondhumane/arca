---
description: What isn’t there yet, and why some of it matters.
group: Reference
order: 15
keywords: roadmap future 7z lzma xz solid symlinks udf planned
---

# Roadmap

What Arca doesn’t do yet and, where it matters, why.

## Available

7z listing/extraction (including solid and encrypted headers), Store/LZMA2
creation, passwords and hidden names are implemented for CLI and desktop. See
[issue #2](https://github.com/beyondhumane/arca/issues/2) and the
[accepted design](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md).

[RAR/CBR reading](rar.md) is enabled by default, including modern and legacy
multivolume sets, passwords, previews and verified extraction. New single-volume
RAR5 archives can be created at four levels. Existing RAR/CBR archives remain
read-only: mutation, recovery/repair, encrypted, solid or multivolume output and
CBR creation are not supported.

Standalone `.xz` and `.tar.xz`/`.txz` can be created, listed, tested and
extracted in CLI and desktop. See
[issue #18](https://github.com/beyondhumane/arca/issues/18) and
[Creating archives](creating-archives.md#xz). bzip2 and Zstandard work the same
way, standalone and around a TAR; see
[Creating archives](creating-archives.md#bzip2-and-zstandard). ZIP entries
compressed with bzip2 are read.

CPIO, AR, DEB, RPM, CAB (MSZIP and LZX) and LZH are listed, tested, extracted,
previewed, hashed and compared, read-only; see
[CPIO, AR, DEB, RPM, CAB and LZH](more-formats.md).

## Not yet

- **UDF:** ISO images are read through ISO 9660, so files stored only in UDF (such as `install.wim` on Windows media) are not listed yet.
- **WIM, ARJ and Quantum-compressed cabinets** are not read yet; WIM waits for a maintained pure-Rust reader.
- **Symbolic links:** skipped for ZIP/TAR creation, rejected for 7z.
- **7z password changes:** adding, deleting and renaming entries rewrite the archive with its own password; changing that password is not offered yet.
- **Desktop integration on Linux and macOS:** the Explorer menu is Windows-only for now.
- **Mutation outside ZIP, TAR and 7z:** RAR stays read-only once created, and ISO images, the ZIP containers and the formats of [CPIO, AR, DEB, RPM, CAB and LZH](more-formats.md) are never modified.
- **A native format:** at which point Zstandard becomes the default codec.

> [!TIP]
> **Want to help?**
>
> Pick an item, open an issue to discuss the approach, and send a pull request. See [Contributing](contributing.md).
