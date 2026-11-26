use arca_core::{Error, Result};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::device::{le32, le64, past_end, Device, Raw};

const MAX_DESCRIPTOR: u64 = 64 * 1024;
const MAX_EXTENTS: usize = 1024;
const MAX_GRAIN_SECTORS: u64 = 2048;
const MAX_TABLES: usize = 64;
const GD_AT_END: u64 = u64::MAX;

pub(crate) fn open(path: &Path) -> Result<Box<dyn Device>> {
    let mut file = Raw::open(path)?;
    let head = file.bytes(0, file.len().min(512))?;
    if head.starts_with(b"KDMV") {
        return Ok(Box::new(Sparse::open(file)?));
    }
    let text = file.bytes(0, file.len().min(MAX_DESCRIPTOR))?;
    let text = String::from_utf8_lossy(&text);
    if !text.contains("# Disk DescriptorFile") && !text.contains("createType") {
        return Err(Error::Format("not a VMDK file".into()));
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(Box::new(Concat::open(&text, dir)?))
}

fn check_parent(descriptor: &str) -> Result<()> {
    for line in descriptor.lines() {
        if let Some(value) = line.trim().strip_prefix("parentCID") {
            let value = value.trim_start_matches([' ', '=']).trim();
            if !value.eq_ignore_ascii_case("ffffffff") {
                return Err(Error::Unsupported(
                    "this is a VMDK snapshot: its data is split with a parent disk Arca does not open"
                        .into(),
                ));
            }
        }
    }
    Ok(())
}

enum Part {
    Flat { file: Raw, start: u64 },
    Zero,
    Sparse(Box<Sparse>),
}

/// A disk described by a text descriptor: its extents, one after another,
/// each in its own file next to the descriptor.
struct Concat {
    parts: Vec<(u64, u64, Part)>,
    len: u64,
}

impl Concat {
    fn open(text: &str, dir: &Path) -> Result<Self> {
        check_parent(text)?;
        let mut parts = Vec::new();
        let mut len = 0u64;
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if !matches!(words.first(), Some(&("RW" | "RDONLY" | "NOACCESS"))) {
                continue;
            }
            if parts.len() >= MAX_EXTENTS {
                return Err(Error::Limit("a VMDK of too many extents".into()));
            }
            let sectors: u64 = words
                .get(1)
                .and_then(|w| w.parse().ok())
                .ok_or_else(|| Error::Format(format!("a VMDK extent line '{line}'")))?;
            let kind = words.get(2).copied().unwrap_or("");
            let size = sectors
                .checked_mul(512)
                .ok_or_else(|| Error::Format("a VMDK extent too large".into()))?;
            let part = match kind {
                "ZERO" => Part::Zero,
                "FLAT" | "VMFS" | "SPARSE" => {
                    let name = quoted(line)
                        .ok_or_else(|| Error::Format(format!("a VMDK extent line '{line}'")))?;
                    if name.is_empty()
                        || name.contains(['/', '\\', ':'])
                        || name == "."
                        || name == ".."
                    {
                        return Err(Error::Unsupported(format!(
                            "the VMDK extent '{name}' is not a file next to the descriptor"
                        )));
                    }
                    let file = Raw::open(&dir.join(name)).map_err(|e| {
                        Error::Format(format!("the VMDK extent '{name}' cannot be opened: {e}"))
                    })?;
                    if kind == "SPARSE" {
                        Part::Sparse(Box::new(Sparse::open(file)?))
                    } else {
                        let offset: u64 = words.last().and_then(|w| w.parse().ok()).unwrap_or(0);
                        Part::Flat {
                            file,
                            start: offset.saturating_mul(512),
                        }
                    }
                }
                other => {
                    return Err(Error::Unsupported(format!("VMDK extents of type {other}")));
                }
            };
            if size == 0 {
                continue;
            }
            parts.push((len, size, part));
            len = len
                .checked_add(size)
                .ok_or_else(|| Error::Format("a VMDK too large".into()))?;
        }
        if parts.is_empty() {
            return Err(Error::Format("the VMDK descriptor lists no extents".into()));
        }
        Ok(Concat { parts, len })
    }
}

fn quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    Some(&line[start..end])
}

impl Device for Concat {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.len {
            return Err(past_end());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let i = self.parts.partition_point(|(start, _, _)| *start <= at) - 1;
            let (start, size, part) = &mut self.parts[i];
            let within = at - *start;
            let n = ((*size - within) as usize).min(buf.len() - done);
            let piece = &mut buf[done..done + n];
            match part {
                Part::Zero => piece.fill(0),
                Part::Flat { file, start } => file.read_at(start.saturating_add(within), piece)?,
                Part::Sparse(sparse) => {
                    if within + n as u64 > sparse.len() {
                        piece.fill(0);
                        let have = sparse.len().saturating_sub(within).min(n as u64) as usize;
                        if have > 0 {
                            sparse.read_at(within, &mut piece[..have])?;
                        }
                    } else {
                        sparse.read_at(within, piece)?;
                    }
                }
            }
            done += n;
        }
        Ok(())
    }
}

/// A hosted sparse extent: a grain directory pointing to grain tables
/// pointing to grains, compressed one by one in a stream-optimized disk.
struct Sparse {
    file: Raw,
    len: u64,
    grain: u64,
    per_table: u64,
    directory: Vec<u32>,
    compressed: bool,
    tables: HashMap<u32, Vec<u32>>,
    cached: Option<(u32, Vec<u8>)>,
}

impl Sparse {
    fn open(mut file: Raw) -> Result<Self> {
        let mut h = file.bytes(0, 512)?;
        if &h[..4] != b"KDMV" {
            return Err(Error::Format("not a VMDK sparse extent".into()));
        }
        if le64(&h, 56) == GD_AT_END {
            let at = file
                .len()
                .checked_sub(1024)
                .ok_or_else(|| Error::Format("the VMDK footer is missing".into()))?;
            h = file.bytes(at, 512)?;
            if &h[..4] != b"KDMV" {
                return Err(Error::Format("the VMDK footer is damaged".into()));
            }
        }
        let version = le32(&h, 4);
        if !(1..=3).contains(&version) {
            return Err(Error::Unsupported(format!("VMDK sparse version {version}")));
        }
        let flags = le32(&h, 8);
        let capacity = le64(&h, 12);
        let grain = le64(&h, 20);
        let (desc_at, desc_len) = (le64(&h, 28), le64(&h, 36));
        let per_table = u64::from(le32(&h, 44));
        let gd = if le64(&h, 56) != 0 {
            le64(&h, 56)
        } else {
            le64(&h, 48)
        };
        if !grain.is_power_of_two() || grain > MAX_GRAIN_SECTORS {
            return Err(Error::Format(format!("VMDK grains of {grain} sectors")));
        }
        if !(1..=4096).contains(&per_table) {
            return Err(Error::Format(format!(
                "VMDK grain tables of {per_table} entries"
            )));
        }
        let len = capacity
            .checked_mul(512)
            .filter(|&l| l <= 1 << 50)
            .ok_or_else(|| Error::Format("a VMDK too large".into()))?;
        if desc_at != 0 && desc_len != 0 && desc_len <= MAX_DESCRIPTOR / 512 {
            let text = file.bytes(desc_at.saturating_mul(512), desc_len * 512)?;
            check_parent(&String::from_utf8_lossy(&text))?;
        }
        let tables = capacity.div_ceil(grain * per_table);
        let raw = file.bytes(gd.saturating_mul(512), tables.saturating_mul(4))?;
        let directory = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        Ok(Sparse {
            file,
            len,
            grain: grain * 512,
            per_table,
            directory,
            compressed: flags & (1 << 16) != 0,
            tables: HashMap::new(),
            cached: None,
        })
    }

    fn grain_at(&mut self, index: u64) -> Result<u32> {
        let table = (index / self.per_table) as usize;
        let Some(&sector) = self.directory.get(table) else {
            return Ok(0);
        };
        if sector == 0 {
            return Ok(0);
        }
        if !self.tables.contains_key(&sector) {
            if self.tables.len() >= MAX_TABLES {
                self.tables.clear();
            }
            let raw = self
                .file
                .bytes(u64::from(sector) * 512, self.per_table * 4)?;
            self.tables.insert(
                sector,
                raw.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_le_bytes(*b))
                    .collect(),
            );
        }
        Ok(self.tables[&sector][(index % self.per_table) as usize])
    }

    fn inflate(&mut self, sector: u32) -> Result<&[u8]> {
        if self.cached.as_ref().is_none_or(|(s, _)| *s != sector) {
            let at = u64::from(sector) * 512;
            let marker = self.file.bytes(at, 12)?;
            let size = u64::from(le32(&marker, 8));
            if size > self.grain * 2 + 4096 {
                return Err(Error::Format("a VMDK compressed grain too large".into()));
            }
            let packed = self.file.bytes(at + 12, size)?;
            let mut out = Vec::with_capacity(self.grain as usize);
            ZlibDecoder::new(&packed[..])
                .take(self.grain)
                .read_to_end(&mut out)
                .map_err(|e| Error::Format(format!("a damaged VMDK grain: {e}")))?;
            out.resize(self.grain as usize, 0);
            self.cached = Some((sector, out));
        }
        Ok(self.cached.as_ref().map_or(&[][..], |(_, g)| g))
    }
}

impl Device for Sparse {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.len {
            return Err(past_end());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let index = at / self.grain;
            let within = at % self.grain;
            let n = ((self.grain - within) as usize).min(buf.len() - done);
            let sector = self.grain_at(index)?;
            if sector <= 1 {
                buf[done..done + n].fill(0);
            } else if self.compressed {
                let grain = self.inflate(sector)?;
                let from = within as usize;
                buf[done..done + n].copy_from_slice(&grain[from..from + n]);
            } else {
                let start = u64::from(sector) * 512 + within;
                self.file.read_at(start, &mut buf[done..done + n])?;
            }
            done += n;
        }
        Ok(())
    }
}
