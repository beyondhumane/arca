# Vendored sevenz-rust2

Source: crates.io `sevenz-rust2` **0.23.0**, Apache-2.0 (LICENSE retained).
Archive SHA-256:
`0a4f883677093690e91fef8ae81fad8bce9e2c3a079b61054f05ce0d3ecf681e`.

Retained: source tree, normalized manifest, upstream README and LICENSE. Omitted:
upstream CI, examples, integration tests, lockfile and registry bookkeeping.
This is a crates.io patch, not a change to Arca's Rust 1.95 / edition 2021 policy.
The vendored dependency itself uses its original edition 2024 / MSRV 1.93.

Local changes:

- Build only an `rlib`: Arca consumes this as a Rust dependency, not a C ABI.
  The upstream `cdylib` collides between Cargo's abort and unwind artifacts in
  release integration tests; removing that unused output keeps release tests
  compatible with Arca's unchanged `panic = "abort"` release profile.
- Pin `lzma-rust2 =0.21.0` and disable its `optimization` feature. Exact upstream
  LZMA source contains unsafe optimization/SIMD code; disabling this feature
  activates its own `forbid(unsafe_code)` guard. Add the same guard to sevenz.
  LZMA crate SHA-256:
  `fde178a3caf126c440fa15628147772d8ee590a39477d711353fbe2e58a73a5b`.
- Bound encoded/decoded headers, counts and decoder dictionaries before
  allocating; cap Zstd windows. Bound coders to four single-stream stages and
  reject malformed/disconnected/cyclic coder graphs and mismatched mappings.
- Detect premature EOF in bounded streams. Decode solid blocks by file-to-block
  mapping, including intervening empty entries, rather than substream count.
- Preserve encrypted-header metadata and verify encoded-header CRCs from either
  the block or its substream (the upstream writer uses the latter). Refuse
  encrypted headers without a verifiable nonempty stream.
- Populate the format's default substream mapping when its optional section is
  absent; reject unused pack streams and inconsistent file/substream counts.

No new compression or cryptography is implemented. Upstream's existing bounded
AES work factor (max power 24) and single-entry KDF cache are unchanged. Arca
creation uses power 19 and OS-random, independent salts/IVs.

Feature audit commands:

```sh
cargo tree -p arca-7z -e features -i lzma-rust2
cargo tree -p arca-cli --no-default-features -e normal
cargo tree -p arca-gui --no-default-features -e normal
```

The resolved LZMA features must remain `std` and `encoder`, never `optimization`.
Native codec feature forwarding is explicit; upstream default features and its
filesystem `util` helpers are not enabled or re-exported. Native Zstd remains
outside the safe-Rust parser boundary. Re-audit the resolved source and run the
adversarial/truncation/interop tests before updating either pinned crate.
