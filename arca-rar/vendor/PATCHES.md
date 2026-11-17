# Vendored rars

Source: crates.io `rars` **0.10.2**, Apache-2.0 (`COPYING` retained).
Archive SHA-256:
`0665518097392be04a322dec9eba41e88028f6aca85f5d61f8abb068691f2266`.

Retained: `src/`, `tests/` (the crate's unit tests embed its fixtures),
`README.md`, `COPYING` and the normalized manifest. Omitted: `benches/` and
the `[[bench]]` targets that name them, and the original `Cargo.toml.orig`.

It is used through `[patch.crates-io]` in the workspace manifest and is left
out of the workspace, as `arca-7z/vendor/sevenz-rust2` is; the manifest gains
an empty `[workspace]` so its own tests run with
`cargo test --manifest-path arca-rar/vendor/rars/Cargo.toml --lib --features write,encryption,parallel`. `arca-rar/fuzz` has
a lockfile of its own and still builds the crates.io copy: the reader it
exercises is unchanged here.

Every change is in the RAR 5 writer and is marked `Arca:` in the source. The
reader, the format and the integrity checks are as published, and no RARLAB
source was consulted for any of it.

- **Tree search for a whole member.** The optimal parse searched the first
  block of a member with the binary-tree finder and then switched to hash
  chains for the rest. A member with no history now keeps the tree for every
  block (`codec/rar50/encoder.rs`, `streaming_blocks_with_allowance`), and a
  member with no blocks returns before any finder is built. The test that
  held the streaming writer byte-identical to the per-block reference encoder
  (`parallel_streaming_blocks_are_byte_identical_to_serial_blocks`) now checks
  that the writer's stream decodes to the input and is no larger, since the
  reference still starts every block afresh. The writer's own output is still
  identical whatever the number of jobs.
- **Nice length and parse passes per level.** `EncodeOptions` gains
  `nice_length` and `parse_passes`, which default to the crate's constants.
  Levels 2 and 3 stop at 64 bytes and price each block once and twice;
  level 3 also searches 32 candidates rather than 128
  (`rar50/write/filter_policy.rs`). Levels 0 and 1 are unchanged; levels 4
  and 5 are lightened below.
- **Hash tables sized to the window.** The chain finder the RAR 5 encoder
  builds (`MatchFinder::with_window_hash`) and the tree finder give their
  head table one entry per two bytes of window, from 2^17 up to 2^22 entries,
  instead of a fixed 2^17 (`codec/match_finder.rs`, `window_hash_bits`). The
  RAR 2.x and 2.9 encoders keep the fixed tables. The writer's workspace
  estimate charges the two bytes per window byte (`rar50/write/mod.rs`,
  `streaming_lz_workspace`, 17 and 13 become 19 and 15).
- **Tree search for runs with history.** A run that starts on history (a
  large member's later runs, or a solid member) searched through hash
  chains. It now gets the tree too, seeded with the newest quarter of a
  dictionary of that history (`codec/rar50/encoder.rs`,
  `OptimalCollector::with_allowance`, `HISTORY_SEED_DIVISOR`); history
  older than that is out of reach of the run's first matches. The chain
  arm of the collector is gone. `parallel_streaming_blocks_are_byte_identical_to_serial_blocks`
  lets the writer's stream come out up to a thousandth larger than the
  reference's, which reaches all its history through chains.
- **Remembered distances settled once per position.** The optimal parse
  copied the four remembered distances and the last length into every
  target a cheaper arrival landed on; it now works them out once, when it
  reaches a position, from the arrival that won (`price_optimal_paths`).
  The tokens are the same. The two pricing tests that compared the whole
  workspace with the reference implementation compare the arrivals (price,
  length, distance), since the remembered distances are now only settled at
  positions the parse prices.
- **Stepping over data that keeps missing.** `EncodeOptions` gains
  `skip_after_misses`: after that many searches in a row find nothing, the
  lazy parse and the optimal parse's collector step over one more position
  per further run of misses, emitting them as literals without searching or
  inserting them, until a match turns up. The first 64 KiB of a parse's
  input are always searched, so small files are not affected. Level 1
  starts after 16 misses, levels 2 and 3 after 64 and levels 4 and 5 after
  128 (`rar50/write/filter_policy.rs`).
- **Lighter top levels.** Level 4 searches 64 candidates out to 128 bytes
  and prices each block twice; level 5 searches 128 out to 256 and prices
  three times (`rar50/write/filter_policy.rs`). They used to search 256 and
  512 out to 512 bytes.

Measured with `arca create out.rar <dir> -l <level>` on 144 MiB of source and
executables (1,382 files), Windows 11, 16 threads, together with the
`parallel` feature and the 4 GiB writer budget set in `arca-rar`; every
archive passes `UnRAR t`. See the commit that brought this copy in for those
numbers. The later patches were measured against WinRAR on the corpus
`arca-rar/examples/rar-corpus.rs` generates, with `arca-rar/tests/rar-bench.ps1`;
the method and the figures are in `docs/guide/benchmarks.md`.

Container fixes, which do not touch the encoder:

- **Locator offsets from the main header.** The quick-open and recovery
  offsets in the main header's locator were measured from the end of the
  signature. WinRAR measures them from the start of the main header, which
  follows the HEAD_CRYPT block when headers are encrypted. WinRAR 7.23 then
  failed sets with encrypted headers, a recovery record and a member split
  across volumes ("Corrupt header is found", "Recovery record is corrupt").
  Archives without encrypted headers are unchanged. The reader finds the
  recovery record by scanning, not by the locator, so it is unaffected
  (`rar50/write/layout.rs`, with `locator_counts_from_the_main_header_after_head_crypt`).
- **Volumes bounded by their whole size.** The volume writer counted only
  member data against its limit, so headers, the recovery record and the end
  block came on top and a set of many small files, or one with `-rr` or `-hp`,
  outgrew the size asked for. The new `write_streaming_volumes_sized_with_progress`
  (`rar50/write/mod.rs`) bounds each volume file as WinRAR's `-v` does. The
  engine's `VolumeLimit::Total` works out, when a volume starts, the largest
  body that still fits with its signature, headers, recovery record and end
  block (`VolumeWriter::max_body`, a binary search over the layout and
  `recovery_service_len`). It cuts each fragment to what is left after a
  header priced for the largest fragment (`next_fragment`, `body_room`), and
  `finish_volume` fails a volume that comes out larger. To share the sizes,
  `finish_volume` now builds its HEAD_CRYPT, layout, flags and end block
  through small helpers, and the recovery service header through
  `recovery_service_header`; the bytes are the same. The payload-limited
  `write_streaming_volumes_with_progress` behaves as before
  (`rar50/write/engine.rs`).

Spool plumbing, which does not touch the encoder or the archive bytes:

- **Member payloads in memory.** Every member's packed payload had a
  `.rars-spool-*` file of its own beside the output: created when the member
  was set up, reopened by path for each wave that appended to it and again to
  copy it out, then deleted. A stored member, whose payload is never written,
  got one too. On Windows each of those file operations costs milliseconds,
  so 3,000 small files took 15 to 45 s of wall time for under 10 s of CPU.
  `Spool::for_member` (`streaming.rs`) now keeps the payload in memory
  (`streaming/resident_spool.rs`) when the resources have
  `with_max_spool_memory_bytes`. Its capacity, old and replacement buffers
  together while it grows, is charged to that cap and to the
  `with_max_memory_bytes` ledger before it is allocated. The spool moves to
  a file, at the same cursor, once it would pass 1 MiB
  (`RESIDENT_SPOOL_LIMIT`) or either ledger or the allocator refuses it, so
  a refusal never fails a write; a move that fails leaves the bytes in memory
  and reports the I/O error. Without the setting member spools are parked
  files, as before. It serves the streaming and whole-member payloads, the
  empty placeholders of stored members, the quick-open payload and a volume
  set's encrypted members (`rar50/write/compress.rs`, `engine.rs`). The
  recovery mirror and scratch, volume bodies and archive rewrites keep files
  from the start. `Spool` now holds a `Store`: a file, a resident buffer, or
  the bare-WASM memory spool. `into_source` returns a `Result`, since it
  moves a resident spool to its file first (`rewrite_staging.rs`). A
  whole-member payload, which the automatic filter search produces, is
  charged to its worker's fixed reservation and so moves to disk whenever
  the estimate leaves no room.
