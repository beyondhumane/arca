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

Feature audit:

```sh
cargo tree -i lzma-rust2 -e features
```

There must be exactly one `lzma-rust2`, this path, with the features `std`,
`encoder` and `xz` and never `optimization`.
