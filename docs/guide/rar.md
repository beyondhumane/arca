---
description: Read RAR/CBR archives, including multivolume sets, passwords and verified extraction, and create new single-volume RAR5 archives.
group: Reference
order: 20
keywords: rar cbr read-only create rar5 encryption solid archive multivolume part1 r00
---

# RAR

Arca reads every RAR family and **creates new single-volume RAR5 archives**.
Existing RAR and CBR archives are never modified. RAR support is enabled by
default in the CLI and desktop window; normal builds and the packages built
from them include it:

```sh
cargo build --release
# Or CLI only:
cargo build --release -p arca-cli
```

The `arca-rar` adapter pins `rars` to 0.10.0, disables its default features and
enables only `encryption` and `write`; `recovery` and `parallel` stay off. No
installed `unrar`, RAR/WinRAR or 7-Zip executable is used at runtime. For a build without RAR, use
`--no-default-features --features codecs-native`; omit `codecs-native` to also
exclude the native ZIP/7z codecs. A pure-Rust build with RAR uses
`--no-default-features --features rar`. This is a build option, not a UI toggle.

## Reading

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
The title identifies an open RAR as read-only. No operating-system file
associations are added by this feature.

Listing, testing, previews and extraction are supported. Solid streams are
decoded sequentially. A selected extraction still verifies every file,
including solid predecessors, before publishing the selection. GUI testing
verifies the whole RAR even if only some entries are selected.

Changing a RAR password is rejected. Add, remove, rename, move and new-folder
jobs cannot modify an existing RAR or CBR. Copying an existing archive as a
file is not a RAR rewrite.

## Creating RAR5 archives

```sh
arca create backup.rar documents/ notes.txt
arca create photos.rar photos/ -l best
arca create plain.rar big.iso -c store      # same as -l store
```

The output is a new, single-volume, non-solid, unencrypted RAR5 archive that
official UnRAR and 7-Zip open. `-l store|fast|normal|best` selects the RAR
compression strength; `-c` accepts only `auto` or `store`. The desktop **Create**
dialog offers RAR next to ZIP and 7z with the same levels. Empty files and
empty directories are stored; modification times are preserved; directories
are walked in sorted order, so the same input always produces the same bytes.

What creation refuses, before any input is read or any file is written:

- A password or **Hide file names**: RAR output is never encrypted. Use ZIP or
  7z for that.
- `-c deflate|zstd|lzma2`, `-j` above 1 (RAR creation is sequential), a `.cbr`
  output name or any other suffix than `.rar`.
- An output path that already exists, even as a dangling link. Arca creates
  new RAR archives only and never replaces a file, including one that appears
  while the archive is being written. The output's parent directory must exist.
- Inputs that are symbolic links, reparse points or special files (they are
  not followed), inputs that alias the output or its directory, and names that
  are invalid, duplicated or differ only in case.
- More than 100,000 members, member names over 4096 bytes, a file over 4 GiB
  or more than 16 GiB in total.

The writer stages a `.arca-*.rar.part` file beside the output, reopens it with
Arca's own reader to list and fully decode every member, then publishes it
without overwriting. Any failure or cancellation leaves nothing at the output
path. In the CLI, Ctrl+C (SIGINT) or SIGTERM during a RAR creation is handled
cooperatively: the walk or the writer stops at its next check, reports
`cancelled` with a nonzero exit status and the staged part file is removed.
On Unix, inherited SIGHUP handling is preserved so `nohup` still works.
An unhandled SIGHUP, SIGKILL or power loss can leave `.arca-*.rar.part` and
`.rars-spool-*` temporary files; these are not published archives.

Not supported in this increment, and refused rather than approximated: solid
archives, multivolume output, encryption, recovery records, RAR data filters,
RAR4 output, CBR creation, and adding to, removing from, renaming inside or
repairing an existing archive.

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
Unselected numbered siblings outside the supported range are ignored; an
out-of-range selected volume or a required 257th volume still fails.

RAR has no universal set identifier. The adapter checks available volume numbers,
flags and split-member metadata; unrelated unsplit volumes with identical metadata
cannot always be distinguished, and older formats lack reliable volume ordinals.
Historical sets without end headers may continue after an unsplit member. If
such a set loses its final volume at an unsplit boundary, the available metadata
cannot prove that anything is missing. Sets whose first volume has an end header
must have end headers on subsequent volumes too.
Integrity verification checks the checksums/authenticators provided by the format.

## Safety and limits

Each reading operation uses these fixed ceilings:

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

Header and output budgets apply across the complete set. Creation has its own
ceilings (members, name bytes, 4 GiB per file, 16 GiB total, 64 MiB of headers
and a 256 MiB managed writer-memory ledger, see
[`arca_rar::create_limits`](https://github.com/beyondhumane/arca/blob/main/arca-rar/src/lib.rs)).
These are decoder and writer limits, not a hard process-RSS or CPU-time sandbox. Some
filtered archives need more buffered decoding than permitted and are rejected.
The GUI's cancellation flag is checked while parsing and decoding as well as
between publication steps.

Multivolume previews cap the selected buffer at 64 MiB while discarded members
use the normal output limits. The backend still decodes and verifies the whole
set, so corruption in another volume can prevent a preview. Standalone previews
stop after the selected member instead of verifying later entries.

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

## Deliberate exclusions and dependency notes

- No recovery/repair, modification of existing archives, OS associations or
  claim of exhaustive RAR compatibility. Unsupported methods/metadata return
  errors. Creation is limited to the single-volume RAR5 profile above.
- Independent fixtures, multivolume regressions, official UnRAR comparisons and
  bounded mutation fuzzing extend reader coverage; the seeded creation harness
  compares archives Arca writes against official UnRAR and 7-Zip. They do not
  establish exhaustive compatibility. See the
  [reader validation record](https://github.com/beyondhumane/arca/blob/main/arca-rar/RAR_VALIDATION.md)
  and the [creation validation record](https://github.com/beyondhumane/arca/blob/main/arca-rar/RAR_CREATE_VALIDATION.md)
  for reproducible checks and uncovered cases.
- The [provenance review](https://github.com/beyondhumane/arca/blob/main/docs/plans/rars-0.10.0-distribution.md)
  records upstream's Apache-2.0 declaration and unresolved source/research
  questions. It did not establish infringement or a license incompatibility
  preventing distribution. These observations are not a claim of legal
  clearance or a reason to label the reader experimental. Published license
  texts and notices for the adapter's dependencies accompany the packages.
  Enabling the writer added no dependency that the reader branch did not
  already carry, so the notice bundle is unchanged.

Run the feature audit, tests and the bounded creation harness with:

```sh
python3 arca-rar/tests/check-features.py
cargo test --workspace
cargo test --workspace --no-default-features --features codecs-native
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p arca-cli
python3 arca-rar/tests/create-stress.py            # bounded smoke, skips absent tools
UNRAR=/path/to/unrar SEVENZIP=/path/to/7zz python3 arca-rar/tests/create-stress.py --stress --require-tools
```

See the [decision record](https://github.com/beyondhumane/arca/blob/main/docs/todos/rar-format.md) and
[fixture provenance](https://github.com/beyondhumane/arca/blob/main/arca-rar/tests/fixtures/README.md).
