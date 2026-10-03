---
description: Build the optional read-only RAR/CBR reader, with passwords, staged extraction and explicit resource limits.
group: Reference
order: 20
keywords: rar cbr read-only experimental encryption solid archive
---

# Experimental RAR reader

RAR is an **opt-in, read-only** format. Normal builds and release packages do
not enable it. Build the CLI and desktop window explicitly:

```sh
cargo build --release --features rar
# Or CLI only:
cargo build --release -p arca-cli --features rar
```

The `arca-rar` adapter pins `rars` to 0.10.0, disables its default features and
enables only `encryption`. Its writer is not compiled. No installed `unrar`,
RAR/WinRAR or 7-Zip executable is used at runtime. Without the Cargo feature,
opening a `.rar` or `.cbr` gives an explanation of how to enable the reader.

## Using it

```sh
arca list archive.rar
arca list private.rar -p 'password'     # also works with encrypted headers
arca test archive.rar -p 'password'
arca extract archive.rar -o extracted/ -p 'password'
arca extract comic.cbr -o comic/
```

Omit `-p` for an unencrypted archive. Command-line passwords can be visible in
shell history and process listings, as with ZIP; prefer the desktop password
dialog on shared machines. The GUI prompts before listing encrypted headers,
retries incorrect passwords, and keeps the password for the open archive.
The title identifies RAR as experimental and read-only. No operating-system
file associations are added by this feature.

Listing, testing, previews and extraction are supported. Solid streams are
decoded sequentially. A selected extraction still verifies every file,
including solid predecessors, before publishing the selection. GUI testing
verifies the whole RAR even if only some entries are selected.

RAR never appears in the creation formats. Creating RAR/CBR or changing a RAR
password is rejected. Add, remove, rename, move and new-folder jobs cannot
modify it. Copying an existing archive as a file is not a RAR rewrite.

## Safety and limits

Each operation uses these fixed ceilings:

| Resource | Limit |
|---|---:|
| Headers | 100,000 |
| Header bytes | 64 MiB |
| RAR5 dictionary | 256 MiB |
| Decoder workspace | 512 MiB |
| Buffered RAR5 filter decoding | 64 MiB |
| Output per file | 4 GiB |
| Total decoded output, including unselected files | 16 GiB |
| In-memory preview | 64 MiB |

These are decoder limits, not a hard process-RSS or CPU-time sandbox. Some
filtered archives need more buffered decoding than permitted and are rejected.
The GUI's cancellation flag is checked while parsing and decoding as well as
between publication steps.

Extraction first decodes into private temporary storage and verifies integrity.
Wrong passwords, corruption and decode failures leave the destination untouched.
Only then are selected files published using same-filesystem temporary files
and atomic replacement. Skip, rename, overwrite and cancel conflicts are honored.
An I/O failure or cancellation **during publication** can leave earlier verified
files in place; this is not a transaction over an entire destination tree.
Temporary storage can approach the output limit; it must have enough free space.

Traversal, absolute paths, Windows drive/stream names, device names, control
characters, duplicate/case-colliding names and file/directory collisions are
rejected. RAR links and special files are unsupported. Existing links/reparse
points below the selected destination are refused. The user-selected root is
resolved to its canonical location. Do not extract into a tree another process
can concurrently modify: path checks are not a filesystem-race sandbox.

RAR5 archives must have a complete end header. Formats that do not store a
CRC32 display a blank CRC column; native RAR integrity checks still run.
Integrity guarantees depend on the checksums/authenticators the archive carries.

## Deliberate exclusions and stability gate

- **Multi-volume sets** (`.part1.rar`, split members and old-numbered sets) are
  not assembled. The adapter rejects volume flags rather than guessing sibling
  paths or publishing a partial set. Open a complete single-volume archive.
- No recovery/repair, RAR writing, OS associations or claim of exhaustive RAR
  compatibility. Unsupported methods/metadata return errors.
- The current corpus covers independent RAR5 stored/compressed/solid/encrypted
  files plus a legacy Unpack29 solid fixture. Broader external corpora and
  sustained fuzzing remain necessary before stable activation.
- `rars` declares Apache-2.0, but the upstream code/research provenance and
  relevant RARLAB license restrictions still need review before distribution.
  Safe Rust and a disabled writer do not settle provenance or resource risks.

Run the feature audit and tests with:

```sh
python3 arca-rar/tests/check-features.py
cargo test --workspace
cargo test --workspace --features rar
cargo clippy --workspace --all-targets --features rar -- -D warnings
```

See the [decision record](https://github.com/beyondhumane/arca/blob/main/docs/todos/rar-format.md) and
[fixture provenance](https://github.com/beyondhumane/arca/blob/main/arca-rar/tests/fixtures/README.md).
