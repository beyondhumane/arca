// VHDX, VMDK and DMG containers around a disk, built as the specifications
// lay them out.

use super::build::pad;
use std::io::Write;

const MIB: usize = 1024 * 1024;

fn seal(block: &mut [u8], at: usize) {
    block[at..at + 4].fill(0);
    let crc = crate::vhdx::crc32c(block);
    block[at..at + 4].copy_from_slice(&crc.to_le_bytes());
}

/// A VHDX with 1 MiB blocks: headers, region tables, metadata at 1 MiB,
/// the block table at 2 MiB and the blocks that hold any data after it.
pub(crate) fn vhdx(disk: &[u8], log: bool) -> Vec<u8> {
    use crate::vhdx::{guid, BAT, DISK_SIZE, FILE_PARAMETERS, LOGICAL_SECTOR, METADATA};
    let mut out = vec![0u8; 3 * MIB];
    out[..8].copy_from_slice(b"vhdxfile");
    for (n, at) in [(1u64, 64 * 1024), (2, 128 * 1024)] {
        let h = &mut out[at..at + 4096];
        h[..4].copy_from_slice(b"head");
        h[8..16].copy_from_slice(&n.to_le_bytes());
        h[16..32].copy_from_slice(&[0x11; 16]);
        if log {
            h[48..64].copy_from_slice(&[0x22; 16]);
        }
        h[66..68].copy_from_slice(&1u16.to_le_bytes());
        h[68..72].copy_from_slice(&(MIB as u32).to_le_bytes());
        seal(h, 4);
    }
    for at in [192 * 1024, 256 * 1024] {
        let t = &mut out[at..at + 64 * 1024];
        t[..4].copy_from_slice(b"regi");
        t[8..12].copy_from_slice(&2u32.to_le_bytes());
        for (i, (id, start)) in [(BAT, 2 * MIB), (METADATA, MIB)].iter().enumerate() {
            let e = &mut t[16 + 32 * i..48 + 32 * i];
            e[..16].copy_from_slice(&guid(id));
            e[16..24].copy_from_slice(&(*start as u64).to_le_bytes());
            e[24..28].copy_from_slice(&(MIB as u32).to_le_bytes());
            e[28..32].copy_from_slice(&1u32.to_le_bytes());
        }
        seal(t, 4);
    }
    let m = MIB;
    out[m..m + 8].copy_from_slice(b"metadata");
    out[m + 10..m + 12].copy_from_slice(&3u16.to_le_bytes());
    for (i, (id, offset, len, flags)) in [
        (FILE_PARAMETERS, 0usize, 8u32, 4u32),
        (DISK_SIZE, 8, 8, 6),
        (LOGICAL_SECTOR, 16, 4, 6),
    ]
    .iter()
    .enumerate()
    {
        let e = &mut out[m + 32 + 32 * i..m + 64 + 32 * i];
        e[..16].copy_from_slice(&guid(id));
        e[16..20].copy_from_slice(&((64 * 1024 + offset) as u32).to_le_bytes());
        e[20..24].copy_from_slice(&len.to_le_bytes());
        e[24..28].copy_from_slice(&flags.to_le_bytes());
    }
    let items = m + 64 * 1024;
    out[items..items + 4].copy_from_slice(&(MIB as u32).to_le_bytes());
    out[items + 8..items + 16].copy_from_slice(&(disk.len() as u64).to_le_bytes());
    out[items + 16..items + 20].copy_from_slice(&512u32.to_le_bytes());
    for (b, block) in disk.chunks(MIB).enumerate() {
        if block.iter().all(|&x| x == 0) {
            continue;
        }
        let at = out.len();
        out.extend_from_slice(block);
        out.resize(at + MIB, 0);
        let entry = 6u64 | ((at / MIB) as u64) << 20;
        let slot = 2 * MIB + 8 * (b + b / 4096);
        out[slot..slot + 8].copy_from_slice(&entry.to_le_bytes());
    }
    out
}

/// The places a VHDX keeps its structures, for damage tests.
pub(crate) fn vhdx_hot_spots() -> Vec<usize> {
    let mut at = Vec::new();
    for base in [
        0,
        64 * 1024,
        128 * 1024,
        192 * 1024,
        256 * 1024,
        MIB,
        MIB + 64 * 1024,
        2 * MIB,
    ] {
        at.extend(base..base + 128);
    }
    at
}

fn sparse_header(capacity: u64, grain: u64, gd: u64, flags: u32) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..4].copy_from_slice(b"KDMV");
    let version: u32 = if flags & (1 << 16) != 0 { 3 } else { 1 };
    h[4..8].copy_from_slice(&version.to_le_bytes());
    h[8..12].copy_from_slice(&(flags | 3).to_le_bytes());
    h[12..20].copy_from_slice(&capacity.to_le_bytes());
    h[20..28].copy_from_slice(&grain.to_le_bytes());
    h[28..36].copy_from_slice(&1u64.to_le_bytes());
    h[36..44].copy_from_slice(&20u64.to_le_bytes());
    h[44..48].copy_from_slice(&512u32.to_le_bytes());
    h[56..64].copy_from_slice(&gd.to_le_bytes());
    h[73] = b'\n';
    h[74] = b' ';
    h[75] = b'\r';
    h[76] = b'\n';
    if flags & (1 << 16) != 0 {
        h[77..79].copy_from_slice(&1u16.to_le_bytes());
    }
    h
}

/// A monolithic sparse VMDK with 64 KiB grains, or a stream-optimized one
/// whose grains are deflated and whose directory is found from the footer.
pub(crate) fn vmdk_sparse(disk: &[u8], compressed: bool, parent: &str) -> Vec<u8> {
    const GRAIN: usize = 64 * 1024;
    let capacity = disk.len().div_ceil(GRAIN) * GRAIN / 512;
    let grains = capacity * 512 / GRAIN;
    let tables = grains.div_ceil(512);
    let descriptor = format!(
        "# Disk DescriptorFile\nversion=1\nCID=12345678\nparentCID={parent}\ncreateType=\"monolithicSparse\"\nRW {capacity} SPARSE \"disk.vmdk\"\n"
    );
    let mut desc = descriptor.into_bytes();
    desc.resize(20 * 512, 0);
    let flags = if compressed { (1 << 16) | (1 << 17) } else { 0 };
    let gd_sector = 21usize;
    let gt_sector = gd_sector + 1;
    let mut out = vec![0u8; (gt_sector + tables * 4) * 512];
    out[512..512 + desc.len()].copy_from_slice(&desc);
    for t in 0..tables {
        let at = gd_sector * 512 + 4 * t;
        out[at..at + 4].copy_from_slice(&((gt_sector + 4 * t) as u32).to_le_bytes());
    }
    for (g, grain) in disk.chunks(GRAIN).enumerate() {
        if grain.iter().all(|&x| x == 0) {
            continue;
        }
        let sector = (out.len() / 512) as u32;
        if compressed {
            let mut full = grain.to_vec();
            full.resize(GRAIN, 0);
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(&full).unwrap();
            let packed = z.finish().unwrap();
            out.extend_from_slice(&((g * GRAIN / 512) as u64).to_le_bytes());
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&packed);
            out.resize(out.len().div_ceil(512) * 512, 0);
        } else {
            out.extend_from_slice(grain);
            out.resize(sector as usize * 512 + GRAIN, 0);
        }
        let at = gt_sector * 512 + 4 * g;
        out[at..at + 4].copy_from_slice(&sector.to_le_bytes());
    }
    let grain = (GRAIN / 512) as u64;
    let header = sparse_header(capacity as u64, grain, gd_sector as u64, flags);
    if compressed {
        out[..512].copy_from_slice(&sparse_header(capacity as u64, grain, u64::MAX, flags));
        let mut marker = vec![0u8; 512];
        marker[12..16].copy_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&marker);
        out.extend_from_slice(&header);
        out.extend_from_slice(&[0u8; 512]);
    } else {
        out[..512].copy_from_slice(&header);
    }
    out
}

type Chunk = (u32, u64, u64, u64, u64);

fn mish(start: u64, sectors: u64, chunks: &[Chunk], crc: u32) -> Vec<u8> {
    let mut m = vec![0u8; 204];
    m[..4].copy_from_slice(b"mish");
    m[4..8].copy_from_slice(&1u32.to_be_bytes());
    m[8..16].copy_from_slice(&start.to_be_bytes());
    m[16..24].copy_from_slice(&sectors.to_be_bytes());
    m[64..68].copy_from_slice(&2u32.to_be_bytes());
    m[68..72].copy_from_slice(&32u32.to_be_bytes());
    m[72..76].copy_from_slice(&crc.to_be_bytes());
    m[200..204].copy_from_slice(&((chunks.len() + 1) as u32).to_be_bytes());
    for &(kind, first, count, at, len) in chunks {
        m.extend_from_slice(&kind.to_be_bytes());
        m.extend_from_slice(&0u32.to_be_bytes());
        m.extend_from_slice(&first.to_be_bytes());
        m.extend_from_slice(&count.to_be_bytes());
        m.extend_from_slice(&at.to_be_bytes());
        m.extend_from_slice(&len.to_be_bytes());
    }
    m.extend_from_slice(&0xffff_ffffu32.to_be_bytes());
    m.extend_from_slice(&[0u8; 36]);
    m
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for (i, three) in bytes.chunks(3).enumerate() {
        if i > 0 && i % 18 == 0 {
            out.push_str("\n\t\t\t");
        }
        let n = u32::from(three[0]) << 16
            | u32::from(*three.get(1).unwrap_or(&0)) << 8
            | u32::from(*three.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= three.len() {
                out.push(ABC[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// ADC with literal runs only, which every decoder must take.
pub(crate) fn adc_literals(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for run in data.chunks(128) {
        out.push(0x80 | (run.len() - 1) as u8);
        out.extend_from_slice(run);
    }
    out
}

fn packed(turn: usize, piece: &[u8]) -> (u32, Vec<u8>) {
    match turn % 4 {
        0 => (1, piece.to_vec()),
        1 => {
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(piece).unwrap();
            (0x8000_0005, z.finish().unwrap())
        }
        2 => {
            let mut z = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            z.write_all(piece).unwrap();
            (0x8000_0006, z.finish().unwrap())
        }
        _ => (0x8000_0004, adc_literals(piece)),
    }
}

/// A UDIF image of the given partitions, each a block of 64-sector chunks
/// that take turns at being stored, deflated, bzip2-ed and ADC-coded, with
/// all-zero chunks left as zero runs.
pub(crate) fn dmg(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut fork = Vec::new();
    let mut dicts = String::new();
    let mut sector = 0u64;
    let mut turn = 0;
    for (id, (name, data)) in parts.iter().enumerate() {
        let data = pad(data);
        let sectors = (data.len() / 512) as u64;
        let mut chunks = Vec::new();
        for (c, piece) in data.chunks(64 * 512).enumerate() {
            let first = c as u64 * 64;
            let count = (piece.len() / 512) as u64;
            let at = fork.len() as u64;
            let (kind, bytes) = if piece.iter().all(|&b| b == 0) {
                (2u32, Vec::new())
            } else {
                turn += 1;
                packed(turn, piece)
            };
            fork.extend_from_slice(&bytes);
            chunks.push((kind, first, count, at, bytes.len() as u64));
        }
        let table = mish(sector, sectors, &chunks, crc32fast::hash(&data));
        dicts.push_str(&format!(
            "\t\t\t<dict>\n\t\t\t\t<key>Attributes</key>\n\t\t\t\t<string>0x0050</string>\n\t\t\t\t<key>CFName</key>\n\t\t\t\t<string>{name}</string>\n\t\t\t\t<key>Data</key>\n\t\t\t\t<data>\n\t\t\t{}\n\t\t\t\t</data>\n\t\t\t\t<key>ID</key>\n\t\t\t\t<string>{}</string>\n\t\t\t\t<key>Name</key>\n\t\t\t\t<string>{name}</string>\n\t\t\t</dict>\n",
            base64(&table),
            id as i64 - 1
        ));
        sector += sectors;
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n\t<key>resource-fork</key>\n\t<dict>\n\t\t<key>blkx</key>\n\t\t<array>\n{dicts}\t\t</array>\n\t</dict>\n</dict>\n</plist>\n"
    );
    let mut out = fork.clone();
    let xml_at = out.len() as u64;
    out.extend_from_slice(xml.as_bytes());
    let mut koly = vec![0u8; 512];
    koly[..4].copy_from_slice(b"koly");
    koly[4..8].copy_from_slice(&4u32.to_be_bytes());
    koly[8..12].copy_from_slice(&512u32.to_be_bytes());
    koly[12..16].copy_from_slice(&1u32.to_be_bytes());
    koly[32..40].copy_from_slice(&(fork.len() as u64).to_be_bytes());
    koly[56..60].copy_from_slice(&1u32.to_be_bytes());
    koly[60..64].copy_from_slice(&1u32.to_be_bytes());
    koly[216..224].copy_from_slice(&xml_at.to_be_bytes());
    koly[224..232].copy_from_slice(&(xml.len() as u64).to_be_bytes());
    koly[488..492].copy_from_slice(&1u32.to_be_bytes());
    koly[492..500].copy_from_slice(&sector.to_be_bytes());
    out.extend_from_slice(&koly);
    out
}
