---
description: Every command, argument, option and default of the arca binary.
group: Reference
order: 11
keywords: cli command line options flags reference help exit status
---

# CLI reference

ZIP, 7z, TAR (plain, gzip, XZ, bzip2 and Zstandard), standalone XZ, bzip2 and Zstandard, and [RAR](rar.md) are enabled by default. RAR supports `list`,
`test` and `extract`, including `-p` for encrypted data or headers, and
`create` for new RAR5 archives, solid, encrypted, in volumes or with a recovery record. Modifying an existing RAR or
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
  comment   Show or change the comment of an archive or of one entry
  repair    Rebuild a damaged archive from what can still be read
  diff      Compare two archives, or an archive and a folder
  bench     Measure every codec on generated data (--codecs), or the R1 and R2 requirements on an archive

Options:
  -h, --help     Print help
  -V, --version  Print version
```

`list`, `extract` and `test` also read the [ZIP containers](zip-containers.md) (.apk, .jar, .epub, .cbz and the rest), [ISO 9660 images](iso.md), [CPIO, AR, DEB, RPM, CAB and LZH](more-formats.md), [disk images](disk-images.md) (.vhd, .vhdx, .vmdk, .dmg, .img) and, in builds with [RAR](rar.md), .rar and .cbr. Those are read-only: `password` refuses them and `create` refuses every one of them except new `.rar` archives. Any part of a [split set](creating-archives.md#split-volumes), such as `backup.zip.002`, opens the whole set; split sets are read-only too.

## create · c

Create an archive. The format is taken from the extension of the output file.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<OUT>` | required | Output archive: .zip, .7z, .rar, .tar, .tar.gz (or .tgz), .tar.xz (or .txz), .xz, .tar.bz2 (or .tbz2, .tbz), .bz2, .tar.zst (or .tzst), .zst. An .xz, .bz2 or .zst takes exactly one file. |
| `<INPUTS>...` | required | Files or directories to include. |
| `-l, --level <LEVEL>` | normal | store · fast · normal · best. |
| `-c, --codec <CODEC>` | auto | auto · store · deflate · zstd · lzma2 · lzma · ppmd · bzip2. auto uses Deflate in .zip, LZMA2 in .7z, .xz and .tar.xz, bzip2 in .bz2 and .tar.bz2, Zstandard in .zst and .tar.zst and the RAR compressor in .rar (auto or store only). .7z takes lzma2, lzma, ppmd, bzip2 (with codecs-native), deflate or store, never zstd, which 7-Zip cannot read in a 7z. |
| `-j, --threads <N>` | 0 | ZIP and XZ threads; 0 means every core. XZ uses as many as fit in 2 GiB, Zstandard as many as asked and bzip2 one; 7z with LZMA2 and RAR use every core their memory budget allows, whatever this says, and 7z with any other method one. |
| `-p, --password <PASSWORD>` | none | Encrypt with AES-256 (.zip, .7z, .rar). Other tools will ask for it to open the archive. |
| `--hide-names` | off | .7z and .rar: encrypt headers too, so names are hidden; requires a nonempty password. In .rar this is WinRAR’s `-hp`. |
| `-v, --volume <SIZE>` | none | Split into parts of SIZE bytes, or with k, m or g for KiB, MiB, GiB: `out.zip.001`, `out.zip.002`… as 7-Zip does. Refuses to replace an existing `.001`. A .rar becomes a RAR volume set instead, `out.part1.rar`, `out.part2.rar`… as WinRAR’s `-v` makes, each at most SIZE (at least 64 KiB); no volume of that name may exist. See [split volumes](creating-archives.md#split-volumes) and [RAR](creating-archives.md#rar). |
| `--solid` | off | .rar only: compress the files as one stream, as WinRAR’s `-s`. |
| `--recovery <PERCENT>` | none | .rar only: add a recovery record of PERCENT (1 to 100, `3` or `3%`), in each volume of a set, as WinRAR’s `-rr`. `arca repair` and WinRAR rebuild damaged data from it. |
| `--sfx` | off | Write a self-extracting Windows program (any name, usually `.exe`) from the `arca-sfx.exe` found next to arca. Not with `-p`, `--hide-names`, `-v`, `--solid` or `--recovery`. See [self-extracting programs](creating-archives.md#self-extracting-programs). |
| `--console` | off | With `--sfx`: the program asks for the folder and reports in the console instead of in windows. |

## list · l

List the contents without extracting them.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to read. |
| `-t, --time` | off | Report how long it took (on standard error). |
| `-p, --password <PASSWORD>` | none | Password for encrypted 7z headers; not needed for visible names. |

For a `.zip`, the archive comment and any entry comments go to standard error after the list, so standard output stays one line per entry. So do a RAR archive’s comment and the pax `comment` records of TAR entries. Control characters in them are shown as `�` so a comment can’t drive the terminal.

## extract · x

Extract the contents.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to extract. |
| `-o, --dest <DEST>` | . | Destination directory. |
| `--on-conflict <POLICY>` | overwrite | overwrite · skip · rename. What to do when the file is already there. |
| `-j, --threads <N>` | 0 | Threads to use. 0 means every core. Only .zip follows it; a multi-block .xz or .tar.xz and the LZMA2 blocks of a .7z always decode on every core their 2 GiB memory budget allows. |
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

7z password changes and archive mutation are unsupported. See [Creating archives](creating-archives.md#7z) for the methods, their parameters and filters, and empty-only encryption.

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive to rewrite (.zip). |
| `-p, --password <PASSWORD>` | none | Current password, if the archive has one. |
| `--new <NEW>` | none | New password. Leave it out to remove the encryption. |
| `-o, --out <PATH>` | in place | Write here instead of replacing the archive in place. |

## comment

Show or change the comment of an archive, or of one entry.

```sh
arca comment notes.zip                          # print the archive comment
arca comment notes.zip --set "Sent on 3 May"
arca comment notes.zip --entry docs/a.txt --file note.txt
arca comment notes.zip --clear -o copy.zip
```

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Archive: a .zip to change; a .zip, .rar or TAR to read. |
| `--set <TEXT>` | none | Set the comment to TEXT. |
| `--file <FILE>` | none | Set the comment to the contents of FILE, byte for byte; `-` reads standard input. |
| `--clear` | off | Remove the comment. |
| `--entry <NAME>` | none | The entry whose comment to show or change, by its path inside the archive. A folder answers with or without the trailing `/`. |
| `-o, --out <PATH>` | in place | Write here instead of replacing the archive in place. |
| `-p, --password <PASSWORD>` | none | Password of a RAR archive whose headers are encrypted. |

- With none of `--set`, `--file` and `--clear` the comment is printed, with control characters shown as `�`. A TAR has no comment of its own, so without `--entry` it lists the entries that carry a pax `comment` record.
- Only ZIP comments can be changed. The archive is written again beside itself with every entry copied unchanged, so no password is needed even for encrypted entries; it is read back and only then put in place of the old one.
- An `--entry` that matches nothing is an error, and nothing is written.

## repair

Rebuild a damaged archive from what can still be read, into a new archive of the same format. The damaged archive is only read.

```sh
arca repair broken.zip -o fixed.zip
arca repair backup.tar.gz -o rescued.tar.gz
```

| Argument / option | Default | Description |
| --- | --- | --- |
| `<ARCHIVE>` | required | Damaged archive: ZIP, TAR (plain, gzip, XZ, bzip2 or Zstandard), 7z or RAR. It is only read, never modified. |
| `-o, --out <PATH>` | required | Where to write the rebuilt archive, with the extension of the same format. It can’t be the damaged archive itself. |
| `-p, --password <PASSWORD>` | none | Password of an encrypted 7z or RAR. |

The rebuilt archive is written under a temporary name, read back, and only then renamed. If nothing can be recovered, nothing is written and the command fails. Lost entries are listed on standard error with the reason.

**ZIP.** Arca scans the file for local headers, takes each entry’s sizes from its header or, for streamed entries, from the data descriptor that follows the data, decompresses it and compares its CRC, and copies the entries that pass byte for byte into a new archive.

- Encrypted entries can’t be checked without the password: they are copied after a structural check only and named on standard error.
- What only the central directory held is lost: archive and entry comments, Unix permissions and other external attributes.
- A ZIP stored inside an entry stays inside it. If that outer entry is damaged, though, the inner entries can surface as entries of their own.

**TAR.** Every entry whose header checksum holds and whose data is all there is copied with its headers unchanged; the new archive is compressed again with the same codec. After a damaged header, Arca looks at the following 512-byte blocks for the next one that is a valid header and carries on from there.

- TAR keeps no checksum over data, so damage inside an entry’s data goes unnoticed. A block inside some entry’s data that happens to look like a header, such as a TAR stored inside the TAR, is taken for one.
- A hard link whose target was lost is left out.
- Damage inside the gzip, XZ, bzip2 or Zstandard stream stops the repair there: nothing after it can be decoded. What came before it is kept, but those formats check their data after handing it out, so the last entries kept before the damage are unchecked, and Arca says so.

**7z.** Each solid block is decoded on its own, so damage in one block costs nothing in the others. In a damaged block, the first entry that fails its CRC or doesn’t decode and every entry after it are lost; an entry without a CRC of its own is kept only if its whole block decodes. The survivors are compressed again at the normal level.

- The header at the end of a 7z is what says where every block and entry is: a 7z with a damaged header can’t be repaired.
- Folders alone are not a repair: when every file is lost, nothing is written and the command fails, even if the folders survive.

**RAR.** Rebuilds the archive from its recovery record, which it must have been made with (WinRAR’s *Add recovery record*). Without one, a damaged RAR can’t be repaired. When the record finds nothing wrong, nothing is written.

- Only single archives: a volume set is refused.
- The recovery record is reached through the archive’s headers, so a RAR whose headers are damaged can’t be repaired.
- A RAR 4 archive is repaired in memory and is refused over 1 GiB.

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
| `-j, --threads <N>` | `0` | Threads for the second measurement, from 0 to 256; `0` means every core. With `1` only one thread is measured. |
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
