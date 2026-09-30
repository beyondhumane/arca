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
arca test site.zip
arca test secret.zip -p "a password"
```

```text
1204 entries verified, no errors (0.121 s)
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

| Requirement | Target | Measured | For comparison |
| --- | --- | --- | --- |
| R1 · cold start | < 15 ms | **1.6 ms** | unzip 2.0 · 7z 3.7 |
| R2 · list 6,000 entries | < 200 ms | **4.5 ms** | unzip 17.2 · 7z 45.4 |
| R3 · scaling to 2 threads | ≥ 1.6× | **1.89×** | 94% efficiency |

Measured on a Xeon at 2.80 GHz with 2 cores, running Linux.

## Benchmarking honestly

- Report the best of several runs, and delete the output directory before every pass.
- Small corpora sit in the operating system’s write cache and flatter everybody — use data that doesn’t fit.
- When a run writes gigabytes, alternate the arms (A B B A) and measure the disk with a plain sequential write before and after.
- Compare CPU time with wall time. If they match, the program is its own bottleneck; if wall time is longer, the disk is.

> [!WARNING]
> **A cautionary tale**
>
> An ordered sweep over thread counts once produced a clean, convincing and completely false result — one thread looking twice as fast as sixteen — because the drive degraded as the run went on, and “more threads” really meant “later in the run”. Re-run alternating, 16 threads beat 1 thread in all three rounds.
