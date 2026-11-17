use arca_core::{Error, Result};
use std::path::Path;

use crate::device::{le16, le32, le64, past_end, Device, Raw};

const KIB64: u64 = 64 * 1024;
const MIB: u64 = 1024 * 1024;

pub(crate) const BAT: &str = "2DC27766-F623-4200-9D64-115E9BFD4A08";
pub(crate) const METADATA: &str = "8B7CA206-4790-4B9A-B8FE-575F050F886E";
pub(crate) const FILE_PARAMETERS: &str = "CAA16737-FA36-4D43-B3B6-33F0AA44E76B";
pub(crate) const DISK_SIZE: &str = "2FA54224-CD1B-4876-B211-5DBED83BF4B8";
pub(crate) const LOGICAL_SECTOR: &str = "8141BF1D-A96F-4709-BA47-F233A8FAAB5F";
const KNOWN_ITEMS: [&str; 6] = [
    FILE_PARAMETERS,
    DISK_SIZE,
    LOGICAL_SECTOR,
    "CDA348C7-445D-4471-9CC9-E9885251C556",
    "BECA12AB-B2E6-4523-93EF-C309E000C746",
    "A8D35F2D-B30B-454D-ABF7-D3D84834AB0C",
];

/// A GUID as it is stored: the first three fields little-endian.
pub(crate) fn guid(text: &str) -> [u8; 16] {
    let hex: Vec<u8> = text
        .bytes()
        .filter(u8::is_ascii_hexdigit)
        .map(|c| (c as char).to_digit(16).unwrap_or(0) as u8)
        .collect();
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte =
            hex.get(2 * i).copied().unwrap_or(0) << 4 | hex.get(2 * i + 1).copied().unwrap_or(0);
    }
    out[..4].reverse();
    out[4..6].reverse();
    out[6..8].reverse();
    out
}

const fn crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0x82f6_3b78
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static CRC32C: [u32; 256] = crc32c_table();

pub(crate) fn crc32c(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &b| {
        CRC32C[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
    })
}

/// Whether the CRC-32C stored at `at` matches `block` with that field zeroed.
pub(crate) fn sealed(block: &[u8], at: usize) -> bool {
    let mut copy = block.to_vec();
    copy[at..at + 4].fill(0);
    crc32c(&copy) == le32(block, at)
}

pub(crate) fn open(path: &Path) -> Result<Box<dyn Device>> {
    let mut file = Raw::open(path)?;
    if file.bytes(0, 8)? != b"vhdxfile" {
        return Err(Error::Format("not a VHDX file".into()));
    }
    let header = header(&mut file)?;
    if header[48..64].iter().any(|&b| b != 0) {
        return Err(Error::Unsupported(
            "this VHDX was not closed cleanly and its log has to be replayed first; \
             attach it once in Windows or Hyper-V and detach it, then open it again"
                .into(),
        ));
    }
    if le16(&header, 66) != 1 {
        return Err(Error::Unsupported(format!(
            "VHDX version {}",
            le16(&header, 66)
        )));
    }
    let regions = regions(&mut file)?;
    let find = |id: &str| {
        regions
            .iter()
            .find(|(g, _, _)| *g == guid(id))
            .map(|&(_, at, len)| (at, len))
    };
    let (bat_at, bat_len) =
        find(BAT).ok_or_else(|| Error::Format("the VHDX has no block table".into()))?;
    let (meta_at, meta_len) =
        find(METADATA).ok_or_else(|| Error::Format("the VHDX has no metadata".into()))?;
    let meta = Metadata::read(&mut file, meta_at, meta_len)?;
    if meta.has_parent {
        return Err(Error::Unsupported(
            "this is a differencing VHDX: its data is split with a parent disk Arca does not open"
                .into(),
        ));
    }
    let ratio = (1u64 << 23) * meta.sector / meta.block;
    let blocks = meta.size.div_ceil(meta.block);
    let entries = if blocks == 0 {
        0
    } else {
        blocks + (blocks - 1) / ratio
    };
    if entries.saturating_mul(8) > bat_len {
        return Err(Error::Format(
            "the VHDX block table is too short for the disk".into(),
        ));
    }
    let raw = file.bytes(bat_at, entries * 8)?;
    let bat = raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect();
    Ok(Box::new(Vhdx {
        file,
        size: meta.size,
        block: meta.block,
        ratio,
        sector: meta.sector,
        bat,
    }))
}

// Two copies, each sealed by its own checksum; the valid one written last wins.
fn header(file: &mut Raw) -> Result<Vec<u8>> {
    let mut best: Option<(u64, Vec<u8>)> = None;
    for at in [KIB64, 2 * KIB64] {
        let Ok(h) = file.bytes(at, 4096) else {
            continue;
        };
        if &h[..4] != b"head" || !sealed(&h, 4) {
            continue;
        }
        let sequence = le64(&h, 8);
        if best.as_ref().is_none_or(|(s, _)| sequence > *s) {
            best = Some((sequence, h));
        }
    }
    best.map(|(_, h)| h)
        .ok_or_else(|| Error::Format("both VHDX headers are damaged".into()))
}

type Region = ([u8; 16], u64, u64);

fn regions(file: &mut Raw) -> Result<Vec<Region>> {
    for at in [3 * KIB64, 4 * KIB64] {
        let Ok(t) = file.bytes(at, KIB64) else {
            continue;
        };
        if &t[..4] != b"regi" || !sealed(&t, 4) {
            continue;
        }
        let count = le32(&t, 8) as usize;
        if count > 2047 {
            return Err(Error::Format(format!("{count} VHDX regions")));
        }
        let mut out = Vec::new();
        for i in 0..count {
            let e = &t[16 + 32 * i..48 + 32 * i];
            let mut id = [0u8; 16];
            id.copy_from_slice(&e[..16]);
            let (start, len) = (le64(e, 16), u64::from(le32(e, 24)));
            let required = le32(e, 28) & 1 != 0;
            if id != guid(BAT) && id != guid(METADATA) {
                if required {
                    return Err(Error::Unsupported(
                        "the VHDX needs a region Arca does not know".into(),
                    ));
                }
                continue;
            }
            if start % MIB != 0
                || start < MIB
                || start.checked_add(len).is_none_or(|e| e > file.len())
            {
                return Err(Error::Format("a VHDX region lies outside the file".into()));
            }
            out.push((id, start, len));
        }
        return Ok(out);
    }
    Err(Error::Format("both VHDX region tables are damaged".into()))
}

struct Metadata {
    block: u64,
    has_parent: bool,
    size: u64,
    sector: u64,
}

impl Metadata {
    fn read(file: &mut Raw, at: u64, len: u64) -> Result<Self> {
        if len < KIB64 {
            return Err(Error::Format(
                "the VHDX metadata region is too small".into(),
            ));
        }
        let table = file.bytes(at, KIB64)?;
        if &table[..8] != b"metadata" {
            return Err(Error::Format("the VHDX metadata table is missing".into()));
        }
        let count = usize::from(le16(&table, 10));
        if count > 2047 {
            return Err(Error::Format(format!("{count} VHDX metadata items")));
        }
        for i in 0..count {
            let e = &table[32 + 32 * i..64 + 32 * i];
            let required = le32(e, 24) & 4 != 0;
            if required && !KNOWN_ITEMS.iter().any(|k| e[..16] == guid(k)) {
                return Err(Error::Unsupported(
                    "the VHDX needs metadata Arca does not know".into(),
                ));
            }
        }
        let mut item = |id: &str, want: u64| -> Result<Vec<u8>> {
            for i in 0..count {
                let e = &table[32 + 32 * i..64 + 32 * i];
                if e[..16] != guid(id) {
                    continue;
                }
                let (offset, size) = (u64::from(le32(e, 16)), u64::from(le32(e, 20)));
                if size < want || offset < KIB64 || offset + size > len {
                    return Err(Error::Format(
                        "a VHDX metadata item lies outside its region".into(),
                    ));
                }
                return file.bytes(at + offset, want);
            }
            Err(Error::Format("the VHDX metadata is incomplete".into()))
        };
        let params = item(FILE_PARAMETERS, 8)?;
        let size = le64(&item(DISK_SIZE, 8)?, 0);
        let sector = u64::from(le32(&item(LOGICAL_SECTOR, 4)?, 0));
        let block = u64::from(le32(&params, 0));
        if !block.is_power_of_two() || !(MIB..=256 * MIB).contains(&block) {
            return Err(Error::Format(format!("VHDX blocks of {block} bytes")));
        }
        if sector != 512 && sector != 4096 {
            return Err(Error::Format(format!("VHDX sectors of {sector} bytes")));
        }
        if !size.is_multiple_of(sector) || size > 64 * 1024 * 1024 * MIB {
            return Err(Error::Format(format!("a VHDX disk of {size} bytes")));
        }
        Ok(Metadata {
            block,
            has_parent: le32(&params, 4) & 2 != 0,
            size,
            sector,
        })
    }
}

struct Vhdx {
    file: Raw,
    size: u64,
    block: u64,
    ratio: u64,
    sector: u64,
    bat: Vec<u64>,
}

impl Device for Vhdx {
    fn len(&self) -> u64 {
        self.size
    }

    fn sector(&self) -> u64 {
        self.sector
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.size {
            return Err(past_end());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let block = at / self.block;
            let within = at % self.block;
            let n = ((self.block - within) as usize).min(buf.len() - done);
            let part = &mut buf[done..done + n];
            let entry = self
                .bat
                .get((block + block / self.ratio) as usize)
                .copied()
                .unwrap_or(0);
            match entry & 7 {
                0..=3 => part.fill(0),
                6 => {
                    let start = (entry >> 20)
                        .checked_mul(MIB)
                        .and_then(|s| s.checked_add(within))
                        .ok_or_else(past_end)?;
                    self.file.read_at(start, part)?;
                }
                7 => {
                    return Err(Error::Unsupported(
                        "a VHDX block that is only partly present".into(),
                    ))
                }
                state => return Err(Error::Format(format!("a VHDX block in state {state}"))),
            }
            done += n;
        }
        Ok(())
    }
}
