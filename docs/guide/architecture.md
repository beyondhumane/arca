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
| `arca-7z` | Bounded 7z parsing, solid extraction, Store/LZMA2 creation and AES | forbidden |
| `arca-rar` | Opt-in, experimental read-only RAR/CBR adapter | forbidden |
| `arca-cli` | The arca binary | allowed, unused |
| `arca-gui` | The arca-gui window | forbidden |
| `arca-icons` | The icon the desktop shows for a file type | Windows only, for the shell call |
| `windows/arca-shell` | Explorer context menu, outside the workspace so cargo build still works on Linux and macOS | required: COM |

The workspace also contains `arca-drag` and `arca-net`; see [their sources](https://github.com/beyondhumane/arca) for details.

CLI and desktop share `arca_core::Format`. Its writable formats include 7z but
exclude RAR. The [RAR reader](rar.md) is enabled only by `--features rar`; the
resolved `rars` dependency enables encryption, never its writer. The desktop
uses one background password-validation flow for ZIP, 7z and RAR, with retries,
cancel and resumption of the original action. CRC metadata is optional: an absent
checksum is not displayed as a fabricated zero.

## The unsafe policy

Arca's archive parser crates declare `#![forbid(unsafe_code)]`; this is not a
blanket guarantee about all transitive dependencies. Native Bzip2/Zstandard are
optional codecs, while desktop and OS libraries have their own unsafe boundaries.
The vendored 7z parser and its pinned LZMA decoder compile with unsafe forbidden.

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
| `sevenz-rust2` 0.23.0 / `lzma-rust2` 0.21.0 | Apache-2.0 7z/AES and LZMA; vendored parser hardening, LZMA unsafe optimization disabled |
| `rayon` | The thread pool behind parallel compression and extraction |
| `clap` | Command-line parsing |
| GPUI | The GPU-accelerated UI framework behind the window |

## 7z boundaries

- Encoded/decoded headers: 16 MiB each; entries, blocks and streams: 100,000.
- At most four single-input/output coders per block; multi-input BCJ2 is unsupported.
- LZMA dictionaries: 256 MiB per stage; Zstd window: 256 MiB. These are not a
  global memory quota. There is no total extracted-byte or CPU-time quota.
- Baseline decoding: Copy, LZMA, LZMA2, DEFLATE, AES, single-stream BCJ/Delta.
  Default `codecs-native` adds Bzip2 and Zstd. PPMd, Brotli and LZ4 are unsupported.
- Encrypted extraction validates the entire stable source before destination
  callbacks, then streams selected blocks. CRC-32 is not authentication.
- Progress/cancellation is cooperative; bounded header parsing and KDF setup
  cannot be interrupted internally. Preview rejects sizes beyond the caller cap.
- `Entry.offset` is an index for 7z, not a file offset. `Method::code()` now returns
  `Result<u16>` and rejects non-ZIP methods instead of inventing ZIP codes.

CLI/GUI explicitly forward `codecs-native`; their archive dependencies disable
default features. `cargo build --release --no-default-features` omits native
compression, not OS/graphics libraries. Audit the graph with:

```sh
cargo tree -p arca-cli --no-default-features -e normal
cargo tree -p arca-gui --no-default-features -e normal
cargo tree -p arca-7z -e features -i lzma-rust2
```

The first two must omit native `zstd-sys` and `bzip2-sys`. The GUI still includes
`libbz2-rs-sys` through GPUI HTTP decompression; despite its name, it is a Rust
implementation with no C build script, not the optional 7z Bzip2 decoder.
LZMA features must be `std`/`encoder`, never `optimization`. Rust 1.95 and edition 2021 remain
the workspace baseline; dependency editions are per crate. See the
[accepted design](https://github.com/beyondhumane/arca/blob/main/docs/plans/7z-format.md)
and [patch inventory](https://github.com/beyondhumane/arca/blob/main/arca-7z/vendor/PATCHES.md).

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
