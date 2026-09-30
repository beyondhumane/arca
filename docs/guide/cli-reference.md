---
description: Every command, argument, option and default of the arca binary.
group: Reference
order: 11
keywords: cli command line options flags reference help exit status
---

# CLI reference

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

## create · c

Create an archive. The format is taken from the extension of the output file.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<OUT>` | required | Output archive: .zip, .tar, .tar.gz (or .tgz). |
| `<INPUTS>...` | required | Files or directories to include. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd. auto uses Deflate in .zip for compatibility. |
| `-j, --threads <N>` | 0 | Threads to use. 0 means every core. |
| `-p, --password <PASSWORD>` | — | Encrypt with AES-256. Other tools will ask for it to open the archive. |

## list · l

List the contents without extracting them.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to read. |
| `-t, --time` | off | Report how long it took (on standard error). |

## extract · x

Extract the contents.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to extract. |
| `-o, --dest <DEST>` | . | Destination directory. |
| `--on-conflict <POLICY>` | overwrite | overwrite · skip · rename — what to do when the file is already there. |
| `-j, --threads <N>` | 0 | Threads to use. 0 means every core. Only .zip can go parallel. |
| `-p, --password <PASSWORD>` | — | Password of an encrypted archive (AES-256 or ZipCrypto). |

## test · t

Check integrity without writing to disk.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to check. |
| `-p, --password <PASSWORD>` | — | Password of an encrypted archive (AES-256 or ZipCrypto). |

## password

Rewrite a `.zip` with a different password, or with none. Entries aren’t compressed again — WinZip AES encrypts the already-compressed bytes.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to rewrite (.zip). |
| `-p, --password <PASSWORD>` | — | Current password, if the archive has one. |
| `--new <NEW>` | — | New password. Leave it out to remove the encryption. |
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
| `1` | Any error. The message is printed to standard error as `arca: <message>`. |
