---
description: Every measurement, the machines behind it — and the cases Arca loses.
group: Reference
order: 14
keywords: performance speed benchmarks nanazip 7-zip zip numbers
---

# Benchmarks

These numbers come from specific machines and should not be read as a general advantage. The methodology is part of the result — including the cases where Arca loses.

## Requirements

Xeon at 2.80 GHz, 2 cores, Linux.

| Requirement | Target | Measured | Others |
| --- | --- | --- | --- |
| R1 · cold start | < 15 ms | **1.6 ms** | unzip 2.0 · 7z 3.7 |
| R2 · list 6,000 entries | < 200 ms | **4.5 ms** | unzip 17.2 · 7z 45.4 |
| R3 · scaling to 2 threads | ≥ 1.6× | **1.89×** | 94% efficiency |

```sh
hyperfine --warmup 20 'arca --version'   # R1
arca bench archive.zip                   # R2, best of 5
```

## Compression

81 MB across 120 separate files, 2 threads, same machine.

| Tool | Time | Size |
| --- | --- | --- |
| **Arca, zstd** | **330 ms** | **26.16 MB** |
| Arca, deflate | 941 ms | 28.35 MB |
| zip -6 | 3934 ms | 27.65 MB |
| 7-Zip zip, 2 threads | 5358 ms | 26.88 MB |

With Zstandard, Arca was 11.9× faster than `zip` and 16.2× faster than 7-Zip, and produced the smallest archive of the four. With Deflate it was 4.2× faster than `zip` at the cost of 2.5% in size — the known zlib-rs trade.

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

With large files Arca compressed 5.7× faster at the cost of 3.7% in size; with many small files 7-Zip was somewhat ahead at the same size. **There is no single winner — it depends on the corpus.**

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

Both tools read every entry, check every CRC and write nothing — one core each.

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

The disk sustained 327 MB/s before the four runs and 90 MB/s after them. Arca ran on both the freshest and the most worn disk — and its worst run still beat NanaZip’s best.

> [!NOTE]
> **Why Arca wins here — and it isn’t the disk**
>
> NanaZip’s wall time equals its CPU time: its own processor is the limit. Arca burns slightly more CPU than wall clock (1.1 cores), so decompression overlaps with writing and the wall clock settles against the disk — 6.28 GB in 21.9 s is 294 MB/s against a 327 MB/s ceiling. The faster decoder is the cause; saturating the disk is the consequence.
