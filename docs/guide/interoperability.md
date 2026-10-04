---
description: What interop.sh verifies against unzip, tar, xz and 7-Zip.
group: Reference
order: 13
keywords: interop compatibility 7-zip unzip tar xz winrar nanazip sha-256
---

# Interoperability

An archiver is only useful if other tools can open what it writes. `interop.sh`
round-trips contents and compares hashes; it also checks rejection and cleanup.
Its final summary reports the count for the selected build and available tools.

## What interop.sh checks

- What Arca writes is read by `unzip`, `tar` and 7-Zip, at all four levels.
- What `zip`, `tar` and 7-Zip write is read by Arca without losing a byte.
- An archive Arca encrypted with AES-256 opens in 7-Zip, and the other way round.
- Adding and removing the password of an existing ZIP archive, Arca’s own or 7-Zip’s.
- An altered ZIP byte is caught by the CRC, or by the AE-2 HMAC when encrypted.
- 7z Copy/LZMA2 at all levels, passwords and hidden headers in both directions.
- External solid LZMA/LZMA2 inputs, Unicode names, empty entries and conflict handling.
- Wrong-password 7z extraction leaves the destination absent or unchanged.
- An entry with `../../` is rejected instead of writing outside the destination.
- The RAR/CBR reader fixtures, also through official UnRAR when `UNRAR` points at it (`ARCA_TEST_RAR=0` skips them).
- `.tar.xz` at all four levels and with one or four threads opens in `xz -t` and `tar -J`, and Arca reads what `tar -cJf` writes, also named `.txz`.
- A standalone `.xz` round-trips with `xz`, including every `--check` type and concatenated streams. Two inputs are refused.
- A TAR inside a file named `.xz` is listed as TAR.XZ; a corrupt `.xz` fails and publishes nothing, and a truncated `.tar.xz` fails.

```sh
bash interop.sh
```

RAR archives Arca creates are checked by a separate seeded harness: it builds
trees at every level, hashes the source and the trees extracted by Arca,
official UnRAR and 7-Zip, and records the result as JSON. Without the two
decoders it reports explicit skips; CI downloads pinned copies and fails if
they are missing.

```sh
cargo build --release -p arca-cli
UNRAR=/path/to/unrar SEVENZIP=/path/to/7zz python3 arca-rar/tests/create-stress.py --require-tools
```

## Zstandard in ZIP

Zstandard is ZIP method 93: registered in the specification, but not yet read by classic `unzip`. That’s why `-c auto` uses Deflate in a `.zip`, so that anything can open it. Zstandard is asked for by hand, and will be the default once there is a native format.

## Encryption

ZIP AES-256 uses WinZip AE-2. Legacy ZipCrypto archives open with their password
and can be upgraded with `arca password`. 7z uses AES-256-CBC/SHA-256 with CRC,
not authenticated encryption; encrypted CRC failure can also mean corruption.

```sh
7z t -p"a password" secret.zip        # 7-Zip reads what Arca encrypted
```

## Parser regression checks

```sh
cargo test -p arca-7z
cargo test -p arca-cli --test sevenz
cargo test -p arca-xz
cargo test -p arca-cli --test xz
cargo test -p arca-7z --test interop -- --ignored
cargo test -p arca-gui -- --ignored
```

The ignored tests require `7z`. The automated suites test every truncated prefix
of representative plain, solid, encrypted and hidden-header fixtures and require
errors, not panics. They also exercise malformed headers and resource limits.
This is regression coverage, not a claim that every malicious archive is covered.
No new 7z performance numbers are claimed. Platform/UI validation must be run
separately from these shell checks.
