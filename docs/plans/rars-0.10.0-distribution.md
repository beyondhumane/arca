# Third-party decision: rars 0.10.0 distribution

Review date: 2026-10-03. Arca baseline:
[`c8918c8f7d9804a98cda45aaf84f8e7483082150`][arca-baseline].
Tracks [#14](https://github.com/beyondhumane/arca/issues/14).

**Engineering recommendation: `distribution_blocked = true`.** Do not enable
`rars` in normal CLI/GUI releases yet. The issue is incomplete reader-specific
provenance and redistribution evidence, not merely a pending TODO or the fact
that upstream also implements writing. Keep the current opt-in development
boundary; it is not clearance to distribute experimental RAR-enabled binaries.

This is a factual dependency review, not legal advice, a finding of infringement,
or a guarantee of legal clearance. Independent RAR readers are not universally
prohibited by the documents reviewed. Conversely, Apache-2.0 metadata and
excluding the writer do not settle the provenance of the included reader.

## Exact artifact and technical boundary

- Dependency: [`rars = "=0.10.0"`][crate], with `default-features = false` and
  `features = ["encryption"]` in [arca-rar/Cargo.toml][adapter].
- Published [crate archive][crate-download] SHA-256:
  `0a1fb39150e8a5b3db405961d6814c6cdfd4965701ba9d9a116ed72db2ad8bbf`.
  It matches [Cargo.lock][lock].
- The archive's `.cargo_vcs_info.json` identifies source commit
  [`8aa1429cc51ba248d81e32ad5b5eac6e4ec63ba2`][rars-source], path `crates/rars`.
  Its `src/` tree compared byte-for-byte equal to that checkout's source tree.
- Published metadata declares Apache-2.0. The archive contains `COPYING` with
  that license, SHA-256
  `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`.
  No separate `NOTICE` or third-party license inventory was found in the crate.

The Linux `x86_64-unknown-linux-gnu` check of both CLI and GUI succeeded with
Rust/Cargo 1.97.1. Cargo's compiler-artifact record for `rars 0.10.0` has exactly
`features: ["encryption"]`. Host and all-target feature resolution likewise
showed no `write`, `recovery`, `parallel`, or `rars` default feature. Reproduce:

```sh
cargo tree --locked -p arca-cli -p arca-gui --features rar -e features -i rars
cargo tree --locked -p arca-cli -p arca-gui --features rar --target all -e features -i rars
cargo tree --locked -p arca-cli -p arca-gui --features rar -e normal,build
cargo check --locked -p arca-cli -p arca-gui --features rar --message-format=json
```

This verifies the compilation graph, not a final-binary symbol audit or runtime
qualification. All-target resolution is not a Windows/macOS build test. The
downloaded crate still contains feature-gated writer source; source redistribution
needs its own scope review even when that code is absent from compiled artifacts.
Arca remains read-only: no creation/mutation, repair/recovery or automatic file
associations are proposed by this decision.

## Concrete provenance evidence

### Public upstream responses and corrections

1. In [issue #21][license-issue], the author initially discussed WTFPL and
   uncertainty about copyright in generated code (2026-07-10). On 2026-07-13 the
   author wrote: "It's generated from spec" and "some of the spec was based on
   dissassembly". These are public statements, not assurances about downstream
   redistribution or permissions from other rights holders.
2. [Issue #39][metadata-issue] reported conflicting Cargo/license declarations.
   [Commit `1a649fe31561d42828d158e2b9d2deded0583c41`][license-fix], dated
   2026-09-06, standardized the project on Apache-2.0 and included `COPYING` in
   distributions. That correction is present in 0.10.0. Treat the metadata/file
   mismatch as resolved, not as an outstanding blocker. It does not demonstrate
   that every research or implementation input can be relicensed.
3. The [author's project account][author-blog] describes "pulling code from free
   decompressor sources in the wild - unar, libarchive, UNRARLIB, plus random web
   pages and folk lore", followed by specification generation. It also describes
   obtaining DOS/Windows RAR binaries and using Ghidra and DOSBox-X. The account
   says a registration-bypass investigation was removed from history and the
   feature was not implemented. This review did not recover or reproduce that
   work; the statement is a reason to ask about separation of inputs, not evidence
   that such a feature is shipped by Arca.

No upstream contact was made for this review. The responses above and the
research corrections below are already public. No reader-specific provenance
attestation or permission resolving the questions below was found in the sources
reviewed; this is not an assertion that no other correspondence exists.

### Research chain and a reader-side example

The linked [`bitplane/rar-research` tree][research] was inspected at
`c8a280d28a466e13d05812a2415ab0359950c823`. No repository-level `LICENSE` or
`COPYING` grant was present. Its specifications describe independent documentation
from public sources, but public availability alone does not establish reuse
rights, and the tree is not mapped to the exact inputs used for each crate file.

- [RAR 1.3/1.4 specification][spec13] calls its AROS reference
  `aminet/util/arc/compatible RAR reader/`, described as "public/AROS license".
  The actual AROS `contrib` tree inspected at
  `09ac4266a8013f487938240b466a82524f304b23` instead has
  [`aminet/util/arc/unrar/license.txt`][aros-license], assigning copyright to
  Eugene Roshal and imposing the UnRAR compression restriction and documentation
  condition. Its README calls it an UnRAR utility containing the RAR
  uncompression algorithm. The named generic path was not found. Upstream needs
  to identify the exact intended source/version and reconcile this discrepancy;
  this review does not assume an entire AROS repository has one license.
- [Research commit `688d05f828066ea30cf8ef5897367efa3a4a7e80`][removed-unrar]
  removes nine direct `_refs/unrar/` pointers while retaining most of the
  documented behavior. Its predecessor expressly cites
  `unpack15.cpp:493 (Unpack::DecodeNum)` for section 6.6. The published
  [Rust reader's `decode_num`][reader-code] has the same mask, table walk and
  return expression as that pseudocode and the official UnRAR `DecodeNum`.
  Decode tables also match. This identifies a concrete reader-side provenance
  question; algorithmic similarity and format constants alone are not a finding
  of copied protectable expression. Removing citations is not evidence that
  earlier inputs were excluded from the implementation.
- [Research commit `c8a280d28a466e13d05812a2415ab0359950c823`][removed-7zip]
  explicitly acknowledges that earlier 7-Zip RAR decoder citations were not
  independent: those decoders derive from UnRAR. It replaces references, records
  measured filter fingerprints and distinguishes public-domain PPMd/Huffman/LzFind
  algorithms from the restricted RAR decoders. This is a substantive upstream
  correction, but does not inventory what prior research reached `rars 0.10.0`.
- Other named references have their own obligations: the inspected
  [libarchive RAR reader][libarchive] has a two-clause BSD notice for Tim Kientzle
  and Andres Mejia; [XADMaster's RAR VM source][xad] is LGPL-2.1-or-later with a
  MacPaw copyright notice. The research's "LGPL-compatible" label for
  `binref/refinery` is not a precise license identification; the inspected
  [refinery license][refinery-license] is BSD-3-Clause.
- The blog's UNRARLIB input has no exact version/file mapping in the reviewed
  provenance materials. Its [official FAQ][unrarlib-faq] states that Eugene Roshal
  permitted **UNRARLIB 0.4.0** under GPL and the custom unrarlib license, explicitly
  not extending that permission to other RARLAB source. The [license page][unrarlib]
  offers a [GPL v2 text][unrarlib-gpl] or [custom license][unrarlib-license]; the
  latter requires notices, free-of-charge distribution of programs using it, and
  decompression-only use. This is a concrete published permission statement to
  investigate, not evidence that the unspecified input was Apache-2.0.

These are research inputs, not linked Cargo dependencies. Whether protected code
or other material from them was incorporated, and what resulting obligations
apply, remains unresolved. Do not automatically label the whole reader LGPL or
UnRAR-derived, but do not assume generated Rust erases third-party obligations.

## Official RARLAB terms and their limits

- [RAR/WinRAR EULA][eula], clause 10, restricts decompilation/disassembly and says
  neither RAR/WinRAR binary code nor UnRAR source/binary code may be used or
  reverse engineered to recreate the proprietary compression algorithm without
  written permission. Clause 3 also restricts bundling the proprietary trial
  package. Applicability to historical binaries, research, reader-only downstream
  distribution, and any statutory exceptions is not decided by this audit.
- Official [UnRAR source archive `unrarsrc-7.3.1.tar.gz`][unrar-src], whose
  `version.hpp` identifies **7.30 beta 1, 2026-09-09**, includes `license.txt`.
  Paragraph 2 allows use to handle RAR archives, but prohibits using that source
  to develop a RAR-compatible archiver/recreate the compression algorithm. For
  modified-source redistribution it requires the full paragraph in the license
  (or documentation if unavailable) and source comments. Paragraph 3 permits
  distributing UnRAR inside other software packages. These are terms for UnRAR,
  not a blanket ban on independent readers or a requirement to bundle UnRAR.
- [RARLAB's technical note][technote] documents the RAR 5.0 container format.
  It is useful format evidence, not a provenance attestation for this crate or
  a license covering all historic decoder implementations and research.
- [7-Zip's license][seven-license] separately identifies LGPL code with the
  "unRAR license restriction" and states its RAR decompression engine was
  developed from UnRAR. A label such as "7-Zip is open source" is not sufficient
  to classify its individual RAR decoder inputs.

Downloaded-byte SHA-256 values, to distinguish these mutable web sources from
the pinned repository/crate evidence:

| Source | SHA-256 |
|---|---|
| RARLAB EULA HTML | `ad93f08e61a28bfb943716c662a6d08b366a51a603ac3d2df202b83004390055` |
| RARLAB technical-note HTML | `e18581d50673c34e83d14436ba4d14caf15a9e85d74a12024771f71753560f06` |
| UnRAR source archive | `634900842a3737d9cc15bbcc71d4c74cc713437e0bca296a573424fe5f2660ab` |
| UnRAR `license.txt` | `6ecc1687808b7d66b24f874755abfed7464d9751ed0001cd4e8e5d9bf397ff8a` |
| 7-Zip license text | `3a184aa13dc8ad30734e28ff901b478ccbcb5b41be52427a5e7609c8fd9e5ddb` |
| libarchive RAR source | `1adb39e5191bf752bd4794329faf6b21111491307669cdef33ee7b15116f09be` |
| XADMaster RAR VM source | `cf4e659e42f09cf50aaf83bd4c9a96ef0dba4ff838ba9229f2ff8ae88ba97804` |
| UNRARLIB FAQ HTML | `ec293f559b2d8722e55537e2d72f718a4d69f80fc6374036fa000073950cd27a` |
| UNRARLIB custom license | `72ba6c6ac7ee8659732d34bd23734a34cf8a4061076fd964b44339e9c05c565c` |

## Dependency licenses and release notices

The following is an inventory of the inspected Linux `rars` normal/build
dependency branch, including proc-macro dependencies enabled by workspace feature
unification. Exact package checksums are in the [baseline lockfile][lock]. It is
not a complete license bundle for the whole Arca application or other targets.

| Packages and exact versions | Declared license / inspected files |
|---|---|
| `rars 0.10.0` | Apache-2.0; `COPYING`; no separate `NOTICE` found |
| `aes 0.9.3`, `block-buffer 0.12.1`, `cfg-if 1.0.4`, `cipher 0.5.2`, `cmov 0.5.4`, `const-oid 0.10.2`, `cpubits 0.1.1`, `cpufeatures 0.3.1`, `crypto-common 0.2.2`, `ctutils 0.4.2`, `digest 0.11.3`, `hmac 0.13.0`, `hybrid-array 0.4.15`, `inout 0.2.2`, `sha1 0.11.0`, `sha2 0.11.0`, `typenum 1.20.1`, `zeroize 1.9.0` | MIT OR Apache-2.0; `LICENSE-MIT`, `LICENSE-APACHE` |
| `aho-corasick 1.1.5`, `memchr 2.8.3` | Unlicense OR MIT; `UNLICENSE`, `LICENSE-MIT`, `COPYING` |
| `zeroize_derive 1.5.0`, `proc-macro2 1.0.107`, `quote 1.0.47`, `syn 2.0.119` | MIT OR Apache-2.0; `LICENSE-MIT`, `LICENSE-APACHE` |
| `unicode-ident 1.0.24` | (MIT OR Apache-2.0) AND Unicode-3.0; also `LICENSE-UNICODE` |

The adapter also uses `tempfile 3.27.0`. Its Linux normal dependency branch
contains `fastrand 2.5.0`, `getrandom 0.4.3`, `cfg-if 1.0.4`, `libc 0.2.189`,
`once_cell 1.21.4`, `rustix 1.1.4`, `bitflags 2.13.1`, and
`linux-raw-sys 0.12.1`. Their inspected license files offer MIT/Apache-2.0
choices; `rustix` and `linux-raw-sys` also offer Apache-2.0 WITH LLVM-exception.
Thus another dependency's `getrandom` does not imply `rars/write` is enabled.
Windows/macOS target-specific dependencies and the rest of the workspace still
need an inventory for the actual shipping artifacts.

Before RAR-enabled distribution:

- Supply the Apache-2.0 text for `rars`, preserve applicable copyright/attribution
  notices, and meet section 4 requirements for any modified or redistributed
  source and any upstream `NOTICE` subsequently identified. The generic Apache
  appendix is not a third-party attribution inventory.
- Select and include appropriate license texts/notices for the actual shipped
  dependency graph. MIT choices require preserving copyright/permission notices;
  include applicable Unicode data terms rather than reducing the compound
  `unicode-ident` expression to MIT alone. Distinguish build-only tools and
  material incorporated in outputs when determining release obligations.
- Resolve source-derived notice obligations separately from Cargo metadata. If
  BSD, LGPL, UnRAR or other source was incorporated, determine and satisfy the
  applicable obligations; merely appending this decision record is not enough.
- Verify the archive/installer contains the resulting license bundle. The
  [baseline release workflow][release] explicitly copies Arca's `LICENSE` and
  `README.md`, not a `rars`/transitive third-party bundle. This audit does not
  modify packaging or certify existing releases' overall license compliance.

## Reference executables are not product dependencies

RAR/UnRAR used to generate fixtures or cross-check extraction are local reference
tools, not redistributed with Arca. The [fixture record][fixtures] explicitly
documents this boundary; the runtime sources and release packaging inspected do
not invoke, link, download or copy UnRAR executables. This audit only inspected
UnRAR source for provenance/terms; it did not install or run its executable.
Keep RAR/UnRAR binaries, registration material and upstream research working trees
out of releases. Local tool use must separately comply with the relevant terms.

## Evidence needed to lift the hold

1. **Reader provenance:** obtain a versioned upstream file/algorithm mapping for
   the exact 0.10.0 reader, encryption, tables and RARVM code: input project,
   version/file, license, whether expression was copied/adapted or only behavior
   observed, and retained notices. Specifically reconcile the AROS path/license,
   UNRARLIB version, historical UnRAR/7-Zip references and XADMaster inputs.
   Explain whether prior restricted inputs reached code shared with the writer.
2. **Research rights and separation:** identify the actual research revision(s)
   used, their reuse grant and third-party exceptions. Explain what disassembly
   contributed to the distributed reader, and how removed investigations were
   kept out. A new blanket license on research alone does not settle third-party
   rights or the effect of earlier inputs.
3. **Qualified assessment where necessary:** have counsel assess the concrete
   source mapping and relevant historical terms, any permissions and applicable
   interoperability/reverse-engineering exceptions for the intended distribution
   jurisdictions. Do not substitute an author assurance or a build flag for this
   assessment when the underlying evidence remains ambiguous.
4. **Release material:** once the above is resolved, produce and inspect the
   actual target-specific notice/source-compliance bundle. Record the evidence
   and accountable decision here before lifting the distribution gate. If the
   current backend cannot meet it, evaluate a separately scoped alternative or
   remediation rather than silently changing the pin.

These are actionable release gates, not a request to contact upstream without
approval. Successful builds, interoperability tests and additional fuzzing can
support technical readiness but cannot answer these provenance questions. The
separate technical acceptance work remains in the [RAR TODO](../todos/rar-format.md).

[arca-baseline]: https://github.com/beyondhumane/arca/commit/c8918c8f7d9804a98cda45aaf84f8e7483082150
[adapter]: https://github.com/beyondhumane/arca/blob/c8918c8f7d9804a98cda45aaf84f8e7483082150/arca-rar/Cargo.toml
[lock]: https://github.com/beyondhumane/arca/blob/c8918c8f7d9804a98cda45aaf84f8e7483082150/Cargo.lock
[crate]: https://crates.io/crates/rars/0.10.0
[crate-download]: https://static.crates.io/crates/rars/rars-0.10.0.crate
[rars-source]: https://github.com/bitplane/rars/tree/8aa1429cc51ba248d81e32ad5b5eac6e4ec63ba2/crates/rars
[license-issue]: https://github.com/bitplane/rars/issues/21
[metadata-issue]: https://github.com/bitplane/rars/issues/39
[license-fix]: https://github.com/bitplane/rars/commit/1a649fe31561d42828d158e2b9d2deded0583c41
[author-blog]: https://bitplane.net/log/2026/05/rars/
[research]: https://github.com/bitplane/rar-research/tree/c8a280d28a466e13d05812a2415ab0359950c823
[spec13]: https://github.com/bitplane/rar-research/blob/c8a280d28a466e13d05812a2415ab0359950c823/doc/RAR13_FORMAT_SPECIFICATION.md
[aros-license]: https://github.com/aros-development-team/contrib/blob/09ac4266a8013f487938240b466a82524f304b23/aminet/util/arc/unrar/license.txt
[removed-unrar]: https://github.com/bitplane/rar-research/commit/688d05f828066ea30cf8ef5897367efa3a4a7e80
[reader-code]: https://github.com/bitplane/rars/blob/8aa1429cc51ba248d81e32ad5b5eac6e4ec63ba2/crates/rars/src/codec/rar13.rs#L2244-L2260
[removed-7zip]: https://github.com/bitplane/rar-research/commit/c8a280d28a466e13d05812a2415ab0359950c823
[libarchive]: https://raw.githubusercontent.com/libarchive/libarchive/master/libarchive/archive_read_support_format_rar.c
[xad]: https://raw.githubusercontent.com/MacPaw/XADMaster/master/RARVirtualMachine.c
[refinery-license]: https://github.com/binref/refinery/blob/02e4f655f44416e2212a21d38fc6130ee0c7a263/LICENSE.md
[unrarlib-faq]: https://www.unrarlib.org/faq.html
[unrarlib]: https://www.unrarlib.org/license.html
[unrarlib-gpl]: https://www.unrarlib.org/gpl.txt
[unrarlib-license]: https://www.unrarlib.org/unrarlib-license.txt
[eula]: https://www.rarlab.com/license.htm
[technote]: https://www.rarlab.com/technote.htm
[unrar-src]: https://www.rarlab.com/rar/unrarsrc-7.3.1.tar.gz
[seven-license]: https://www.7-zip.org/license.txt
[release]: https://github.com/beyondhumane/arca/blob/c8918c8f7d9804a98cda45aaf84f8e7483082150/.github/workflows/release.yml
[fixtures]: https://github.com/beyondhumane/arca/blob/c8918c8f7d9804a98cda45aaf84f8e7483082150/arca-rar/tests/fixtures/README.md
