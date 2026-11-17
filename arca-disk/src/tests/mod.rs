use super::*;
use std::fs as stdfs;

pub(crate) mod build;
pub(crate) mod containers;
mod crafted;
pub(crate) mod filesystems;
mod hfs;
mod images;

use build::*;
use containers::*;

pub(crate) fn open(format: Format, bytes: &[u8]) -> (tempfile::TempDir, Result<DiskArchive>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("disk");
    stdfs::write(&path, bytes).unwrap();
    let archive = DiskArchive::open(&path, format);
    (dir, archive)
}

pub(crate) fn names(a: &DiskArchive) -> Vec<&str> {
    a.entries().iter().map(|e| e.name.as_str()).collect()
}

pub(crate) fn read(a: &DiskArchive, name: &str) -> Vec<u8> {
    let i = a
        .entries()
        .iter()
        .position(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} not in {:?}", names(a)));
    a.read_entry(i).unwrap()
}

// One position in three up to `dense`, where the headers are, then one in
// `step`: a flipped byte at each, and a cut at one in four of them, must
// open, test and read without a panic.
pub(crate) fn survives_damage(format: Format, bytes: &[u8], dense: usize, step: usize) {
    let positions: Vec<usize> = (0..bytes.len())
        .filter(|&at| (at < dense && at % 3 == 0) || at % step == 0)
        .collect();
    survives_damage_at(format, bytes, &positions);
}

pub(crate) fn survives_damage_at(format: Format, bytes: &[u8], positions: &[usize]) {
    use std::io::{Seek, SeekFrom, Write};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("damaged");
    let exercise = || {
        if let Ok(a) = DiskArchive::open(&path, format) {
            let _ = a.test(&[], &|_, _, _| true);
            for i in 0..a.entries().len().min(8) {
                let _ = a.read_entry(i);
            }
        }
    };
    stdfs::write(&path, bytes).unwrap();
    let mut file = stdfs::OpenOptions::new().write(true).open(&path).unwrap();
    let mut poke = |at: usize, byte: u8| {
        file.seek(SeekFrom::Start(at as u64)).unwrap();
        file.write_all(&[byte]).unwrap();
        file.flush().unwrap();
    };
    for &at in positions {
        poke(at, bytes[at] ^ 0x5a);
        exercise();
        poke(at, bytes[at]);
    }
    let mut cuts: Vec<usize> = positions.iter().step_by(4).copied().collect();
    cuts.sort_unstable_by(|a, b| b.cmp(a));
    let file = stdfs::OpenOptions::new().write(true).open(&path).unwrap();
    for cut in cuts {
        file.set_len(cut as u64).unwrap();
        exercise();
    }
}

pub(crate) fn check_sample(a: &DiskArchive, prefix: &str) {
    let p = |name: &str| fs::join(prefix, name);
    let listed = names(a);
    for name in [
        "README.TXT",
        "A long file name.txt",
        "empty.dat",
        "Docs",
        "Docs/inner.bin",
        "Docs/Deeper",
        "Docs/Deeper/note.txt",
    ] {
        assert!(
            listed.contains(&p(name).as_str()),
            "{} in {listed:?}",
            p(name)
        );
    }
    assert_eq!(read(a, &p("README.TXT")), b"hello");
    assert_eq!(read(a, &p("A long file name.txt")), long_content());
    assert_eq!(read(a, &p("empty.dat")), b"");
    assert_eq!(read(a, &p("Docs/inner.bin")), [7]);
    assert_eq!(read(a, &p("Docs/Deeper/note.txt")), b"deep down");
    let docs = a.entries().iter().find(|e| e.name == p("Docs")).unwrap();
    assert!(docs.is_dir);
    let readme = a
        .entries()
        .iter()
        .find(|e| e.name == p("README.TXT"))
        .unwrap();
    assert!(readme.mtime.is_some());
}

#[test]
fn a_raw_fat12_image_lists_reads_and_extracts() {
    let image = sample_fat(12);
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    check_sample(&a, "");
    assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 5);
    let dest = tempfile::tempdir().unwrap();
    let out = a
        .extract(dest.path(), &[], &|_, _, _| true, &|_| Conflict::Overwrite)
        .unwrap();
    assert_eq!(out.files, 5);
    assert_eq!(
        stdfs::read(dest.path().join("Docs/Deeper/note.txt")).unwrap(),
        b"deep down"
    );
    assert_eq!(
        stdfs::read(dest.path().join("A long file name.txt")).unwrap(),
        long_content()
    );
}

#[test]
fn fat16_and_fat32_read_the_same_tree() {
    for width in [16, 32] {
        let (_dir, a) = open(Format::Img, &sample_fat(width));
        check_sample(&a.unwrap(), "");
    }
}

#[test]
fn mbr_partitions_become_numbered_folders_and_unknown_ones_raw_files() {
    let fat = sample_fat(12);
    let other = vec![0xa5u8; 64 * 512];
    let image = mbr(&[(0x01, &fat), (0x83, &other)], Some(&fat));
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    check_sample(&a, "0.fat");
    check_sample(&a, "2.fat");
    assert!(names(&a).contains(&"0.fat"));
    assert_eq!(read(&a, "1.img"), other);
}

#[test]
fn gpt_partitions_are_found_with_their_checksums() {
    let fat = sample_fat(16);
    let image = gpt(&[&fat, &[0x11; 8192]]);
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    check_sample(&a, "0.fat");
    assert_eq!(read(&a, "1.img"), vec![0x11; 8192]);
    let mut broken = image.clone();
    broken[512 + 24] ^= 1;
    let (_dir, a) = open(Format::Img, &broken);
    let a = a.unwrap();
    check_sample(&a, "0.fat");
    assert!(a.notices().iter().any(|n| n.contains("backup")));
}

#[test]
fn fixed_and_dynamic_vhds_hold_the_same_disk() {
    let disk = mbr(&[(0x0e, &sample_fat(12))], None);
    for image in [vhd_fixed(&disk), vhd_dynamic(&disk)] {
        let (_dir, a) = open(Format::Vhd, &image);
        check_sample(&a.unwrap(), "0.fat");
    }
    let mut differencing = vhd_fixed(&disk);
    let at = differencing.len() - 512;
    differencing[at + 63] = 4;
    fix_vhd_checksum(&mut differencing[at..]);
    let (_dir, a) = open(Format::Vhd, &differencing);
    assert!(matches!(a, Err(Error::Unsupported(_))));
}

#[test]
fn a_dynamic_vhd_reads_unallocated_blocks_as_zeros() {
    let mut disk = vec![0u8; 64 * 1024];
    disk[40_000] = 9;
    let (_dir, a) = open(Format::Vhd, &vhd_dynamic(&disk));
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    assert_eq!(read(&a, "disk.img"), disk);
}

#[test]
fn an_image_without_a_known_layout_is_one_raw_file() {
    let (_dir, a) = open(Format::Img, &[0x42; 3000]);
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    assert_eq!(read(&a, "disk.img"), vec![0x42; 3000]);
    assert!(!a.notices().is_empty());
}

#[test]
fn a_looping_folder_chain_ends_instead_of_hanging() {
    let image = sample_fat_looping();
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    assert!(names(&a).contains(&"Docs"));
    let _ = a.test(&[], &|_, _, _| true);
}

#[test]
fn a_stop_is_reported_as_one() {
    let (_dir, a) = open(Format::Img, &sample_fat(12));
    let a = a.unwrap();
    assert!(matches!(
        a.test(&[], &|_, _, name| name.is_empty()),
        Err(Error::Cancelled)
    ));
}

#[test]
fn damaged_fat_images_never_panic() {
    survives_damage(Format::Img, &sample_fat(12), 4096, 499);
}

#[test]
fn damaged_partition_tables_never_panic() {
    let fat = sample_fat(12);
    survives_damage(Format::Img, &mbr(&[(0x01, &fat)], Some(&fat)), 1024, 4093);
    survives_damage(Format::Img, &gpt(&[&fat]), 1536, 2039);
}

#[test]
fn damaged_vhds_never_panic() {
    let disk = mbr(&[(0x0e, &sample_fat(12))], None);
    let image = vhd_dynamic(&disk);
    survives_damage(Format::Vhd, &image, 2048, 2039);
    let tail = image.len() - 512;
    for at in tail..image.len() {
        let mut flipped = image.clone();
        flipped[at] ^= 0xff;
        let (_dir, _) = open(Format::Vhd, &flipped);
    }
}

#[test]
fn an_mbr_that_lists_nothing_falls_back_to_a_raw_image() {
    let mut image = vec![0u8; 64 * 512];
    image[510] = 0x55;
    image[511] = 0xaa;
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    let mut protective = gpt(&[&sample_fat(12)]);
    protective[512 + 24] ^= 1;
    let last = protective.len() - 512;
    protective[last + 24] ^= 1;
    let (_dir, a) = open(Format::Img, &protective);
    let a = a.unwrap();
    assert_eq!(names(&a), ["disk.img"]);
    assert!(a.notices().iter().any(|n| n.contains("GPT are damaged")));
}

#[test]
fn many_gpt_entries_over_one_volume_share_one_budget() {
    let mut image = gpt(&[&sample_fat(12)]);
    let entry = image[1024..1024 + 128].to_vec();
    for i in 1..128 {
        image[1024 + 128 * i..1024 + 128 * (i + 1)].copy_from_slice(&entry);
    }
    let crc = crc32fast::hash(&image[1024..1024 + 128 * 128]);
    image[512 + 88..512 + 92].copy_from_slice(&crc.to_le_bytes());
    image[512 + 16..512 + 20].fill(0);
    let header = crc32fast::hash(&image[512..512 + 92]);
    image[512 + 16..512 + 20].copy_from_slice(&header.to_le_bytes());
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    check_sample(&a, "0.fat");
    check_sample(&a, "127.fat");
}
