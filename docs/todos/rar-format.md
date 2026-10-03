# RAR format (read-only)

Status: experimental implementation behind an opt-in Cargo feature.
Tracks [#3](https://github.com/beyondhumane/arca/issues/3) and
[#14](https://github.com/beyondhumane/arca/issues/14); multivolume assembly
and stable activation remain follow-up work. **Distribution remains blocked**
by the investigated provenance/notice questions below, independently of
technical acceptance testing.

## Decision (2026-10-03)

Use `rars = "=0.10.0"` through a dedicated `arca-rar` adapter, with
`default-features = false` and only `encryption` enabled. Normal CLI and GUI
builds do not activate it. The shared `arca_core::Format` distinguishes readable
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
| `rars` | Yes | Yes, excluded here | Selected for opt-in development; provenance/distribution gate unresolved |
| UnRAR / Rust `unrar` wrapper | Yes | No | In-process C/C++ parser crosses Arca's safe-Rust parser boundary |
| External `unrar` or 7-Zip | Yes | No | Separate process is useful isolation, but requires an installed executable |
| Official RAR/WinRAR | Yes | Yes | Proprietary tool and separate licensing/distribution terms |

## Distribution review (2026-10-03)

The [third-party decision record](../plans/rars-0.10.0-distribution.md) audits
the exact crate checksum and source revision, public upstream responses,
research history, official RARLAB terms, feature graph and license files.
The upstream license metadata mismatch is fixed in 0.10.0, and an actual Linux
CLI/GUI compile confirms only `rars/encryption` is enabled.

**Engineering recommendation: `distribution_blocked = true`.** Research history
includes direct UnRAR references and subsequently removed 7-Zip RAR citations;
the AROS reference's claimed license also needs reconciliation. The published
crate has Apache-2.0 `COPYING`, but no complete reader-specific provenance/notice
mapping was found, and the separate research tree lacks an explicit top-level
license. Public author statements acknowledge code-derived research and
disassembly. These are concrete unresolved inputs, not a conclusion that the
reader is unlawful or that independent RAR readers are universally prohibited.

Before enabling normal CLI/GUI distribution, obtain the source-to-reader mapping
and applicable research permissions, resolve the historical-source questions
with qualified review where needed, and assemble the target-specific license
and notice bundle. The decision record lists exact evidence and questions.
Disabling `write` or relying on Apache metadata alone does not lift this gate.
The current opt-in flag is a development boundary, not distribution clearance.
No upstream contact was made during this audit.

## Implemented boundary

- RAR/CBR detection, listing, testing, previews and staged extraction in CLI/GUI.
- Password before encrypted-header listing; solid archives decoded sequentially.
- Explicit header, memory, dictionary and output limits, plus cancellation.
- Private staging and complete verification before publishing selected files.
- Optional CRC32 metadata and a RAR method label, without inventing ZIP method codes.
- Read-only mutation guards, no default file associations.
- Explicit rejection of volume sets, links/special files and ambiguous/unsafe paths.

The [RAR guide](../guide/rar.md) documents exact limits, publication semantics,
commands and remaining exclusions. The [fixture record](../../arca-rar/tests/fixtures/README.md)
identifies independent official-RAR fixtures and the legacy upstream sample.
RAR/UnRAR reference executables are local test tools, not redistributed with
Arca or required by its runtime.
CI tests both feature states and checks the opt-in reader on all three platforms.

## Before stable activation

- Resolve the evidence-based [distribution gate](../plans/rars-0.10.0-distribution.md#evidence-needed-to-lift-the-hold)
  and verify required notices in the actual release artifacts. The initial
  provenance audit is complete; the unresolved questions are not legal clearance.
- Expand external corpora across RAR generations, compression modes, Unicode
  names, encrypted headers and filtered streams.
- Sustained malformed-input fuzzing with memory/time supervision.
- Native GUI interaction testing, including cancellation and password retries.
- Design explicit volume discovery/missing-volume errors before adding multivolume.
- Revisit resource defaults with representative workloads, not anecdotal benchmarks.
