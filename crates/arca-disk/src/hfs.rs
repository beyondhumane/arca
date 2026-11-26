use arca_core::{Error, Result};
use std::collections::{HashMap, HashSet};

use crate::device::{be16, be32, be64, Volume};
use crate::fs::{self, Kind, Listing, Member, Run, Runs};

const ROOT: u32 = 2;
const CATALOG_FILE: u32 = 4;
const MAX_PARENTS: usize = 256;
const MAX_LINK: u64 = 4096;
const MAX_TREE: u64 = 1024 * 1024 * 1024;
const HFS_EPOCH: i64 = 2_082_844_800;
const PRIVATE_FILES: &str = "\0\0\0\0HFS+ Private Data";
const PRIVATE_FOLDERS: &str = ".HFS+ Private Directory Data\r";

struct Geometry {
    block: u64,
    blocks: u64,
}

/// A fork: its size and the extents that hold it, as blocks.
#[derive(Clone, Default)]
struct Fork {
    size: u64,
    extents: Vec<(u64, u64)>,
}

impl Fork {
    fn parse(b: &[u8]) -> Fork {
        let mut fork = Fork {
            size: be64(b, 0),
            extents: Vec::new(),
        };
        for i in 0..8 {
            let (start, count) = (be32(b, 16 + 8 * i), be32(b, 20 + 8 * i));
            if count == 0 {
                break;
            }
            fork.extents.push((u64::from(start), u64::from(count)));
        }
        fork
    }

    fn blocks(&self) -> u64 {
        self.extents.iter().map(|(_, n)| n).sum()
    }

    /// The fork's bytes as runs on the device, or `None` when an extent
    /// lies outside the volume or the extents hold less than the size.
    fn runs(&self, geo: &Geometry, base: u64, runs: &mut Runs, out: &mut Vec<Run>) -> Result<bool> {
        let mut left = self.size;
        for &(start, count) in &self.extents {
            if left == 0 {
                break;
            }
            if start.saturating_add(count) > geo.blocks {
                return Ok(false);
            }
            let len = (count * geo.block).min(left);
            runs.add(
                out,
                Run::Disk {
                    at: base + start * geo.block,
                    len,
                },
            )?;
            left -= len;
        }
        Ok(left == 0)
    }
}

fn read_fork(vol: &mut Volume<'_>, geo: &Geometry, fork: &Fork, max: u64) -> Result<Vec<u8>> {
    if fork.size > max {
        return Err(Error::Limit(format!(
            "an HFS+ structure of {} bytes",
            fork.size
        )));
    }
    let mut pieces = Vec::new();
    if !fork.runs(geo, 0, &mut Runs::default(), &mut pieces)? {
        return Err(Error::Format("an HFS+ fork lies outside the volume".into()));
    }
    let mut out = Vec::new();
    for piece in pieces {
        if let Run::Disk { at, len } = piece {
            out.extend(vol.bytes(at, len)?);
        }
    }
    Ok(out)
}

/// Reads `buf` from `pos` in a fork laid out as `pieces` on the volume.
fn read_pieces(vol: &mut Volume<'_>, pieces: &[Run], pos: u64, buf: &mut [u8]) -> Result<()> {
    let mut start = 0u64;
    let mut done = 0usize;
    for piece in pieces {
        let &Run::Disk { at, len } = piece else {
            continue;
        };
        let here = pos + done as u64;
        if done < buf.len() && here >= start && here - start < len {
            let within = here - start;
            let n = ((len - within) as usize).min(buf.len() - done);
            vol.read(at + within, &mut buf[done..done + n])?;
            done += n;
        }
        start += len;
    }
    if done < buf.len() {
        return Err(Error::Format(
            "an HFS+ B-tree node lies past its fork".into(),
        ));
    }
    Ok(())
}

/// Hands `visit` the records of every leaf node of the B-tree in `fork`,
/// reading one node at a time: the header node first, then the leaf chain
/// from the first leaf, which stops at a node seen before.
fn leaves(
    vol: &mut Volume<'_>,
    geo: &Geometry,
    fork: &Fork,
    carry_on: &dyn Fn() -> bool,
    visit: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let bad = |what: &str| Error::Format(format!("HFS+ B-tree: {what}"));
    if fork.size > MAX_TREE {
        return Err(Error::Limit(format!(
            "an HFS+ structure of {} bytes",
            fork.size
        )));
    }
    let mut pieces = Vec::new();
    if !fork.runs(geo, 0, &mut Runs::default(), &mut pieces)? {
        return Err(Error::Format("an HFS+ fork lies outside the volume".into()));
    }
    if fork.size < 512 {
        return Err(bad("header node"));
    }
    let mut header = [0u8; 512];
    read_pieces(vol, &pieces, 0, &mut header)?;
    if header[8] != 1 {
        return Err(bad("header node"));
    }
    let node = usize::from(be16(&header, 14 + 18));
    if !node.is_power_of_two() || !(512..=32768).contains(&node) || node as u64 > fork.size {
        return Err(bad("node size"));
    }
    let mut next = be32(&header, 14 + 10);
    let mut seen = HashSet::new();
    let mut n = vec![0u8; node];
    while next != 0 {
        if !carry_on() {
            return Err(Error::Cancelled);
        }
        let at = u64::from(next) * node as u64;
        if !seen.insert(next) || at + node as u64 > fork.size {
            break;
        }
        read_pieces(vol, &pieces, at, &mut n)?;
        if n[8] as i8 != -1 {
            return Err(bad("a leaf chain reaches a node that is not a leaf"));
        }
        let count = usize::from(be16(&n, 10));
        if 14 + 2 * (count + 1) > node {
            return Err(bad("too many records in a node"));
        }
        for r in 0..count {
            let start = usize::from(be16(&n, node - 2 * (r + 1)));
            let end = usize::from(be16(&n, node - 2 * (r + 2)));
            if start < 14 || end <= start || end > node - 2 * (count + 1) {
                return Err(bad("a record outside its node"));
            }
            visit(&n[start..end])?;
        }
        next = be32(&n, 0);
    }
    Ok(())
}

type Overflow = HashMap<u32, Vec<(u32, Vec<(u64, u64)>)>>;

struct Item {
    parent: u32,
    name: String,
    created: Option<i64>,
    mtime: Option<i64>,
    accessed: Option<i64>,
    mode: u16,
    compressed: bool,
    hard: Option<u32>,
    folder_link: bool,
    data: Fork,
}

fn date(seconds: u32) -> Option<i64> {
    (seconds != 0).then(|| i64::from(seconds) - HFS_EPOCH)
}

fn name(key: &[u8]) -> Option<String> {
    let len = usize::from(be16(key, 6));
    let raw = key.get(8..8 + 2 * len)?;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|u| u16::from_be_bytes(*u))
        .collect();
    Some(fs::utf16(&units).replace('/', ":"))
}

pub(crate) fn list(
    vol: &mut Volume<'_>,
    runs: &mut Runs,
    carry_on: &dyn Fn() -> bool,
) -> Result<Listing> {
    let header = vol.bytes(1024, 512)?;
    let block = u64::from(be32(&header, 40));
    if !block.is_power_of_two() || !(512..=1 << 20).contains(&block) {
        return Err(Error::Format(format!("HFS+ blocks of {block} bytes")));
    }
    let geo = Geometry {
        block,
        blocks: u64::from(be32(&header, 44)).min(vol.len / block),
    };
    let extents_fork = Fork::parse(&header[192..272]);
    let mut catalog_fork = Fork::parse(&header[272..352]);
    let mut overflow: Overflow = HashMap::new();
    if extents_fork.size > 0 {
        leaves(vol, &geo, &extents_fork, carry_on, &mut |r| {
            if r.len() < 12 + 64 || be16(r, 0) != 10 || r[2] != 0 {
                return Ok(());
            }
            let fork = Fork::parse(&[&[0u8; 16][..], &r[12..76]].concat());
            overflow
                .entry(be32(r, 4))
                .or_default()
                .push((be32(r, 8), fork.extents));
            Ok(())
        })?;
    }
    let extend = |fork: &mut Fork, id: u32| {
        if let Some(more) = overflow.get(&id) {
            let mut more = more.clone();
            more.sort_by_key(|(start, _)| *start);
            for (start, extents) in more {
                if u64::from(start) == fork.blocks() {
                    fork.extents.extend(extents);
                }
            }
        }
    };
    extend(&mut catalog_fork, CATALOG_FILE);
    let mut folders: HashMap<u32, Item> = HashMap::new();
    let mut files: Vec<Item> = Vec::new();
    leaves(vol, &geo, &catalog_fork, carry_on, &mut |r| {
        let key_len = usize::from(be16(r, 0));
        if r.len() < 8 || key_len < 6 {
            return Ok(());
        }
        let Some(name) = name(r) else {
            return Ok(());
        };
        let data_at = (2 + key_len + 1) & !1;
        let Some(d) = r.get(data_at..) else {
            return Ok(());
        };
        if d.len() < 2 {
            return Ok(());
        }
        let parent = be32(r, 2);
        match be16(d, 0) {
            1 if d.len() >= 88 => {
                folders.insert(
                    be32(d, 8),
                    Item {
                        parent,
                        name,
                        created: date(be32(d, 12)),
                        mtime: date(be32(d, 16)),
                        accessed: date(be32(d, 24)),
                        mode: be16(d, 42),
                        compressed: false,
                        hard: None,
                        folder_link: false,
                        data: Fork::default(),
                    },
                );
            }
            2 if d.len() >= 248 => {
                let mut data = Fork::parse(&d[88..168]);
                extend(&mut data, be32(d, 8));
                let (kind, creator) = (&d[48..52], &d[52..56]);
                files.push(Item {
                    parent,
                    name,
                    created: date(be32(d, 12)),
                    mtime: date(be32(d, 16)),
                    accessed: date(be32(d, 24)),
                    mode: be16(d, 42),
                    compressed: d[41] & 0x20 != 0,
                    hard: (kind == b"hlnk" && creator == b"hfs+").then(|| be32(d, 44)),
                    folder_link: kind == b"fdrp" && creator == b"MACS",
                    data,
                });
            }
            _ => {}
        }
        if folders.len() + files.len() >= arca_core::limits::MAX_ENTRIES as usize {
            return Err(Error::Limit("too many entries".into()));
        }
        Ok(())
    })?;
    build(vol, &geo, folders, files, runs)
}

fn path_of(
    folders: &HashMap<u32, Item>,
    id: u32,
    memo: &mut HashMap<u32, Option<String>>,
) -> Option<String> {
    let mut chain = Vec::new();
    let mut at = id;
    let mut prefix = String::new();
    while at != ROOT {
        if let Some(known) = memo.get(&at) {
            prefix = known.clone()?;
            break;
        }
        if chain.len() > MAX_PARENTS || chain.contains(&at) {
            return None;
        }
        let folder = folders.get(&at)?;
        if !fs::usable(&folder.name) {
            return None;
        }
        chain.push(at);
        at = folder.parent;
    }
    let mut path = prefix;
    for &n in chain.iter().rev() {
        path = fs::join(&path, &folders[&n].name);
        memo.insert(n, Some(path.clone()));
    }
    Some(path)
}

fn build(
    vol: &mut Volume<'_>,
    geo: &Geometry,
    folders: HashMap<u32, Item>,
    files: Vec<Item>,
    runs: &mut Runs,
) -> Result<Listing> {
    let base = vol.base;
    let private: Vec<u32> = folders
        .iter()
        .filter(|(_, f)| f.parent == ROOT && (f.name == PRIVATE_FILES || f.name == PRIVATE_FOLDERS))
        .map(|(id, _)| *id)
        .collect();
    let nodes: HashMap<String, Fork> = files
        .iter()
        .filter(|f| private.contains(&f.parent))
        .map(|f| (f.name.clone(), f.data.clone()))
        .collect();
    let mut memo = HashMap::new();
    let mut listing = Listing::default();
    let (mut lost, mut compressed, mut skipped, mut damaged) = (0usize, 0usize, 0usize, 0usize);
    let mut ids: Vec<&u32> = folders.keys().collect();
    ids.sort_unstable();
    for &id in ids {
        let folder = &folders[&id];
        if id == ROOT || private.contains(&id) || private.contains(&folder.parent) {
            continue;
        }
        match path_of(&folders, id, &mut memo) {
            Some(path) => {
                let mut entry = fs::entry(path, 0, true, folder.mtime);
                entry.created = folder.created;
                entry.accessed = folder.accessed;
                listing.push(Member {
                    entry,
                    kind: Kind::Dir,
                    runs: Vec::new(),
                })?;
            }
            None => lost += 1,
        }
    }
    for file in &files {
        if private.contains(&file.parent) {
            continue;
        }
        if !fs::usable(&file.name) {
            lost += 1;
            continue;
        }
        let Some(dir) = path_of(&folders, file.parent, &mut memo) else {
            lost += 1;
            continue;
        };
        let path = fs::join(&dir, &file.name);
        if file.compressed {
            compressed += 1;
            continue;
        }
        if file.folder_link {
            skipped += 1;
            continue;
        }
        let data = match file.hard {
            Some(node) => match nodes.get(&format!("iNode{node}")) {
                Some(fork) => fork,
                None => {
                    damaged += 1;
                    continue;
                }
            },
            None => &file.data,
        };
        let mut entry = fs::entry(path, data.size, false, file.mtime);
        entry.created = file.created;
        entry.accessed = file.accessed;
        if file.mode & 0o170000 == 0o120000 {
            if data.size > MAX_LINK {
                damaged += 1;
                continue;
            }
            let target = read_fork(vol, geo, data, MAX_LINK)
                .map(|bytes| String::from_utf8_lossy(&bytes).to_string());
            match target {
                Ok(target) => {
                    entry.size = 0;
                    entry.compressed_size = 0;
                    listing.push(Member {
                        entry,
                        kind: Kind::Symlink(target),
                        runs: Vec::new(),
                    })?;
                }
                Err(_) => damaged += 1,
            }
            continue;
        }
        let mut pieces = Vec::new();
        if !data.runs(geo, base, runs, &mut pieces)? {
            damaged += 1;
            continue;
        }
        listing.push(Member {
            entry,
            kind: Kind::File,
            runs: pieces,
        })?;
    }
    for (count, what) in [
        (
            lost,
            "files or folders whose parent folder is missing are not listed",
        ),
        (
            compressed,
            "files compressed by HFS+ are not listed: Arca does not decode HFS+ compression",
        ),
        (skipped, "folder hard links are not listed"),
        (
            damaged,
            "files whose data lies outside the volume are not listed",
        ),
    ] {
        if count > 0 {
            listing.notices.push(format!("{count} {what}"));
        }
    }
    Ok(listing)
}
