---
description: Browse VHD, VHDX, VMDK, DMG and raw disk images read-only, the way 7-Zip does, with MBR and GPT partitions and FAT, exFAT, NTFS, HFS+ and ISO 9660 inside.
group: Reference
order: 23
keywords: vhd vhdx vmdk dmg img disk image virtual machine partition mbr gpt fat exfat ntfs hfs read-only
---

# Disk images

Arca opens the disk images of virtual machines, of macOS installers and of
SD cards and USB sticks, and shows the files inside them, as 7-Zip does. They
are built in, need no Cargo feature and are **read-only**: Arca never creates,
modifies or mounts them. Listing, testing, extracting, previews, the checksums
of entries in the window and comparisons (`arca diff`) work as for any other
archive.

```sh
arca list server.vhdx
arca test macOS-installer.dmg
arca extract sdcard.img -o sdcard/
arca diff guest.vmdk restored/
```

## Containers

| Format | Suffix | What is read |
|---|---|---|
| Raw image | `.img` | The bytes of the disk as they are. |
| VHD | `.vhd` | Fixed and dynamic disks, with their footer and dynamic header checksums checked. Differencing disks are refused: their data is split with a parent disk. |
| VHDX | `.vhdx` | Fixed and dynamic disks with 512 or 4096-byte sectors. The headers and the region table are checked against their CRC-32C, and the newer of the two headers wins. An image whose log still has to be replayed, because it was not closed cleanly, is refused rather than misread: attach it once in Windows or Hyper-V and detach it. Differencing disks are refused. |
| VMDK | `.vmdk` | Monolithic sparse and stream-optimized (deflated grains) extents, and text descriptors whose `FLAT`, `ZERO` and `SPARSE` extents sit next to them, such as `monolithicFlat` and the split `twoGbMaxExtent` disks. An extent named with a folder, as in `../disk.vmdk`, is refused. Snapshots (a `parentCID` other than `ffffffff`) are refused. |
| DMG | `.dmg` | Apple UDIF images: the `koly` trailer, the `blkx` block map of the property list, and chunks stored, zero-filled, or compressed with zlib, bzip2, ADC or LZMA (xz). With nothing selected, `arca test` also checks each block's CRC-32 against its decoded data, as 7-Zip does. |

A dynamic VHD, a VHDX and a VMDK are also recognized by their first bytes when
their name says nothing. A fixed VHD, a DMG and a raw image keep nothing
telling at the start, so only the suffix says what they are.

## Partitions and file systems

A disk with an MBR (including logical partitions in an extended one) or a GPT
shows each partition as a numbered folder at the top, named after its file
system: `0.ntfs/`, `1.fat/`, `2.exfat/`, `3.hfs/`, `4.iso/`. A DMG lists its
blocks the same way, leaving out the free space. A partition whose file system
Arca does not read, such as ext4, APFS or a Linux swap area, is listed as one
raw file, `N.img`, that can be extracted and opened on its own. A disk without
a partition table that holds a file system directly, such as a floppy or a
"superfloppy" USB stick, shows that file system's files at the top; one that
holds nothing Arca knows is one raw file named after the image.

| File system | What is read |
|---|---|
| FAT12, FAT16, FAT32 | Long file names, the NT lower-case flags for short names, and the creation, modification and access times. |
| exFAT | Contiguous and chained files, the valid data length (the rest reads as zeros) and the time zone of each time. Entry sets are checked against their checksum. |
| NTFS | Every file and folder of the master file table, with resident, sparse and fragmented data and files whose records spill into extension records. Alternate data streams are not shown. |
| HFS+ and HFSX | The catalog, the extents overflow file, symbolic links and file hard links. A `/` in a name is shown as `:`, as the Finder does. |
| ISO 9660 | Through the same reader as [`.iso` files](iso.md), with Rock Ridge and Joliet names. |

## What is not read

- **APFS and ext2/3/4** are not browsed: their partitions are listed as raw
  `N.img` files. Most current macOS DMGs hold APFS.
- **LZFSE chunks in a DMG.** The only pure-Rust LZFSE decoder relies on
  hundreds of `unsafe` blocks, and Arca does not hand it untrusted bytes. The
  listing says so, and the partitions those chunks belong to cannot be read.
- **Encrypted DMGs** (`encrcdsa`), **segmented DMGs** and old DMGs that keep
  their block map only in a resource fork are refused.
- **NTFS files compressed or encrypted by NTFS**, and its symbolic links,
  junctions and other reparse points, are left out and counted in a note.
- **HFS+ files compressed by HFS+** (the `UF_COMPRESSED` flag, common for files
  copied by the Finder or installed by macOS) and folder hard links are left out
  and counted in a note.
- **Differencing VHD and VHDX disks and VMDK snapshots**, which need their
  parent disk.
- Logical volumes (LVM, Storage Spaces), RAID and BitLocker or FileVault
  volumes show as raw partitions.

## Why Arca never takes these files over

Disk images belong to the system that mounts them: Windows attaches VHD and
VHDX disks and mounts `.img` files with a double click, macOS opens DMGs with
its own mounter, and VMDK disks belong to VMware and VirtualBox. The installer
therefore never registers these suffixes, not even on request; open them from
the window, by dragging them onto it, or with *Open with*.

## Safety

Every parser is Arca's own, in safe Rust, and a damaged or hostile image gives
an error or a note, never a crash:

- no structure is allocated from a declared size before that size is held
  against the image: block tables, grain directories, the DMG property list
  (at most 64 MiB) and each decoded chunk (at most 64 MiB) are checked first;
- reads never go past the virtual disk, and regions the image does not store
  read as zeros;
- partitions that start past the disk are left out, and ones that run past it
  are cut where it ends; the chain of logical partitions is followed at most
  128 records and stops at a loop;
- FAT and exFAT cluster chains, NTFS parent links and HFS+ B-tree leaf chains
  stop at a loop, folders are followed at most 128 levels deep, and every file
  system shares one budget of data runs;
- extraction goes through the same destination checks as every other format,
  so a name that leads out of the folder is refused.

Tests build each image byte by byte (FAT, exFAT and HFS+ volumes, MBR and GPT
tables, VHD, VHDX, VMDK and DMG containers) and damage it: cut short at many
lengths, or with bytes flipped over its headers, it must open, test and read
without a panic. The
NTFS volume is the 2 MiB `testfs1` image of the [`ntfs`](https://crates.io/crates/ntfs)
crate (MIT or Apache-2.0), kept as `arca-disk/tests/fixtures/ntfs.img.xz`.

```sh
cargo test -p arca-disk
cargo test -p arca-cli --test disk
```
