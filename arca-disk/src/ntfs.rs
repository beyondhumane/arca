use arca_core::{Error, Result};
use std::collections::HashMap;

use crate::device::{le16, le32, le64, Volume};
use crate::fs::{self, Kind, Listing, Member, Run, Runs};

const ROOT: u64 = 5;
const FIRST_USER_RECORD: u64 = 16;
const MAX_PARENTS: usize = 256;
const MAX_ATTRIBUTE_LIST: u64 = 1024 * 1024;
const BATCH: u64 = 1024 * 1024;
const REFERENCE: u64 = 0xffff_ffff_ffff;

#[derive(Clone, Copy, Debug)]
struct Extent {
    lcn: Option<u64>,
    clusters: u64,
}

#[derive(Default)]
struct Data {
    resident: Option<Vec<u8>>,
    pieces: Vec<(u64, Vec<Extent>)>,
    size: u64,
    initialized: u64,
    packed: bool,
    broken: bool,
}

type Times = (Option<i64>, Option<i64>, Option<i64>, u8);

#[derive(Default)]
struct Record {
    used: bool,
    sequence: u16,
    is_dir: bool,
    names: Vec<(u64, u8, String)>,
    times: Option<Times>,
    data: Data,
    reparse: bool,
}

struct Geometry {
    cluster: u64,
    clusters: u64,
    record: u64,
}

/// Applies the update sequence: the last two bytes of every 512-byte
/// stride were swapped for a check value when the record was written.
fn fixup(rec: &mut [u8]) -> bool {
    let off = usize::from(le16(rec, 4));
    let count = usize::from(le16(rec, 6));
    if count < 2 || off + 2 * count > rec.len() || (count - 1) * 512 > rec.len() {
        return false;
    }
    let check = [rec[off], rec[off + 1]];
    for i in 1..count {
        let end = i * 512;
        if rec[end - 2..end] != check {
            return false;
        }
        let (a, b) = (rec[off + 2 * i], rec[off + 2 * i + 1]);
        rec[end - 2] = a;
        rec[end - 1] = b;
    }
    true
}

fn attributes(rec: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let limit = (le32(rec, 24) as usize).min(rec.len());
    let mut at = usize::from(le16(rec, 20));
    while at + 16 <= limit {
        if le32(rec, at) == 0xffff_ffff {
            break;
        }
        let len = le32(rec, at + 4) as usize;
        if len < 16 || !len.is_multiple_of(8) || at + len > limit {
            break;
        }
        out.push(&rec[at..at + len]);
        at += len;
    }
    out
}

fn resident(a: &[u8]) -> Option<&[u8]> {
    if a[8] != 0 || a.len() < 24 {
        return None;
    }
    let len = le32(a, 16) as usize;
    let off = usize::from(le16(a, 20));
    a.get(off..off.checked_add(len)?)
}

fn decode_runs(b: &[u8], geo: &Geometry) -> Option<Vec<Extent>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while i < b.len() && b[i] != 0 {
        let (size_len, off_len) = (usize::from(b[i] & 0x0f), usize::from(b[i] >> 4));
        if size_len == 0 || size_len > 8 || off_len > 8 || i + 1 + size_len + off_len > b.len() {
            return None;
        }
        let field = &b[i + 1..i + 1 + size_len + off_len];
        let mut clusters = 0u64;
        for (k, &byte) in field[..size_len].iter().enumerate() {
            clusters |= u64::from(byte) << (8 * k);
        }
        let at = if off_len == 0 {
            None
        } else {
            let mut delta = 0i64;
            for (k, &byte) in field[size_len..].iter().enumerate() {
                delta |= i64::from(byte) << (8 * k);
            }
            if off_len < 8 && field[size_len + off_len - 1] & 0x80 != 0 {
                delta -= 1i64 << (8 * off_len);
            }
            lcn = lcn.checked_add(delta)?;
            let start = u64::try_from(lcn).ok()?;
            if start.checked_add(clusters)? > geo.clusters {
                return None;
            }
            Some(start)
        };
        out.push(Extent { lcn: at, clusters });
        i += 1 + size_len + off_len;
    }
    Some(out)
}

/// The unnamed data stream, or one piece of it, from one attribute.
fn data_piece(a: &[u8], geo: &Geometry, data: &mut Data) -> Option<()> {
    if a[9] != 0 {
        return Some(());
    }
    let flags = le16(a, 12);
    data.packed |= flags & 0x00ff != 0 || flags & 0x4000 != 0;
    if a[8] == 0 {
        let value = resident(a)?;
        data.size = value.len() as u64;
        data.initialized = data.size;
        data.resident = Some(value.to_vec());
        return Some(());
    }
    if a.len() < 64 {
        return None;
    }
    let start = le64(a, 16);
    if start == 0 {
        data.size = le64(a, 48);
        data.initialized = le64(a, 56).min(data.size);
    }
    let runs = a.get(usize::from(le16(a, 32))..)?;
    data.pieces.push((start, decode_runs(runs, geo)?));
    Some(())
}

/// Where each byte of the MFT lies on the volume.
struct Map {
    parts: Vec<(u64, Option<u64>, u64)>,
    len: u64,
}

impl Map {
    fn new(pieces: &mut [(u64, Vec<Extent>)], geo: &Geometry, len: u64) -> Map {
        pieces.sort_by_key(|(vcn, _)| *vcn);
        let mut parts = Vec::new();
        let mut at = 0u64;
        for (vcn, extents) in pieces.iter() {
            if vcn.saturating_mul(geo.cluster) != at {
                break;
            }
            for e in extents {
                let bytes = e.clusters.saturating_mul(geo.cluster);
                parts.push((at, e.lcn.map(|l| l * geo.cluster), bytes));
                at = at.saturating_add(bytes);
            }
        }
        Map {
            parts,
            len: len.min(at),
        }
    }

    fn read(&self, vol: &mut Volume<'_>, pos: u64, buf: &mut [u8]) -> Result<()> {
        if pos.saturating_add(buf.len() as u64) > self.len {
            return Err(Error::Format("a read past the end of the NTFS MFT".into()));
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let i = self.parts.partition_point(|(start, _, _)| *start <= at) - 1;
            let (start, place, len) = self.parts[i];
            let n = ((start + len - at) as usize).min(buf.len() - done);
            match place {
                Some(base) => vol.read(base + (at - start), &mut buf[done..done + n])?,
                None => buf[done..done + n].fill(0),
            }
            done += n;
        }
        Ok(())
    }
}

pub(crate) fn list(
    vol: &mut Volume<'_>,
    runs: &mut Runs,
    carry_on: &dyn Fn() -> bool,
) -> Result<Listing> {
    let boot = vol.bytes(0, 512)?;
    let bad = |what: &str| Error::Format(format!("NTFS boot sector: {what}"));
    let bps = u64::from(le16(&boot, 11));
    if !matches!(bps, 512 | 1024 | 2048 | 4096) {
        return Err(bad("sector size"));
    }
    let spc = match boot[13] {
        0 => return Err(bad("cluster size")),
        n @ 1..=0x80 => u64::from(n),
        n => 1u64 << (256 - u32::from(n)).min(31),
    };
    let cluster = bps * spc;
    if !spc.is_power_of_two() || cluster > 2 * 1024 * 1024 {
        return Err(bad("cluster size"));
    }
    let record = match boot[64] as i8 {
        n @ 1.. => u64::from(n as u8) * cluster,
        n => 1u64 << u32::from(n.unsigned_abs()).min(31),
    };
    if !record.is_power_of_two() || !(512..=64 * 1024).contains(&record) {
        return Err(bad("file record size"));
    }
    let clusters = (le64(&boot, 40) / spc).min(vol.len / cluster);
    let geo = Geometry {
        cluster,
        clusters,
        record,
    };
    let mft_at = le64(&boot, 48)
        .checked_mul(cluster)
        .filter(|at| at.saturating_add(record) <= vol.len)
        .ok_or_else(|| bad("MFT position"))?;
    let mut first = vol.bytes(mft_at, record)?;
    if &first[..4] != b"FILE" || !fixup(&mut first) {
        return Err(Error::Format(
            "the NTFS MFT's first record is damaged".into(),
        ));
    }
    let mut mft = Data::default();
    let mut list_bytes = None;
    for a in attributes(&first) {
        match le32(a, 0) {
            0x80 => data_piece(a, &geo, &mut mft).ok_or_else(|| bad("MFT data"))?,
            0x20 => list_bytes = Some(a.to_vec()),
            _ => {}
        }
    }
    let size = mft.size;
    let mut map = Map::new(&mut mft.pieces, &geo, size);
    if map.len < size {
        if let Some(list) = list_bytes {
            for record_number in mft_extensions(vol, &list, &geo)? {
                let mut rec = vec![0u8; record as usize];
                if map
                    .read(vol, record_number.saturating_mul(record), &mut rec)
                    .is_err()
                    || &rec[..4] != b"FILE"
                    || !fixup(&mut rec)
                {
                    continue;
                }
                for a in attributes(&rec) {
                    if le32(a, 0) == 0x80 {
                        let _ = data_piece(a, &geo, &mut mft);
                    }
                }
                map = Map::new(&mut mft.pieces, &geo, size);
            }
        }
    }
    let records = scan(vol, &map, &geo, carry_on)?;
    build(&records, &geo, vol.base, runs)
}

// The records an attribute list puts the rest of the MFT's own data in.
fn mft_extensions(vol: &mut Volume<'_>, a: &[u8], geo: &Geometry) -> Result<Vec<u64>> {
    let bytes = if a[8] == 0 {
        resident(a).map(<[u8]>::to_vec).unwrap_or_default()
    } else {
        let mut data = Data::default();
        if data_piece(a, geo, &mut data).is_none() || data.size > MAX_ATTRIBUTE_LIST {
            return Ok(Vec::new());
        }
        let map = Map::new(&mut data.pieces, geo, data.size);
        let mut out = vec![0u8; map.len as usize];
        map.read(vol, 0, &mut out)?;
        out
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at + 26 <= bytes.len() {
        let len = usize::from(le16(&bytes, at + 4));
        if len < 26 {
            break;
        }
        let number = le64(&bytes, at + 16) & REFERENCE;
        if le32(&bytes, at) == 0x80 && number != 0 && !out.contains(&number) {
            out.push(number);
        }
        at += len;
    }
    Ok(out)
}

fn scan(
    vol: &mut Volume<'_>,
    map: &Map,
    geo: &Geometry,
    carry_on: &dyn Fn() -> bool,
) -> Result<HashMap<u64, Record>> {
    let count = map.len / geo.record;
    if count >= arca_core::limits::MAX_ENTRIES {
        return Err(Error::Limit("an NTFS volume of too many records".into()));
    }
    let per_batch = (BATCH / geo.record).max(1);
    let mut records: HashMap<u64, Record> = HashMap::new();
    let mut batch = Vec::new();
    let mut n = 0u64;
    while n < count {
        if !carry_on() {
            return Err(Error::Cancelled);
        }
        let take = per_batch.min(count - n);
        batch.resize((take * geo.record) as usize, 0);
        map.read(vol, n * geo.record, &mut batch)?;
        for (k, rec) in batch.chunks_exact_mut(geo.record as usize).enumerate() {
            let number = n + k as u64;
            if &rec[..4] != b"FILE" || le16(rec, 22) & 1 == 0 || !fixup(rec) {
                continue;
            }
            let base = le64(rec, 32) & REFERENCE;
            let owner = if base == 0 { number } else { base };
            let entry = records.entry(owner).or_default();
            if base == 0 {
                entry.used = true;
                entry.sequence = le16(rec, 16);
                entry.is_dir = le16(rec, 22) & 2 != 0;
            }
            for a in attributes(rec) {
                match le32(a, 0) {
                    0x10 => {
                        if let Some(v) = resident(a).filter(|v| v.len() >= 36) {
                            entry.times = Some((
                                fs::filetime(le64(v, 0)),
                                fs::filetime(le64(v, 8)),
                                fs::filetime(le64(v, 24)),
                                (le32(v, 32) & 0x27) as u8,
                            ));
                        }
                    }
                    0x30 => {
                        if let Some(v) = resident(a).filter(|v| v.len() >= 66) {
                            let len = usize::from(v[64]) * 2;
                            if let Some(name) = v.get(66..66 + len) {
                                entry.names.push((le64(v, 0), v[65], fs::utf16_le(name)));
                            }
                        }
                    }
                    0x80 => {
                        if data_piece(a, geo, &mut entry.data).is_none() {
                            entry.data.broken = true;
                        }
                    }
                    0xc0 => entry.reparse = true,
                    _ => {}
                }
            }
        }
        n += take;
    }
    Ok(records)
}

fn name_of(rec: &Record) -> Option<&(u64, u8, String)> {
    rec.names
        .iter()
        .find(|(_, space, _)| *space == 1 || *space == 3)
        .or_else(|| rec.names.iter().find(|(_, space, _)| *space == 0))
        .or_else(|| rec.names.first())
}

enum Place {
    Path(String),
    System,
    Lost,
}

fn place(
    records: &HashMap<u64, Record>,
    number: u64,
    memo: &mut HashMap<u64, Option<String>>,
) -> Place {
    let mut chain = Vec::new();
    let mut at = number;
    let mut prefix = String::new();
    loop {
        if let Some(known) = memo.get(&at) {
            match known {
                Some(path) => {
                    prefix = path.clone();
                    break;
                }
                None => return Place::Lost,
            }
        }
        if chain.len() > MAX_PARENTS || chain.contains(&at) {
            return Place::Lost;
        }
        let Some(rec) = records.get(&at).filter(|r| r.used) else {
            return Place::Lost;
        };
        let Some((parent, _, name)) = name_of(rec) else {
            return Place::Lost;
        };
        chain.push(at);
        let (up, sequence) = (parent & REFERENCE, (parent >> 48) as u16);
        if up == ROOT {
            break;
        }
        if up < FIRST_USER_RECORD {
            return Place::System;
        }
        let Some(dir) = records.get(&up).filter(|r| r.used && r.is_dir) else {
            return Place::Lost;
        };
        if sequence != 0 && dir.sequence != sequence {
            return Place::Lost;
        }
        if !fs::usable(name) {
            return Place::Lost;
        }
        at = up;
    }
    let mut path = prefix;
    for &n in chain.iter().rev() {
        let name = name_of(&records[&n]).map_or("", |(_, _, name)| name.as_str());
        if !fs::usable(name) {
            return Place::Lost;
        }
        path = fs::join(&path, name);
        memo.insert(n, Some(path.clone()));
    }
    Place::Path(path)
}

fn build(
    records: &HashMap<u64, Record>,
    geo: &Geometry,
    base: u64,
    runs: &mut Runs,
) -> Result<Listing> {
    let mut numbers: Vec<u64> = records
        .iter()
        .filter(|(n, r)| **n >= FIRST_USER_RECORD && r.used && !r.names.is_empty())
        .map(|(n, _)| *n)
        .collect();
    numbers.sort_unstable();
    let mut memo = HashMap::new();
    let mut listing = Listing::default();
    let (mut lost, mut links, mut packed, mut broken) = (0usize, 0usize, 0usize, 0usize);
    for n in numbers {
        let rec = &records[&n];
        let path = match place(records, n, &mut memo) {
            Place::Path(path) => path,
            Place::System => continue,
            Place::Lost => {
                lost += 1;
                continue;
            }
        };
        if rec.reparse {
            links += 1;
            continue;
        }
        let (created, mtime, accessed, attributes) = rec.times.unwrap_or_default();
        if rec.is_dir {
            let mut entry = fs::entry(path, 0, true, mtime);
            entry.created = created;
            entry.accessed = accessed;
            entry.attributes = attributes;
            listing.push(Member {
                entry,
                kind: Kind::Dir,
                runs: Vec::new(),
            })?;
            continue;
        }
        if rec.data.packed {
            packed += 1;
            continue;
        }
        if rec.data.broken {
            broken += 1;
            continue;
        }
        let mut data = Vec::new();
        let size = rec.data.size;
        if let Some(bytes) = &rec.data.resident {
            runs.add(&mut data, Run::Inline(bytes.clone()))?;
        } else {
            let mut pieces: Vec<&(u64, Vec<Extent>)> = rec.data.pieces.iter().collect();
            pieces.sort_by_key(|(vcn, _)| *vcn);
            let mut at = 0u64;
            let want = rec.data.initialized;
            'pieces: for (vcn, extents) in pieces {
                if vcn.saturating_mul(geo.cluster) != at {
                    break;
                }
                for e in extents {
                    if at >= want {
                        break 'pieces;
                    }
                    let len = e.clusters.saturating_mul(geo.cluster).min(want - at);
                    let run = match e.lcn {
                        Some(lcn) => Run::Disk {
                            at: base + lcn * geo.cluster,
                            len,
                        },
                        None => Run::Zero(len),
                    };
                    runs.add(&mut data, run)?;
                    at += len;
                }
            }
            if at == want {
                runs.add(&mut data, Run::Zero(size - want))?;
            }
        }
        let mut entry = fs::entry(path, size, false, mtime);
        entry.created = created;
        entry.accessed = accessed;
        entry.attributes = attributes;
        listing.push(Member {
            entry,
            kind: Kind::File,
            runs: data,
        })?;
    }
    if lost > 0 {
        listing.notices.push(format!(
            "{lost} files or folders whose parent folder is missing or damaged are not listed"
        ));
    }
    if links > 0 {
        listing.notices.push(format!(
            "{links} symbolic links, junctions and other reparse points are not listed"
        ));
    }
    if packed > 0 {
        listing.notices.push(format!(
            "{packed} files compressed or encrypted by NTFS are not listed"
        ));
    }
    if broken > 0 {
        listing.notices.push(format!(
            "{broken} files whose data runs are damaged are not listed"
        ));
    }
    Ok(listing)
}
