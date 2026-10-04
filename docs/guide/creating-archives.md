---
description: Levels, codecs, threads and formats for arca create.
group: Using Arca
order: 4
keywords: create compress level codec zstd deflate store threads tar gzip xz txz lzma2 -l -c -j
---

# Creating archives

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names]
```

| Option | Values | Default | Description |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Compression level. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 | auto | Deflate in .zip, LZMA2 in .7z, .xz and .tar.xz. |
| `-j, --threads` | a number | 0 | ZIP and XZ thread limit. 0 means every core; 7z is sequential. |
| `-p, --password` | text | none | Encrypt ZIP or 7z with AES-256. |
| `--hide-names` | flag | off | Encrypt 7z headers too; requires a nonempty password. |

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
Creation is sequential regardless of `-j`, with one independent block per file,
not solid compression. Reading existing solid archives is supported.

7z includes empty files and directories and rejects symlink/reparse-point and
special-file inputs rather than following them. Creation writes a temporary
archive beside the output and replaces it only on success; failure preserves an
existing output. The output's parent must exist. If all inputs are empty, 7z
encryption requires `--hide-names` so the password can actually be checked.

## TAR and gzip

```sh
arca create copy.tar my-files/
arca create copy.tar.gz my-files/ -l best
```

The gzip layer honours the chosen level. TAR has nowhere to put encryption, so `-p` is refused for `.tar` and `.tar.gz`.

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

## What gets stored

- Directories are walked recursively in sorted order, so the same input always produces the same entry order.
- Paths are stored relative to the parent of each input: `arca create a.zip ~/work/site` stores entries as `site/…`.
- Modification times are preserved.
- ZIP/TAR add regular files and skip symbolic links. 7z also preserves empty directories and rejects symbolic links (see above).

## Encrypting while you create

```sh
arca create secret.zip folder/ -p "a password"
```

See [Encryption](encryption.md) for the scheme, its guarantees and its limits.

## Output

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
