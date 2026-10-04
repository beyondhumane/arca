# RAR format

Status: RAR/CBR reading and new single-volume RAR5 creation enabled by default
in CLI and GUI; existing archives stay read-only.
Tracks [#3](https://github.com/beyondhumane/arca/issues/3) and
[#14](https://github.com/beyondhumane/arca/issues/14). Modern and legacy
multivolume assembly is implemented. The dependency review records provenance
questions, not an established infringement or distribution prohibition.

## Decision (2026-10-03)

Use `rars = "=0.10.0"` through a dedicated `arca-rar` adapter, with
`default-features = false` and only `encryption` enabled. Normal CLI and GUI
builds activate it by default. The shared `arca_core::Format` distinguishes readable
formats from `WRITABLE`; RAR has no creation or mutation path.

This supersedes the earlier 0.9.4 recommendation in this document. Two
premises needed correcting:

- `rars` can create/rewrite RAR, but 0.10.0 splits its writer into a feature
  that Arca excludes. CI checks the unified dependency feature graph.
- The UnRAR license forbids using *its code* to develop a compatible compressor.
  That is not evidence of a universal prohibition on independent RAR writers.
  Arca's read-only scope is an explicit product decision for this integration,
  not a conclusion that no dependency can write RAR.

## Alternatives

| Backend | Read RAR | Write RAR | Reason not selected |
|---|---|---|---|
| `rars` | Yes | Yes, excluded here | Selected safe-Rust reader; exact pin, encryption-only feature graph |
| UnRAR / Rust `unrar` wrapper | Yes | No | In-process C/C++ parser crosses Arca's safe-Rust parser boundary |
| External `unrar` or 7-Zip | Yes | No | Separate process is useful isolation, but requires an installed executable |
| Official RAR/WinRAR | Yes | Yes | Proprietary tool and separate licensing/distribution terms |

## Distribution review (2026-10-03)

The [third-party decision record](../plans/rars-0.10.0-distribution.md) audits
the exact crate checksum and source revision, public upstream responses,
research history, official RARLAB terms, feature graph and license files.
The upstream license metadata mismatch is fixed in 0.10.0, and an actual Linux
CLI/GUI compile confirms only `rars/encryption` is enabled.

Research history
includes direct UnRAR references and subsequently removed 7-Zip RAR citations;
the AROS reference's claimed license also needs reconciliation. The published
crate has Apache-2.0 `COPYING`, but no complete reader-specific provenance/notice
mapping was found, and the separate research tree lacks an explicit top-level
license. Public author statements acknowledge code-derived research and
disassembly. These are concrete unresolved inputs, not a conclusion that the
reader is unlawful or that independent RAR readers are universally prohibited.

The original recommendation to block activation overstated what this evidence
established. The maintainer authorized normal CLI/GUI support with the questions
documented, without claiming legal clearance. Published license texts/notices
for the adapter's dependency branch accompany release packages, and CI checks
them against the lockfile. Investigate additional obligations if concrete
evidence identifies them. No upstream contact was made during this audit.

## Implemented boundary

- RAR/CBR detection, listing, testing, previews and staged extraction in CLI/GUI.
- Password before encrypted-header listing; solid archives decoded sequentially.
- Explicit header, memory, dictionary and output limits, plus cancellation.
- Private staging and complete verification before publishing selected files.
- Optional CRC32 metadata and a RAR method label, without inventing ZIP method codes.
- Read-only mutation guards, no default file associations.
- Same-directory modern and legacy volume discovery from any selected part,
  split-stream assembly, available ordering/identity checks and per-set budgets.
- Deterministic missing/duplicate/inconsistent volume errors; rejection of input
  links/reparse points, special files and ambiguous/unsafe extraction paths.

The [RAR guide](../guide/rar.md) documents exact limits, publication semantics,
commands and remaining exclusions. The [fixture record](../../arca-rar/tests/fixtures/README.md)
identifies independent official-RAR fixtures and the legacy upstream sample.
RAR/UnRAR reference executables are local test tools, not redistributed with
Arca or required by its runtime.
CI tests both feature states and checks the default reader on all three platforms.
The adapter crate keeps an empty default feature set; CLI and GUI independently
enable `arca-rar/rar`. `--no-default-features` still excludes the backend, while
`--no-default-features --features codecs-native` retains the native ZIP/7z codecs.

## Release validation and follow-ups

- [x] Record the [dependency review and activation decision](../plans/rars-0.10.0-distribution.md).
- [x] Enable RAR in normal CLI/GUI builds, update English/Spanish docs and retain
  the feature-off build. No writing, recovery or OS associations are added.
- [x] Include the published adapter dependency license texts/notices in packaging.
- [ ] Inspect the license bundle in each release archive/installer before publishing.
- [ ] On the release candidate, pass workspace formatting, Clippy, feature-on/off
  tests, release builds and the feature audit. Confirm only `rars/encryption`
  is compiled for CLI and GUI on Linux, Windows and macOS.
- [ ] Pass the checked-in interoperability matrix against official UnRAR and
  record producer/tool versions, fixture checksums, metadata and output hashes.
  Explicitly accept or resolve uncovered generations, filters and dictionary sizes.
- [ ] Run the checked-in fuzz campaign with recorded seed, execution count,
  elapsed time and memory/time bounds. Triage every panic, timeout and invariant
  violation; add minimized regressions. A sustained campaign remains useful
  follow-up: the initial bounded run is not exhaustive evidence.
- [ ] Verify missing/corrupt later volumes, unsafe paths, cancellation and staging
  disk-full failures leave the destination untouched; publication failures may
  leave earlier verified files. Run platform-specific reparse and disk-full tests
  with their required host permissions rather than counting ignored tests as passed.
- [ ] Record native GUI interaction checks on Linux, Windows and macOS: opening
  later volumes, full/selected extraction, previews, password retries, cancellation,
  missing-volume messages and unavailable mutation actions. Controller tests and
  cross-platform compilation do not replace interaction checks.
- [ ] Resolve or explicitly document RAR4 encrypted-header bad-password ambiguity
  and validate representative workloads against the published resource limits.

## Creation increment (2026-10-04)

Decided and implemented after the read-only activation above, superseding the
"no writing" scope statement there for new archives only:

- `arca_rar::create_rar` writes **new single-volume, non-solid, unencrypted
  RAR5 archives** at Store/Fast/Normal/Best (`rars` levels 0/1/3/5) through the
  public `rars::rar50` streaming API with `FilterPolicy::None`, so the managed
  writer-memory ledger holds for every member including empty ones.
- `rars` now compiles with `encryption` and `write`; `recovery` and `parallel`
  stay off and `check-features.py` asserts exactly that set. Enabling `write`
  added no dependency the reader branch did not already carry, so
  `RAR-NOTICES.txt` is unchanged.
- `Format::CREATABLE` lists RAR; `Format::WRITABLE` does not. Existing RAR and
  CBR archives stay read-only everywhere, CBR is never created, and every
  unsupported creation option (password, hidden names, solid, volumes,
  recovery, data filters, non-RAR codecs, parallel threads) is refused before
  any output exists instead of being ignored.
- Nothing is ever replaced: the archive is staged beside the output, verified
  with the reader, and published with `persist_noclobber`.
- Validation: unit and integration tests in `arca-rar`, `arca-cli` and
  `arca-gui`, plus the seeded CLI harness `arca-rar/tests/create-stress.py`
  (bounded smoke in CI against official UnRAR 7.12 and 7-Zip 24.09; `--stress`
  for the larger campaign). Results and gaps: [RAR_CREATE_VALIDATION.md](../../arca-rar/RAR_CREATE_VALIDATION.md).
- Still out of scope: modifying existing archives, repair/recovery, encrypted,
  solid or multivolume output, RAR4 output, CBR creation and OS associations.
  The CLI cancels RAR creation cooperatively on SIGINT/SIGTERM (`ctrlc`, flag
  read by the walk and the progress callback); ZIP/7z creation still has no
  signal handler and is a separate follow-up.
