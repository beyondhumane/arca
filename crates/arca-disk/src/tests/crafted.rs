// Images built to make the parsers do too much work or allocate too much:
// each must open, list and fail quickly instead.
use super::filesystems::exfat_set;
use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const VHD_BLOCK: u64 = 2 * 1024 * 1024;

/// A dynamic VHD of `size` bytes with 2 MiB blocks, storing only the blocks
/// the `pieces` reach into.
pub(crate) fn vhd_sparse(size: u64, pieces: &[(u64, &[u8])]) -> Vec<u8> {
    let size = size.div_ceil(VHD_BLOCK) * VHD_BLOCK;
    let mut blocks: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    for &(at, bytes) in pieces {
        for (k, byte) in bytes.iter().enumerate() {
            let pos = at + k as u64;
            let block = blocks
                .entry(pos / VHD_BLOCK)
                .or_insert_with(|| vec![0; VHD_BLOCK as usize]);
            block[(pos % VHD_BLOCK) as usize] = *byte;
        }
    }
    let mut footer = vec![0u8; 512];
    footer[..8].copy_from_slice(b"conectix");
    footer[8..12].copy_from_slice(&2u32.to_be_bytes());
    footer[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    footer[16..24].copy_from_slice(&512u64.to_be_bytes());
    footer[40..48].copy_from_slice(&size.to_be_bytes());
    footer[48..56].copy_from_slice(&size.to_be_bytes());
    footer[60..64].copy_from_slice(&3u32.to_be_bytes());
    fix_vhd_checksum(&mut footer);
    let entries = (size / VHD_BLOCK) as u32;
    let mut header = vec![0u8; 1024];
    header[..8].copy_from_slice(b"cxsparse");
    header[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
    header[16..24].copy_from_slice(&1536u64.to_be_bytes());
    header[24..28].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    header[28..32].copy_from_slice(&entries.to_be_bytes());
    header[32..36].copy_from_slice(&(VHD_BLOCK as u32).to_be_bytes());
    let sum = header
        .iter()
        .enumerate()
        .filter(|(i, _)| !(36..40).contains(i))
        .fold(0u32, |s, (_, &b)| s.wrapping_add(u32::from(b)));
    header[36..40].copy_from_slice(&(!sum).to_be_bytes());
    let mut out = footer.clone();
    out.extend_from_slice(&header);
    let table_at = out.len();
    out.resize(table_at + (entries as usize * 4).div_ceil(512) * 512, 0xff);
    for (index, data) in blocks {
        let sector = (out.len() / 512) as u32;
        let at = table_at + 4 * index as usize;
        out[at..at + 4].copy_from_slice(&sector.to_be_bytes());
        out.extend_from_slice(&[0xff; 512]);
        out.extend_from_slice(&data);
    }
    out.extend_from_slice(&footer);
    out
}

/// An exFAT volume of `clusters` 512-byte clusters in a sparse VHD, with the
/// root folder at cluster `root`, the FAT `fat` and `data` put at clusters.
fn exfat_sparse(clusters: u32, root: u32, fat: &[u32], data: &[(u32, &[u8])]) -> Vec<u8> {
    let (len, pieces) = exfat_layout(clusters, root, fat, data);
    let pieces: Vec<(u64, &[u8])> = pieces.iter().map(|(at, b)| (*at, &b[..])).collect();
    vhd_sparse(len, &pieces)
}

/// The bytes of that exFAT volume: its length and the pieces that are not
/// zeros.
fn exfat_layout(
    clusters: u32,
    root: u32,
    fat: &[u32],
    data: &[(u32, &[u8])],
) -> (u64, Vec<(u64, Vec<u8>)>) {
    let fat_sectors = (u64::from(clusters) + 2).saturating_mul(4).div_ceil(512) as u32;
    let heap = 24 + fat_sectors;
    let total = u64::from(heap) + u64::from(clusters);
    let mut boot = vec![0u8; 512];
    boot[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[72..80].copy_from_slice(&total.to_le_bytes());
    boot[80..84].copy_from_slice(&24u32.to_le_bytes());
    boot[84..88].copy_from_slice(&fat_sectors.to_le_bytes());
    boot[88..92].copy_from_slice(&heap.to_le_bytes());
    boot[92..96].copy_from_slice(&clusters.to_le_bytes());
    boot[96..100].copy_from_slice(&root.to_le_bytes());
    boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
    boot[108] = 9;
    boot[109] = 0;
    boot[110] = 1;
    boot[510] = 0x55;
    boot[511] = 0xaa;
    let fat: Vec<u8> = fat.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut pieces = vec![(0, boot), (24 * 512, fat)];
    for &(c, bytes) in data {
        pieces.push(((u64::from(heap) + u64::from(c - 2)) * 512, bytes.to_vec()));
    }
    (total * 512, pieces)
}

/// An exFAT volume of `clusters` 512-byte clusters, in a sparse VHD, whose
/// FAT loops 2 -> 3 -> ... -> `ring` + 1 -> 2, with `files` files of `size`
/// bytes that all start at cluster 2.
pub(crate) fn exfat_loop(clusters: u32, ring: u32, files: usize, size: u64) -> Vec<u8> {
    let mut dir = Vec::new();
    for i in 0..files {
        dir.extend(exfat_set(&format!("f{i}"), false, 2, size, size, true));
    }
    let root = ring + 2;
    let root_clusters = dir.len().div_ceil(512).max(1) as u32;
    assert!(root + root_clusters <= clusters + 2);
    let mut fat = vec![0u32; (root + root_clusters) as usize];
    fat[0] = 0xffff_fff8;
    fat[1] = 0xffff_ffff;
    for c in 2..=ring {
        fat[c as usize] = c + 1;
    }
    fat[(ring + 1) as usize] = 2;
    for c in root..root + root_clusters {
        fat[c as usize] = c + 1;
    }
    fat[(root + root_clusters - 1) as usize] = 0xffff_ffff;
    exfat_sparse(clusters, root, &fat, &[(root, &dir)])
}

/// A FAT16 volume of 5000 512-byte clusters holding one file of almost
/// 4 GiB whose chain loops 2 -> 3 -> ... -> 40 -> 2.
fn fat16_loop() -> Vec<u8> {
    let (fat_sectors, root_sectors, clusters) = (20u32, 32u32, 5000u32);
    let data = 1 + fat_sectors + root_sectors;
    let total = data + clusters;
    let mut img = vec![0u8; total as usize * 512];
    let b = &mut img[..512];
    b[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    b[11..13].copy_from_slice(&512u16.to_le_bytes());
    b[13] = 1;
    b[14..16].copy_from_slice(&1u16.to_le_bytes());
    b[16] = 1;
    b[17..19].copy_from_slice(&(root_sectors as u16 * 16).to_le_bytes());
    b[19..21].copy_from_slice(&(total as u16).to_le_bytes());
    b[21] = 0xf8;
    b[22..24].copy_from_slice(&(fat_sectors as u16).to_le_bytes());
    b[510] = 0x55;
    b[511] = 0xaa;
    let mut set = |c: u32, v: u16| {
        let at = 512 + 2 * c as usize;
        img[at..at + 2].copy_from_slice(&v.to_le_bytes());
    };
    set(0, 0xfff8);
    set(1, 0xffff);
    for c in 2..40 {
        set(c, c as u16 + 1);
    }
    set(40, 2);
    let root = (1 + fat_sectors) as usize * 512;
    let e = &mut img[root..root + 32];
    e[..11].copy_from_slice(b"LOOP    BIN");
    e[11] = 0x20;
    e[26..28].copy_from_slice(&2u16.to_le_bytes());
    e[28..32].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
    img
}

fn read_fails(a: &DiskArchive, name: &str) {
    let i = a.entries().iter().position(|e| e.name == name).unwrap();
    let got = a.read_entry_to(i, &mut io::sink(), &|_, _, _| true);
    assert!(matches!(got, Err(Error::Format(_))), "{name}: {got:?}");
}

#[test]
fn looping_exfat_chains_are_cut_where_they_loop() {
    let (_dir, a) = open(Format::Vhd, &exfat_loop(1 << 20, 4096, 8, 512 << 20));
    let a = a.unwrap();
    for i in 0..8 {
        assert!(
            names(&a).contains(&format!("f{i}").as_str()),
            "{:?}",
            names(&a)
        );
    }
    assert!(
        a.notices()
            .iter()
            .any(|n| n.contains("8 files have a cluster chain that loops")),
        "{:?}",
        a.notices()
    );
    read_fails(&a, "f3");
}

#[test]
fn chains_that_cross_stop_once_the_volume_is_walked() {
    let clusters = 1 << 18;
    let (_dir, a) = open(
        Format::Vhd,
        &exfat_loop(clusters, clusters - 16, 6, u64::from(clusters) * 512),
    );
    let a = a.unwrap();
    assert_eq!(a.entries().len(), 6);
    assert!(
        a.notices().iter().any(|n| n.contains("cross one another")),
        "{:?}",
        a.notices()
    );
    read_fails(&a, "f5");
}

#[test]
fn a_long_chain_walk_can_be_stopped() {
    let clusters = 1 << 18;
    let image = exfat_loop(clusters, clusters - 16, 1, u64::from(clusters) * 512);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loop.vhd");
    stdfs::write(&path, image).unwrap();
    let calls = AtomicUsize::new(0);
    let stop = |_: usize, _: usize, _: &str| calls.fetch_add(1, Ordering::Relaxed) < 8;
    let got = DiskArchive::open_with_progress(&path, Format::Vhd, &stop);
    assert!(matches!(got, Err(Error::Cancelled)), "{:?}", got.err());
}

#[test]
fn a_looping_fat_chain_is_cut_where_it_loops() {
    let (_dir, a) = open(Format::Img, &fat16_loop());
    let a = a.unwrap();
    assert_eq!(names(&a), ["LOOP.BIN"]);
    assert!(
        a.notices().iter().any(|n| n.contains("loops")),
        "{:?}",
        a.notices()
    );
    read_fails(&a, "LOOP.BIN");
}

#[test]
fn a_huge_sparse_exfat_folder_is_read_only_up_to_its_end() {
    let mut fat = vec![0u32; 3];
    fat[0] = 0xffff_fff8;
    fat[1] = 0xffff_ffff;
    fat[2] = 0xffff_ffff;
    let root = exfat_set("Big", true, 3, 256 << 20, 256 << 20, false);
    let inner = exfat_set("inner.txt", false, 1 << 19, 2, 2, false);
    let image = exfat_sparse(
        1 << 20,
        2,
        &fat,
        &[(2, &root), (3, &inner), (1 << 19, b"hi")],
    );
    let (_dir, a) = open(Format::Vhd, &image);
    let a = a.unwrap();
    assert_eq!(names(&a), ["Big", "Big/inner.txt"]);
    assert_eq!(read(&a, "Big/inner.txt"), b"hi");
}

/// The HFS+ sample in a sparse VHD of a little over 1 GiB, whose catalog
/// fork claims the whole GiB: only the nodes the leaf chain names are read.
fn hfs_big_catalog(broken: bool) -> Vec<u8> {
    let mut first = super::hfs::hfs();
    if broken {
        assert_eq!(first[24 * 512 + 8], 1);
        first[24 * 512 + 8] = 0;
    }
    let block = 512u64;
    let size = 1u64 << 30;
    let blocks = (size + (4 << 20)) / block;
    let h = &mut first[1024..1536];
    h[44..48].copy_from_slice(&(blocks as u32).to_be_bytes());
    h[272..280].copy_from_slice(&size.to_be_bytes());
    h[272 + 20..272 + 24].copy_from_slice(&((size / block) as u32).to_be_bytes());
    vhd_sparse(blocks * block, &[(0, &first)])
}

#[test]
fn a_huge_hfs_plus_catalog_is_read_a_node_at_a_time() {
    let (_dir, a) = open(Format::Vhd, &hfs_big_catalog(false));
    super::hfs::check_hfs(&a.unwrap(), "");
    let (_dir, a) = open(Format::Vhd, &hfs_big_catalog(true));
    assert!(
        matches!(a, Err(Error::Format(ref e)) if e.contains("header node")),
        "{:?}",
        a.err()
    );
}

#[test]
fn a_gpt_entry_that_spans_every_sector_is_cut_at_the_disk_end() {
    let mut image = gpt(&[&sample_fat(12)]);
    image[1024 + 32..1024 + 40].copy_from_slice(&0u64.to_le_bytes());
    image[1024 + 40..1024 + 48].copy_from_slice(&u64::MAX.to_le_bytes());
    let crc = crc32fast::hash(&image[1024..1024 + 128 * 128]);
    image[512 + 88..512 + 92].copy_from_slice(&crc.to_le_bytes());
    image[512 + 16..512 + 20].fill(0);
    let header = crc32fast::hash(&image[512..512 + 92]);
    image[512 + 16..512 + 20].copy_from_slice(&header.to_le_bytes());
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    assert!(
        a.notices().iter().any(|n| n.contains("runs past the end")),
        "{:?}",
        a.notices()
    );
}

#[test]
fn a_dynamic_vhd_is_no_larger_than_its_block_table_reaches() {
    let mut image = vhd_sparse(VHD_BLOCK, &[(0, &[0x42; 512])]);
    let tail = image.len() - 512;
    for at in [0, tail] {
        let footer = &mut image[at..at + 512];
        footer[40..48].copy_from_slice(&(1u64 << 50).to_be_bytes());
        footer[48..56].copy_from_slice(&(1u64 << 50).to_be_bytes());
        fix_vhd_checksum(footer);
    }
    let (_dir, a) = open(Format::Vhd, &image);
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    assert_eq!(a.entries()[0].size, VHD_BLOCK);
}

#[test]
fn a_dmg_is_no_larger_than_its_blocks_reach() {
    let mut image = containers::dmg(&[("free (Apple_Free : 0)", &[0x42; 4096])]);
    let koly = image.len() - 512;
    image[koly + 492..koly + 500].copy_from_slice(&(1u64 << 40).to_be_bytes());
    let (_dir, a) = open(Format::Dmg, &image);
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    assert_eq!(a.entries()[0].size, 4096);
}

#[test]
fn an_exfat_file_larger_than_the_volume_is_left_out() {
    let fat = [0xffff_fff8, 0xffff_ffff, 0xffff_ffff];
    let mut root = exfat_set("huge", false, 3, 1 << 50, 512, false);
    root.extend(exfat_set("fits", false, 3, 1 << 20, 2, false));
    let image = exfat_sparse(1 << 12, 2, &fat, &[(2, &root), (3, b"ok")]);
    let (_dir, a) = open(Format::Vhd, &image);
    let a = a.unwrap();
    assert_eq!(names(&a), ["fits"]);
    assert!(a.notices().iter().any(|n| n.contains("damaged")));
    let fits = read(&a, "fits");
    assert_eq!(&fits[..2], b"ok");
    assert_eq!(fits.len(), 1 << 20);
}

/// A device of `len` bytes that holds `pieces` and zeros elsewhere, and
/// counts the bytes read from it.
struct Counting {
    len: u64,
    pieces: Vec<(u64, Vec<u8>)>,
    read: u64,
}

impl device::Device for Counting {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        self.read += buf.len() as u64;
        buf.fill(0);
        let end = pos + buf.len() as u64;
        for (at, bytes) in &self.pieces {
            let from = pos.max(*at);
            let to = end.min(at + bytes.len() as u64);
            if from < to {
                buf[(from - pos) as usize..(to - pos) as usize]
                    .copy_from_slice(&bytes[(from - at) as usize..(to - at) as usize]);
            }
        }
        Ok(())
    }
}

fn list_counting(kind: crate::fs::Fs, dev: &mut Counting) -> Vec<String> {
    let len = dev.len;
    let mut vol = Volume { dev, base: 0, len };
    let listing =
        crate::fs::list(kind, &mut vol, &mut crate::fs::Runs::default(), &|| true).unwrap();
    listing.members.into_iter().map(|m| m.entry.name).collect()
}

#[test]
fn listing_a_huge_sparse_exfat_folder_reads_little() {
    let fat = [0xffff_fff8, 0xffff_ffff, 0xffff_ffff];
    let root = exfat_set("Big", true, 3, 256 << 20, 256 << 20, false);
    let inner = exfat_set("inner.txt", false, 1 << 19, 2, 2, false);
    let (len, pieces) = exfat_layout(1 << 20, 2, &fat, &[(2, &root), (3, &inner)]);
    let mut dev = Counting {
        len,
        pieces,
        read: 0,
    };
    assert_eq!(
        list_counting(crate::fs::Fs::Exfat, &mut dev),
        ["Big", "Big/inner.txt"]
    );
    assert!(dev.read < 1 << 20, "{} bytes read", dev.read);
}

#[test]
fn listing_a_huge_hfs_plus_catalog_reads_little() {
    let image = super::hfs::hfs();
    let block = 512u64;
    let size = 1u64 << 30;
    let blocks = (size + (4 << 20)) / block;
    let mut first = image.clone();
    let h = &mut first[1024..1536];
    h[44..48].copy_from_slice(&(blocks as u32).to_be_bytes());
    h[272..280].copy_from_slice(&size.to_be_bytes());
    h[272 + 20..272 + 24].copy_from_slice(&((size / block) as u32).to_be_bytes());
    let mut dev = Counting {
        len: blocks * block,
        pieces: vec![(0, first)],
        read: 0,
    };
    let names = list_counting(crate::fs::Fs::Hfs, &mut dev);
    assert!(names.contains(&"Docs/big.bin".to_string()), "{names:?}");
    assert!(dev.read < 1 << 20, "{} bytes read", dev.read);
}
