---
description: Levels, codecs, threads and formats for arca create, including new RAR5 archives.
group: Using Arca
order: 4
keywords: create compress level codec zstd deflate store threads tar gzip rar rar5 -l -c -j
---

# Creating archives

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD] [--hide-names]
```

| Option | Values | Default | Description |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Compression level. |
| `-c, --codec` | auto · store · deflate · zstd · lzma2 | auto | Deflate in .zip, LZMA2 in .7z, the RAR compressor in .rar (auto or store only). |
| `-j, --threads` | a number | 0 | ZIP thread limit. 0 means every core; 7z and RAR are sequential. |
| `-p, --password` | text | none | Encrypt ZIP or 7z with AES-256. Refused for .rar. |
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

## RAR

```sh
arca create backup.rar documents/ notes.txt -l best
arca create stored.rar big.iso -c store
```

Creates a new single-volume, non-solid, unencrypted RAR5 archive that official
UnRAR and 7-Zip open. The four levels select the RAR compressor strength;
`-c store` equals `-l store` and any other explicit codec is refused. Creation
is sequential (`-j` above 1 is refused), stores empty files and directories,
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

## What gets stored

- Directories are walked recursively in sorted order, so the same input always produces the same entry order.
- Paths are stored relative to the parent of each input: `arca create a.zip ~/work/site` stores entries as `site/…`.
- Modification times are preserved.
- ZIP/TAR add regular files and skip symbolic links. 7z and RAR also preserve empty directories and reject symbolic links (see above).

## Encrypting while you create

```sh
arca create secret.zip folder/ -p "a password"
```

See [Encryption](encryption.md) for the scheme, its guarantees and its limits.

## Output

```text
silesia.zip: 12 files, 202.1 MB -> 63.1 MB (68.8% smaller) in 0.282 s · 717 MB/s · 24 threads
```
