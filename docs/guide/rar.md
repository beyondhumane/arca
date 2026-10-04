---
description: Build the optional read-only RAR/CBR reader, including multivolume sets, passwords and verified extraction.
group: Reference
order: 20
keywords: rar cbr read-only experimental encryption solid archive multivolume part1 r00
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
arca list backup.part03.rar             # resolves the complete set
arca extract backup.r01 -o backup/      # legacy .rar/.r00/.r01 naming
```

Omit `-p` for an unencrypted archive. Command-line passwords can be visible in
shell history and process listings, as with ZIP; prefer the desktop password
dialog on shared machines. The GUI prompts before listing encrypted headers,
retries identifiable incorrect passwords, and keeps the password for the open archive.
RAR4 encrypted headers can report an incorrect password as a format error;
reopen with the correct password rather than assuming the archive is damaged.
The title identifies RAR as experimental and read-only. No operating-system
file associations are added by this feature.

Listing, testing, previews and extraction are supported. Solid streams are
decoded sequentially. A selected extraction still verifies every file,
including solid predecessors, before publishing the selection. GUI testing
verifies the whole RAR even if only some entries are selected.

RAR never appears in the creation formats. Creating RAR/CBR or changing a RAR
password is rejected. Add, remove, rename, move and new-folder jobs cannot
modify it. Copying an existing archive as a file is not a RAR rewrite.

## Multivolume sets

Keep every volume in the same directory. Modern `name.part1.rar` or
`name.part01.rar` sets and legacy `name.rar`, `name.r00`, `name.r01` sets can
be opened from any part. The adapter resolves the first volume and lists,
previews, tests and extracts the complete set, including split, solid and
encrypted streams. In the desktop file picker, use **All files (including RAR
volumes)** to select a legacy later volume. Passwords apply to the complete set.

Discovery does not search other directories or download missing parts. Missing,
duplicate, out-of-order and inconsistent volumes are rejected with the affected
path. Selected volumes and discovered siblings must be regular files, not links
or Windows reparse points. Standalone archives with volume-like filenames are
still treated as standalone files when their headers do not declare a set.

RAR has no universal set identifier. The adapter checks available volume numbers,
flags and split-member metadata; unrelated unsplit volumes with identical metadata
cannot always be distinguished, and older formats lack reliable volume ordinals.
Integrity verification checks the checksums/authenticators provided by the format.

## Safety and limits

Each operation uses these fixed ceilings:

| Resource | Limit |
|---|---:|
| Volumes per set | 256 |
| Directory entries inspected during volume discovery | 100,000 |
| Headers | 100,000 |
| Header bytes | 64 MiB |
| RAR5 dictionary | 256 MiB |
| Decoder workspace | 512 MiB |
| Buffered RAR5 filter decoding | 64 MiB |
| Output per file | 4 GiB |
| Total decoded output, including unselected files | 16 GiB |
| In-memory preview | 64 MiB |

Header and output budgets apply across the complete set. These are decoder
limits, not a hard process-RSS or CPU-time sandbox. Some
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

- No recovery/repair, RAR writing, OS associations or claim of exhaustive RAR
  compatibility. Unsupported methods/metadata return errors.
- Independent fixtures, multivolume regressions, official UnRAR comparisons and
  bounded mutation fuzzing extend coverage. They do not establish exhaustive
  compatibility. See the [validation record](https://github.com/beyondhumane/arca/tree/main/docs/plans)
  for reproducible checks and uncovered cases.
- The [provenance review](https://github.com/beyondhumane/arca/blob/main/docs/plans/rars-0.10.0-distribution.md)
  found unresolved source/research and notice questions. Stable distribution
  remains blocked. Apache-2.0 metadata, safe Rust and a disabled writer do not
  resolve those questions; the opt-in feature is not distribution clearance.

Run the feature audit and tests with:

```sh
python3 arca-rar/tests/check-features.py
cargo test --workspace
cargo test --workspace --features rar
cargo clippy --workspace --all-targets --features rar -- -D warnings
```

See the [decision record](https://github.com/beyondhumane/arca/blob/main/docs/todos/rar-format.md) and
[fixture provenance](https://github.com/beyondhumane/arca/blob/main/arca-rar/tests/fixtures/README.md).
