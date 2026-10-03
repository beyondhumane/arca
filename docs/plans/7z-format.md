# 7z format

Accepted design for [issue #2](https://github.com/beyondhumane/arca/issues/2).
This replaces the proposal formerly in `docs/todos/7z-format.md`.

## Scope and format choice

The approved scope is complete in one integration: list/extract plain and
password-protected 7z, including encrypted headers and solid input; create
Store/LZMA2 using existing levels; create password-protected 7z with optional
hidden names. Existing-password changes, archive mutation and solid creation
are not part of this design.

ZIP's central directory remains visible even with WinZip AES. 7z encrypted
headers solve the hidden-name requirement without inventing a format. RAR
reading is a separate opt-in, read-only integration. The UnRAR license restricts
use of its code to develop a compatible compressor; it is not a universal ban
on independent writers or evidence that readers require native code. See
[the RAR design](../todos/rar-format.md) for its separate provenance review gate.

## Dependency decision and safe-Rust boundary

Use Apache-2.0 `sevenz-rust2 =0.23.0`, vendored from crates.io, and
`lzma-rust2 =0.21.0`. Version/provenance hashes and every patch are recorded in
[PATCHES.md](../../arca-7z/vendor/PATCHES.md). No custom cipher or compressor is
implemented. The dependency uses edition 2024/MSRV 1.93; Arca keeps Rust 1.95 and
edition 2021. Dependency editions do not require a workspace migration.

The old proposal's claim that all current LZMA code was unsafe-free was wrong:
the exact pinned LZMA source has unsafe optimization/SIMD code. Its default
features are disabled and only `std`/`encoder` enabled, activating its own
`forbid(unsafe_code)`. The vendored sevenz parser also forbids unsafe. Arca
bounds hostile metadata before allocations and fixes premature EOF and stream
mapping issues in the dependency. Future upgrades require re-auditing these
patches and the resolved feature graph, not merely changing a version.

Upstream default features are disabled. CLI/GUI explicitly forward
`codecs-native` to ZIP and 7z. Bzip2/Zstandard decoding is optional; pure mode
keeps Copy/LZMA/LZMA2/DEFLATE/AES and single-stream BCJ/Delta. PPMd, Brotli, LZ4
and multi-input BCJ2 are unsupported. Native codec/OS dependencies are not part
of the safe-Rust container-parser claim.

Only the Rust `rlib` output is built. The unused upstream `cdylib` causes
abort/unwind output collisions in Cargo release integration tests.

## Read and password flow

`SevenZArchive::open` validates headers, mappings, entry names and resource
bounds. Clear-header encrypted archives can be listed without a password;
hidden headers require it before there is a listing to display. The GUI lists
and validates in background workers, supports retry/cancel and resumes the
original extract/test/preview/open action. Stale workers cannot overwrite a
newer archive's state.

7z AES-256-CBC uses a SHA-256 password KDF (power 19 on creation) and fresh
random salt/IV material per block/header. CRC-32 is not a MAC. Decode/CRC errors
on encrypted streams are reported as incorrect password **or corrupt archive**.
Neither header parsing nor a CRC success proves cryptographic authentication.

Before **any** extraction callback on an encrypted archive, all content is
decoded and checked, including unselected blocks and empty selections. Only
after this pass may CLI/GUI create a destination, stage a file or replace an
existing one. The source must remain stable between validation and extraction;
CRC forgery and concurrent source mutation are not addressed by two passes.

Solid selection is decoded once per block per pass, draining skipped data,
without buffering the whole archive or parallel per-entry reopening. Plain
extraction may fail after previous entries are written; there is no
whole-destination rollback. Sink I/O errors stay I/O errors.

## Filesystem and creation boundaries

CLI/GUI recheck safe names and refuse destination symlinks/reparse points;
staged replacement avoids truncating an existing hard link. These checks are
not race-free handles against a hostile process replacing paths concurrently.
Callers own conflict prompts, overwrite/skip/rename policy, and temp cleanup.

Creation recursively enumerates regular files/directories in the interfaces.
The core takes explicit nonrecursive `Source` entries and rejects links/special
files. Store uses Copy; Fast/Normal/Best use LZMA2 1/6/9. Creation is sequential
with independent blocks. A nonempty password enables encryption; hidden names
require it. Empty-only encrypted output requires hidden names so a password is
actually checkable. Output is flushed/synced in a sibling temporary file and
persisted only on success; failure preserves existing output. This is not a
directory-fsync crash-durability guarantee.

ZIP-only add/delete/rename/password operations stay unavailable for 7z.
Virtual-file drag-out is disabled for 7z; Copy/Extract avoid per-member solid
decoder reopening. Windows registration advertises `.7z` for open/extract,
not mutation.

## Resource policy and API

Headers: 16 MiB encoded and decoded. Entries/blocks/streams: 100,000. Coder chains:
four single-input/output stages. LZMA memory: 256 MiB per stage; Zstd window:
256 MiB. Upstream AES work factor is bounded at power 24 on reads. Limits are
not a global CPU-time, memory or extracted-byte quota. Header parsing/KDF setup
is bounded but not internally cancellable; stream progress is cooperative.

See [arca-7z API](../../arca-7z/README.md) for reader, writer, progress, error and
callback contracts. Shared `Entry.offset` is a 7z index. Only the first member
reports a solid block's packed size. `Method::code() -> Result<u16>` rejects
non-ZIP metadata values instead of assigning invented ZIP method numbers.

## Verification

The required shell checks are in [AGENTS.md](../../AGENTS.md). Additionally:

```sh
cargo test -p arca-7z --no-default-features
cargo test -p arca-cli --release --locked
cargo test -p arca-7z --test interop -- --ignored
cargo test -p arca-gui -- --ignored
cargo tree -p arca-cli --no-default-features -e normal
cargo tree -p arca-gui --no-default-features -e normal
cargo tree -p arca-7z -e features -i lzma-rust2
```

Ignored tests need external `7z`. Regression fixtures check every truncation
prefix of representative plain/solid/encrypted/hidden-header inputs without
`catch_unwind`, malformed counts/coders/options, memory limits, wrong passwords,
unselected corruption, sink errors, cancellation and untouched destinations.
`interop.sh` verifies both directions with external 7-Zip. These are shell
checks, not Windows/macOS or interactive GUI runtime verification, and they do
not establish new 7z benchmark claims.
