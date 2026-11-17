---
description: Every measurement, the machine behind it, and the cases Arca loses.
group: Reference
order: 14
keywords: performance speed benchmarks nanazip 7-zip zip numbers rating codecs threads rar rar5 winrar
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

With large files Arca compressed 5.7× faster at the cost of 3.7% in size; with many small files 7-Zip was somewhat ahead at the same size. The winner depends on the corpus. Those figures predate the small-file work in the next section.

### Many small files on Windows 11

Ryzen 7 3700X (8 cores, 16 threads), Windows 11, Microsoft Defender real-time protection on, a DRAM-less SATA SSD (Kingston A400), warm cache. Other builds were running on the machine at the same time, so wall times move: each row is the median of the runs in A B B A order with the range in brackets, and the process CPU time (user plus kernel) is given as well because it moves less. Before is Arca 0.10.0 at commit `4b1d67b`.

The corpus is 50,000 files of 0 to 16,384 bytes in 100 folders, 410,069,593 bytes in all, three parts prose-like text to one part random bytes, generated from a fixed seed:

```sh
cargo run --release -p arca-bench --example small_files -- C:/b/small
```

| Creating, 16 threads, 16 runs | Wall | CPU | Peak memory | Archive |
| --- | --- | --- | --- | --- |
| Before | 5.003 s [4.420–7.743] | 19.75 s | 463 MiB | 252,139,697 |
| **After** | **1.246 s** [1.067–2.490] | **12.33 s** | **59 MiB** | **251,139,697** |

| Creating, `-j 1`, 6 runs | Wall | CPU | Peak memory |
| --- | --- | --- | --- |
| Before | 14.284 s [12.426–24.841] | 14.02 s | 64 MiB |
| **After** | **11.504 s** [10.039–12.117] | **11.75 s** | **34 MiB** |

```powershell
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\small" -Runs 16 -Clean C:\b\out.zip
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\small -j 1" -Runs 6 -Clean C:\b\out.zip
```

Where the time went, and what changed:

- **The walk.** Every file was asked for its metadata twice before it was read, and on Windows each ask opens and closes the file, all on one thread. The size and time now come from the folder listing, which already has them.
- **The writer.** Each entry’s local header was patched after its data, two seeks back and one forward, and every seek flushed the output buffer. An entry compressed in memory now gets its header complete, without the 20-byte Zip64 field it never needs; that field is what makes the archive 1,000,000 bytes smaller.
- **The compressor.** A fresh libdeflate or Zstandard compressor per file cost more than compressing a small file. Each thread now keeps its own.
- **The order.** Every file was compressed before the first one was written. Now files are written as soon as they are next in line, while later ones are still being compressed. At most 32 MiB of file data per thread is in flight, as before, and with small files the writer keeps up, so far less is held.

| Extracting, 16 threads, 6 runs | Wall | CPU |
| --- | --- | --- |
| Before | 16.948 s [11.255–24.115] | 34.50 s (kernel 31.89 s) |
| After | 12.310 s [10.614–19.535] | 33.09 s (kernel 30.38 s) |

```powershell
arca create C:\b\small.zip C:\b\small
powershell -File benchmarks\time.ps1 -Exes arca-0.10.0.exe,arca.exe `
  -ArgLine "extract C:\b\small.zip -o C:\b\out" -Runs 6 -Clean C:\b\out
```

Extracting is the file system’s time, not Arca’s: of 33 to 35 seconds of CPU, about 30 are spent in the kernel creating 50,000 files, and the two ranges overlap. The command line already read through one buffered reader per thread; it now creates each new file in one call instead of checking for it first, and a write that fails no longer leaves a short file behind. There is no speed claim here.

The desktop app opened the archive again for every entry it extracted, behind a fresh 256 KiB buffer that the seek to the entry then discarded. Counted on one thread with the example that models it, that is 50,001 opens and 26,218,015,557 bytes read for this 251 MB archive, against 2 opens and 251,341,637 bytes now. The window itself was not timed.

```sh
cargo run --release -p arca-zip --example read_amplification -- C:/b/small.zip baseline
cargo run --release -p arca-zip --example read_amplification -- C:/b/small.zip both
```

### Deflate size against WinRAR’s ZIP

Sizes do not depend on the machine. 7-Zip was not installed on the machine, so WinRAR 7.23, which also writes ZIP with Deflate, stands in for it. Archive bytes, everything included:

| Corpus | Arca -l 6 | Arca -l 8 | Arca -l 9 | WinRAR -m3 | WinRAR -m5 |
| --- | --- | --- | --- | --- | --- |
| The 50,000 small files, 410,069,593 bytes | 251,139,697 | 247,824,582 | **247,469,613** | 251,926,583 | 251,891,469 |
| This repository at `4b1d67b`, 973 files, 16,551,105 bytes | 7,939,633 | 7,826,524 | **7,820,343** | 8,145,493 | 8,133,072 |
| 16 large text files, 326,882,067 bytes | 123,443,446 | 117,071,830 | **117,038,415** | 123,301,735 | 122,832,147 |

```sh
cargo run --release -p arca-bench --example small_files -- C:/b/small
cargo run --release -p arca-bench --example small_files -- C:/b/text 16 33554432 text
mkdir C:/b/src && git archive 4b1d67b | tar -x -C C:/b/src
arca create C:/b/out.zip C:/b/small -l 6
```

```powershell
Start-Process -Wait "C:\Program Files\WinRAR\WinRAR.exe" `
  -ArgumentList "a -afzip -m5 -ibck -y -r -ep1 C:\b\out.zip C:\b\small\*"
```

What each level costs, on the same machine and under the same load, 3 runs each, 16 threads:

| Level | libdeflate level | Large text: wall, CPU | Small files: wall, CPU |
| --- | --- | --- | --- |
| 6 (default) | 6 | 1.493 s, 8.13 s | 1.325 s, 12.09 s |
| 8 | 10 | 4.964 s, 38.86 s | 3.307 s, 29.83 s |
| 9 (best) | 12 | 7.648 s, 58.86 s | 4.494 s, 45.77 s |

```powershell
powershell -File benchmarks\time.ps1 -Exes arca.exe `
  -ArgLine "create C:\b\out.zip C:\b\text -l 8" -Runs 3 -Clean C:\b\out.zip
```

The default is smaller than WinRAR’s best on small files and on source code, and 0.5% larger on large text. Level 8 is the first of libdeflate’s near-optimal parsers: 4.7% smaller than WinRAR’s best on large text and 1.6% smaller on small files, for two and a half to five times the CPU of the default. Level 9 adds very little over 8. The mapping from Arca’s levels to libdeflate’s is unchanged. For the 16-file corpus in the table further up, 7-Zip -mx5 was 3.5% below Arca’s default; level 8 is 5.2% below the default on the large text here, so it should close that gap, but that has not been measured against 7-Zip.


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

## LZMA and LZMA2: .xz, .tar.xz and .7z

Windows 11, Ryzen 7 3700X (8 cores, 16 threads), against xz 5.8.1 from Git for Windows; 7-Zip was not installed. Other builds shared the machine while these ran, so every row is the median of interleaved runs with its range in brackets, and both the wall time and the process’s own CPU time are given. “Before” is Arca at commit `4b1d67b`, “after” the same source with the LZMA work on `roadmap/lzma-speed`; both say `arca 0.10.0`.

The corpus is 80 MiB that every machine regenerates byte for byte: 24 MiB each of prose, Rust-like source and x86-like machine code, and 8 MiB of random bytes.

```sh
cargo build --release
cargo run --release -p arca-bench --example lzma_corpus -- corpus
tar -cf corpus.tar corpus
xz -1 -T0 -kc corpus.tar > t1.tar.xz   # 27 blocks
xz -6 -T0 -kc corpus.tar > t6.tar.xz   # 4 blocks
xz -6 -T1 -kc corpus.tar > s6.tar.xz   # 1 block
```

### Extracting a .tar.xz

Wall time with its range, then CPU time, median of 5. Arca also reads the TAR and writes the four files; xz writes the one `.tar`.

| File | Arca before | Arca after | xz -T1 | xz -T0 |
| --- | --- | --- | --- | --- |
| `t1.tar.xz`, 27 blocks | 1.70 s [1.44–14.11] · 1.56 s | 0.27 s [0.18–0.89] · 1.64 s | 1.08 s [0.88–1.12] · 1.06 s | **0.20 s** [0.17–0.22] · 1.11 s |
| `t6.tar.xz`, 4 blocks | 1.38 s [1.28–2.96] · 1.34 s | 0.59 s [0.57–0.64] · 1.23 s | 0.87 s [0.81–0.99] · 0.88 s | **0.42 s** [0.40–0.47] · 0.89 s |
| `s6.tar.xz`, 1 block | 1.32 s [1.29–1.53] · 1.28 s | 1.13 s [1.11–1.20] · 1.13 s | **0.86 s** [0.83–0.90] · 0.81 s | 0.86 s [0.85–1.00] · 0.86 s |

```sh
arca extract t1.tar.xz -o out
xz -dkf -T1 t1.tar.xz
xz -dkf -T0 t1.tar.xz
```

A `.xz` written on several threads is a series of independent blocks, and Arca now decodes one block per core, as `xz -T0` does. A file of one block still decodes on one thread; there the faster XZ checksums cut Arca’s time by 14%, but it still takes 1.13 s of CPU against xz’s 0.81 s, part of it reading the TAR and writing the files: its decoder, in safe Rust, is slower than liblzma’s. With many small blocks Arca comes closest to `xz -T0`; with four, the largest block sets the pace for both tools.

### Creating a .tar.xz

Median of 3. Arca at `-l 1` and `-l 6` against `xz -1` and `xz -6` on the same files; the sizes are within 0.03% of each other at every level.

| Command | Wall | CPU | Size |
| --- | --- | --- | --- |
| Arca before, `-l 1`, 16 threads | 1.63 s [1.51–1.80] | 11.09 s | 36,000,336 |
| Arca after, `-l 1`, 16 threads | 1.59 s [1.49–1.62] | 10.98 s | 36,000,336 |
| `xz -1 -T0` | **1.44 s** [1.43–1.55] | 12.13 s | 36,001,028 |
| Arca before, `-l 6`, 14 threads | 32.46 s [29.64–36.25] | 86.91 s | 30,456,380 |
| Arca after, `-l 6`, 14 threads | 38.66 s [26.36–48.31] | 88.78 s | 30,456,380 |
| `xz -6 -T0` | **32.60 s** [24.94–44.92] | 69.53 s | 30,463,552 |
| Arca after, `-l 6 -j 1` | 91.68 s [86.20–91.96] | 76.22 s | 30,341,456 |
| `xz -6 -T1` | **69.56 s** [65.70–149.00] | 65.05 s | 30,347,164 |

```sh
arca create out.tar.xz corpus -l 6
arca create out.tar.xz corpus -l 6 -j 1
xz -6 -T0 -kf corpus.tar
xz -6 -T1 -kf corpus.tar
```

The encoder did not change: the two Arca rows at each level differ by no more than their ranges, and on one thread Arca takes 17% more CPU than xz at level 6. At that level most of the time goes into the binary-tree match finder walking memory the cache does not hold, the same work xz does. Both cut the input into blocks of three dictionaries, 24 MiB at level 6, so 80 MiB gives only four blocks to run side by side, and the block of prose, the slowest, sets the wall time.

### 7z, level by level

Median of 3 for creation, 5 for extraction, of archives made by Arca from the same `corpus` directory. The sizes did not change by a byte.

| Level | Create before | Create after | Extract before | Extract after | Size |
| --- | --- | --- | --- | --- | --- |
| 1 | 1.44 s [1.38–1.48] | 1.57 s [1.56–1.62] | 1.52 s [1.29–7.80] | **0.29 s** [0.23–0.58] | 35,998,493 |
| 6 | 32.30 s [31.21–32.97] | 33.83 s [30.69–35.30] | 2.49 s [1.59–15.27] | **0.76 s** [0.59–1.24] | 30,457,591 |
| 9 | 127.48 s [114.34–169.06] | 128.72 s [117.23–173.85] | 1.72 s [1.38–2.06] | 1.59 s [1.24–8.76] | 30,231,863 |

```sh
arca create out.7z corpus -l 6
arca extract out.7z -o out
```

Extraction now decodes the LZMA2 data on several threads wherever the stream resets its dictionary, as multithreaded writers do every few dictionaries; a block of one run, as level 9 writes, still decodes on one thread. CPU time for extraction stayed between 1.2 and 1.6 s before and after. Creation at level 1 lost 9% of wall time, a gap wider than the ranges, probably because the multithreaded LZMA2 writer of lzma-rust2 0.22.0, which .7z now shares with .xz, keeps at most one chunk queued beyond its threads, where 0.21.0 read ahead without a bound.

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

## RAR 5 against WinRAR

Arca writes RAR 5 with its own compressor, a patched copy of `rars` (see `arca-rar/vendor/PATCHES.md`). These figures compare it with WinRAR's console `Rar.exe` 7.23 at the level each Arca level stands in for: Arca `-l 1`, `3`, `6`, `7` and `9` against `-m1` to `-m5`.

AMD Ryzen 7 3700X (8 cores, 16 threads), 32 GB, Windows 11. **The machine was busy**: other builds ran throughout, and the processor load sampled before each run was mostly 60–100%. Wall times swing by a factor of two or more from run to run, so the tables lead with the CPU time of the whole process (all its threads) and the size, which load moves far less. Each figure is the median of 3 runs; the CSV the script writes keeps every run and its load.

The corpus is generated, so anyone can rebuild the same bytes, plus this repository at a pinned commit as real data:

```powershell
cargo build --release -p arca-cli
cargo run --release -p arca-rar --example rar-corpus -- $env:TEMP\rar-corpus
mkdir $env:TEMP\rar-corpus\repo
git archive --format=tar 4b1d67b | tar -x -C $env:TEMP\rar-corpus\repo
powershell -ExecutionPolicy Bypass -File arca-rar\tests\rar-bench.ps1 `
    -Corpus $env:TEMP\rar-corpus -Runs 3 `
    -Kinds "text,log,source,exe,media,random,small,repo,all"
```

Each run is `arca create out.rar <dir> -l N` or `Rar.exe a -mN -ep1 -r -idq out.rar <dir>`, both non-solid, and every archive Arca writes is checked with `Rar.exe t`. "Before" is Arca at commit `4b1d67b`, measured the same way the same day.

### The whole corpus

`all` is the whole tree as one archive: the 77 MiB the generator writes plus the 16 MiB `repo` kind, 97,441,208 bytes in 4,286 files.

| Level | Before: CPU | wall | size | **Now: CPU** | wall | size | WinRAR: CPU | wall | size |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `-l 1` / `-m1` | 33.8 s | 123.1 s | 46,836,620 | **21.3 s** | 36.8 s | 46,936,551 | 8.1 s | 2.9 s | 47,234,842 |
| `-l 3` / `-m2` | 81.4 s | 104.6 s | 44,067,047 | **62.2 s** | 32.5 s | 43,880,430 | 12.5 s | 2.9 s | 44,341,281 |
| `-l 6` / `-m3` | 96.7 s | 46.9 s | 43,648,504 | **89.8 s** | 51.9 s | 43,419,805 | 21.2 s | 5.7 s | 42,537,059 |
| `-l 7` / `-m4` | 188.2 s | 135.7 s | 43,332,800 | **79.5 s** | 44.3 s | 43,353,431 | 29.9 s | 6.5 s | 42,258,443 |
| `-l 9` / `-m5` | 181.5 s | 201.7 s | 43,244,674 | **101.7 s** | 82.0 s | 43,240,314 | 31.7 s | 6.2 s | 42,206,709 |

When these figures were taken, Arca's wall time here was mostly not the compressor: the writer spooled every member to its own temporary file before assembling the archive, and on Windows that cost milliseconds per file. Payloads now stay in memory; see "Many small files" below. CPU time is the figure to compare codecs by. Against WinRAR, Arca now uses 2.6× its CPU time at `-l 1`, 4.2× at `-l 6` and 3.2× at `-l 9`, against 4.9×, 5.3× and 4.9× before. Its archives are 0.6% and 1.0% smaller at `-l 1` and `-l 3`, and 2.1% to 2.6% larger from `-l 6` up, almost all of it the `media` kind, where WinRAR applies a delta filter and Arca none.

### By kind, `-l 6` against `-m3`

| Kind | Before: CPU | size | **Now: CPU** | size | WinRAR: CPU | size |
| --- | --- | --- | --- | --- | --- | --- |
| `text` | 12.48 s | 3,106,714 | **12.45 s** | 3,106,714 | 2.09 s | 3,303,767 |
| `log` | 32.89 s | 3,311,320 | **21.12 s** | 3,042,982 | 5.95 s | 3,524,354 |
| `source` | 3.78 s | 2,125,845 | **4.11 s** | 2,125,845 | 1.11 s | 2,188,815 |
| `exe` | 15.45 s | 7,255,811 | **12.02 s** | 7,255,835 | 3.64 s | 7,711,395 |
| `media` | 8.80 s | 7,985,548 | **1.59 s** | 8,023,086 | 2.30 s | 6,056,962 |
| `random` | 7.62 s | 8,388,863 | **1.11 s** | 8,388,863 | 1.58 s | 8,388,944 |
| `small` | 8.91 s | 3,668,621 | **7.81 s** | 3,668,621 | 1.31 s | 3,549,840 |
| `repo` | 10.30 s | 7,774,219 | **6.84 s** | 7,776,296 | 2.25 s | 7,778,575 |

On text, logs and source Arca writes 3% to 14% less than WinRAR at three to six times its CPU time; on the real `repo` kind the two are within 0.1% of each other at three times. `media` and `random` are now quicker than WinRAR, because the search steps over input that keeps finding nothing.

### Many small files

The writer used to give each member's packed bytes a temporary file of its own beside the output, created, reopened by path for every batch of work that added to it and again to copy it into the archive, then deleted. On Windows those 3,000 rounds of the file system took far longer than compressing the `small` kind. Payloads now stay in memory, up to 1 MiB each and 128 MiB in all, counted inside the writer's 4 GiB managed-memory budget; a larger payload, or one past the 128 MiB, still goes to a temporary file. The archives are byte for byte the same.

Same machine, shared and busy again (median sampled load 59–71%). The `small` kind is 3,000 files and 7,246,984 bytes, non-solid, 16 threads; each figure is the median of 5 runs with the range in brackets, and the script checks the archive Arca writes at each level with `Rar.exe t`. Before is commit `17769b4`, after is `2c4e246`.

| Level | Before: wall | CPU | **After: wall** | **CPU** | WinRAR: wall | CPU |
| --- | --- | --- | --- | --- | --- | --- |
| `-l 1` / `-m1` | 23.73 s [22.66–32.66] | 7.81 s [6.48–8.50] | **1.63 s** [1.63–1.66] | **3.52 s** [3.19–4.27] | 0.93 s [0.90–6.02] | 1.08 s [1.02–1.23] |
| `-l 6` / `-m3` | 21.62 s [17.81–29.10] | 9.81 s [7.81–10.23] | **2.05 s** [1.99–2.26] | **5.97 s** [5.59–6.17] | 1.21 s [1.19–1.26] | 1.61 s [1.23–1.64] |

Sizes are unchanged: 3,752,642 and 3,668,621 bytes, against WinRAR's 3,814,162 and 3,549,840.

```powershell
cargo build --release -p arca-cli
cargo run --release -p arca-rar --example rar-corpus -- $env:TEMP\rar-corpus 1.0 small
powershell -ExecutionPolicy Bypass -File arca-rar\tests\rar-bench.ps1 `
    -Corpus $env:TEMP\rar-corpus -Kinds small -Levels "1,3" -Runs 5 -Tools "arca,rar"
```

The before row is the same command with `-Arca` pointing at a build of `17769b4` and `-Tools arca`.

### Data filters

From `-l 5` on Arca applies RAR 5's E8, E8E9, ARM and delta filters, chosen for each 64 KiB stretch of a file, as WinRAR does from `-m3` (see `arca-rar/vendor/PATCHES.md`). The tables above predate them. Measured the same way, `-Runs 3`, with `-Levels "3,4,5" -Kinds "exe,media,all"`; "Before" is Arca at commit `17769b4`, the last without filters, run the same day. The machine was busy again (load 40–99%), so CPU times are rough; sizes are exact.

| Kind, level | Before: CPU | size | **Filters: CPU** | size | WinRAR: CPU | size |
| --- | --- | --- | --- | --- | --- | --- |
| `all`, `-l 6` / `-m3` | 88.1 s | 43,419,805 | **85.1 s** | 39,186,348 | 23.1 s | 42,537,059 |
| `all`, `-l 7` / `-m4` | 87.9 s | 43,353,431 | **81.1 s** | 39,145,842 | 35.2 s | 42,258,443 |
| `all`, `-l 9` / `-m5` | 99.9 s | 43,240,314 | **112.4 s** | 39,039,838 | 35.6 s | 42,206,709 |
| `media`, `-l 6` / `-m3` | 2.23 s | 8,023,086 | **8.14 s** | 4,172,707 | 2.28 s | 6,056,962 |
| `media`, `-l 9` / `-m5` | 2.63 s | 7,995,463 | **9.75 s** | 4,159,438 | 2.45 s | 6,052,582 |
| `exe`, `-l 6` / `-m3` | 18.84 s | 7,255,835 | **15.20 s** | 6,874,309 | 4.23 s | 7,711,395 |
| `exe`, `-l 9` / `-m5` | 18.14 s | 7,227,594 | **17.77 s** | 6,864,731 | 8.08 s | 7,647,659 |
| `arca.exe`, `-l 6` / `-m3` | 5.03 s | 2,776,474 | **4.89 s** | 2,630,061 | 1.63 s | 2,625,050 |
| `arca.exe`, `-l 9` / `-m5` | 6.39 s | 2,718,248 | **7.27 s** | 2,569,717 | 2.70 s | 2,612,567 |

The whole corpus is now 7.9%, 7.4% and 7.5% smaller than WinRAR at `-l 6`, `-l 7` and `-l 9`, where it was 2.1%, 2.6% and 2.4% larger. `media` packs a third smaller than WinRAR's and costs three to four times the CPU it did, because filtered audio and pixels are worth searching where the raw bytes were stepped over, so `media` is no longer quicker than WinRAR. Random data still is: deciding that a region gets no filter is a pass over its bytes on the thread that reads, and on the `random` kind at `-l 6` Arca took 0.91 to 1.05 s of CPU before and after alike. `exe` gains 5%; WinRAR's own filters make that synthetic code larger than its `-mc-` output. Text, logs, source and random data are left alone, byte for byte; `repo` comes out 1,449 bytes smaller (7,774,847) and `small` 103 bytes smaller (3,668,518), where a few binary files are filtered.

`arca.exe` is a real executable: the 7,475,200-byte release build of `17769b4` itself (`cargo build --release -p arca-cli`, SHA-256 `a1f29ea94bc3ac117c89df4634bb083941f68148a6212e3cb70842be5c82cc09`), placed alone in a directory and run with `-Kinds arcaexe -Levels "3,5"`. Another build of the same commit gives other bytes, so the figures hold for that file. Arca is 0.2% larger than WinRAR at `-l 6` and 1.6% smaller at `-l 9`.

### Where Arca still loses

- **CPU time.** The optimal parse searches a binary tree at every position and prices every match length. It packs smaller than WinRAR on text, logs and source, and from `-l 3` up it still costs three to six times WinRAR's CPU time.
- **Filters where WinRAR has them and Arca does not.** Solid archives and files over 64 MiB are written without data filters, which WinRAR applies to both; the filters also see each 64 KiB stretch whole, where WinRAR narrows a delta range to a few hundred bytes inside one.
- **Many small files.** Still slower than WinRAR, though no longer by a factor of twenty: 2.05 s against 1.21 s at `-l 6`, most of the difference the compressor's CPU time. Each source is still asked for its metadata while the archive is planned and reopened to be read, and a stored member is read twice.
- **Encrypted archives of many files.** Every encrypted member gets its own salt, so the writer runs the 2^15-round key derivation, about 15 ms, once per member, one member after another, and the check that follows runs it again for every member it decrypts.
- **Headers.** Arca records a BLAKE2sp hash for every file where WinRAR records a CRC32, 28 bytes more per file: about 84 KB of the `small` kind.
- **A few big files.** A member is compressed in runs of one dictionary each (1 to 16 MiB by level), and each run is one job, so a single 4 MiB file at `-l 6` uses one core.
