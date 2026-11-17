---
description: Every command, argument, option and default of the arca binary.
group: Reference
order: 11
keywords: cli command line options flags reference help exit status
---

# CLI reference

ZIP, 7z, TAR (plain, gzip, XZ, bzip2 and Zstandard), standalone XZ, bzip2 and Zstandard, and [RAR](rar.md) are enabled by default. RAR supports `list`,
`test` and `extract`, including `-p` for encrypted data or headers, and
`create` for new single-volume RAR5 archives. Modifying an existing RAR or
CBR, and creating CBR, is always rejected.

```text
Fast, safe archiver

Usage: arca <COMMAND>

Commands:
  create    Create an archive                                          [aliases: c]
  list      List the contents without extracting them                  [aliases: l]
  extract   Extract the contents                                       [aliases: x]
  test      Check integrity without writing to disk                    [aliases: t]
  hash      Print or check checksums of files, in the sha256sum format
  password  Add, change or remove the password of an existing archive
  repair    Rebuild a damaged ZIP from the entries that can still be read
  diff      Compare two archives, or an archive and a folder
  bench     Measure every codec on generated data (--codecs), or the R1 and R2 requirements on an archive

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`list`, `extract` and `test` also read the [ZIP containers](zip-containers.md) (.apk, .jar, .epub, .cbz and the rest), [ISO 9660 images](iso.md) and, in builds with [RAR](rar.md), .rar and .cbr. Those are read-only: `password` refuses them and `create` refuses every one of them except new `.rar` archives. Any part of a [split set](creating-archives.md#split-volumes), such as `backup.zip.002`, opens the whole set; split sets are read-only too.

## create · c

Create an archive. The format is taken from the extension of the output file.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<OUT>` | required | Output archive: .zip, .7z, .rar, .tar, .tar.gz (or .tgz), .tar.xz (or .txz), .xz, .tar.bz2 (or .tbz2, .tbz), .bz2, .tar.zst (or .tzst), .zst. An .xz, .bz2 or .zst takes exactly one file. |
| `<INPUTS>...` | required | Files or directories to include. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd · lzma2. auto uses Deflate in .zip, LZMA2 in .7z, .xz and .tar.xz, bzip2 in .bz2 and .tar.bz2, Zstandard in .zst and .tar.zst and the RAR compressor in .rar (auto or store only). |
| `-j, --threads <N>` | 0 | ZIP and XZ threads; 0 means every core. XZ uses as many as fit in 2 GiB, Zstandard as many as asked and bzip2 one; 7z and RAR use every core their memory budget allows, whatever this says. |
| `-p, --password <PASSWORD>` | none | Encrypt with AES-256 (.zip, .7z). Other tools will ask for it to open the archive. Refused for .rar. |
| `--hide-names` | off | 7z only: encrypt headers; requires a nonempty password. |
| `-v, --volume <SIZE>` | none | Split into parts of SIZE bytes, or with k, m or g for KiB, MiB, GiB: `out.zip.001`, `out.zip.002`… as 7-Zip does. Refuses to replace an existing `.001`. Not for .rar. See [split volumes](creating-archives.md#split-volumes). |
| `--sfx` | off | Write a self-extracting Windows program (any name, usually `.exe`) from the `arca-sfx.exe` found next to arca. Not with `-p`, `--hide-names` or `-v`. See [self-extracting programs](creating-archives.md#self-extracting-programs). |
| `--console` | off | With `--sfx`: the program asks for the folder and reports in the console instead of in windows. |

## list · l

List the contents without extracting them.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to read. |
| `-t, --time` | off | Report how long it took (on standard error). |
| `-p, --password <PASSWORD>` | none | Password for encrypted 7z headers; not needed for visible names. |

For a `.zip`, the archive comment and any entry comments go to standard error after the list, so standard output stays one line per entry. Control characters in them are shown as `�` so a comment can’t drive the terminal.

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

## hash

Print a checksum for every file, one line each in the format `sha256sum` writes (`<hex>  <path>`), or verify a file of such lines. Directories are read recursively, in name order. Files are read in pieces, never whole into memory, and several are hashed at once across the cores; the lines still come out in order.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<PATHS>...` | required unless `--check` | Files or directories to hash. |
| `-a, --algorithm <ALGORITHM>` | sha256 | crc32 · sha1 · sha256. With `--check`, each line’s algorithm is taken from the length of its checksum unless this is given. |
| `-c, --check <SUMS>` | none | Verify the files listed in `SUMS` and print `<path>: OK` or `<path>: FAILED` for each. |

```sh
arca hash photos > SHA256SUMS
arca hash --check SHA256SUMS     # or: sha256sum -c SHA256SUMS
arca hash -a crc32 setup.exe
```

Paths are printed as given, with `/` between folders on every system. A sum file may use `*` before the name (binary mode), Windows line endings, a byte-order mark and the escaped names `sha256sum` writes for names holding a backslash or a line break. Lines that aren’t checksums are reported and skipped. A file that can’t be read, a checksum that doesn’t match, or a sum file with no checksum lines at all, gives exit status 1. BLAKE3 isn’t offered: its Rust crate needs a C compiler by default.

## diff

Compare two archives, or an archive and a folder, and print one line for each name the two sides hold differently. Either side can be any archive `list` reads, a RAR volume set named by any of its volumes, or a folder. Entries are read straight out of the archives, never extracted.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<A>` | required | First archive or folder. |
| `<B>` | required | Second archive or folder. |
| `--quick` | off | Compare sizes, and the CRC32 where both sides store one, without reading any contents. |
| `--times` | off | Also report files whose contents match but whose modification times are more than 2 seconds apart. |
| `-p, --password <PASSWORD>` | none | Password of the first archive, and of the second unless `--password-b` is given. |
| `--password-b <PASSWORD>` | the `-p` one | Password of the second archive. |

```sh
arca diff release-1.0.zip release-1.1.zip
arca diff backup.tar.zst ~/project       # does the backup still match the folder?
arca diff --quick -p secret a.7z b.zip
```

| Line | Meaning |
| --- | --- |
| `+ path` | Only in B. |
| `- path` | Only in A. |
| `M path` | In both, with different contents, or a link pointing somewhere else. |
| `T path` | A file on one side and a folder or a link on the other. |
| `m path` | Same contents, different modification time (only with `--times`). |

- Lines come in name order. Names use `/` and folders end in `/`. They are compared exactly as written, so case matters. A leading `./` and doubled slashes are ignored, and a backslash reads as `/`.
- A folder is compared by what it holds, not by its own name: `arca diff a.zip out` after `arca x a.zip -o out` prints nothing. Folders that only appear in the paths of their files count as folders, so an archive without folder entries still matches its extraction.
- By default, files of the same size are read on both sides and compared by SHA-256, unless both sides store CRC32s and those differ. A standalone `.xz`, `.bz2` or `.zst` has no reliable size, so it is always read, except with `--quick`, which never reports it as changed.
- Links can only be told apart in TAR and on the disk. A TAR hard link counts as the file it names. Devices and pipes are left out.
- A side that can’t be opened, a wrong or missing password, or an entry that has to be read and can’t be, gives exit status 2 rather than a difference.

## password

Rewrite a `.zip` with a different password, or with none. Entries aren’t compressed again, because WinZip AES encrypts the already-compressed bytes.

7z password changes and archive mutation are unsupported. See [Creating archives](creating-archives.md#7z) for codec restrictions and empty-only encryption.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to rewrite (.zip). |
| `-p, --password <PASSWORD>` | none | Current password, if the archive has one. |
| `--new <NEW>` | none | New password. Leave it out to remove the encryption. |
| `-o, --out <PATH>` | in place | Write here instead of replacing the archive in place. |

## repair

Rebuild a ZIP whose central directory is missing or damaged. Arca scans the file for local headers, takes each entry’s sizes from its header or, for streamed entries, from the data descriptor that follows the data, decompresses it and compares its CRC, and copies the entries that pass byte for byte into a new archive. Skipped entries are listed on standard error with their offset and the reason.

```sh
arca repair broken.zip -o fixed.zip
```

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Damaged archive. It is only read, never modified. |
| `-o, --out <PATH>` | required | Where to write the rebuilt archive (.zip). It can’t be the damaged archive itself. |

- Encrypted entries can’t be checked without the password: they are copied after a structural check only and named on standard error.
- What only the central directory held is lost: archive and entry comments, Unix permissions and other external attributes.
- A ZIP stored inside an entry stays inside it. If that outer entry is damaged, though, the inner entries can surface as entries of their own.
- The rebuilt archive is written under a temporary name, read back, and only then renamed. If nothing can be recovered, nothing is written and the command fails.

## bench

Two benchmarks. With `--codecs`, every codec Arca writes is measured on data generated in memory, on one thread and on `--threads`; see [Measure your own machine](benchmarks.md#measure-your-own-machine) for the method and the rating. With an archive instead, the R1 and R2 performance requirements are measured against it.

```sh
arca bench --codecs
arca bench --codecs --threads 4 --seconds 2 --json
arca bench big.zip
```

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required without `--codecs` | Archive to measure R1 and R2 against. Not with `--codecs`. |
| `--codecs` | off | Measure compression and decompression speed and ratio of Store, Deflate, Zstandard, LZMA2, bzip2 and RAR5, and print a rating. Nothing is read from or written to the disk. |
| `-j, --threads <N>` | `0` | Threads for the second measurement; `0` means every core. With `1` only one thread is measured. |
| `--seconds <S>` | `1` | Seconds each measurement keeps repeating its runs, from 0 to 3600. At least one run is always made. |
| `--size <MIB>` | `16` | MiB of generated data, from 1 to 4096; raised to 1 MiB per thread. |
| `--json` | off | Print one JSON object instead of the table: speeds in MB/s, ratios as compressed over original size, and `null` for whatever was not measured. |

The table and the JSON end with the command that reproduces them. Progress goes to standard error, and only when it is a terminal. Ctrl+C stops between runs, prints what was measured and exits with status 130.

## Exit status

| Status | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | Archive/I/O error, including wrong password or corrupted encrypted data. Printed to standard error as `arca: <message>`. Also a `hash` that couldn’t read a file or found a checksum that didn’t match, and a `diff` that found differences. |
| `2` | Invalid command-line syntax/options, reported by the argument parser. Also a `diff` that couldn’t read one of its sides: for `diff`, 1 only ever means the two differ. |
| `130` | `bench --codecs` stopped by Ctrl+C, after printing what it measured. Only in builds with RAR, which install the handler; elsewhere Ctrl+C ends the process as usual. |
