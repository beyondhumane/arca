# RAR format (read-only)

Status: not started, with a recommendation. Tracked in [#5](https://github.com/beyondhumane/arca/issues/5).

Unlike 7z, the core decision here is already made by the license: **Arca reads RAR and never writes it**. What remains is choosing how to read it.

## Why this document exists

The 7z document came from wanting to hide file names. This one comes from something simpler: `.rar` files are out there. Downloads, attachments, comics in `.cbr`, archives from fifteen years ago. Arca cannot open them today, and users have to install something else for a job an archiver is supposed to do.

It is not a new feature. It is the most visible gap left in the format list.

## What the license allows

The UnRAR license is explicit:

> UnRAR source code may be used in any software to handle RAR archives without
> limitations free of charge, but cannot be used to develop RAR (WinRAR)
> compatible archiver and to re-create RAR compression algorithm, which is
> proprietary.

Opening, listing, testing and extracting are allowed; creating is not. So Arca reads `.rar` and does not write it. That is not a temporary limitation waiting for a phase 2; it is the end of the road.

The README should say so when this lands. A format that shows up when opening but not when compressing looks like an oversight unless it is explained.

## The three options, checked

| Option | What it is | The catch |
| --- | --- | --- |
| Delegate to an installed binary | Call `unrar.exe` or `7z.exe` if present | Zero dependencies, no license to carry, and hostile bytes are parsed in **another process**. But it only works if the user already has one, which is exactly the user who does not need Arca |
| FFI to unrar (`unrar` crate) | What almost everyone uses, and the most battle-tested | C++ parsing untrusted archives **inside** our process. It clashes with `unsafe_code = "forbid"` and with the rule against parsing untrusted bytes with `unsafe`. It also means building C++ on three platforms and shipping the UnRAR license in the installer |
| Pure Rust (`rars`) | A clean implementation of the format, with no RARLAB code | It is young. That is the only catch, and it is not a small one |

### Facts about `rars`, checked in its source tree rather than assumed

- Version 0.9.4, Apache-2.0. It does not carry the UnRAR license because it uses none of its code; it has its own format research repository.
- `unsafe_code = "forbid"` in the workspace lints, the same rule as here.
- Dependencies: `aes`, `hmac`, `sha1`, `sha2`, `zeroize`, `getrandom`, `aho-corasick` and `rayon`. All pure Rust, and half of them are already in Arca's tree.
- A `fuzz/` folder with fuzzing targets, which is what you want to see before trusting a parser of foreign archives.
- `rust-version = "1.87"`, edition 2021. The workspace is already on 1.95, so it raises nothing.
- Covers RAR 1.3 through RAR 7: solid archives, per-file encryption and encrypted headers.
- **It also writes**, and no flag can leave the writer out of the build. That does not oblige us to anything, since RAR would not appear in the compress dialog, but the code is linked even if never called.
- Its own README says it started as an agentic development experiment and "could use more testing at volume". Take that at its word: it is the reason for the build flag in phase 1.

Recommendation: `rars`, read-only, behind a cargo feature so it can be pulled in a release if it goes wrong. A bug in it arrives as an error in safe Rust, not as memory corruption, which is exactly the line the project has drawn.

## What it touches in the current tree

Measured on today's code:

- **It would be the first read-only format.** Today `Format` assumes everything that opens can also be created: it appears in the compress dropdown, in `extension()` and in the `match format` of `arca-gui/src/archive_ops/io.rs` (7 of them). It has to split into two sets, readable and writable. The enum is also duplicated in `arca-gui/src/model.rs:15` and `arca-cli/src/main.rs:162`, with 60 uses of `Format::*`. Before adding anything, consider whether it should live once in `arca-core`.
- **`arca-core::Method`** (`arca-core/src/lib.rs:76`) only knows `Store`, `Deflate` and `Zstd`. RAR brings its own per generation (15, 20, 29, 50), plus PPMd. The GUI *Method* column displays `name()`, so it only has to be able to say it; `code()` is a ZIP number and means nothing here.
- **`Entry`** works as is: `encrypted` true and `zipcrypto` false.
- **The password flow inverts**, just as with 7z. With an encrypted header there is no listing to show until the password is correct, and today (`arca-gui/src/controller/mod.rs`, `Message::Listing`) the archive is listed first and the password asked for afterwards.
- **Per-entry parallel extraction does not apply** to solid archives: extracting entry 40 means decompressing the 39 before it. Same as 7z.
- **Multi-volume** is new. A `.part1.rar` or `.r00` means opening its siblings on disk. The whole current model assumes "an open archive is one path"; that has to change, along with the check for what happens when a volume is missing.

Three of these five points are the same ones 7z requires. Whichever lands first pays for them, and the second arrives in a tree that already has them.

## Phases

1. List `.rar` (and `.cbr`) without a password, behind a build flag.
2. Extract, including solid archives.
3. Passwords: per file and with encrypted headers, with the dialog flow already inverted.
4. Multi-volume.

No writing. Ever.

## Checks

- `interop.sh` gains a read-only section. Archives created with `rar` and with `7z` are extracted by Arca and compared byte for byte with what the real `unrar` produces. With and without a password, solid and non-solid.
- A `.rar` truncated at every possible offset must not cause a `panic`, like the existing ZIP tests. A library doing the parsing does not remove the test: it checks that the error arrives as an error.
- A missing volume must be reported by name, not fail in some arbitrary way.
- Wrong password: a clear message and not one byte written to the destination.

## The cost of not doing it

Higher than with 7z. Someone who finds a `.rar` that Arca will not open does not think "what a shame, it is a proprietary format". They install something else and stick with it. Opening RAR does not make Arca a better archiver, but not opening it costs the whole user.
