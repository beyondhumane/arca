---
description: Create, list, test and extract an archive in four commands.
group: Getting started
order: 3
keywords: tutorial first steps aliases getting started basics
---

# Quick start

Four commands cover almost everything. Each one prints a single summary line when it’s done.

1. **Create an archive**

   ```sh
   arca create photos.zip ~/Pictures/trip/
   ```

   The format comes from the extension. The summary looks like this:

   ```text
   photos.zip: 412 files, 1.8 GB -> 1.7 GB (4.1% smaller) in 2.114 s · 871 MB/s · 16 threads
   ```

2. **Look inside**

   ```sh
   arca list photos.zip
   ```

   One line per entry: size, method, ratio and name. Nothing is extracted.

3. **Check it**

   ```sh
   arca test photos.zip
   ```

   Every CRC is verified, and nothing is written to disk.

4. **Extract it**

   ```sh
   arca extract photos.zip -o trip/
   ```

   `.zip` archives are extracted in parallel, one entry per thread.

## Short aliases

| Command | Alias | Example |
| --- | --- | --- |
| `create` | `c` | `arca c out.zip src/` |
| `list` | `l` | `arca l out.zip` |
| `extract` | `x` | `arca x out.zip -o dest/` |
| `test` | `t` | `arca t out.zip` |

## Formats come from the extension

| Extension | Format |
| --- | --- |
| `.zip` | ZIP, with Zip64 when needed |
| `.tar` | ustar TAR |
| `.tar.gz` · `.tgz` | TAR inside a gzip stream |

Any other extension is refused with an error that lists the supported ones.

## Getting help

```sh
arca --help
arca create --help
arca --version
```
