# Vendored lzma-rust2

Source: crates.io `lzma-rust2` **0.22.0**, Apache-2.0 (`LICENSE` retained).
Archive SHA-256:
`7277c9742229dc25243236d578b6471a798c554512c6f6a04365ec2e87c2a3bb`.

Retained: `src/` (with its unit tests), `tests/fixtures/` (which those unit
tests embed), `README.md`, `LICENSE` and the normalized manifest. Omitted:
`benches/`, `examples/`, the integration tests in `tests/` (most of them read
`tests/data`, which the published crate does not ship, or build liblzma),
`CHANGELOG.md`, `rustfmt.toml`, the lockfile, `Cargo.toml.orig`, and the
`[[example]]`, `[[test]]` and `[[bench]]` targets and dev-dependencies that
name them.

It is used through `[patch.crates-io]` in the workspace manifest and is left
out of the workspace, as `arca-7z/vendor/sevenz-rust2` and
`arca-rar/vendor/rars` are; the manifest gains an empty `[workspace]` so its own
unit tests run with
`cargo test --manifest-path arca-xz/vendor/lzma-rust2/Cargo.toml --lib --no-default-features --features std,encoder,xz -- --skip _roundtrip`
(the skipped BCJ round trips read `tests/data` too).
This is a crates.io patch, not a change to Arca's Rust 1.95 / edition 2021
policy: the crate keeps its own edition 2024 / MSRV 1.85.

Arca never enables the `optimization` feature, the only one with `unsafe`
code; without it the crate is `forbid(unsafe_code)`. No change here adds
`unsafe`. Every change is marked `Arca:` in the source.

Local changes:

- **CRC32 and CRC64 by slicing-by-8** (`src/crc.rs`). The byte-at-a-time
  loop was a chain of dependent table lookups, one per byte, and the XZ block
  check runs it over every byte in both directions. Eight tables fold eight
  bytes with independent lookups. Same values; a test checks them against the
  bit-at-a-time definition for every length and split up to 300 bytes.
- **A bounded multithreaded LZMA2 reader** (`src/lzma2_reader_mt.rs`,
  rewritten). The published one gathered a whole run of chunks between
  dictionary resets into memory however long it was (a stream without resets
  is one run), decoded it into a buffer that grew with the output, kept
  reading ahead while an early run was still decoding, and reported a
  worker's error before the runs ahead of it. This one gathers a run of at
  most `run_max` bytes, compressed or decoded (`new` uses four dictionaries,
  at least 1 MiB; `new_bounded` takes it), keeps at most one run queued
  beyond its threads, returns runs and errors in order, stops its workers on
  drop, and from the first run longer than `run_max` decodes the rest of the
  stream on the calling thread with `Lzma2Reader`, streaming. Unit tests
  cover runs decoded on threads and in place, truncation and corruption at
  many offsets, and errors after the runs before them.
- **Gathering errors in that reader come in order too**
  (`src/lzma2_reader_mt.rs`). A source that ended or an invalid control
  byte met while a run was being gathered was returned at once, ahead of the
  runs already queued, and a failure while reading ahead dropped a run that
  had just been decoded: a damaged solid 7z gave out none of its entries on
  many threads where one thread gave out most. Now the run being gathered,
  as far as it was read, is decoded on the calling thread with
  `Lzma2Reader` once every run before it has been returned, so the output
  and the error are those of a single-threaded reader; a thread that cannot
  be started or reached is reported after the runs before it, and a decoded
  run is never dropped. A unit test cuts a stream at fixed fractions and at
  every chunk boundary and damages control bytes, and holds the output to
  the single-threaded reader's.

Feature audit:

```sh
cargo tree -i lzma-rust2 -e features
```

There must be exactly one `lzma-rust2`, this path, with the features `std`,
`encoder` and `xz` and never `optimization`.
