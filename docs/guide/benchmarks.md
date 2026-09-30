---
description: Every measurement, the machine behind it, and the cases Arca loses.
group: Reference
order: 14
keywords: performance speed benchmarks nanazip 7-zip zip numbers
---

# Benchmarks

These numbers come from specific machines and should not be read as a general advantage. Each one comes with its method, including the cases where Arca loses.

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
