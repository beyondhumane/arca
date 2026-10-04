# Read-only RAR validation record

Linux x86_64, 2026-10-03; Rust 1.97.1, Python 3.12.13, pinned `rars 0.10.0`.
Implementation base: `c53a8ea4b90f706cd585d3391c1ab737e272b4f2`, issue #14.
This is historical engineering evidence for that implementation base, not a
legal conclusion or release approval. Default activation was implemented
subsequently; see the [decision record](../docs/todos/rar-format.md). The initial
claim of a legal/distribution blocker was not established by the audit and is
superseded by the [updated review](../docs/plans/rars-0.10.0-distribution.md).

## Reproducible evidence

| Surface | Coverage |
| --- | --- |
| Independent producer matrix | 18 synthetic cases from official Linux RAR 7.23 / 6.24: RAR5 padded and unpadded volumes, RAR4 genuine legacy `.rar`/`.r00` naming, stored/compressed and solid/non-solid members, split members, content/header encryption, empty files and Unicode names. Every volume is opened, tested, previewed and extracted in the Rust regression. |
| Generations | Four unchanged upstream fixtures exercise the RAR13 container, Unpack15, Unpack20 and Unpack20 solid state. Existing RAR3 cases and independent RAR4/RAR5 cases exercise Unpack29/50. Historical producer commands were unavailable; these are not independent local producer output. |
| Interoperability | `compare-matrix.py` checks every archive SHA-256, names and sizes from CLI and official UnRAR technical listings, verifies with both decoders, and compares complete extracted file sets, sizes and SHA-256. API tests compare CRC32 when present and not encrypted. Timestamps are intentionally omitted by generation; packed fragment sizes are not treated as logical sizes. |
| Hostile inputs | Missing, wrong, duplicate, out-of-order and corrupt volumes; wrong passwords; all-byte truncation of existing RAR5 fixtures; bounded late-volume mutations/truncations of solid/content/header-encrypted RAR5 and encrypted RAR4; unsafe paths, collisions, archive links/special files, input and destination symlinks. |
| Budgets and cancellation | Existing aggregate header/output limits plus actual 128 KiB and 64 MiB dictionaries, rejection of a CRC-correct 512 MiB dictionary claim (256 MiB maximum), 65 MiB preview claim (64 MiB maximum), cancellation in discovery/parsing/staging and publication. Large claims are rejected without producing their declared output. |
| I/O failure | Deterministic publication obstacle after the first replacement, cancellation after publication starts, real ENOSPC in a private 64 KiB tmpfs for staging and publication. Staging failure publishes nothing. A later publication error does **not** roll back an earlier verified replacement. |

Archive producer commands, executable hashes, fixture hashes, payload hashes
and provenance are in [fixtures/README.md](tests/fixtures/README.md) and the two
fixture manifests. The official tools are local-only; no executables or
proprietary documents are redistributed. The synthetic matrix and fuzz seeds
use the repository's Apache-2.0 license. Upstream fixture license declarations
do not resolve the separate distribution review.

### Defect found and fixed

Truncating the last volume of the independent `rar4-legacy-content` set at byte
414 could produce a successful extraction omitting trailing members. The
adapter treated any missing legacy end header as the older format's legal EOF.
It now requires later RAR 1.5-4.x volumes to have an end header when the first
volume has one. The bounded late-volume regression failed before this fix and
passes afterward. No backend upgrade or unsafe parsing was introduced.

This does not invent an EOF marker for historical archives: truly older sets
without end headers cannot always distinguish a clean member-boundary truncation
from an intentionally shorter archive. Nor is an available set UUID invented.

### Bounded fuzz campaign

See [fuzz/README.md](fuzz/README.md) for the checked-in harness, input format,
license, safeguards and replay procedure; [campaign JSON](fuzz/campaign-2026-10-03.json)
records the actual run. Command:

```sh
cargo build --locked --manifest-path arca-rar/fuzz/Cargo.toml
python3 arca-rar/fuzz/run.py --runs 2000 --seed 140014 --seconds 300 --output /path/to/results
```

The final post-fix optimized run completed **10 seeds + 2,000 mutations in 55.319 s**
within the 300 s campaign budget: 838 opened, 423 verified and 353 extracted;
1,019 were rejected during opening and 153 had no complete frame to open.
All ten baseline seeds reached extraction. There were **zero process failures,
timeouts or asserted invariant failures**. Ordered input stream SHA-256:
`70abcbec242c88fde6469a57390e23c0a31917e4e4d4ecd615150a604154f316`.
The initial unoptimized exploratory run was stopped after the adapter change;
only the complete post-fix run is used as evidence.

Budgets: at most 128 KiB input, four 32 KiB frames, 128 entries and 256 KiB
declared output; 250 ms cooperative open/test/extract cancellation, 3 s external
wall timeout, 2 s CPU, 1 GiB address space and 2 MiB per-file limit per process.
The exact input sequence is reproducible; timing and cancellation counts can
vary by host. This is short deterministic mutation fuzzing, **not coverage-guided
fuzzing, sanitizer coverage, an exhaustive compatibility proof or a stability gate**.

## Commands

Focused checks passed:

```sh
cargo fmt --all -- --check
cargo clippy -p arca-rar --all-targets --features rar -- -D warnings
cargo test -p arca-rar --features rar
bash arca-rar/tests/disk-full.sh
cargo clippy --locked --manifest-path arca-rar/fuzz/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path arca-rar/fuzz/Cargo.toml -- --check
```

The adapter suite has 41 passing tests; two ENOSPC tests are ignored during a
normal run and both pass through the isolated script above. Linux user/mount
namespaces and tmpfs mounts are required for that script, not root filesystem
filling or privileged host mounts.

Full regression commands (status recorded after completion):

```sh
cargo clippy --workspace --all-targets --features arca-rar/rar,arca-cli/rar,arca-gui/rar -- -D warnings
cargo test --workspace --features arca-rar/rar,arca-cli/rar,arca-gui/rar
cargo build -p arca-cli --features rar
ARCA_TEST_RAR=1 ARCA="$PWD/target/debug/arca" UNRAR=/path/to/official/unrar bash interop.sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --no-default-features
cargo build --release
bash interop.sh
git diff --check
```

## Explicit gaps and limits

- No Windows/macOS execution or native UI interaction here. Windows reparse
  integration remains ignored pending an appropriately privileged Windows host.
- No independent vintage-encoder regeneration, RAR7 revision-1 codec fixtures,
  true Unpack20 audio blocks, exhaustive PPMd/filter combinations, long solid
  histories, real-world corpus, huge actual output, near-limit 256 MiB dictionary
  decode, or long-running coverage-guided/sanitizer campaign.
- RAR4 encrypted headers lack a reliable password verifier. Wrong passwords can
  report `Format` rather than `BadPassword`; the test records this ambiguity and
  still requires no publication or replacement of existing files.
- No universal RAR set UUID, and older formats lack reliable volume ordinals.
  Matching unsplit metadata cannot establish cryptographic set identity.
- The inherited RAR3 encrypted split pair still has inconsistent first-volume
  flags and is rejected unmodified; its temporary correction is explicitly not
  independent producer evidence. New genuine RAR4 encrypted sets need no fixup.
- ENOSPC and publication behavior were checked on Linux tmpfs only; this does
  not prove Windows locks/antivirus behavior, quota exhaustion, power-loss
  durability, races with external filesystem writers, or directory rollback.
- These checks do not answer the open provenance questions. Creation, mutation,
  repair, recovery and associations remain out of scope.
