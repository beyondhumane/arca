---
description: The crates that make up Arca and the unsafe policy that holds them together.
group: Reference
order: 12
keywords: crates workspace unsafe design dependencies profiles lto
---

# Architecture

Arca is a Cargo workspace. Each crate has one job and a declared stance on unsafe code.

## Crates

| Crate | What it does | unsafe |
| --- | --- | --- |
| `arca-core` | Errors, limits, bounded header reads, MS-DOS dates, Zip Slip defence | forbidden |
| `arca-zip` | ZIP with Zip64; store, Deflate over zlib-rs, Zstandard, AES-256 | forbidden |
| `arca-tar` | ustar TAR with checksum verification | forbidden |
| `arca-cli` | The arca binary | allowed, unused |
| `arca-gui` | The arca-gui window | forbidden |
| `arca-icons` | The icon the desktop shows for a file type | Windows only, for the shell call |
| `windows/arca-shell` | Explorer context menu, outside the workspace so cargo build still works on Linux and macOS | required: COM |

The workspace also contains `arca-drag` and `arca-net`; see [their sources](https://github.com/beyondhumane/arca) for details.

## The unsafe policy

Every crate that parses bytes from an archive declares `#![forbid(unsafe_code)]` at crate level, so the compiler guarantees there’s no unsafe code in them. It appears only where the operating system demands it: one shell call for icons on Windows, and COM for the Explorer extension.

```rust
#![forbid(unsafe_code)]

// A malformed archive produces an error, never memory corruption.
match ZipArchive::open(file) {
    Ok(archive) => list(archive),
    Err(e) => eprintln!("arca: {e}"),
}
```

## Key dependencies

| Crate | Used for |
| --- | --- |
| `flate2` + `zlib-rs` | Deflate, using the fastest Rust implementation measured during design |
| `zstd` | Zstandard bindings to libzstd, with its internal multithreading enabled |
| `crc32fast` | CRC-32 checks |
| `rayon` | The thread pool behind parallel compression and extraction |
| `clap` | Command-line parsing |
| GPUI | The GPU-accelerated UI framework behind the window |

## Build profiles

```toml
[profile.dev.package."*"]
opt-level = 3

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

Dependencies are optimised even in debug builds: GPUI’s layout engine, text shaper and rasteriser redo their work every frame, and at `opt-level = 0` that shows up as stutter while dragging. Arca’s own code builds at level 1, which keeps the debugger useful.
