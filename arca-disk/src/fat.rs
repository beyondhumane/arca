use arca_core::{dos_to_unix, Error, Result};
use std::collections::{HashMap, HashSet};

use crate::device::{le16, le32, Volume};
use crate::fs::{self, Kind, Listing, Member, Run, Runs, MAX_DEPTH};

const MAX_DIRECTORY_BYTES: u64 = 4 * 1024 * 1024;
const PAGE: u64 = 64 * 1024;
const MAX_PAGES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Width {
    Fat12,
    Fat16,
    Fat32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Bpb {
    width: Width,
    cluster: u64,
    fat_at: u64,
    fat_len: u64,
    root_at: u64,
    root_len: u64,
    data_at: u64,
    clusters: u32,
    root_cluster: u32,
}

impl Bpb {
    /// The BIOS parameter block of a FAT boot sector, checked well enough to
    /// tell it from any other first sector.
    pub fn parse(b: &[u8]) -> Result<Bpb> {
        let bad = || Error::Format("not a FAT boot sector".into());
        if b.len() < 512 || !matches!(b[0], 0xeb | 0xe9) {
            return Err(bad());
        }
        let bps = u64::from(le16(b, 11));
        let spc = u64::from(b[13]);
        let reserved = u64::from(le16(b, 14));
        let fats = u64::from(b[16]);
        let root_entries = u64::from(le16(b, 17));
        let total16 = u64::from(le16(b, 19));
        let media = b[21];
        let fat16 = u64::from(le16(b, 22));
        let total32 = u64::from(le32(b, 32));
        let fat32 = u64::from(le32(b, 36));
        if !matches!(bps, 512 | 1024 | 2048 | 4096)
            || !spc.is_power_of_two()
            || spc > 128
            || reserved == 0
            || !(1..=4).contains(&fats)
            || !(media == 0xf0 || media >= 0xf8)
        {
            return Err(bad());
        }
        let total = if total16 != 0 { total16 } else { total32 };
        let fat_size = if fat16 != 0 { fat16 } else { fat32 };
        if total == 0 || fat_size == 0 {
            return Err(bad());
        }
        let root_sectors = (root_entries * 32).div_ceil(bps);
        let data_sector = reserved + fats * fat_size + root_sectors;
        if data_sector >= total {
            return Err(bad());
        }
        let clusters = (total - data_sector) / spc;
        let width = if clusters < 4085 {
            Width::Fat12
        } else if clusters < 65525 {
            Width::Fat16
        } else {
            Width::Fat32
        };
        if clusters == 0 || clusters > 0x0fff_fff5 {
            return Err(bad());
        }
        let root_cluster = le32(b, 44);
        if width == Width::Fat32 && (fat16 != 0 || root_entries != 0 || root_cluster < 2) {
            return Err(bad());
        }
        if width != Width::Fat32 && root_entries == 0 {
            return Err(bad());
        }
        let need = match width {
            Width::Fat12 => (clusters + 2) * 3 / 2,
            Width::Fat16 => (clusters + 2) * 2,
            Width::Fat32 => (clusters + 2) * 4,
        };
        if fat_size * bps < need {
            return Err(bad());
        }
        Ok(Bpb {
            width,
            cluster: bps * spc,
            fat_at: reserved * bps,
            fat_len: fat_size * bps,
            root_at: (reserved + fats * fat_size) * bps,
            root_len: root_sectors * bps,
            data_at: data_sector * bps,
            clusters: clusters as u32,
            root_cluster,
        })
    }

    fn offset(&self, cluster: u32) -> u64 {
        self.data_at + u64::from(cluster - 2) * self.cluster
    }

    fn valid(&self, cluster: u32) -> bool {
        cluster >= 2 && cluster - 2 < self.clusters
    }
}

/// The allocation table, read a page at a time as chains reach into it.
struct Table {
    bpb: Bpb,
    pages: HashMap<u64, Vec<u8>>,
}

enum Next {
    Cluster(u32),
    End,
    Broken,
}

impl Table {
    fn byte(&mut self, vol: &mut Volume<'_>, at: u64) -> Result<u8> {
        if at >= self.bpb.fat_len {
            return Err(Error::Format("a FAT chain points past the table".into()));
        }
        let page = at / PAGE;
        if !self.pages.contains_key(&page) {
            if self.pages.len() >= MAX_PAGES {
                self.pages.clear();
            }
            let start = page * PAGE;
            let len = PAGE.min(self.bpb.fat_len - start);
            let bytes = vol.bytes(self.bpb.fat_at + start, len)?;
            self.pages.insert(page, bytes);
        }
        let bytes = &self.pages[&page];
        Ok(bytes[(at - page * PAGE) as usize])
    }

    fn next(&mut self, vol: &mut Volume<'_>, cluster: u32) -> Result<Next> {
        let c = u64::from(cluster);
        let (value, end) = match self.bpb.width {
            Width::Fat12 => {
                let at = c + c / 2;
                let pair = u32::from(self.byte(vol, at)?) | u32::from(self.byte(vol, at + 1)?) << 8;
                let v = if cluster & 1 == 1 {
                    pair >> 4
                } else {
                    pair & 0xfff
                };
                (v, 0xff8)
            }
            Width::Fat16 => {
                let at = c * 2;
                let v = u32::from(self.byte(vol, at)?) | u32::from(self.byte(vol, at + 1)?) << 8;
                (v, 0xfff8)
            }
            Width::Fat32 => {
                let at = c * 4;
                let mut v = 0u32;
                for i in 0..4 {
                    v |= u32::from(self.byte(vol, at + i)?) << (8 * i);
                }
                (v & 0x0fff_ffff, 0x0fff_fff8)
            }
        };
        Ok(if value >= end {
            Next::End
        } else if self.bpb.valid(value) {
            Next::Cluster(value)
        } else {
            Next::Broken
        })
    }

    /// The clusters from `start` on, as runs of bytes on the device, for at
    /// most `limit` bytes. A chain that loops, or one that breaks off, ends
    /// where it goes wrong, so a file on it comes out short when it is read.
    fn chain(
        &mut self,
        vol: &mut Volume<'_>,
        start: u32,
        limit: u64,
        runs: &mut Runs,
        out: &mut Vec<Run>,
    ) -> Result<()> {
        if !self.bpb.valid(start) {
            return Ok(());
        }
        let mut cluster = start;
        let mut got = 0u64;
        let mut steps = 0u32;
        loop {
            let len = self.bpb.cluster.min(limit - got);
            runs.add(
                out,
                Run::Disk {
                    at: vol.base + self.bpb.offset(cluster),
                    len,
                },
            )?;
            got += len;
            steps += 1;
            if got >= limit || steps >= self.bpb.clusters {
                return Ok(());
            }
            match self.next(vol, cluster)? {
                Next::Cluster(next) => cluster = next,
                Next::End | Next::Broken => return Ok(()),
            }
        }
    }
}

struct Item {
    name: String,
    raw: Vec<u8>,
    is_dir: bool,
    start: u32,
    size: u64,
    attributes: u8,
    mtime: Option<i64>,
    created: Option<i64>,
    accessed: Option<i64>,
}

pub(crate) fn list(vol: &mut Volume<'_>, carry_on: &dyn Fn() -> bool) -> Result<Listing> {
    let mut first = [0u8; 512];
    vol.read(0, &mut first)?;
    let bpb = Bpb::parse(&first)?;
    if bpb.data_at > vol.len {
        return Err(Error::Format(
            "the FAT volume is larger than its partition".into(),
        ));
    }
    let mut table = Table {
        bpb,
        pages: HashMap::new(),
    };
    let mut runs = Runs::default();
    let mut listing = Listing::default();
    let mut seen = HashSet::new();
    let mut broken = 0usize;
    let root = if bpb.width == Width::Fat32 {
        seen.insert(bpb.root_cluster);
        directory(vol, &mut table, bpb.root_cluster)?
            .ok_or_else(|| Error::Format("the FAT root folder chain is broken".into()))?
    } else {
        vol.bytes(bpb.root_at, bpb.root_len)?
    };
    let mut pending = vec![(root, String::new(), 0usize)];
    while let Some((bytes, path, depth)) = pending.pop() {
        if !carry_on() {
            return Err(Error::Cancelled);
        }
        for item in items(&bytes) {
            let name = fs::join(&path, &item.name);
            let mut entry = fs::entry(name.clone(), item.size, item.is_dir, item.mtime);
            entry.raw_name = item.raw;
            entry.utf8 = false;
            entry.created = item.created;
            entry.accessed = item.accessed;
            entry.attributes = item.attributes;
            if item.is_dir {
                listing.push(Member {
                    entry,
                    kind: Kind::Dir,
                    runs: Vec::new(),
                })?;
                if depth + 1 >= MAX_DEPTH {
                    listing.notices.push(format!(
                        "'{name}' is nested too deep; its contents are not listed"
                    ));
                } else if !seen.insert(item.start) {
                    broken += 1;
                } else {
                    match directory(vol, &mut table, item.start)? {
                        Some(bytes) => pending.push((bytes, name, depth + 1)),
                        None => broken += 1,
                    }
                }
                continue;
            }
            let mut data = Vec::new();
            if item.size > 0 {
                table.chain(vol, item.start, item.size, &mut runs, &mut data)?;
            }
            listing.push(Member {
                entry,
                kind: Kind::File,
                runs: data,
            })?;
        }
    }
    if broken > 0 {
        listing.notices.push(format!(
            "{broken} folders have a damaged or shared cluster chain; their contents are not listed"
        ));
    }
    Ok(listing)
}

fn directory(vol: &mut Volume<'_>, table: &mut Table, start: u32) -> Result<Option<Vec<u8>>> {
    if !table.bpb.valid(start) {
        return Ok(None);
    }
    let mut seen = HashSet::new();
    let mut bytes = Vec::new();
    let mut cluster = start;
    while seen.insert(cluster) && (bytes.len() as u64) < MAX_DIRECTORY_BYTES {
        bytes.extend(vol.bytes(table.bpb.offset(cluster), table.bpb.cluster)?);
        match table.next(vol, cluster)? {
            Next::Cluster(next) => cluster = next,
            Next::End | Next::Broken => break,
        }
    }
    Ok(Some(bytes))
}

fn checksum(short: &[u8]) -> u8 {
    short
        .iter()
        .fold(0u8, |sum, &b| sum.rotate_right(1).wrapping_add(b))
}

fn short_name(e: &[u8]) -> String {
    let mut base: Vec<u8> = e[..8].to_vec();
    if base[0] == 0x05 {
        base[0] = 0xe5;
    }
    let trim = |part: &[u8]| -> String {
        let end = part.iter().rposition(|&b| b != b' ').map_or(0, |p| p + 1);
        part[..end].iter().map(|&b| char::from(b)).collect()
    };
    let mut name = trim(&base);
    let mut ext = trim(&e[8..11]);
    if e[12] & 0x08 != 0 {
        name = name.to_lowercase();
    }
    if e[12] & 0x10 != 0 {
        ext = ext.to_lowercase();
    }
    if ext.is_empty() {
        name
    } else {
        format!("{name}.{ext}")
    }
}

fn items(bytes: &[u8]) -> Vec<Item> {
    let mut out = Vec::new();
    let mut long: Vec<Option<[u16; 13]>> = Vec::new();
    let mut sum = 0u8;
    for e in bytes.as_chunks::<32>().0 {
        match e[0] {
            0x00 => break,
            0xe5 => {
                long.clear();
                continue;
            }
            _ => {}
        }
        let attr = e[11];
        if attr & 0x3f == 0x0f {
            let ord = usize::from(e[0] & 0x1f);
            if e[0] & 0x40 != 0 {
                long = vec![None; ord];
                sum = e[13];
            }
            if ord == 0 || ord > long.len() || e[13] != sum {
                long.clear();
                continue;
            }
            let mut units = [0u16; 13];
            for (i, at) in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
                .iter()
                .enumerate()
            {
                units[i] = le16(e, *at);
            }
            long[ord - 1] = Some(units);
            continue;
        }
        let parts = std::mem::take(&mut long);
        if attr & 0x08 != 0 {
            continue;
        }
        let short = short_name(e);
        let mut name = short.clone();
        if !parts.is_empty() && parts.iter().all(Option::is_some) && checksum(&e[..11]) == sum {
            let units: Vec<u16> = parts
                .iter()
                .flatten()
                .flat_map(|p| p.iter().copied())
                .take_while(|&u| u != 0)
                .collect();
            name = fs::utf16(&units);
        }
        if !fs::usable(&name) {
            continue;
        }
        let start = u32::from(le16(e, 20)) << 16 | u32::from(le16(e, 26));
        let is_dir = attr & 0x10 != 0;
        out.push(Item {
            raw: if name == short {
                e[..11].to_vec()
            } else {
                name.as_bytes().to_vec()
            },
            name,
            is_dir,
            start,
            size: if is_dir { 0 } else { u64::from(le32(e, 28)) },
            attributes: attr & 0x27,
            mtime: dos_to_unix(le16(e, 24), le16(e, 22)),
            created: dos_to_unix(le16(e, 16), le16(e, 14)),
            accessed: dos_to_unix(le16(e, 18), 0),
        });
    }
    out
}
