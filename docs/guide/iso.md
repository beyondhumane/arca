---
description: Read ISO 9660 disc images (Rock Ridge, Joliet, multi-extent files) without ever writing one.
group: Reference
order: 21
keywords: iso 9660 disc image rock ridge joliet udf read-only cd dvd
---

# ISO disc images

Arca reads ISO 9660 images (`.iso`): CD/DVD images, Linux installation media,
software bundles. It is built in, needs no Cargo feature and is **read-only**:
Arca never creates or modifies an ISO.

```sh
arca list ubuntu.iso
arca test ubuntu.iso
arca extract ubuntu.iso -o ubuntu/
```

In the window, open the image like any archive. The title says
`ISO: read-only`; previews, extraction and testing work, while add, delete,
rename, move, new folder and password are refused before the image is touched.

## What is read

- **Names:** Rock Ridge (`NM`) first, then Joliet (UTF-16), then plain
  ISO 9660 with the `;1` version and the trailing dot removed.
- **Directories relocated by Rock Ridge** (`CL`/`RE`, the `rr_moved` trick for
  deep trees) appear in their real place.
- **Files larger than 4 GiB**, stored as several extents, come out as one file.
- **Dates** come from the directory records.

ISO 9660 stores no checksums. `arca test` proves every byte of every file can be
read from the image, not that the bytes are the original ones.

## What is left out, and said so

- **Symbolic links, devices, FIFOs and sockets** from Rock Ridge are neither
  listed nor extracted. The CLI prints `note: skipped N symbolic links or special
  files: ...` on stderr; the window shows the same line.
- **UDF.** Images that also carry UDF (DVD/Blu-ray, Windows installation media)
  are read through their ISO 9660 side, with a note that files stored only in
  UDF are not listed. On Windows media `sources/install.wim` is usually one of
  them. UDF-only images are rejected.
- Not supported: El Torito boot images as files, associated files, interleaved
  files, logical blocks other than 2048 bytes, mounting.

## Safety

The parser is safe Rust with no dependencies. A damaged image gives an error,
never a crash. Each image is checked for:

- volume descriptors, truncation and extents past the end of the file;
- directory records that are malformed or cross a sector boundary;
- directory loops, nesting deeper than 64 levels, more than 256 MiB of
  directory data, and Rock Ridge continuation chains longer than 32 areas or
  64 KiB;
- names that escape the destination or are unsafe on Windows (`..`, `/`, `\`,
  drive letters, `:<>"|?*`, control characters, `CON`, `NUL`, `COM1`, trailing
  dots or spaces);
- duplicate paths, including names that differ only in case, and file/directory
  collisions.

Extraction streams each file in 256 KiB chunks into a temporary file next to its
destination and renames it into place once complete. A link already inside the
destination tree is refused instead of followed. Skip, rename, overwrite and
cancel conflicts are honored. A failure or cancellation can leave files that
were already finished. Previews are limited to 64 MiB.

## Checking it against other tools

`bash interop.sh` builds images with `genisoimage` and `xorriso` (plain,
Joliet, Rock Ridge, relocated directories) and compares what Arca extracts with
`bsdtar` and `7z`, byte for byte. Set `ARCA_TEST_ISO_BIG=1` to also build a
4.7 GB sparse image and check a multi-extent file; it needs about 5 GB of free
disk space.
