---
description: What isn’t there yet, and why some of it matters.
group: Reference
order: 15
keywords: roadmap future 7z lzma xz solid symlinks planned
---

# Roadmap

What Arca doesn’t do yet and, where it matters, why.

## Available

7z listing/extraction (including solid and encrypted headers), Store/LZMA2
creation, passwords and hidden names are implemented for CLI and desktop. See
[issue #2](https://github.com/beyondhumane/arca/issues/2) and the
[accepted design](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md).

Standalone `.xz` and `.tar.xz`/`.txz` can be created, listed, tested and
extracted in CLI and desktop. See
[issue #18](https://github.com/beyondhumane/arca/issues/18) and
[Creating archives](creating-archives.md#xz).

## Not yet

[RAR/CBR reading](rar.md) is experimental and opt-in with `--features rar`.
Stable activation, provenance review and multi-volume support remain follow-up
work. RAR creation and mutation are not supported.

- **Symbolic links:** skipped for ZIP/TAR creation, rejected for 7z.
- **GNU tar long names.**
- **Solid 7z creation:** existing solid archives can be read, but new ones use independent blocks.
- **7z password changes and mutation:** adding, deleting and renaming can require solid-block rewrites and are out of scope.
- **Desktop integration on Linux and macOS:** the Explorer menu is Windows-only for now.
- **Mutation outside ZIP:** ZIP desktop editing exists; 7z remains create/read only.
- **A native format:** at which point Zstandard becomes the default codec.

> [!TIP]
> **Want to help?**
>
> Pick an item, open an issue to discuss the approach, and send a pull request. See [Contributing](contributing.md).
