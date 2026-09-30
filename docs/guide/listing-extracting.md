---
description: Look inside archives and unpack them in parallel — safely.
group: Using Arca
order: 5
keywords: list extract unpack parallel conflict overwrite skip rename zip slip -o
---

# Listing & extracting

## Listing

```text
arca list <ARCHIVE> [-t | --time]
```

Prints one line per entry — uncompressed size, method, ratio and name — without extracting anything.

```sh
arca list arca.zip --time
```

```text
       22966  deflate   72.6%  arca/arca-cli/src/main.rs
      114710  deflate   76.8%  arca/arca-zip/src/lib.rs
174 entries, 2.3 MB uncompressed, listed in 0.1 ms
```

With `--time`, the summary goes to standard error, so piping the listing into other tools stays clean.

## Extracting

```text
arca extract <ARCHIVE> [-o DIR] [--on-conflict POLICY] [-j THREADS] [-p PASSWORD]
```

| Option | Default | Description |
| --- | --- | --- |
| `-o, --dest` | `.` | Destination directory. Created if it doesn’t exist. |
| `--on-conflict` | overwrite | What to do when a file already exists: overwrite, skip or rename. |
| `-j, --threads` | 0 | Threads to use. 0 means every core. Only .zip can go parallel. |
| `-p, --password` | — | Password of an encrypted archive (AES-256 or ZipCrypto). |

### Conflicts

| Policy | Behaviour |
| --- | --- |
| `overwrite` | Replace the file that is already there. |
| `skip` | Leave the existing file alone and move on. |
| `rename` | Write it next to the other one as name (1).ext. |

Destinations are decided up front, on a single thread, before anything is written — so two entries with the same name can never race for the same free name.

## Parallel extraction

A `.zip` is random access: the central directory says where every entry starts, so one thread per core can each open the file and decompress a different entry. A `.tar` is a single stream — and a `.tar.gz` a single gzip stream on top of it — so there’s nothing to split and extraction stays sequential.

| 287 MB in 16 text files · Windows 11 | Time |
| --- | --- |
| Arca, 16 threads | **0.207 s** |
| Arca, -j 1 | 0.686 s |
| 7-Zip | 1.020 s |

> [!NOTE]
> **Many small files**
>
> Splitting changes nothing when there are thousands of tiny files: extracting 5,358 source files took 4.5 s, but decompressing those same 55 MB took 0.128 s. 97% of the time goes into creating files on NTFS — and 7-Zip, at 4.6 s, hits the same wall.

## Safe paths

Every entry name passes through Arca’s Zip Slip defence before it’s joined to the destination. An entry such as `../../etc/passwd` is rejected with an error instead of being written outside the target directory.
