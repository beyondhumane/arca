---
description: Read CPIO, AR, DEB, RPM, CAB and LZH/LHA archives, the other formats 7-Zip opens, without ever writing one.
group: Reference
order: 22
keywords: cpio ar deb rpm cab cabinet lzh lha mszip lzx initramfs debian package read-only
---

# CPIO, AR, DEB, RPM, CAB and LZH

Arca reads six more formats that 7-Zip and NanaZip open. They are built in,
need no Cargo feature and are **read-only**: Arca never creates or modifies
them. Listing, testing, extracting, previews, the checksums of entries in the
window and comparisons (`arca diff`) work as for any other archive.

```sh
arca list hello_1.0_amd64.deb
arca test driver.cab
arca extract initrd.cpio -o initrd/
arca diff hello-1.0-1.x86_64.rpm unpacked/
```

In the window the title says `CAB: read-only` (or the format's name), and add,
delete, rename, move, new folder and password are refused before the file is
touched.

| Format | Suffixes | What is read |
|---|---|---|
| CPIO | `.cpio` | `newc` and `crc` (uncompressed Linux initramfs, RPM payloads), POSIX `odc` and the old binary headers in either byte order. The `crc` sums are checked. |
| AR | `.ar`, `.a` | Unix archives and static libraries, with GNU (`//`) and BSD (`#1/`) long names. Symbol tables are not files and are not listed. Thin archives are refused. |
| DEB | `.deb`, `.udeb` | The files the package installs, plus its control files under `DEBIAN/`, as `dpkg-deb -R` lays them out. `data.tar` and `control.tar` may be plain, gzip, xz, bzip2 or Zstandard. |
| RPM | `.rpm` | The CPIO payload, plain or compressed with gzip, xz, bzip2 or Zstandard. Signatures and header digests are not checked, and the listing says so. |
| CAB | `.cab` | Stored, MSZIP and LZX folders, with every block checksum checked. |
| LZH | `.lzh`, `.lha` | Header levels 0 to 3; `-lh0-` to `-lh7-`, `-lz4-`, `-lzs-`, `-lz5-`, `-pm0-` to `-pm2-`. Every file's CRC-16 is checked. |

A file whose name says nothing is recognized by its first bytes, and an `ar`
whose first member is `debian-binary` is read as a DEB.

### Why a DEB shows its files and not its two tarballs

7-Zip opens a `.deb` as the `ar` it is: `debian-binary`, `control.tar.xz` and
`data.tar.xz`, each to be opened in turn. Arca shows what is inside instead,
the same tree `dpkg-deb -R` unpacks: the package's files where they install,
and `control`, `md5sums` and the maintainer scripts under `DEBIAN/`. An RPM is
shown the same way, as the files of its payload.

## Links and special files

CPIO, DEB and RPM carry Unix symbolic and hard links. They are handled as for
TAR: a symbolic link is made only on Unix and only when its target stays inside
the destination, and a hard link becomes a link to the file extracted with it,
or a copy where the file system has none. Anything else is left out and
reported. Devices, pipes and sockets are neither listed nor extracted; the CLI
prints `note: N device files, pipes or sockets are not listed`.

## What is not read

- **CAB:** Quantum-compressed folders are listed but not extracted, and files
  that continue in another cabinet of a set are left out with a note.
- **RPM:** payloads in legacy LZMA (not XZ), and the large-file `07070X` CPIO
  headers of packages with files over 4 GiB.
- **DEB:** packages in the old 0.939 format and `data.tar.lzma`.
- **LZH:** `-lhx-` and the other UNLHA32 extensions.
- **Compressed CPIO:** an `initrd.img` is a CPIO inside gzip, often after an uncompressed microcode CPIO, and does not open as CPIO; a `.cpio.gz`, `.cpio.xz`, `.cpio.bz2` or `.cpio.zst` opens as a single compressed file. Decompress it first.
- **LZH names** in a code page, usually Shift-JIS in Japanese archives, are shown with every non-ASCII byte as `%xx`.
- **WIM** and **ARJ** are not supported. No maintained pure-Rust WIM reader
  exists yet: the crates published for it are first releases, and WIM also
  needs XPRESS and LZMS decoders.

## Safety

The CPIO, AR, DEB, RPM and CAB parsers are Arca's own, in safe Rust. LZX comes
from the [`lzxd`](https://crates.io/crates/lzxd) crate and LZH from
[`delharc`](https://crates.io/crates/delharc), both MIT or Apache-2.0, with
`delharc` built without its unchecked hints. A damaged or hostile file gives an
error, never a crash:

- every size is held against the file or against the stream: a member that
  declares more data than there is fails as soon as the data runs out, and
  nothing is allocated from a declared size;
- names are limited to 4096 bytes, entries to Arca's usual ten million, `ar`
  name tables to 16 MiB, RPM headers to rpm's own limits and CAB blocks to
  32 KiB;
- names go through the same checks as every other format, so `../` and drive
  letters are refused when extracting.

Each file is written to a temporary name and renamed once complete. Previews
are limited to 64 MiB.

## Checking it against other tools

The CAB fixtures in `crates/arca-extra/tests/fixtures` were made with Windows'
`makecab` (MSZIP and LZX) and are listed by `bsdtar` too. The CPIO and AR
readers were checked by hand on archives written by `bsdtar --format newc`,
`odc`, `bin`, `ar` and `argnu`.
