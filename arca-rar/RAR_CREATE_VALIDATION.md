# RAR creation validation record

Linux x86_64 (8 CPUs), 2026-10-04; Rust 1.97.1, Python 3.12, pinned
`rars 0.10.0` with `encryption,write`. Implementation: adapter commit
`3b1c8d5b6e4a22d7289320324e5149b1a796754f`, CLI `0b28666`, GUI `21ca714` plus the
integration dialog change (RAR shows its fixed compressor and a no-encryption
policy note instead of disabled ZIP controls), merged
on the `devin/1791110803-rar-create-integration` branch. This is engineering
evidence for that revision, not a legal conclusion, a release approval or a
claim of WinRAR equivalence. The earlier [read-only record](RAR_VALIDATION.md)
is unchanged and its measurements were reader tests only.

Scope under test: creation of new single-volume, non-solid, unencrypted RAR5
archives at Store/Fast/Normal/Best from the CLI (`arca create x.rar ...`) and the
GUI job path, never modification of existing archives. Everything else named in
the guide as unsupported must be refused before output exists.

## Reproducible evidence

| Surface | Coverage | How |
| --- | --- | --- |
| Adapter | Every level round-trips through the reader; empty files/dirs under the managed memory ledger; malformed, duplicate, case-colliding and prefix-colliding names; links and special files; existing outputs never replaced, including one that appears mid-write; cancellation before and during the write leaves nothing; sources that change size/mtime/identity or are swapped for pipes/links at reopen fail without output; size/count/header budgets checked before reading; seeded stress trees; external tool read-back when a proven decoder is present | `cargo test -p arca-rar --features rar` (writer.rs, 62 tests with the feature, 2 without) |
| CLI | Bounded iterative traversal (100000 entries counted before any directory is enumerated, 4096-byte names, 16 GiB aggregate) with injected bounds, wide and deep trees; refusals for password, hidden names, non-RAR codecs, `-j > 1`, `.cbr`, existing output, links, duplicates, output aliasing; feature-off errors | `cargo test -p arca-cli` with and without default features |
| GUI | Selector feature gating; RAR policy note in English and Spanish replacing the ZIP codec and password controls; job options per level; password restore for ZIP; create then open read-only, Test, Add rejected with bytes unchanged; unsupported options leave no output; existing file never replaced for Cancel/Replace/Rename; cancellation leaves nothing; symlink refusal; traversal ceilings, cancellation between members, 400-deep tree without recursion | `cargo test -p arca-gui` with and without default features |
| CLI harness | Seeded, process-isolated campaign below, comparing Arca, official UnRAR 7.12 and 7-Zip 24.09 extractions by exact tree hash | `arca-rar/tests/create-stress.py` |

### Harness: `python3 arca-rar/tests/create-stress.py --stress --require-tools`

Release CLI (`cargo build --release -p arca-cli`, `arca 0.7.2`), seed 20261004,
687 child processes, each under `RLIMIT_AS` 4 GiB and a 120 s wall clock (x4
for the 10k-entry archive, x8 for the 128 MiB stream). Tools:
`UNRAR=/tmp/rar-tools/rar/unrar` (`UNRAR 7.12 freeware`, from
`rarlinux-x64-712.tar.gz`, SHA-256
`630d9a9dd131367273667bee079ad103f469f1b7cdbc9b42a4f283cc2993bab2`) and
`SEVENZIP=/tmp/rar-tools/7z/7zz` (`7-Zip 24.09`, from `7z2409-linux-x64.tar.xz`,
SHA-256 `914c7e20ad5ef8e4d3cf08620ff8894b28fe11b7eb99809d6930870fbe48a281`).
Both decoders are first proven on the WinRAR-made fixture `tests/fixtures/plain.rar`
(the VM's p7zip 16.02 fails that probe and is therefore not used). Result:
10 pass, 1 warn, 0 fail, 0 skip; `report.json` holds per-process arguments,
exit codes, durations and `ru_maxrss`.

| Check | Result |
| --- | --- |
| levels | 13-file/9-directory tree (text, random, repetitive, zeros, empty file, one byte, empty dirs, five-deep nesting, spaces, accented, Japanese, Cyrillic and emoji names) at store/fast/normal/best: listing equals the expected member set, `arca test` passes, Arca, UnRAR and 7-Zip extractions hash identical to the source. Store 241,250 B for 239,381 B of payload; fast 108,809 B; normal and best 107,022 B. |
| compression-ratio | 4 MiB generated text: store 4,194,473 B (never below payload, overhead under 64 KiB); fast 17.8 %, normal 13.0 %, best 12.1 % of the input; best not larger than fast; two creates of the same tree produce identical bytes; `-c store` equals `-l store`. Create times on this VM: store 0.05 s, fast 0.17 s, normal 5.4 s, best 21.8 s; peak RSS 35 MiB. |
| refusals | Existing output (bytes unchanged), `.cbr`, `-p`, `--hide-names`, `-c zstd/deflate/lzma2`, `-j 4`, output inside a source, source equal to output, duplicate and case-colliding names, symlink and FIFO sources, missing source, missing parent, unknown suffix: every one exits non-zero with the documented message and the directory listing is unchanged (no output, no staged file). |
| limits | Sparse 4 GiB + 1 B member refused "exceeds 4 GiB"; five sparse 4 GiB - 1 B members refused "exceeds 17179869184 bytes"; a 4,400-byte member path is refused, but by the OS (`File name too long`) before the writer's 4096-byte ceiling can report, so that ceiling is evidenced by the injected-bounds unit tests only. Nothing written. |
| entry-ceiling (stress only) | 100,000 files in 101 directories refused with "would hold more than 100000 members", nothing written, 2.8 s. |
| corruption | 16 single-bit flips at seeded offsets, 6 truncations, an empty file, random bytes and a ZIP signature: `arca test` and `arca extract` fail every time and the destination stays empty; UnRAR and 7-Zip also reject all 25 mutated archives. |
| disk-full | In a private 1 MiB tmpfs (user+mount namespace), creating a 3 MiB store archive fails with ENOSPC; the tmpfs afterwards holds only the untouched pre-existing `existing.rar`, no output and no staged file. |
| many-entries | 10,000 files (text/random/repetitive/empty, 1,123,686 B) in 37 directories at fast: 1,488,499 B archive, create 0.68 s, test 0.09 s, extract 3.0 s, peak RSS 33 MiB; UnRAR and 7-Zip trees identical. |
| large-stream | 128 MiB stream of 1 MiB repetitive/random/text/zero blocks plus a text tail (134,221,824 B) at normal: 36,030,497 B archive, create 71.2 s, test 1.7 s, extract 1.9 s, peak RSS 91 MiB; UnRAR and 7-Zip trees identical. |
| cycles | 100 seeded create/list/test/extract cycles (1 to 12 members, five content kinds, nested and Unicode names, 31 store / 22 fast / 23 normal / 24 best): 1,500 members, 18,228,538 B in, 8,963,807 B out, every cycle hash-identical through Arca; every fifth cycle (20) also through UnRAR and 7-Zip. |
| interrupt | **warn.** SIGINT and SIGTERM 0.5 s into a 128 MiB `best` write: no file ever appears at the output path, but the staged `.arca-*.rar.part` survives because the CLI has no signal handler. The same kill leaves `.tmp*` files for `.7z` and a partial `.zip` on the current code, so this is an existing CLI-wide gap, not a RAR regression; cooperative cancellation is covered by the adapter and GUI tests. |

The bounded smoke subset (`create-stress.py` without `--stress`: 400 files, 8 MiB
stream, 8 cycles) passed with the same tools and is wired into the Linux
`build` job of `ci.yml` with `--require-tools`, downloading the two decoders by
pinned SHA-256 for that job only.

## Workspace commands

Status recorded after completion on the merged branch:

```sh
cargo fmt --all -- --check                                                           # pass
cargo clippy --workspace --all-targets -- -D warnings                                # pass
cargo test --workspace                                                               # pass, see counts below
cargo clippy --workspace --all-targets --no-default-features --features codecs-native -- -D warnings  # pass
cargo test --workspace --no-default-features --features codecs-native                # pass
cargo build --release --no-default-features                                          # pass
cargo build --release                                                                # pass
python3 arca-rar/tests/check-features.py                                             # encryption,write in every default build; no rars in opt-outs
python3 packaging/rar-notices.py --check                                             # bundle unchanged by the write feature
ARCA=$PWD/target/release/arca UNRAR=/tmp/rar-tools/rar/unrar bash interop.sh                   # 120 passed, 0 failed (RAR reader matrix included)
UNRAR=... SEVENZIP=... python3 arca-rar/tests/create-stress.py --stress --require-tools        # 10 pass, 1 warn, 0 fail
UNRAR=... SEVENZIP=... python3 arca-rar/tests/create-stress.py --require-tools                 # smoke rerun after the final merge: 9 pass, 1 warn, 0 fail
git diff --check                                                                     # clean
```

Test totals summed over every workspace test binary: default features 434
passed, 0 failed, 7 ignored; `codecs-native` without defaults 343 passed, 0
failed, 5 ignored. The single `warn` in both harness runs is the CLI
interruption case in the harness table above.

## Explicit gaps and limits

- No Windows or macOS execution of the harness and no native GUI interaction
  here; the GUI creation path is covered by controller and job tests only.
  Windows reparse-point refusal at source reopen is compiled, not exercised.
- Official WinRAR/RAR itself was not run; UnRAR 7.12 and 7-Zip 24.09 are the
  independent decoders. No claim is made about other decoders, older UnRAR
  versions or RAR features Arca does not emit.
- Throughput is low at the higher levels on this VM (about 0.8 MiB/s at normal
  and 0.2 MiB/s at best on text, 1.9 MiB/s at normal on the mixed stream). These
  are single measurements for scale, not benchmarks; reproduce with the harness
  command above.
- The 4096-byte member-name ceiling cannot be reached through the Linux
  filesystem (PATH_MAX intervenes first); it is unit-tested with injected bounds.
  The 16 GiB spool ceiling, the 64 MiB header budget and the 256 MiB writer
  memory ledger were exercised by adapter unit tests, not by the CLI harness.
- Disk-full was checked on a Linux tmpfs only; quota exhaustion, Windows
  locks/antivirus, power loss and concurrent writers were not tested.
- Killing the CLI mid-write leaves the staged part file (see interrupt above).
- The harness does not fuzz the writer's input beyond seeded trees, and the
  corruption check mutates Arca's own output; it is not a coverage-guided
  campaign.
- These checks do not answer the open provenance questions in the
  [distribution review](../docs/plans/rars-0.10.0-distribution.md).
