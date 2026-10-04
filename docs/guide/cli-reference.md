---
description: Every command, argument, option and default of the arca binary.
group: Reference
order: 11
keywords: cli command line options flags reference help exit status
---

# CLI reference

ZIP, 7z, TAR (plain, gzip and XZ) and standalone XZ are enabled by default. [RAR/CBR reading](rar.md) requires
`--features rar`; it supports `list`, `test` and `extract`, including `-p` for
encrypted data or headers. RAR creation and mutation are always rejected.

```text
Fast, safe archiver

Usage: arca <COMMAND>

Commands:
  create    Create an archive                                          [aliases: c]
  list      List the contents without extracting them                  [aliases: l]
  extract   Extract the contents                                       [aliases: x]
  test      Check integrity without writing to disk                    [aliases: t]
  password  Add, change or remove the password of an existing archive
  bench     Measure the R1 and R2 performance requirements

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`list`, `extract` and `test` also read the [ZIP containers](zip-containers.md) (.apk, .jar, .epub, .cbz and the rest), [ISO 9660 images](iso.md) and, in builds with the [RAR reader](rar.md), .rar and .cbr. Those are read-only: `create` and `password` refuse them.

## create · c

Create an archive. The format is taken from the extension of the output file.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<OUT>` | required | Output archive: .zip, .7z, .tar, .tar.gz (or .tgz), .tar.xz (or .txz), .xz. An .xz takes exactly one file. |
| `<INPUTS>...` | required | Files or directories to include. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd · lzma2. auto uses Deflate in .zip, LZMA2 in .7z, .xz and .tar.xz. |
| `-j, --threads <N>` | 0 | ZIP and XZ threads; 0 means every core. XZ uses as many as fit in 2 GiB. 7z creation is sequential. |
| `-p, --password <PASSWORD>` | none | Encrypt with AES-256. Other tools will ask for it to open the archive. |
| `--hide-names` | off | 7z only: encrypt headers; requires a nonempty password. |

## list · l

List the contents without extracting them.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to read. |
| `-t, --time` | off | Report how long it took (on standard error). |
| `-p, --password <PASSWORD>` | none | Password for encrypted 7z headers; not needed for visible names. |

## extract · x

Extract the contents.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to extract. |
| `-o, --dest <DEST>` | . | Destination directory. |
| `--on-conflict <POLICY>` | overwrite | overwrite · skip · rename. What to do when the file is already there. |
| `-j, --threads <N>` | 0 | Threads to use. 0 means every core. Only .zip can go parallel. |
| `-p, --password <PASSWORD>` | none | Password of an encrypted archive (AES-256 or ZipCrypto). |

## test · t

Check integrity without writing to disk.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to check. |
| `-p, --password <PASSWORD>` | none | Password of an encrypted archive (AES-256 or ZipCrypto). |

## password

Rewrite a `.zip` with a different password, or with none. Entries aren’t compressed again, because WinZip AES encrypts the already-compressed bytes.

7z password changes and archive mutation are unsupported. See [Creating archives](creating-archives.md#7z) for codec restrictions and empty-only encryption.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to rewrite (.zip). |
| `-p, --password <PASSWORD>` | none | Current password, if the archive has one. |
| `--new <NEW>` | none | New password. Leave it out to remove the encryption. |
| `-o, --out <PATH>` | in place | Write here instead of replacing the archive in place. |

## bench

Measure the R1 and R2 performance requirements against an archive.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to measure against. |

## Exit status

| Status | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | Archive/I/O error, including wrong password or corrupted encrypted data. Printed to standard error as `arca: <message>`. |
| `2` | Invalid command-line syntax/options, reported by the argument parser. |
