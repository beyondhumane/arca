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
- Pin `lzma-rust2 =0.22.0` and disable its `optimization` feature. Exact upstream
  LZMA source contains unsafe optimization/SIMD code; disabling this feature
  activates its own `forbid(unsafe_code)` guard. Add the same guard to sevenz.
  The workspace resolves this pin to the copy vendored in
  `crates/arca-xz/vendor/lzma-rust2` (see `crates/arca-xz/vendor/PATCHES.md`), the same one
  arca-xz uses, so .7z and .xz share one LZMA implementation. LZMA crate
  SHA-256: `7277c9742229dc25243236d578b6471a798c554512c6f6a04365ec2e87c2a3bb`.
  It was `=0.21.0` before; 0.22.0 bounds the pending jobs of the
  multithreaded LZMA2 writer and keeps the same API.
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
- Arca: cap the threads of a multithreaded LZMA2 decoder (`decoder.rs`) so
  that, per thread plus one queued run and one being read, a run of up to
  four dictionaries (at least 1 MiB, at most the coder's unpack size) with
  its output and window fits in 2 GiB, the budget Arca's encoders keep to;
  a block of no more than one run is decoded on one thread. The reader it
  uses is the bounded `Lzma2ReaderMt::new_bounded` of the vendored
  lzma-rust2.
- Add `BlockDecoder::into_reader`, the decoded block as one pull stream built
  by the existing decode stack, so that rewriting an archive can feed a block
  straight into the encoder without buffering it. It adds no decoding; the
  caller checks each entry's CRC.
- Pin `ppmd-rust =1.5.2` (CC0-1.0 OR MIT-0, itself `forbid(unsafe_code)`) for
  the `ppmd` feature, which Arca enables to read and write PPMd. PPMd crate
  SHA-256: `cd4acc8d264ea385db64ccfdcdfd5909ee9eda0975666723d59f3bfec80b3611`.
- Arca: bound the PPMd decoder to its coder's unpack size (`decoder.rs`). A 7z
  PPMd stream has no end marker, so without the bound a block read to its end
  went on producing bytes from whatever followed the last symbol.

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
adversarial/truncation/interop tests before updating any pinned crate.
