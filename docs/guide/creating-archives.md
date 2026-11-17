---
description: Levels, codecs, threads and formats for arca create, including new RAR5 archives.
group: Using Arca
order: 4
keywords: create compress level codec zstd deflate store threads tar gzip xz txz lzma2 bzip2 bz2 tbz2 zst tzst rar rar5 split volume 001 sfx self-extracting exe -l -c -j -v
---

# Creating archives

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names] [-v SIZE] [--sfx [--console]]
```

| Option | Values | Default | Description |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Compression level. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 | auto | Deflate in .zip, LZMA2 in .7z, .xz and .tar.xz, bzip2 in .bz2 and .tar.bz2, Zstandard in .zst and .tar.zst, the RAR compressor in .rar (auto or store only). |
| `-j, --threads` | a number | 0 | ZIP, XZ, Zstandard and RAR thread limit. 0 means every core; bzip2 always uses one; 7z uses every core its memory budget allows. |
| `-p, --password` | text | none | Encrypt ZIP or 7z with AES-256. Refused for .rar. |
| `--hide-names` | flag | off | Encrypt 7z headers too; requires a nonempty password. |
| `-v, --volume` | a size: bytes, or k, m, g | none | Split the archive into parts of that size, `out.zip.001`, `out.zip.002`… Not for .rar. |
| `--sfx` | flag | off | Make a self-extracting Windows program instead of an archive. |
| `--console` | flag | off | With `--sfx`: the program asks and reports in the console. |

## Levels

```sh
arca create copy.zip my-files/ -l store    # no compression, fastest
arca create copy.zip my-files/ -l fast
arca create copy.zip my-files/ -l normal   # the default
arca create copy.zip my-files/ -l best     # smallest output
```

## Codecs

`auto` picks Deflate for `.zip`, because a zip exists so that anything can open it. Ask for Zstandard explicitly when you control the receiving side:

```sh
arca create copy.zip my-files/ -c zstd
```

> [!WARNING]
> **Zstandard inside ZIP**
>
> Zstandard is ZIP method 93. It’s registered in the specification, but classic `unzip` can’t read it yet, so use it when you know who opens the archive. It will become the default once Arca has a native format.

## Threads

Arca uses every core by default. Pin the count with `-j`, for example on shared CI runners:

```sh
arca create copy.zip my-files/ -j 8
arca create copy.zip my-files/ -j 0    # every core (the default)
```

Scaling measured 1.79× on two threads, 90% efficiency, against a design requirement (R3) of at least 1.6×. See [benchmarks](benchmarks.md) for the command.

## 7z

```sh
arca create copy.7z my-files/ -l normal
arca create stored.7z my-files/ -c store
arca create secret.7z my-files/ -c lzma2 -p "a password" --hide-names
```

The levels `store`, `fast`, `normal` and `best` select Copy or LZMA2 levels 1, 6
and 9. `-c store` overrides the level. Only `auto`, `store` and `lzma2` are
accepted for 7z; explicit Deflate/Zstandard codec selection applies to ZIP,
not 7z. TAR ignores those codec choices and `.tar.gz` always uses gzip;
`lzma2` is rejected outside 7z.
Files are compressed together in solid blocks of up to 256 MiB, grouped by
extension the way 7-Zip does; x86 executables (`.exe`, `.dll` and the like) go
in blocks of their own behind the BCJ filter, which makes machine code smaller.
LZMA2 runs on as many cores as fit in 2 GiB (every core at `fast` and `normal`,
one at `best`), and entries are listed in that grouped order.

7z includes empty files and directories and rejects symlink/reparse-point and
special-file inputs rather than following them. Creation writes a temporary
archive beside the output and replaces it only on success; failure preserves an
existing output. The output's parent must exist. If all inputs are empty, 7z
encryption requires `--hide-names` so the password can actually be checked.

## RAR

```sh
arca create backup.rar documents/ notes.txt -l best
arca create stored.rar big.iso -c store
```

Creates a new single-volume, non-solid, unencrypted RAR5 archive that official
UnRAR and 7-Zip open. The four levels select the RAR compressor strength;
`-c store` equals `-l store` and any other explicit codec is refused. Creation
uses every core by default, or as many threads as `-j` says, stores empty files and directories,
and rejects symbolic links and special files instead of following them.

RAR output is never encrypted (`-p` and `--hide-names` are refused), never a
`.cbr`, never solid or multivolume, and never replaces an existing file: the
archive is staged beside the output, verified with Arca's reader and published
only if the name is still free. Existing RAR/CBR archives stay read-only.
Limits and the complete list of refusals are in the [RAR guide](rar.md#creating-rar5-archives).

## TAR and gzip

```sh
arca create copy.tar my-files/
arca create copy.tar.gz my-files/ -l best
```

The gzip layer honours the chosen level. TAR has nowhere to put encryption, so `-p` is refused for `.tar` and `.tar.gz`.

On Linux and macOS a symbolic link inside the folders being archived goes into a TAR (`.tar`, `.tar.gz`, `.tar.xz`) as a link, after the regular files, holding exactly the target it had. It is never followed, so a link to a folder elsewhere does not pull that folder in. Targets longer than 100 bytes go in a pax `linkpath` record. Whether a link is safe to recreate is decided on extraction (see [Links in TAR archives](listing-extracting.md#links-in-tar-archives)). On Windows links are left out, as before.

## XZ

```sh
arca create copy.tar.xz my-files/ -l best   # .txz works too
arca create notes.txt.xz notes.txt          # one file, no container
```

An `.xz` is a compressed stream, not an archive: it holds the bytes of exactly
one file and no name, date or folder. Arca refuses a folder or more than one
input for it and suggests `.tar.xz`, which puts a TAR inside the XZ stream.
Extracting `notes.txt.xz` writes `notes.txt`; a file without the suffix gets
`.out` appended.

The levels map to xz presets: `store` is preset 0, `fast` 1, `normal` 6 and
`best` 9. Even preset 0 compresses with LZMA2; XZ has no stored mode. Only
`auto` and `lzma2` are accepted as codecs, and `-p` and `--hide-names` are
refused. The output carries a CRC64 check that `xz` and `tar -J` verify.

`-j` sets how many blocks are compressed at the same time. Each worker needs
the memory of its preset, so Arca uses as many as fit in 2 GiB (one at `best`,
up to 14 at `normal`) and prints the number actually used. With more than one worker the
stream is split into independent blocks, which costs a little ratio. Reading
is a single sequential stream. Creation writes a temporary file beside the
output and renames it only on success.

## bzip2 and Zstandard

```sh
arca create copy.tar.bz2 my-files/          # .tbz2 and .tbz work too
arca create copy.tar.zst my-files/ -j 8     # .tzst works too
arca create notes.txt.bz2 notes.txt         # one file, no container
arca create notes.txt.zst notes.txt
```

They behave like XZ: a `.bz2` or `.zst` holds exactly one file, and a folder or
several inputs need the TAR variant. When reading, the signature decides the
codec and the contents decide whether a TAR is inside, whatever the name says.

bzip2 compresses on one thread. `store` and `fast` both use its 100 kB blocks,
and `best` its 900 kB ones. Its only accepted codec is `auto`. Every block and
the whole stream carry a CRC that `bzip2 -t` and `tar -j` verify, and
concatenated streams, as `pbzip2` writes them, are read as one.

Zstandard uses the same levels as Zstandard inside a ZIP and as many threads as
`-j` asks for; `auto` and `zstd` are accepted. Arca writes a content checksum
and, for a single file, its size, so `list` can show it without decoding.
Reading accepts windows up to 1 GiB, the same ceiling as the XZ dictionary, so
frames made with `zstd --long=31` are refused. Zstandard needs the default
`codecs-native` feature.

bzip2 never records the original size, and a Zstandard stream written to a
pipe may not either, so `list` shows 0 for them. Trailing data that is not
another stream is an error.

## Split volumes

```sh
arca create backup.zip photos/ -v 700m     # backup.zip.001, backup.zip.002, …
arca create backup.7z photos/ -v 4092m     # parts that fit on FAT32
arca create logs.tar.xz logs/ -v 100m
```

`-v` cuts the finished archive’s bytes into parts of the given size, the way
7-Zip’s `-v` does: every part but the last has exactly that size, and nothing
is added to them, so 7-Zip and NanaZip open the set too. The size is a number
of bytes, or one ending in `k`, `m` or `g` for KiB, MiB and GiB. It works for
ZIP, 7z, every TAR variant and the single-file streams; `.rar` is refused,
because RAR has volumes of its own that Arca does not write. A set has at most
9999 parts: a size too small for that is an error, and nothing is left behind.

The parts are written straight to their final folder, one after the other, so
the archive never exists as one big file there; that is what lets a 4 GB set
land on a FAT32 stick. They are written under temporary names and only take
their real names once the whole set is complete, so a stop or an error leaves
no partial set. `create` refuses to replace an existing `.001`; remove the old
parts first, or pick another name. Passing `backup.zip.001` as the output name
is the same as passing `backup.zip`.

To read a set, open any of its parts: `arca list backup.zip.002`, `arca test`
and `arca extract` find the others in the same folder and read them as one
archive. Every part must be there and numbered without gaps, and every part
but the last must have the same size; a missing or short part is named in the
error. A split archive is read-only: `arca password` and the window’s
editing commands refuse it. RAR multivolume sets (`.part1.rar`) and split ISO
images are not joined this way.

## Self-extracting programs

```sh
arca create setup.exe my-files/ --sfx
arca create setup.exe my-files/ --sfx --console
```

`--sfx` writes a Windows program that carries a ZIP of the inputs. Running it
shows a folder picker and extracts into the folder chosen, with the same path
checks as `arca extract`: an entry cannot land outside that folder, and a file
whose checksum does not match is not written. Files already there are
replaced. With `--console` the program asks for the folder in the console it
was started from, and offers a folder beside itself, named after it, by
default. Either kind also accepts `-o FOLDER` to extract there without asking,
for scripts.

The program is the `arca-sfx.exe` that comes with Arca, installed next to
`arca.exe`; `arca create` copies it, appends the ZIP and a short trailer that
says where the ZIP starts. If it is not there, `create` says where it looked
(`cargo build -p arca-sfx` builds it from source, and `ARCA_SFX_STUB` can name
another copy). The ZIP inside uses Deflate or Store, so `-c zstd` and
`-c lzma2` are refused, and `-p`, `--hide-names` and `-v` cannot be combined
with `--sfx`. Self-extractors are made for Windows; the Linux and macOS
packages do not include the stub.

## What gets stored

- Directories are walked recursively in sorted order, so the same input always produces the same entry order.
- Paths are stored relative to the parent of each input: `arca create a.zip ~/work/site` stores entries as `site/…`.
- Modification times are preserved.
- ZIP adds regular files and skips symbolic links. TAR adds regular files and, on Linux and macOS, stores symbolic links as links without following them (see above). 7z and RAR also preserve empty directories and reject symbolic links (see above).

## Encrypting while you create

```sh
arca create secret.zip folder/ -p "a password"
```

See [Encryption](encryption.md) for the scheme, its guarantees and its limits.

## Output

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
