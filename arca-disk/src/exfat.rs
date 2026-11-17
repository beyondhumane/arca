use arca_core::{dos_to_unix, Error, Result};
use std::collections::{HashMap, HashSet};

use crate::device::{le16, le32, le64, Volume};
use crate::fs::{self, Kind, Listing, Member, Run, Runs, MAX_DEPTH};

const MAX_DIRECTORY_BYTES: u64 = 256 * 1024 * 1024;
const PAGE: u64 = 64 * 1024;
const MAX_PAGES: usize = 256;

struct Geometry {
    fat_at: u64,
    fat_len: u64,
    heap_at: u64,
    cluster: u64,
    clusters: u32,
}

impl Geometry {
    fn valid(&self, c: u32) -> bool {
        c >= 2 && c - 2 < self.clusters
    }

    fn offset(&self, c: u32) -> u64 {
        self.heap_at + u64::from(c - 2) * self.cluster
    }
}

struct Table {
    geo: Geometry,
    pages: HashMap<u64, Vec<u8>>,
}

impl Table {
    fn next(&mut self, vol: &mut Volume<'_>, c: u32) -> Result<Option<u32>> {
        let at = u64::from(c) * 4;
        if at + 4 > self.geo.fat_len {
            return Ok(None);
        }
        let page = at / PAGE;
        if !self.pages.contains_key(&page) {
            if self.pages.len() >= MAX_PAGES {
                self.pages.clear();
            }
            let start = page * PAGE;
            let len = PAGE.min(self.geo.fat_len - start);
            self.pages
                .insert(page, vol.bytes(self.geo.fat_at + start, len)?);
        }
        let bytes = &self.pages[&page];
        let within = (at - page * PAGE) as usize;
        let value = le32(bytes, within);
        Ok(Some(value).filter(|&v| self.geo.valid(v)))
    }

    /// The runs holding `len` bytes from cluster `first`, which lie one after
    /// another when the file needs no chain. A broken chain ends the runs
    /// early, and the file reads short.
    fn runs(
        &mut self,
        vol: &mut Volume<'_>,
        first: u32,
        len: u64,
        contiguous: bool,
        runs: &mut Runs,
        out: &mut Vec<Run>,
    ) -> Result<()> {
        if len == 0 || !self.geo.valid(first) {
            return Ok(());
        }
        if contiguous {
            let clusters = len.div_ceil(self.geo.cluster);
            if u64::from(first - 2) + clusters > u64::from(self.geo.clusters) {
                return Ok(());
            }
            return runs.add(
                out,
                Run::Disk {
                    at: vol.base + self.geo.offset(first),
                    len,
                },
            );
        }
        let mut c = first;
        let mut got = 0;
        let mut steps = 0u32;
        while got < len && steps < self.geo.clusters {
            let n = self.geo.cluster.min(len - got);
            runs.add(
                out,
                Run::Disk {
                    at: vol.base + self.geo.offset(c),
                    len: n,
                },
            )?;
            got += n;
            steps += 1;
            if got < len {
                match self.next(vol, c)? {
                    Some(next) => c = next,
                    None => break,
                }
            }
        }
        Ok(())
    }

    fn read(
        &mut self,
        vol: &mut Volume<'_>,
        first: u32,
        len: u64,
        contiguous: bool,
    ) -> Result<Vec<u8>> {
        if contiguous {
            let mut pieces = Vec::new();
            self.runs(vol, first, len, true, &mut Runs::default(), &mut pieces)?;
            let mut out = Vec::new();
            for piece in pieces {
                if let Run::Disk { at, len } = piece {
                    out.extend(vol.bytes(at - vol.base, len)?);
                }
            }
            return Ok(out);
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut c = first;
        while self.geo.valid(c) && seen.insert(c) && (out.len() as u64) < len {
            out.extend(vol.bytes(self.geo.offset(c), self.geo.cluster)?);
            match self.next(vol, c)? {
                Some(next) => c = next,
                None => break,
            }
        }
        Ok(out)
    }
}

fn stamp(time: u32, centis: u8, zone: u8) -> Option<i64> {
    let (date, clock) = ((time >> 16) as u16, time as u16);
    let extra = i64::from(centis / 100);
    if zone & 0x80 == 0 {
        return dos_to_unix(date, clock).map(|t| t + extra);
    }
    let quarter = i64::from(((zone << 1) as i8) >> 1);
    dos_naive(date, clock).map(|t| t + extra - quarter * 15 * 60)
}

fn dos_naive(date: u16, time: u16) -> Option<i64> {
    let year = 1980i64 + i64::from((date >> 9) & 0x7f);
    let month = i64::from((date >> 5) & 0x0f);
    let day = i64::from(date & 0x1f);
    let (h, m, s) = (
        i64::from(time >> 11),
        i64::from((time >> 5) & 0x3f),
        i64::from((time & 0x1f) * 2),
    );
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 23 || m > 59 || s > 59 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + m * 60 + s)
}

struct Item {
    name: String,
    is_dir: bool,
    first: u32,
    contiguous: bool,
    size: u64,
    valid: u64,
    attributes: u8,
    mtime: Option<i64>,
    created: Option<i64>,
    accessed: Option<i64>,
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

fn items(bytes: &[u8], broken: &mut usize) -> Vec<Item> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 32 <= bytes.len() {
        let e = &bytes[i..i + 32];
        if e[0] == 0 {
            break;
        }
        if e[0] != 0x85 {
            i += 32;
            continue;
        }
        let secondary = usize::from(e[1]);
        let end = i + 32 * (secondary + 1);
        if secondary < 2 || end > bytes.len() {
            *broken += 1;
            i += 32;
            continue;
        }
        let set = &bytes[i..end];
        let stream = &set[32..64];
        if stream[0] != 0xc0 || set_sum(set) != le16(e, 2) {
            *broken += 1;
            i += 32;
            continue;
        }
        let length = usize::from(stream[3]);
        let mut units = Vec::new();
        for part in set[64..].as_chunks::<32>().0 {
            if part[0] != 0xc1 {
                break;
            }
            units.extend((0..15).map(|k| le16(part, 2 + 2 * k)));
        }
        i = end;
        if units.len() < length {
            *broken += 1;
            continue;
        }
        units.truncate(length);
        let name = fs::utf16(&units);
        if !fs::usable(&name) {
            continue;
        }
        let attributes = le16(e, 4);
        out.push(Item {
            name,
            is_dir: attributes & 0x10 != 0,
            first: le32(stream, 20),
            contiguous: stream[1] & 2 != 0,
            size: le64(stream, 24),
            valid: le64(stream, 8),
            attributes: (attributes & 0x27) as u8,
            created: stamp(le32(e, 8), e[20], e[22]),
            mtime: stamp(le32(e, 12), e[21], e[23]),
            accessed: stamp(le32(e, 16), 0, e[24]),
        });
    }
    out
}

pub(crate) fn list(vol: &mut Volume<'_>, carry_on: &dyn Fn() -> bool) -> Result<Listing> {
    let boot = vol.bytes(0, 512)?;
    let bad = |what: &str| Error::Format(format!("exFAT boot sector: {what}"));
    if &boot[3..11] != b"EXFAT   " {
        return Err(bad("signature"));
    }
    let sector_shift = u32::from(boot[108]);
    let cluster_shift = u32::from(boot[109]);
    if !(9..=12).contains(&sector_shift) || sector_shift + cluster_shift > 25 {
        return Err(bad("sector or cluster size"));
    }
    let sector = 1u64 << sector_shift;
    let geo = Geometry {
        fat_at: u64::from(le32(&boot, 80)) * sector,
        fat_len: u64::from(le32(&boot, 84)) * sector,
        heap_at: u64::from(le32(&boot, 88)) * sector,
        cluster: sector << cluster_shift,
        clusters: le32(&boot, 92),
    };
    if geo.clusters == 0 || geo.clusters > 0xffff_fff5 || u64::from(geo.clusters) * 4 > geo.fat_len
    {
        return Err(bad("cluster count"));
    }
    if geo.fat_at.saturating_add(geo.fat_len) > vol.len || geo.heap_at > vol.len {
        return Err(bad("layout past the end of the partition"));
    }
    let root = le32(&boot, 96);
    let mut table = Table {
        geo,
        pages: HashMap::new(),
    };
    let mut listing = Listing::default();
    let mut runs = Runs::default();
    let mut seen = HashSet::from([root]);
    let mut broken = 0usize;
    let mut skipped = 0usize;
    let root_bytes = table.read(vol, root, MAX_DIRECTORY_BYTES, false)?;
    if root_bytes.is_empty() {
        return Err(bad("root folder"));
    }
    let mut pending = vec![(root_bytes, String::new(), 0usize)];
    while let Some((bytes, path, depth)) = pending.pop() {
        if !carry_on() {
            return Err(Error::Cancelled);
        }
        for item in items(&bytes, &mut broken) {
            let name = fs::join(&path, &item.name);
            let mut entry = fs::entry(name.clone(), item.size, item.is_dir, item.mtime);
            entry.created = item.created;
            entry.accessed = item.accessed;
            entry.attributes = item.attributes;
            if item.is_dir {
                listing.push(Member {
                    entry,
                    kind: Kind::Dir,
                    runs: Vec::new(),
                })?;
                if depth + 1 >= MAX_DEPTH || !seen.insert(item.first) {
                    skipped += 1;
                    continue;
                }
                let len = item.size.min(MAX_DIRECTORY_BYTES);
                let bytes = table.read(vol, item.first, len, item.contiguous)?;
                pending.push((bytes, name, depth + 1));
                continue;
            }
            let mut data = Vec::new();
            let stored = item.valid.min(item.size);
            table.runs(
                vol,
                item.first,
                stored,
                item.contiguous,
                &mut runs,
                &mut data,
            )?;
            if data.iter().map(Run::len).sum::<u64>() == stored {
                runs.add(&mut data, Run::Zero(item.size - stored))?;
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
            "{broken} directory entries are damaged and were left out"
        ));
    }
    if skipped > 0 {
        listing.notices.push(format!(
            "{skipped} folders are nested too deep or share their clusters; their contents are not listed"
        ));
    }
    Ok(listing)
}
