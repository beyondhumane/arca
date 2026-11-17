use super::*;
use std::io::Read;

// The ntfs crate's testfs1 (MIT OR Apache-2.0, by Colin Finck), made on
// Linux with `mkntfs -c 512 -L mylabel -F testfs1` on a 2 MiB file, then
// mounted with ntfs-3g to add: empty-file (mtime 2021-01-01 13:37),
// file-with-12345 (resident), 1000-bytes-file ("12345" 200 times),
// sparse-file ("12345", a hole, then "11111" at byte 500000) and
// many_subdirs/1..512. Stored here as `xz -9e` of the image.
fn ntfs_image() -> Vec<u8> {
    let packed = include_bytes!("../../tests/fixtures/ntfs.img.xz");
    let mut out = Vec::new();
    arca_xz::Decoder::new(&packed[..])
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn check_ntfs(a: &DiskArchive, prefix: &str) {
    let p = |name: &str| fs::join(prefix, name);
    assert_eq!(read(a, &p("file-with-12345")), b"12345");
    assert_eq!(read(a, &p("1000-bytes-file")), b"12345".repeat(200));
    assert_eq!(read(a, &p("empty-file")), b"");
    let sparse = read(a, &p("sparse-file"));
    assert_eq!(sparse.len(), 500_005);
    assert_eq!(&sparse[..5], b"12345");
    assert_eq!(&sparse[500_000..], b"11111");
    assert!(sparse[5..500_000].iter().all(|&b| b == 0));
    let listed = names(a);
    assert!(listed.contains(&p("many_subdirs").as_str()));
    for n in [1, 256, 512] {
        assert!(listed.contains(&p(&format!("many_subdirs/{n}")).as_str()));
    }
    assert!(!listed.iter().any(|n| n.contains('$')), "{listed:?}");
    let empty = a
        .entries()
        .iter()
        .find(|e| e.name == p("empty-file"))
        .unwrap();
    let mtime = empty.mtime.unwrap();
    assert!((1_609_459_200 - 86_400..1_609_459_200 + 2 * 86_400).contains(&mtime));
}

#[test]
fn an_ntfs_volume_is_read_from_its_mft() {
    let image = ntfs_image();
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    check_ntfs(&a, "");
    a.test(&[], &|_, _, _| true).unwrap();
    let (_dir, a) = open(Format::Img, &mbr(&[(0x07, &image)], None));
    check_ntfs(&a.unwrap(), "0.ntfs");
}

#[test]
fn damaged_ntfs_volumes_never_panic() {
    let image = ntfs_image();
    let mft = (u64::from_le_bytes(image[48..56].try_into().unwrap()) * 512) as usize;
    let spots: Vec<usize> = (0..image.len())
        .filter(|&at| {
            (at < 512 && at % 5 == 0)
                || ((mft..mft + 24 * 1024).contains(&at) && at % 37 == 0)
                || at % 131_071 == 0
        })
        .collect();
    survives_damage_at(Format::Img, &image, &spots);
}

fn set_sum(set: &[u8]) -> u16 {
    set.iter().enumerate().fold(0u16, |sum, (i, &b)| {
        if i == 2 || i == 3 {
            sum
        } else {
            sum.rotate_right(1).wrapping_add(u16::from(b))
        }
    })
}

const STAMP: u32 = (DOS_DATE as u32) << 16 | DOS_TIME as u32;

fn exfat_set(name: &str, dir: bool, first: u32, size: u64, valid: u64, chain: bool) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let parts = units.len().div_ceil(15);
    let mut set = vec![0u8; 32 * (2 + parts)];
    set[0] = 0x85;
    set[1] = (1 + parts) as u8;
    set[4..6].copy_from_slice(&if dir { 0x10u16 } else { 0x20 }.to_le_bytes());
    for at in [8, 12, 16] {
        set[at..at + 4].copy_from_slice(&STAMP.to_le_bytes());
    }
    set[22] = 0x80;
    set[23] = 0x80 | 8;
    set[24] = 0x80;
    let s = &mut set[32..64];
    s[0] = 0xc0;
    s[1] = if chain { 1 } else { 3 };
    s[3] = units.len() as u8;
    s[8..16].copy_from_slice(&valid.to_le_bytes());
    s[20..24].copy_from_slice(&first.to_le_bytes());
    s[24..32].copy_from_slice(&size.to_le_bytes());
    for (p, chunk) in units.chunks(15).enumerate() {
        let e = &mut set[64 + 32 * p..96 + 32 * p];
        e[0] = 0xc1;
        for (k, u) in chunk.iter().enumerate() {
            e[2 + 2 * k..4 + 2 * k].copy_from_slice(&u.to_le_bytes());
        }
    }
    let sum = set_sum(&set);
    set[2..4].copy_from_slice(&sum.to_le_bytes());
    set
}

pub(crate) fn fragmented() -> Vec<u8> {
    (0..1500u32).map(|i| (i % 253) as u8).collect()
}

/// An exFAT volume with 512-byte clusters: the FAT at sector 24, the heap at
/// 32, a two-cluster root folder at cluster 4 chained through the FAT.
pub(crate) fn exfat() -> Vec<u8> {
    let clusters = 200u32;
    let mut img = vec![0u8; (32 + clusters as usize) * 512];
    let b = &mut img[..512];
    b[0..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    b[3..11].copy_from_slice(b"EXFAT   ");
    b[72..80].copy_from_slice(&(32 + u64::from(clusters)).to_le_bytes());
    b[80..84].copy_from_slice(&24u32.to_le_bytes());
    b[84..88].copy_from_slice(&8u32.to_le_bytes());
    b[88..92].copy_from_slice(&32u32.to_le_bytes());
    b[92..96].copy_from_slice(&clusters.to_le_bytes());
    b[96..100].copy_from_slice(&4u32.to_le_bytes());
    b[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
    b[108] = 9;
    b[109] = 0;
    b[110] = 1;
    b[510] = 0x55;
    b[511] = 0xaa;
    let cluster = |c: u32| (32 + c as usize - 2) * 512;
    let mut fat = vec![0u32; 1024];
    fat[0] = 0xffff_fff8;
    fat[1] = 0xffff_ffff;
    fat[2] = 0xffff_ffff;
    fat[3] = 0xffff_ffff;
    fat[4] = 5;
    fat[5] = 0xffff_ffff;
    let put = |img: &mut Vec<u8>, c: u32, data: &[u8]| {
        let at = cluster(c);
        img[at..at + data.len()].copy_from_slice(data);
    };
    put(&mut img, 10, b"hello");
    let frag = fragmented();
    for (i, piece) in frag.chunks(512).enumerate() {
        let c = 20 + 3 * i as u32;
        put(&mut img, c, piece);
        fat[c as usize] = if i == 2 { 0xffff_ffff } else { c + 3 };
    }
    put(&mut img, 40, &[0x77; 600]);
    put(&mut img, 50, b"tiny");
    let sub = exfat_set("\u{fc}n\u{ef}code.txt", false, 50, 4, 4, false);
    put(&mut img, 45, &sub);
    let mut root = Vec::new();
    let mut bitmap = [0u8; 32];
    bitmap[0] = 0x81;
    bitmap[20..24].copy_from_slice(&2u32.to_le_bytes());
    bitmap[24..32].copy_from_slice(&25u64.to_le_bytes());
    root.extend_from_slice(&bitmap);
    let mut label = [0u8; 32];
    label[0] = 0x83;
    label[1] = 4;
    for (k, u) in "DISK".encode_utf16().enumerate() {
        label[2 + 2 * k..4 + 2 * k].copy_from_slice(&u.to_le_bytes());
    }
    root.extend_from_slice(&label);
    root.extend(exfat_set("hello.txt", false, 10, 5, 5, false));
    root.extend(exfat_set(
        "A rather long fragmented file name.bin",
        false,
        20,
        1500,
        1500,
        true,
    ));
    root.extend(exfat_set("valid.dat", false, 40, 1000, 600, false));
    root.extend(exfat_set("Sub", true, 45, 512, 512, false));
    let mut gone = exfat_set("gone.txt", false, 10, 5, 5, false);
    gone[0] = 0x05;
    root.extend(gone);
    assert!(root.len() <= 1024);
    let (first, second) = root.split_at(root.len().min(512));
    put(&mut img, 4, first);
    put(&mut img, 5, second);
    for (n, v) in fat.iter().enumerate() {
        let at = 24 * 512 + 4 * n;
        img[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    img
}

fn check_exfat(a: &DiskArchive, prefix: &str) {
    let p = |name: &str| fs::join(prefix, name);
    assert_eq!(read(a, &p("hello.txt")), b"hello");
    assert_eq!(
        read(a, &p("A rather long fragmented file name.bin")),
        fragmented()
    );
    let valid = read(a, &p("valid.dat"));
    assert_eq!(&valid[..600], &[0x77; 600][..]);
    assert!(valid[600..].iter().all(|&b| b == 0));
    assert_eq!(valid.len(), 1000);
    assert_eq!(read(a, &p("Sub/\u{fc}n\u{ef}code.txt")), b"tiny");
    assert!(!names(a).iter().any(|n| n.contains("gone")));
    let hello = a
        .entries()
        .iter()
        .find(|e| e.name == p("hello.txt"))
        .unwrap();
    let created = hello.created.unwrap();
    assert_eq!(hello.mtime.unwrap(), created - 2 * 3600);
}

#[test]
fn an_exfat_volume_lists_chained_contiguous_and_short_files() {
    let image = exfat();
    let (_dir, a) = open(Format::Img, &image);
    check_exfat(&a.unwrap(), "");
    let (_dir, a) = open(Format::Img, &gpt(&[&image]));
    let a = a.unwrap();
    check_exfat(&a, "0.exfat");
    a.test(&[], &|_, _, _| true).unwrap();
}

#[test]
fn a_damaged_exfat_entry_set_is_left_out() {
    let mut image = exfat();
    let root = 32 * 512 + 2 * 512;
    let at = image[root..root + 1024]
        .windows(2)
        .position(|w| w == [b'h', 0])
        .unwrap();
    image[root + at] = b'j';
    let (_dir, a) = open(Format::Img, &image);
    let a = a.unwrap();
    assert!(!names(&a).contains(&"hello.txt"));
    assert!(names(&a).contains(&"valid.dat"));
    assert!(a.notices().iter().any(|n| n.contains("damaged")));
}

#[test]
fn damaged_exfat_volumes_never_panic() {
    let image = exfat();
    let spots: Vec<usize> = (0..image.len())
        .filter(|&at| {
            (at < 512 && at % 3 == 0)
                || (24 * 512..24 * 512 + 256).contains(&at)
                || ((34 * 512..36 * 512).contains(&at) && at % 3 == 0)
                || at % 211 == 0
        })
        .collect();
    survives_damage_at(Format::Img, &image, &spots);
}
