# 7z format

Status: not started. Tracked in [#4](https://github.com/beyondhumane/arca/issues/4).

This document records a decision; the design is not settled. The first question is whether to hand-write the format or take it from a library, and the answer shapes everything else.

It lives in `docs/todos/` with the other pending work. Once the approach is chosen and the phases are in order, the design moves to `docs/plans/`.

## Why this document exists

It started with a question about an encrypted ZIP: its file names are visible without the password. That is not a bug in Arca or in the program that created the archive. ZIP never encrypts its central directory, with ZipCrypto or with AES-256, because the format has nowhere to put it. Names, sizes, dates, CRCs and the folder structure are always readable.

If the listing itself ever has to be hidden, the archive has to be something other than ZIP. Two formats can do that, but only one is viable:

| Format | Hides names | Can be implemented |
| --- | --- | --- |
| ZIP (ZipCrypto or AES-256) | No, never | Already done |
| 7z with `-mhe=on` | Yes, the header is encrypted | Yes: open format, LGPL reference implementation, LZMA SDK in the public domain |
| RAR with `-hp` | Yes | No: win.rar GmbH owns the algorithm. The `unrar` source is published, but its license explicitly forbids using it to build a compatible compressor. Reading would bring in that license and C code; writing requires a commercial license |

RAR is ruled out **as a format Arca writes**. Reading it is legal and has its own document: `rar-format.md`. Three of the underlying changes this work requires are the same ones that one requires, and whichever lands first pays for them.

## What 7z brings besides hiding names

- **Better ratio.** LZMA2 compresses text and binaries noticeably better than deflate.
- **Solid compression.** Entries are compressed as one continuous stream, which makes a big difference for a folder of similar files. It has a cost, and that cost breaks the current model; see below.
- **A format people expect.** It is the third option in the create dialog, after ZIP and TAR.

## The two decisions to make before writing code

### 1. Hand-written or `sevenz-rust2`

`arca-zip` is 2,900 lines written here; `arca-tar` is 299. A hand-written `arca-7z` would look nothing like the latter. LZMA2, the *folder* model (chained coders), the encoded header and AES-256-SHA-256 add up to a whole format, not a variant of an existing one.

Facts about `sevenz-rust2`, checked in its `Cargo.toml` and `CHANGELOG.md` rather than assumed:

- Apache-2.0 license, compatible with the project.
- Pure Rust. Version 0.8.0 (2025-02-25) removed the remaining `unsafe` from both `sevenz-rust2` and `lzma-rust2`. That settles the project rule about parsing untrusted archive bytes without `unsafe`, but it must be checked again on the exact version that gets pinned.
- Encrypted headers are supported: "Added support for encrypted headers" in 0.6.0 and "Support write encoded header" in 0.4.3. That covers `-mhe=on` for both reading and writing, which is exactly what prompted this document.
- Encryption: AES-256-SHA-256 only, which is the only one 7-Zip uses in practice. The password is validated while parsing the header, the same way `arca_zip::check_password` works, so the GUI dialog does not change.
- Its default dependencies include `bzip2` and `zstd`, which are C. If it is adopted, they go behind the existing `codecs-native` feature, and the `--no-default-features` build stays pure Rust.
- **It raises the edition.** It requires `rust-version = "1.93"` and `edition = "2024"`; the workspace is on 1.95 and 2021, so the MSRV is already covered (GPUI needs 1.95). The edition is per crate, so Arca's crates can stay on 2021.

Recommendation: adopt it. Writing a correct LZMA2 is not where Arca's value lies, and an almost-correct LZMA2 produces archives nobody else can open.

### 2. What to support, and in what order

Read before write. A 7z that opens correctly is useful on day one; one written incorrectly is a lost archive. Proposed order:

1. List and extract `.7z` without a password.
2. Extract password-protected `.7z`, including encrypted headers.
3. Create `.7z` with LZMA2 and the levels the interface already has.
4. Create `.7z` with a password and the option to hide names.

Changing the password of an existing `.7z` is out of scope for the first round. In ZIP it works without recompressing, because encryption sits on top of the already compressed bytes, per entry. In a 7z with solid blocks, the whole block has to be rebuilt.

## What it touches in the current tree

Measured on today's code, not estimated:

- **`arca-core::Method`** (`arca-core/src/lib.rs:76`) only knows `Store`, `Deflate` and `Zstd`. At least LZMA2 and BZip2 are missing. Watch out for `Method::code()`: it returns the **ZIP** method number, which means nothing in 7z. Either document it as ZIP-specific or split it. What the interface displays is `name()`.
- **`Format` is duplicated** in `arca-gui/src/model.rs:15` and `arca-cli/src/main.rs:162`, with 60 uses of `Format::*` between them. Adding a variant touches both, plus the 7 `match format` in `arca-gui/src/archive_ops/io.rs`. Before touching it, consider whether the enum should live once in `arca-core`.
- **`Entry`** works as is. In 7z everything encrypted is AES-256, so `encrypted` is true and `zipcrypto` stays `false`.
- **The GUI password flow inverts.** Today (`arca-gui/src/controller/mod.rs`, `Message::Listing`) the archive is listed first and the password asked for afterwards, because a ZIP listing is always readable. With an encrypted header there is no listing to show until the password is correct: the order becomes ask, open, and only then fill the window. This, not the codec, is the fundamental change in this work.
- **Per-entry parallel extraction does not apply.** `arca-zip` opens the file once per thread and extracts a different entry on each, because a ZIP offers direct access through its central directory. In a solid 7z block, extracting entry 40 means decompressing the 39 before it. Progress reporting and selective extraction have to be redesigned: extraction works per block, not per entry.

## Checks

The same as everything else, plus what this format needs:

- `interop.sh` gains a section that creates with Arca and verifies with `7z t`, then creates with `7z` and extracts with Arca, byte for byte both ways. With a password, without one, and with `-mhe=on`.
- A `.7z` truncated at every possible offset must not cause a `panic`, like the existing ZIP tests. If a library does the parsing, the test is still ours: it checks that the error arrives as an error.
- Wrong password: a clear message and not one byte written to the destination.

## The cost of not doing it

Low. ZIP with AES-256 covers "nobody can read the contents", which is what nearly everyone asks for. Hiding names is a real but niche requirement, and people who have it usually already have 7-Zip installed. This document exists so the decision is made with the numbers in front of us, not to promise the work.
