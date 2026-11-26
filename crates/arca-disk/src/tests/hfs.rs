// A tiny HFS+ volume with 512-byte blocks and 4 KiB B-tree nodes, laid out
// as TN1150 describes: the volume header at byte 1024, an extents overflow
// file at block 8, the catalog at block 24 and file data from block 64.

use super::*;

const NODE: usize = 4096;
const BLOCK: usize = 512;
const HFS_EPOCH: u32 = 2_082_844_800;
pub(crate) const MTIME: u32 = 1_700_000_000;

fn key(parent: u32, name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut k = Vec::new();
    k.extend_from_slice(&((6 + 2 * units.len()) as u16).to_be_bytes());
    k.extend_from_slice(&parent.to_be_bytes());
    k.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for u in units {
        k.extend_from_slice(&u.to_be_bytes());
    }
    k
}

fn fork(size: u64, extents: &[(u32, u32)]) -> Vec<u8> {
    let mut f = vec![0u8; 80];
    f[..8].copy_from_slice(&size.to_be_bytes());
    let total: u32 = extents.iter().map(|(_, n)| n).sum();
    f[12..16].copy_from_slice(&total.to_be_bytes());
    for (i, (start, count)) in extents.iter().take(8).enumerate() {
        f[16 + 8 * i..20 + 8 * i].copy_from_slice(&start.to_be_bytes());
        f[20 + 8 * i..24 + 8 * i].copy_from_slice(&count.to_be_bytes());
    }
    f
}

fn folder(parent: u32, name: &str, id: u32) -> Vec<u8> {
    let mut r = key(parent, name);
    let mut d = vec![0u8; 88];
    d[..2].copy_from_slice(&1u16.to_be_bytes());
    d[8..12].copy_from_slice(&id.to_be_bytes());
    for at in [12, 16, 20, 24] {
        d[at..at + 4].copy_from_slice(&(MTIME + HFS_EPOCH).to_be_bytes());
    }
    d[42..44].copy_from_slice(&0o040755u16.to_be_bytes());
    r.extend(d);
    r
}

struct File<'a> {
    parent: u32,
    name: &'a str,
    id: u32,
    mode: u16,
    flags: u8,
    kind: &'a [u8; 8],
    special: u32,
    data: Vec<u8>,
}

fn file(f: &File<'_>) -> Vec<u8> {
    let mut r = key(f.parent, f.name);
    let mut d = vec![0u8; 248];
    d[..2].copy_from_slice(&2u16.to_be_bytes());
    d[8..12].copy_from_slice(&f.id.to_be_bytes());
    for at in [12, 16, 20, 24] {
        d[at..at + 4].copy_from_slice(&(MTIME + HFS_EPOCH).to_be_bytes());
    }
    d[41] = f.flags;
    d[42..44].copy_from_slice(&f.mode.to_be_bytes());
    d[44..48].copy_from_slice(&f.special.to_be_bytes());
    d[48..56].copy_from_slice(f.kind);
    d[88..168].copy_from_slice(&f.data);
    r.extend(d);
    r
}

fn node(kind: i8, next: u32, records: &[Vec<u8>]) -> Vec<u8> {
    let mut n = vec![0u8; NODE];
    n[..4].copy_from_slice(&next.to_be_bytes());
    n[8] = kind as u8;
    n[9] = 1;
    n[10..12].copy_from_slice(&(records.len() as u16).to_be_bytes());
    let mut at = 14;
    for (i, r) in records.iter().enumerate() {
        n[at..at + r.len()].copy_from_slice(r);
        n[NODE - 2 * (i + 1)..NODE - 2 * i].copy_from_slice(&(at as u16).to_be_bytes());
        at += r.len();
    }
    let free = records.len();
    n[NODE - 2 * (free + 1)..NODE - 2 * free].copy_from_slice(&(at as u16).to_be_bytes());
    n
}

fn tree(leaves: &[Vec<Vec<u8>>]) -> Vec<u8> {
    let mut header = vec![0u8; 106];
    header[..2].copy_from_slice(&1u16.to_be_bytes());
    header[2..6].copy_from_slice(&1u32.to_be_bytes());
    let total: usize = leaves.iter().map(Vec::len).sum();
    header[6..10].copy_from_slice(&(total as u32).to_be_bytes());
    header[10..14].copy_from_slice(&1u32.to_be_bytes());
    header[14..18].copy_from_slice(&(leaves.len() as u32).to_be_bytes());
    header[18..20].copy_from_slice(&(NODE as u16).to_be_bytes());
    header[20..22].copy_from_slice(&516u16.to_be_bytes());
    header[22..26].copy_from_slice(&(leaves.len() as u32 + 1).to_be_bytes());
    let mut out = node(1, 0, &[header]);
    for (i, records) in leaves.iter().enumerate() {
        let next = if i + 1 < leaves.len() {
            i as u32 + 2
        } else {
            0
        };
        out.extend(node(-1, next, records));
    }
    out
}

pub(crate) fn big() -> Vec<u8> {
    (0..9 * BLOCK as u32).map(|i| (i % 241) as u8).collect()
}

pub(crate) fn hfs() -> Vec<u8> {
    let mut img = vec![0u8; 256 * BLOCK];
    let mut put = |block: u32, data: &[u8]| {
        let at = block as usize * BLOCK;
        img[at..at + data.len()].copy_from_slice(data);
    };
    put(64, b"hello");
    let big = big();
    let mut big_extents = Vec::new();
    for (i, piece) in big.chunks(BLOCK).enumerate() {
        let block = 70 + 2 * i as u32;
        put(block, piece);
        big_extents.push((block, 1));
    }
    put(100, b"readme.txt");
    put(101, b"linked");
    let none = b"\0\0\0\0\0\0\0\0";
    let plain = |parent, name, id, data| File {
        parent,
        name,
        id,
        mode: 0o100644,
        flags: 0,
        kind: none,
        special: 0,
        data,
    };
    let first = vec![
        folder(1, "Untitled", 2),
        folder(2, "Docs", 16),
        file(&plain(2, "readme.txt", 17, fork(5, &[(64, 1)]))),
        file(&plain(
            16,
            "big.bin",
            18,
            fork(big.len() as u64, &big_extents[..8]),
        )),
        file(&File {
            mode: 0o120755,
            ..plain(2, "link", 19, fork(10, &[(100, 1)]))
        }),
    ];
    let second = vec![
        file(&File {
            flags: 0x20,
            ..plain(2, "packed", 22, fork(0, &[]))
        }),
        folder(2, "\0\0\0\0HFS+ Private Data", 20),
        file(&plain(20, "iNode77", 21, fork(6, &[(101, 1)]))),
        file(&File {
            kind: b"hlnkhfs+",
            special: 77,
            ..plain(2, "hard", 23, fork(0, &[]))
        }),
        file(&plain(2, "a/b.txt", 24, fork(5, &[(64, 1)]))),
    ];
    let catalog = tree(&[first, second]);
    let mut overflow = key(0, "");
    overflow.clear();
    overflow.extend_from_slice(&10u16.to_be_bytes());
    overflow.push(0);
    overflow.push(0);
    overflow.extend_from_slice(&18u32.to_be_bytes());
    overflow.extend_from_slice(&8u32.to_be_bytes());
    overflow.extend_from_slice(&fork(0, &big_extents[8..])[16..]);
    let extents = tree(&[vec![overflow]]);
    put(8, &extents);
    put(24, &catalog);
    let h = &mut img[1024..1536];
    h[..2].copy_from_slice(b"H+");
    h[2..4].copy_from_slice(&4u16.to_be_bytes());
    h[40..44].copy_from_slice(&(BLOCK as u32).to_be_bytes());
    h[44..48].copy_from_slice(&256u32.to_be_bytes());
    let ext_blocks = (extents.len() / BLOCK) as u32;
    let cat_blocks = (catalog.len() / BLOCK) as u32;
    h[192..272].copy_from_slice(&fork(extents.len() as u64, &[(8, ext_blocks)]));
    h[272..352].copy_from_slice(&fork(catalog.len() as u64, &[(24, cat_blocks)]));
    img
}

pub(crate) fn check_hfs(a: &DiskArchive, prefix: &str) {
    let p = |name: &str| fs::join(prefix, name);
    assert_eq!(read(a, &p("readme.txt")), b"hello");
    assert_eq!(read(a, &p("Docs/big.bin")), big());
    assert_eq!(read(a, &p("hard")), b"linked");
    assert_eq!(read(a, &p("a:b.txt")), b"hello");
    let link = a
        .entries()
        .iter()
        .position(|e| e.name == p("link"))
        .unwrap();
    assert_eq!(a.link(link), Some(Link::Symbolic("readme.txt")));
    let listed = names(a);
    assert!(listed.contains(&p("Docs").as_str()));
    assert!(!listed
        .iter()
        .any(|n| n.contains("Private") || n.contains("iNode")));
    assert!(!listed.contains(&p("packed").as_str()));
    let readme = a
        .entries()
        .iter()
        .find(|e| e.name == p("readme.txt"))
        .unwrap();
    assert_eq!(readme.mtime, Some(i64::from(MTIME)));
}

#[test]
fn an_hfs_plus_volume_is_walked_from_its_catalog() {
    let (_dir, a) = open(Format::Img, &hfs());
    let a = a.unwrap();
    check_hfs(&a, "");
    assert!(a.notices().iter().any(|n| n.contains("compressed by HFS+")));
    let dest = tempfile::tempdir().unwrap();
    let wanted: Vec<bool> = a.entries().iter().map(|e| !e.name.contains(':')).collect();
    let out = a
        .extract(dest.path(), &wanted, &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .unwrap();
    assert!(out.files >= 3);
    assert_eq!(
        std::fs::read(dest.path().join("Docs/big.bin")).unwrap(),
        big()
    );
}

#[test]
fn a_dmg_of_an_hfs_plus_partition_is_browsed() {
    let image = containers::dmg(&[
        ("Protective Master Boot Record (MBR : 0)", &[0u8; 512]),
        ("disk image (Apple_HFS : 1)", &hfs()),
    ]);
    let (_dir, a) = open(Format::Dmg, &image);
    let a = a.unwrap();
    check_hfs(&a, "1.hfs");
    a.test(&[], &|_, _, _| true).unwrap();
}

#[test]
fn damaged_hfs_plus_volumes_never_panic() {
    let image = hfs();
    let spots: Vec<usize> = (0..image.len())
        .filter(|&at| {
            (1024..1536).contains(&at) && at % 8 == 0
                || ((8 * BLOCK..8 * BLOCK + 2 * NODE).contains(&at) && at % 61 == 0)
                || ((24 * BLOCK..24 * BLOCK + 3 * NODE).contains(&at) && at % 47 == 0)
                || at % 1021 == 0
        })
        .collect();
    survives_damage_at(Format::Img, &image, &spots);
}
