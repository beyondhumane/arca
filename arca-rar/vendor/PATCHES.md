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
  (`rar50/write/filter_policy.rs`). Levels 0, 1, 4 and 5 are unchanged.
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

Measured with `arca create out.rar <dir> -l <level>` on 144 MiB of source and
executables (1,382 files), Windows 11, 16 threads, together with the
`parallel` feature and the 4 GiB writer budget set in `arca-rar`; every
archive passes `UnRAR t`. See the commit that brought this copy in for the
numbers.
