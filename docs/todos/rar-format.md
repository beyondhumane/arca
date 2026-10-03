# RAR format (read-only)

Status: experimental implementation behind an opt-in Cargo feature.
Tracks [#3](https://github.com/beyondhumane/arca/issues/3); multivolume assembly
and stable activation remain follow-up work.

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
| `rars` | Yes | Yes, excluded here | Selected, but young and provenance review outstanding |
| UnRAR / Rust `unrar` wrapper | Yes | No | In-process C/C++ parser crosses Arca's safe-Rust parser boundary |
| External `unrar` or 7-Zip | Yes | No | Separate process is useful isolation, but requires an installed executable |
| Official RAR/WinRAR | Yes | Yes | Proprietary tool and separate licensing/distribution terms |

`rars` declares Apache-2.0; that declaration alone is not a provenance audit.
The upstream author's reverse-engineering research and RARLAB's license terms
need independent review before enabling distribution. Disabling `write` is
not a legal clearance for the reader.

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
CI tests both feature states and checks the opt-in reader on all three platforms.

## Before stable activation

- Review source/research provenance and licensing.
- Expand external corpora across RAR generations, compression modes, Unicode
  names, encrypted headers and filtered streams.
- Sustained malformed-input fuzzing with memory/time supervision.
- Native GUI interaction testing, including cancellation and password retries.
- Design explicit volume discovery/missing-volume errors before adding multivolume.
- Revisit resource defaults with representative workloads, not anecdotal benchmarks.
