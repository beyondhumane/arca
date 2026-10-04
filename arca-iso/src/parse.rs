use arca_core::{limits, Entry, Error, Method, Result};
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

pub(crate) const SECTOR: u64 = 2048;
const FIRST_DESCRIPTOR: u64 = 16;
const MAX_DESCRIPTORS: u64 = 256;
const MAX_DEPTH: usize = 64;
const MAX_DIRECTORY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CONTINUATIONS: usize = 32;
const MAX_SYSTEM_USE: usize = 64 * 1024;
const MAX_REPORTED: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    pub start: u64,
    pub len: u64,
}

pub(crate) struct Parsed {
    pub entries: Vec<Entry>,
    pub extents: Vec<Vec<Extent>>,
    pub notices: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Names {
    Plain,
    Joliet,
    RockRidge { skip: usize },
}

struct Record<'a> {
    extent: u32,
    xattr: u8,
    size: u32,
    date: &'a [u8],
    flags: u8,
    unit: u8,
    gap: u8,
    ident: &'a [u8],
    system_use: &'a [u8],
}

impl Record<'_> {
    fn is_dir(&self) -> bool {
        self.flags & 0x02 != 0
    }

    fn is_special(&self) -> bool {
        self.ident == [0] || self.ident == [1]
    }
}

#[derive(Default)]
struct RockRidge {
    name: Option<Vec<u8>>,
    mode: Option<u32>,
    symlink: bool,
    device: bool,
    child: Option<u32>,
    relocated: bool,
}

struct Walker<'a, R> {
    source: &'a mut R,
    len: u64,
    names: Names,
    progress: &'a dyn Fn() -> bool,
    entries: Vec<Entry>,
    extents: Vec<Vec<Extent>>,
    paths: HashSet<String>,
    visited: HashSet<u32>,
    directory_bytes: u64,
    skipped: Vec<String>,
    skipped_count: usize,
}

pub(crate) fn read_at<R: Read + Seek>(
    source: &mut R,
    len: u64,
    offset: u64,
    n: u64,
    what: &str,
) -> Result<Vec<u8>> {
    let end = offset
        .checked_add(n)
        .ok_or_else(|| Error::Format(format!("overflow while reading {what}")))?;
    if end > len {
        return Err(Error::Format(format!(
            "image truncated while reading {what}: {} bytes missing",
            end - len
        )));
    }
    let n = usize::try_from(n).map_err(|_| Error::Limit(format!("{what} too large")))?;
    let mut buf = vec![0; n];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut buf)?;
    Ok(buf)
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn record(d: &[u8]) -> Result<Record<'_>> {
    let len = d[0] as usize;
    if len < 34 || len > d.len() {
        return Err(Error::Format(format!(
            "directory record of {len} bytes is malformed"
        )));
    }
    let ident_len = d[32] as usize;
    let ident_end = 33 + ident_len;
    if ident_len == 0 || ident_end > len {
        return Err(Error::Format("directory record name overflows".into()));
    }
    let system_start = (ident_end + (ident_len + 1) % 2).min(len);
    Ok(Record {
        extent: u32le(d, 2),
        xattr: d[1],
        size: u32le(d, 10),
        date: &d[18..25],
        flags: d[25],
        unit: d[26],
        gap: d[27],
        ident: &d[33..ident_end],
        system_use: &d[system_start..len],
    })
}

fn records(data: &[u8]) -> Result<Vec<Record<'_>>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let sector = SECTOR as usize;
    while pos < data.len() {
        let len = data[pos] as usize;
        if len == 0 {
            pos = (pos / sector + 1) * sector;
            continue;
        }
        if pos % sector + len > sector {
            return Err(Error::Format(
                "directory record crosses a sector boundary".into(),
            ));
        }
        out.push(record(&data[pos..data.len().min(pos + len)])?);
        pos += len;
    }
    Ok(out)
}

fn unix_time(d: &[u8]) -> Option<i64> {
    let (year, month, day) = (1900 + i64::from(d[0]), i64::from(d[1]), i64::from(d[2]));
    let (h, m, s) = (i64::from(d[3]), i64::from(d[4]), i64::from(d[5]));
    let offset = i64::from(d[6] as i8);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || h > 23
        || m > 59
        || s > 59
        || !(-48..=52).contains(&offset)
    {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + m * 60 + s - offset * 15 * 60)
}

fn strip_version(name: &str) -> &str {
    match name.rsplit_once(';') {
        Some((base, version)) if version.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => name,
    }
}

fn joliet_name(ident: &[u8]) -> String {
    let units = ident
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]));
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn check_component(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || name.len() > limits::MAX_NAME
    {
        return Err(Error::Format(format!("unsafe ISO name: '{name}'")));
    }
    Ok(())
}

// The same rules the RAR reader applies, so an image cannot name something a
// Windows destination would resolve to a device or a different file.
fn check_path(path: &str) -> Result<()> {
    if path
        .chars()
        .any(|c| c.is_control() || ":<>\"|?*".contains(c))
    {
        return Err(Error::Format(format!("unsafe ISO path: '{path}'")));
    }
    arca_core::safe_name(path)?;
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let numbered = ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{b9}"
                        | "\u{b2}"
                        | "\u{b3}"
                )
            })
        });
        if part.ends_with(['.', ' '])
            || matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            || numbered
        {
            return Err(Error::Format(format!(
                "unsafe ISO path component: '{part}'"
            )));
        }
    }
    Ok(())
}

pub(crate) fn parse<R: Read + Seek>(source: &mut R, progress: &dyn Fn() -> bool) -> Result<Parsed> {
    let len = source.seek(SeekFrom::End(0))?;
    let mut primary = None;
    let mut joliet = None;
    let mut terminated = None;
    for n in 0..MAX_DESCRIPTORS {
        let sector = FIRST_DESCRIPTOR + n;
        let d = read_at(source, len, sector * SECTOR, SECTOR, "volume descriptor")?;
        if &d[1..6] != b"CD001" {
            if n == 0 && matches!(&d[1..6], b"BEA01" | b"NSR02" | b"NSR03") {
                return Err(Error::Unsupported(
                    "UDF-only image; Arca reads ISO 9660 file systems".into(),
                ));
            }
            return Err(Error::Format(if n == 0 {
                "not an ISO 9660 image (no volume descriptor at sector 16)".into()
            } else {
                "volume descriptor set has no terminator".into()
            }));
        }
        match d[0] {
            1 if primary.is_none() => primary = Some(d),
            2 if joliet.is_none() && matches!(&d[88..91], b"%/@" | b"%/C" | b"%/E") => {
                joliet = Some(d)
            }
            255 => {
                terminated = Some(sector);
                break;
            }
            _ => {}
        }
    }
    let terminator = terminated
        .ok_or_else(|| Error::Format("volume descriptor set has no terminator".into()))?;
    let primary =
        primary.ok_or_else(|| Error::Format("image has no primary volume descriptor".into()))?;
    for d in [Some(&primary), joliet.as_ref()].into_iter().flatten() {
        if u64::from(u16le(d, 128)) != SECTOR {
            return Err(Error::Unsupported(format!(
                "logical block size {} (only 2048 is supported)",
                u16le(d, 128)
            )));
        }
    }
    let declared = u64::from(u32le(&primary, 80)) * SECTOR;
    if declared > len {
        return Err(Error::Format(format!(
            "image truncated: the volume declares {declared} bytes and the file has {len}"
        )));
    }
    let mut udf = false;
    for sector in terminator + 1..terminator + 1 + MAX_DESCRIPTORS {
        let Ok(d) = read_at(source, len, sector * SECTOR, SECTOR, "volume recognition") else {
            break;
        };
        match &d[1..6] {
            b"BEA01" | b"BOOT2" | b"CD001" => {}
            b"NSR02" | b"NSR03" => udf = true,
            _ => break,
        }
    }

    let primary_root = record(&primary[156..190])?;
    let mut names = Names::Plain;
    let mut root = (primary_root.extent, primary_root.size);
    let first = read_at(
        source,
        len,
        u64::from(primary_root.extent) * SECTOR,
        SECTOR,
        "root directory",
    )?;
    let own = records(&first)?;
    let rock_ridge = own.first().and_then(|r| {
        let s = r.system_use;
        (s.len() >= 7 && &s[..2] == b"SP" && s[4] == 0xBE && s[5] == 0xEF).then(|| s[6] as usize)
    });
    if let Some(skip) = rock_ridge {
        names = Names::RockRidge { skip };
    } else if let Some(j) = &joliet {
        let r = record(&j[156..190])?;
        root = (r.extent, r.size);
        names = Names::Joliet;
    }

    let mut walker = Walker {
        source,
        len,
        names,
        progress,
        entries: Vec::new(),
        extents: Vec::new(),
        paths: HashSet::new(),
        visited: HashSet::new(),
        directory_bytes: 0,
        skipped: Vec::new(),
        skipped_count: 0,
    };
    walker.directory(root.0, root.1, "", 0)?;

    let mut notices = Vec::new();
    if walker.skipped_count > 0 {
        let mut shown = walker.skipped.join(", ");
        if walker.skipped_count > walker.skipped.len() {
            shown.push_str(", ...");
        }
        notices.push(format!(
            "skipped {} symbolic links or special files: {shown}",
            walker.skipped_count
        ));
    }
    if udf {
        notices.push(
            "this image also carries a UDF file system; files stored only in UDF \
             (for example install.wim on Windows installation media) are not listed"
                .into(),
        );
    }
    Ok(Parsed {
        entries: walker.entries,
        extents: walker.extents,
        notices,
    })
}

impl<R: Read + Seek> Walker<'_, R> {
    fn directory(&mut self, extent: u32, size: u32, prefix: &str, depth: usize) -> Result<()> {
        if !(self.progress)() {
            return Err(Error::Cancelled);
        }
        if depth > MAX_DEPTH {
            return Err(Error::Limit(format!(
                "directories nested deeper than {MAX_DEPTH} levels"
            )));
        }
        if !self.visited.insert(extent) {
            return Err(Error::Format(format!(
                "directory loop at sector {extent} ('{prefix}')"
            )));
        }
        self.directory_bytes += u64::from(size);
        if self.directory_bytes > MAX_DIRECTORY_BYTES {
            return Err(Error::Limit("directory data exceeds 256 MiB".into()));
        }
        let data = read_at(
            self.source,
            self.len,
            u64::from(extent) * SECTOR,
            u64::from(size),
            "directory",
        )?;
        let mut pending: Vec<Extent> = Vec::new();
        let mut pending_ident: Option<&[u8]> = None;
        for r in records(&data)? {
            if r.is_special() {
                continue;
            }
            if r.unit != 0 || r.gap != 0 {
                return Err(Error::Unsupported("interleaved ISO files".into()));
            }
            if pending_ident.is_some_and(|ident| ident != r.ident) {
                return Err(Error::Format("multi-extent file is interrupted".into()));
            }
            let start = (u64::from(r.extent) + u64::from(r.xattr)) * SECTOR;
            let part = Extent {
                start,
                len: u64::from(r.size),
            };
            if !r.is_dir() && part.len > 0 && start + part.len > self.len {
                return Err(Error::Format(format!(
                    "file data at sector {} lies past the end of the image",
                    r.extent
                )));
            }
            if r.flags & 0x80 != 0 {
                if r.is_dir() {
                    return Err(Error::Format("multi-extent directory".into()));
                }
                pending.push(part);
                pending_ident = Some(r.ident);
                continue;
            }
            pending.push(part);
            pending_ident = None;
            let parts = std::mem::take(&mut pending);
            if r.flags & 0x04 != 0 {
                continue;
            }
            let rr = match self.names {
                Names::RockRidge { skip } => self.rock_ridge(r.system_use, skip)?,
                _ => RockRidge::default(),
            };
            if rr.relocated {
                continue;
            }
            let name = match (self.names, &rr.name) {
                (Names::RockRidge { .. }, Some(raw)) => String::from_utf8_lossy(raw).into_owned(),
                (Names::Joliet, _) => strip_version(&joliet_name(r.ident)).to_string(),
                _ => {
                    let plain = String::from_utf8_lossy(r.ident).into_owned();
                    if r.is_dir() {
                        plain
                    } else {
                        let base = strip_version(&plain);
                        base.strip_suffix('.').unwrap_or(base).to_string()
                    }
                }
            };
            check_component(&name)?;
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let file_type = rr.mode.map(|m| m & 0o170000);
            let is_dir = r.is_dir() || rr.child.is_some();
            if rr.symlink
                || rr.device
                || file_type.is_some_and(|t| !matches!(t, 0 | 0o100000 | 0o040000))
            {
                self.skipped_count += 1;
                if self.skipped.len() < MAX_REPORTED {
                    self.skipped.push(path);
                }
                continue;
            }
            check_path(&path)?;
            if !self.paths.insert(path.to_lowercase()) {
                return Err(Error::Format(format!("duplicate ISO path: '{path}'")));
            }
            if self.entries.len() as u64 >= limits::MAX_ENTRIES {
                return Err(Error::Limit("too many ISO entries".into()));
            }
            let size = if is_dir {
                0
            } else {
                parts
                    .iter()
                    .try_fold(0u64, |total, p| total.checked_add(p.len))
                    .ok_or_else(|| Error::Format("file size overflows".into()))?
            };
            self.entries.push(Entry {
                name: path.clone(),
                size,
                compressed_size: size,
                method: Method::Store,
                crc32: None,
                is_dir,
                mtime: unix_time(r.date),
                created: None,
                accessed: None,
                attributes: if r.flags & 0x01 != 0 { 0x02 } else { 0 },
                offset: self.entries.len() as u64,
                raw_name: rr.name.clone().unwrap_or_else(|| r.ident.to_vec()),
                utf8: true,
                encrypted: false,
                zipcrypto: false,
            });
            self.extents.push(if is_dir { Vec::new() } else { parts });
            if is_dir {
                let (extent, size) = match rr.child {
                    Some(child) => (child, self.relocated_size(child)?),
                    None => (r.extent, r.size),
                };
                let before = self.entries.len();
                self.directory(extent, size, &path, depth + 1)?;
                let rr_moved = matches!(self.names, Names::RockRidge { .. })
                    && depth == 0
                    && path.eq_ignore_ascii_case("rr_moved");
                if rr_moved && self.entries.len() == before {
                    self.entries.pop();
                    self.extents.pop();
                    self.paths.remove(&path.to_lowercase());
                }
            }
        }
        if pending_ident.is_some() {
            return Err(Error::Format(
                "multi-extent file has no final extent".into(),
            ));
        }
        Ok(())
    }

    fn relocated_size(&mut self, extent: u32) -> Result<u32> {
        let first = read_at(
            self.source,
            self.len,
            u64::from(extent) * SECTOR,
            SECTOR,
            "relocated directory",
        )?;
        records(&first)?
            .first()
            .filter(|r| r.ident == [0])
            .map(|r| r.size)
            .ok_or_else(|| Error::Format("relocated directory has no self record".into()))
    }

    fn rock_ridge(&mut self, system_use: &[u8], skip: usize) -> Result<RockRidge> {
        let mut rr = RockRidge::default();
        let mut area = system_use.get(skip..).unwrap_or_default().to_vec();
        let mut hops = 0;
        let mut total = area.len();
        loop {
            let mut next = None;
            let mut pos = 0;
            while pos + 4 <= area.len() {
                let len = area[pos + 2] as usize;
                if len < 4 || pos + len > area.len() {
                    break;
                }
                let e = &area[pos..pos + len];
                match &e[..2] {
                    b"CE" if len >= 28 => {
                        next = Some((u32le(e, 4), u32le(e, 12), u32le(e, 20)));
                    }
                    b"NM" if len >= 5 => {
                        if e[4] & 0x06 != 0 {
                            return Err(Error::Format(
                                "Rock Ridge name points at '.' or '..'".into(),
                            ));
                        }
                        let name = rr.name.get_or_insert_with(Vec::new);
                        name.extend_from_slice(&e[5..]);
                        if name.len() > limits::MAX_NAME {
                            return Err(Error::Limit("Rock Ridge name too long".into()));
                        }
                    }
                    b"PX" if len >= 8 => rr.mode = Some(u32le(e, 4)),
                    b"SL" => rr.symlink = true,
                    b"PN" => rr.device = true,
                    b"CL" if len >= 8 => rr.child = Some(u32le(e, 4)),
                    b"RE" => rr.relocated = true,
                    b"ST" => break,
                    _ => {}
                }
                pos += len;
            }
            let Some((block, offset, length)) = next else {
                break;
            };
            hops += 1;
            total += length as usize;
            if hops > MAX_CONTINUATIONS || total > MAX_SYSTEM_USE {
                return Err(Error::Limit(
                    "Rock Ridge continuation chain too long".into(),
                ));
            }
            area = read_at(
                self.source,
                self.len,
                u64::from(block) * SECTOR + u64::from(offset),
                u64::from(length),
                "Rock Ridge continuation area",
            )?;
        }
        Ok(rr)
    }
}
