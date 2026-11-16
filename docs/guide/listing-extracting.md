---
description: Look inside archives and unpack them in parallel and safely.
group: Using Arca
order: 5
keywords: list extract unpack parallel conflict overwrite skip rename zip slip -o
---

# Listing & extracting

## Listing

```text
arca list <ARCHIVE> [-t | --time] [-p PASSWORD]
```

Prints one line per entry (uncompressed size, method, ratio and name) without extracting anything.

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
| `-p, --password` | none | Password of an encrypted archive (AES-256 or ZipCrypto). |

### Conflicts

| Policy | Behaviour |
| --- | --- |
| `overwrite` | Replace the file that is already there. |
| `skip` | Leave the existing file alone and move on. |
| `rename` | Write it next to the other one as name (1).ext. |

ZIP destinations are decided up front on one thread. 7z resolves conflicts sequentially while processing blocks; duplicate names cannot race for the same free name.

## Parallel extraction

A `.zip` is random access: the central directory says where every entry starts, so one thread per core can each open the file and decompress a different entry. A `.tar` is a single stream, and a `.tar.gz` or `.tar.xz` a single compressed stream on top of it, so there’s nothing to split and extraction stays sequential. TAR entries and a standalone `.xz` are written under a temporary name and renamed only once their data has been read and checked, so a truncated or corrupt archive leaves no short file under the real name. Entries that finished before the damage stay.

| 287 MB in 16 text files · Windows 11 | Time |
| --- | --- |
| Arca, 16 threads | **0.207 s** |
| Arca, -j 1 | 0.686 s |
| 7-Zip | 1.020 s |

> [!NOTE]
> **Many small files**
>
> Splitting changes nothing when there are thousands of tiny files: extracting 5,358 source files took 4.5 s, but decompressing those same 55 MB took 0.128 s. 97% of the time goes into creating files on NTFS, and 7-Zip took 4.6 s on the same files.

## 7z, passwords and solid blocks

```sh
arca list secret.7z -p "a password"
arca test secret.7z -p "a password"
arca extract secret.7z -o restored/ -p "a password" --on-conflict rename
```

Plain and clear-header encrypted 7z archives can be listed without a password.
Hidden headers require one even to list. Listing does not validate file contents;
use `test` for that. In a solid block, only the first member reports the packed
size, so per-file ratios are not independent compression measurements.

7z extraction is sequential, including selected entries in the desktop window.
Skipped solid data still has to be decoded. For any encrypted 7z, Arca validates
all contents before any destination directory/file creation or replacement, then
extracts selected contents in a second pass. A wrong password or failed encrypted
checksum leaves the destination untouched. CRC is not authentication: the error
can also mean corruption, and the source must stay unchanged between passes.

Existing destination symlinks and reparse points are refused, and replacements
are staged rather than truncating existing hard links. This does not eliminate
concurrent filesystem replacement races. Plain extraction can fail after earlier
entries have been written; there is no whole-directory rollback. See
[Architecture](architecture.md#7z-boundaries) for resource and codec limits.

## Safe paths

Every entry name passes through Arca’s Zip Slip defence before it’s joined to the destination. An entry such as `../../etc/passwd` is rejected with an error instead of being written outside the target directory.
