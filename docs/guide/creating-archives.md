---
description: Levels, codecs, threads and formats for arca create.
group: Using Arca
order: 4
keywords: create compress level codec zstd deflate store threads tar gzip -l -c -j
---

# Creating archives

```text
arca create <OUT> <INPUTS>... [-l LEVEL] [-c CODEC] [-j THREADS] [-p PASSWORD]
```

| Option | Values | Default | Description |
| --- | --- | --- | --- |
| `-l, --level` | store · fast · normal · best | normal | Compression level. |
| `-c, --codec` | auto · store · deflate · zstd | auto | Compressor. auto uses Deflate in .zip for compatibility. |
| `-j, --threads` | a number | 0 | Threads to use. 0 means every core. |
| `-p, --password` | text | — | Encrypt with AES-256. .zip only. |

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
> Zstandard is ZIP method 93. It’s registered in the specification, but classic `unzip` can’t read it yet — so use it when you know who opens the archive. It will become the default once Arca has a native format.

## Threads

Arca uses every core by default. Pin the count with `-j` — handy on shared CI runners:

```sh
arca create copy.zip my-files/ -j 8
arca create copy.zip my-files/ -j 0    # every core (the default)
```

Scaling measured **1.89× on two threads** — 94% efficiency, against a design requirement (R3) of at least 1.6×.

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
- Only regular files are added — symbolic links are skipped for now (see the [roadmap](roadmap.md)).

## Encrypting while you create

```sh
arca create secret.zip folder/ -p "a password"
```

See [Encryption](encryption.md) for the scheme, its guarantees and its limits.

## Output

```text
copy.zip: 120 files, 81.0 MB -> 26.2 MB (67.7% smaller) in 0.330 s · 245 MB/s · 2 threads
```
