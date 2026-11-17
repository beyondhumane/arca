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

ZIP comments, both the archive’s and any entry’s, also go to standard error after the list, as do a RAR archive’s comment and the pax `comment` records of TAR entries. [`arca comment`](cli-reference.md#comment) prints one comment, or changes it in a ZIP. The desktop window shows a ZIP or RAR archive comment in the status line when the archive opens, and a ZIP entry’s comment as a tooltip on its row; see [Comments and repair](desktop-app.md#comments-and-repair).

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

A `.zip` is random access: the central directory says where every entry starts, so one thread per core can each open the file and decompress a different entry. A `.tar` is a single stream, and a `.tar.gz`, `.tar.xz`, `.tar.bz2` or `.tar.zst` a single compressed stream on top of it, so there’s nothing to split and extraction stays sequential. TAR entries and a standalone `.xz`, `.bz2` or `.zst` are written under a temporary name and renamed only once their data has been read and checked, so a truncated or corrupt archive leaves no short file under the real name. Entries that finished before the damage stay.

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

## Links in TAR archives

A TAR can hold symbolic links and hard links. `arca list` shows where each one points: `name -> target` for a symbolic link, `name link to target` for a hard link. GNU long link names and pax `linkpath` records are read, so a target of any length up to 4,096 bytes comes through whole.

Extraction recreates links only when they can do no harm:

- **Symbolic links** are made after every other entry has been written, so no file in the archive can be written through a link from the same archive. A link is made only when its target is relative and, read from where the link stands, stays inside the destination: it may climb with `..` only as far as the destination itself, and every folder it goes through must already be a real folder there, not a link. Absolute targets, targets that climb out, and targets that pass through another link or a missing folder are left out. A link is never put where a folder already stands. Existing files in its place follow `--on-conflict`.
- **Hard links** are made only to a file this same extraction wrote, under the name it was given (after a `rename`, the renamed one). A hard link to anything else is left out. Where the file system has no hard links, such as FAT32 or exFAT, the file is copied instead.
- **On Windows** symbolic links need privileges most accounts do not have, so they are always left out and reported; their targets are not copied in their place. Hard links work on NTFS without privileges.

Every link left out is reported on standard error with its reason (in the window, as a count after the summary), and the extraction carries on. A destination that already holds a link from earlier is still never written through.
