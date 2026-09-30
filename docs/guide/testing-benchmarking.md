---
description: Verify archives without writing to disk, and measure performance honestly.
group: Using Arca
order: 6
keywords: test verify crc ci github actions bench benchmark r1 r2 methodology
---

# Testing & benchmarking

## arca test

Reads every entry and checks its CRC — or its HMAC, when the entry is encrypted — without writing anything to disk.

```sh
arca test arca.zip
arca test secret.zip -p "a password"
```

```text
174 entries verified, no errors (0.006 s)
```

Corrupt entries are reported on standard error, and the command exits with status 1 and a count of the entries that failed.

### Verifying artifacts in CI

```yaml
# with arca on the PATH — see Installation
- name: Package
  run: arca create dist.zip build/ -l best
- name: Verify
  run: arca test dist.zip
```

## arca bench

Measures the design’s performance requirements R1 and R2 against a real archive and reports PASS or FAIL.

```sh
arca bench big.zip
```

It prints the R2 figure for the archive you give it, and the `hyperfine` command for R1:

```text
Performance requirements (design document, section 05)

  R2  list without extracting
      6000 entries in a 828.8 KB archive
      1.1 ms   target < 200 ms   PASS

  R1  cold start: measured from outside, with hyperfine
      hyperfine --warmup 20 'arca --version'
```

The published figures for R1, R2 and R3, and the script that reproduces them, are on the [benchmarks](benchmarks.md) page.

## Benchmarking honestly

- Report the best of several runs, and delete the output directory before every pass.
- Small corpora sit in the operating system’s write cache and flatter everybody — use data that doesn’t fit.
- When a run writes gigabytes, alternate the arms (A B B A) and measure the disk with a plain sequential write before and after.
- Compare CPU time with wall time. If they match, the program is its own bottleneck; if wall time is longer, the disk is.

> [!WARNING]
> **A cautionary tale**
>
> An ordered sweep over thread counts once produced a clean, convincing and completely false result — one thread looking twice as fast as sixteen — because the drive degraded as the run went on, and “more threads” really meant “later in the run”. Re-run alternating, 16 threads beat 1 thread in all three rounds.
