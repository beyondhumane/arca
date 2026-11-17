---
description: Every measurement, the machine behind it, and the cases Arca loses.
group: Reference
order: 14
keywords: performance speed benchmarks nanazip 7-zip zip numbers rating codecs threads
---

# Benchmarks

These numbers come from specific machines and should not be read as a general advantage. Each one comes with its method, including the cases where Arca loses.

## Measure your own machine

`arca bench --codecs` measures every codec Arca writes on the machine it runs on, much as 7-Zip’s Tools → Benchmark does. It needs no files and touches no disk: the data is generated in memory. The desktop app runs the same benchmark from **Benchmark…** in the **More** menu; see [the desktop app](desktop-app.md#benchmark).

```sh
arca bench --codecs
arca bench --codecs --threads 8 --seconds 3 --size 32
arca bench --codecs --json
```

This page publishes no figures from it. Its output always ends with the command that reproduces it; keep that line next to any number you share.

How it measures:

- **The data.** `--size` MiB (16 by default, and at least 1 MiB per thread) generated from a fixed seed, so every machine and every run gets the same bytes. Of every four 64 KiB stretches, three are prose-like text, made from a fixed vocabulary with common words far more frequent than rare ones, and the fourth is random bytes.
- **The codecs.** Every codec runs at level 6 (normal), the default of `arca create`, through the same writers and readers Arca uses: Store, Deflate and Zstandard as ZIP entries, LZMA2 as one `.xz` stream per chunk, bzip2 as one `.bz2` stream per chunk, and RAR5 as one RAR archive held in memory. A codec the build lacks is listed as not in this build.
- **The threads.** The data is cut into 1 MiB chunks, each compressed on its own as a ZIP entry would be. One thread takes one chunk at a time; `--threads N` (every core by default) takes N at once. Both columns do exactly the same work, so only the parallelism differs. LZMA2 uses no more threads than fit in the 2 GiB that `arca create` gives a `.tar.xz`, and the table says so when that is fewer. `arca create` writes a `.bz2` on a single thread, so the bzip2 column for N threads shows what the processor does with independent chunks, not how fast `arca create x.bz2` is.
- **The timing.** Each measurement repeats its runs until `--seconds` have passed (1 by default; at least one run is always made) and reports the median. Speeds are MB/s of uncompressed data in both directions, with 1 MB = 10^6 bytes. The ratio is the compressed size over the original size, archive headers included.
- **The check.** Every decompressed chunk is compared with the original, outside the timed part. A difference stops the benchmark with an error.

### The rating

The rating is the geometric mean of six speeds: compression and decompression of Deflate, LZMA2 and bzip2. That is, with `c` for compression and `d` for decompression, in MB/s:

```text
rating = (c_Deflate × d_Deflate × c_LZMA2 × d_LZMA2 × c_bzip2 × d_bzip2) ^ (1/6)
```

It is worked out once for one thread and once for `--threads`. Those three codecs are used because every build has them: Zstandard is missing from builds without the native codecs, RAR5 from builds without RAR, and Store only measures memory bandwidth. A geometric mean keeps a fast codec from drowning out a slow one: doubling any one of the six speeds raises the rating by the same 12%, whichever it is.

Two ratings compare only when they come from the same Arca version, the same kind of build and the same options. The first line of the output says which build it is: `native codecs` uses libdeflate for Deflate, `pure Rust codecs` uses zlib-rs, and the two do not run at the same speed. The multithreaded rating also depends on `--threads`.

### Memory and stopping

The benchmark holds the data, one compressed copy of it and two chunks per thread, about twice `--size` plus 2 MiB per thread. The output prints that figure. Codec state comes on top, and the largest is the LZMA2 encoders: the 94 MiB xz(1) documents for level 6, on at most as many threads as fit in 2 GiB, which the output also prints as an upper bound.

Ctrl+C stops between runs, so the wait is at most one run, the longest being one chunk of LZMA2 on one thread. Arca then prints the table with what it had measured, says it stopped before the end, and exits with status 130. In a build without RAR the handler is not installed and Ctrl+C ends the process at once, which loses nothing because the benchmark writes nothing.

## Requirements

Ryzen 9 5900X, Linux. Everything in this section and the next is reproduced by one script, which downloads the [Silesia corpus](https://sun.aei.polsl.pl/~sdeor/index.php?page=silesia), pins it by hash and prints the machine and tool versions it ran on:

```sh
cargo build --release
nix-shell -p hyperfine zip unzip _7zz --run 'bash bench.sh'
```

| Requirement | Target | Measured | Others |
| --- | --- | --- | --- |
| R1 · cold start, list a one-entry zip | < 15 ms | **0.8 ms** | unzip 2.0 · 7z 2.5 |
| R2 · list 6,000 entries | < 200 ms | **3.1 ms** | unzip 18.6 · 7z 71.8 |
| R3 · scaling to 2 threads | ≥ 1.6× | **1.79×** | 90% efficiency |

```sh
hyperfine -N --warmup 20 'arca list tiny.zip' 'unzip -l tiny.zip' '7zz l tiny.zip'
hyperfine -N --warmup 5 'arca list list-6000.zip' 'unzip -l list-6000.zip' '7zz l list-6000.zip'
hyperfine -N --prepare 'rm -f out.zip' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate -j 1' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate -j 2'
```

## Compression

The Silesia corpus, 211.9 MB cut into 120 equal files, pinned to two physical cores.

| Tool | Time | Size |
| --- | --- | --- |
| **Arca, zstd** | **436 ms** | 66.87 MB |
| Arca, deflate | 1012 ms | 67.62 MB |
| zip -6 | 5746 ms | 68.34 MB |
| 7-Zip zip, -mx5 | 7877 ms | **65.81 MB** |

```sh
hyperfine -N --prepare 'rm -f out.zip' \
  'taskset -c 0,1 arca create out.zip corpus -c zstd' \
  'taskset -c 0,1 arca create out.zip corpus -c deflate' \
  'taskset -c 0,1 zip -qr -6 out.zip corpus' \
  'taskset -c 0,1 7zz a -tzip -mx5 out.zip corpus'
```

With Zstandard, Arca was 13.2× faster than `zip` and 18.1× faster than 7-Zip; 7-Zip still wrote the smallest archive, 1.6% below Arca’s. With Deflate, Arca was 5.7× faster than `zip` and its archive 1.1% smaller.

> [!NOTE]
> **Why 120 files**
>
> Each entry is compressed on one thread. Silesia’s own twelve files are dominated by a 49 MB one, so on them two threads only reach 1.56×.

### Deflate against deflate on Windows 11

16 threads, best of 3.

| Corpus | Arca | 7-Zip -mx5 | Arca size | 7-Zip size |
| --- | --- | --- | --- | --- |
| 5,358 source files, 54.8 MB | 0.710 s | **0.627 s** | 13,729,308 | 13,747,705 |
| 16 files, 287 MB | **0.402 s** | 2.285 s | 56,757,284 | 54,753,868 |

```sh
arca create c1.zip src
7z a -tzip -mx5 c2.zip src
```

With large files Arca compressed 5.7× faster at the cost of 3.7% in size; with many small files 7-Zip was somewhat ahead at the same size. The winner depends on the corpus.

## Parallel extraction

Windows 11, 16 threads, 287 MB in 16 text files, best of 3, deleting the output before each pass.

| Tool | Time |
| --- | --- |
| **Arca, 16 threads** | **0.207 s** |
| Arca, -j 1 | 0.686 s |
| 7-Zip | 1.020 s |

```sh
arca create big.zip big
arca extract big.zip -o out -j 1
arca extract big.zip -o out
7z x -o"out" big7z.zip
```

## Against NanaZip, on an archive big enough to hurt

A 3.13 GB `.zip` holding 1,513 files that unpack to 6.28 GB, all Deflate, with 13 entries over 100 MB. Windows 11, 16 cores, NanaZip 7.0.1832, and a DRAM-less SATA SSD (Kingston A400).

### Decompression alone

Both tools read every entry, check every CRC and write nothing, on one core each.

| Tool | Wall | CPU | Cores |
| --- | --- | --- | --- |
| **Arca** | **13.2 s** | 13.2 s | 1.0 |
| NanaZip | 25.9 s | 25.9 s | 1.0 |

```sh
arca test big.zip
NanaZipC t big.zip
```

### The whole job

Writing all 6.28 GB, run in the order A B B A so the disk’s decline can’t favour either side.

| Order | Tool | Time |
| --- | --- | --- |
| 1st | **Arca** | **21.9 s** |
| 2nd | NanaZip | 49.0 s |
| 3rd | NanaZip | 68.0 s |
| 4th | **Arca** | **28.0 s** |

```sh
arca extract big.zip -o out
NanaZipC x -y -o"out" big.zip
```

The disk sustained 327 MB/s before the four runs and 90 MB/s after them. Arca ran on both the freshest and the most worn disk, and its worst run still beat NanaZip’s best.

> [!NOTE]
> **Why Arca wins here**
>
> NanaZip’s wall time equals its CPU time: its own processor is the limit. Arca burns slightly more CPU than wall clock (1.1 cores), so decompression overlaps with writing and the wall clock settles against the disk: 6.28 GB in 21.9 s is 294 MB/s against a 327 MB/s ceiling. The faster decoder is what gets it there.
