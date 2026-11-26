//! CPIO in its four common layouts: "newc" and "crc" (the SVR4 ASCII
//! headers Linux initramfs and RPM use), "odc" (POSIX octal) and the old
//! binary headers in either byte order.

use crate::{
    check_count, check_name, clean, entry, file_len, skipped_notice, unix_kind, Data, Exact, Kind,
    Member, Parsed, BUF,
};
use arca_core::{limits, Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

const TRAILER: &[u8] = b"TRAILER!!!";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    Newc,
    Crc,
    Odc,
    BinLe,
    BinBe,
}

impl Layout {
    fn header_len(self) -> usize {
        match self {
            Layout::Newc | Layout::Crc => 110,
            Layout::Odc => 76,
            Layout::BinLe | Layout::BinBe => 26,
        }
    }

    fn align(self) -> u64 {
        match self {
            Layout::Newc | Layout::Crc => 4,
            Layout::Odc => 1,
            Layout::BinLe | Layout::BinBe => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Header {
    pub name: Vec<u8>,
    pub mode: u32,
    pub size: u64,
    pub mtime: i64,
    pub inode: (u64, u64),
    pub nlink: u64,
    pub sum: Option<u32>,
    align: u64,
}

/// Skipping file data: by seeking in a file, by reading in a stream.
pub(crate) trait Skip: Read {
    fn skip(&mut self, n: u64) -> io::Result<u64> {
        io::copy(&mut self.take(n), &mut io::sink())
    }
}

impl Skip for Box<dyn Read + Send> {}

impl Skip for BufReader<File> {
    fn skip(&mut self, n: u64) -> io::Result<u64> {
        let by = i64::try_from(n).map_err(|_| io::Error::other("offset out of range"))?;
        self.seek_relative(by)?;
        Ok(n)
    }
}

pub(crate) struct Reader<R> {
    source: R,
    pos: u64,
    finished: bool,
}

impl<R: Skip> Reader<R> {
    pub(crate) fn new(source: R) -> Self {
        Reader {
            source,
            pos: 0,
            finished: false,
        }
    }

    pub(crate) fn into_inner(self) -> R {
        self.source
    }

    pub(crate) fn position(&self) -> u64 {
        self.pos
    }

    fn fill(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut filled = 0;
        while filled < buf.len() {
            match self.source.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.pos += filled as u64;
        Ok(filled)
    }

    fn exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if self.fill(buf)? < buf.len() {
            return Err(truncated());
        }
        Ok(())
    }

    fn pad(&mut self, align: u64) -> Result<()> {
        let pad = (align - self.pos % align) % align;
        if pad > 0 {
            let mut discard = [0u8; 4];
            self.exact(&mut discard[..pad as usize])?;
        }
        Ok(())
    }

    /// The next member, or `None` after the trailer. An archive that stops
    /// cleanly where a header would start is read as ended, as `cpio` and
    /// `bsdtar` do; one that stops inside a header is truncated.
    pub(crate) fn next_header(&mut self) -> Result<Option<Header>> {
        if self.finished {
            return Ok(None);
        }
        let mut head = [0u8; 110];
        let got = self.fill(&mut head[..6])?;
        if got == 0 {
            self.finished = true;
            return Ok(None);
        }
        if got < 6 {
            return Err(truncated());
        }
        let layout = match &head[..6] {
            b"070701" => Layout::Newc,
            b"070702" => Layout::Crc,
            b"070707" => Layout::Odc,
            b"07070X" => {
                return Err(Error::Unsupported(
                    "the CPIO data uses RPM's large-file headers, which Arca does not read".into(),
                ))
            }
            [0xc7, 0x71, ..] => Layout::BinLe,
            [0x71, 0xc7, ..] => Layout::BinBe,
            _ => return Err(Error::Format("a CPIO header is missing or damaged".into())),
        };
        let len = layout.header_len();
        self.exact(&mut head[6..len])?;
        let head = &head[..len];
        let (mut h, namesize) = match layout {
            Layout::Newc | Layout::Crc => {
                let field = |i: usize| hex(&head[6 + 8 * i..14 + 8 * i]);
                let (dev, ino) = ((field(7)? << 32) | field(8)?, field(0)?);
                let sum = (layout == Layout::Crc).then_some(field(12)? as u32);
                (
                    Header {
                        name: Vec::new(),
                        mode: field(1)? as u32,
                        size: field(6)?,
                        mtime: field(5)? as i64,
                        inode: (dev, ino),
                        nlink: field(4)?,
                        sum,
                        align: layout.align(),
                    },
                    field(11)?,
                )
            }
            Layout::Odc => {
                let at = |from: usize, to: usize| octal(&head[from..to]);
                (
                    Header {
                        name: Vec::new(),
                        mode: at(18, 24)? as u32,
                        size: at(65, 76)?,
                        mtime: at(48, 59)? as i64,
                        inode: (at(6, 12)?, at(12, 18)?),
                        nlink: at(36, 42)?,
                        sum: None,
                        align: layout.align(),
                    },
                    at(59, 65)?,
                )
            }
            Layout::BinLe | Layout::BinBe => {
                let word = |i: usize| {
                    let b = [head[2 * i], head[2 * i + 1]];
                    u64::from(if layout == Layout::BinLe {
                        u16::from_le_bytes(b)
                    } else {
                        u16::from_be_bytes(b)
                    })
                };
                (
                    Header {
                        name: Vec::new(),
                        mode: word(3) as u32,
                        size: (word(11) << 16) | word(12),
                        mtime: ((word(8) << 16) | word(9)) as i64,
                        inode: (word(1), word(2)),
                        nlink: word(6),
                        sum: None,
                        align: layout.align(),
                    },
                    word(10),
                )
            }
        };
        if namesize == 0 {
            return Err(Error::Format("a CPIO header has an empty name".into()));
        }
        if namesize > limits::MAX_NAME as u64 + 1 {
            return Err(Error::Limit(format!("a CPIO name of {namesize} bytes")));
        }
        let mut name = vec![0u8; namesize as usize];
        self.exact(&mut name)?;
        self.pad(layout.align())?;
        if let Some(end) = name.iter().position(|&b| b == 0) {
            name.truncate(end);
        }
        if name == TRAILER {
            self.finished = true;
            return Ok(None);
        }
        h.name = name;
        Ok(Some(h))
    }

    /// Hands `f` the member's data, which fails instead of ending early when
    /// the archive is cut short. Whatever `f` leaves unread is skipped.
    pub(crate) fn with_data<T>(
        &mut self,
        h: &Header,
        f: impl FnOnce(&mut dyn Read) -> Result<T>,
    ) -> Result<T> {
        let name = String::from_utf8_lossy(&h.name).into_owned();
        let mut data = Exact::new(&mut self.source, h.size, &name);
        let out = f(&mut data)?;
        let left = data.left;
        if left > 0 && self.source.skip(left)? < left {
            return Err(truncated());
        }
        self.pos += h.size;
        self.pad(h.align)?;
        Ok(out)
    }

    pub(crate) fn skip_data(&mut self, h: &Header) -> Result<()> {
        if self.source.skip(h.size)? < h.size {
            return Err(truncated());
        }
        self.pos += h.size;
        self.pad(h.align)
    }
}

fn truncated() -> Error {
    Error::Format("the CPIO archive is truncated".into())
}

fn hex(field: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(field).map_err(|_| bad_number())?;
    u64::from_str_radix(text, 16).map_err(|_| bad_number())
}

fn octal(field: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(field).map_err(|_| bad_number())?;
    u64::from_str_radix(text, 8).map_err(|_| bad_number())
}

fn bad_number() -> Error {
    Error::Format("a CPIO header holds a number that is not one".into())
}

/// Collects the members of a CPIO archive read through `reader`, placing
/// each file's data with `place(ordinal, data position, size)`.
pub(crate) fn list<R: Skip>(
    reader: &mut Reader<R>,
    end: Option<u64>,
    place: &dyn Fn(u64, u64, u64) -> Data,
    carry_on: &dyn Fn(&str) -> bool,
) -> Result<(Vec<Member>, Vec<String>)> {
    let mut members: Vec<Member> = Vec::new();
    let mut special = 0usize;
    let mut ordinal = 0u64;
    let mut links: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    while let Some(h) = reader.next_header()? {
        check_name(&h.name)?;
        let name = clean(&String::from_utf8_lossy(&h.name));
        if !carry_on(&name) {
            return Err(Error::Cancelled);
        }
        let at = reader.position();
        if end.is_some_and(|end| at.checked_add(h.size).is_none_or(|stop| stop > end)) {
            return Err(Error::Format(format!(
                "'{name}' runs past the end of the archive"
            )));
        }
        let symlink = h.mode & 0o170000 == 0o120000;
        let target = if symlink {
            if h.size > limits::MAX_NAME as u64 {
                return Err(Error::Limit("a link target too long".into()));
            }
            let mut target = Vec::new();
            reader.with_data(&h, |data| Ok(data.read_to_end(&mut target)?))?;
            Some(String::from_utf8_lossy(&target).into_owned())
        } else {
            None
        };
        let kind = unix_kind(h.mode, target);
        let this = ordinal;
        ordinal += 1;
        let special_file = kind.is_none();
        let Some(kind) = kind.filter(|_| !name.is_empty()) else {
            if special_file {
                special += 1;
            }
            if !symlink {
                reader.skip_data(&h)?;
            }
            continue;
        };
        check_count(&members)?;
        let is_dir = kind == Kind::Dir;
        let size = if kind == Kind::File { h.size } else { 0 };
        let mut e = entry(name, h.name.clone(), size, Some(h.mtime));
        e.is_dir = is_dir;
        if kind == Kind::File && h.nlink > 1 {
            links.entry(h.inode).or_default().push(members.len());
        }
        members.push(Member {
            entry: e,
            data: if kind == Kind::File {
                place(this, at, h.size)
            } else {
                Data::None
            },
            kind,
            mode: h.mode & 0o7777,
            sum: h.sum.filter(|_| size > 0),
        });
        if !symlink {
            reader.skip_data(&h)?;
        }
    }
    join_links(&mut members, links);
    let notices = if special > 0 {
        vec![skipped_notice(special)]
    } else {
        Vec::new()
    };
    Ok((members, notices))
}

// SVR4 CPIO stores the data of a file with several names once, under the
// last of them, and the others with no data at all. Those become hard links
// to the one that has it.
fn join_links(members: &mut [Member], links: HashMap<(u64, u64), Vec<usize>>) {
    for group in links.into_values() {
        let Some(&holder) = group.iter().rev().find(|&&i| members[i].entry.size > 0) else {
            continue;
        };
        let (target, size) = (
            members[holder].entry.name.clone(),
            members[holder].entry.size,
        );
        for i in group {
            if i != holder && members[i].entry.size == 0 {
                members[i].kind = Kind::Hard(target.clone());
                members[i].entry.size = size;
                members[i].entry.compressed_size = size;
                members[i].data = Data::None;
                members[i].sum = None;
            }
        }
    }
}

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut reader = Reader::new(BufReader::with_capacity(BUF, file));
    let (members, notices) = list(
        &mut reader,
        Some(len),
        &|_, start, size| Data::Range { start, len: size },
        carry_on,
    )?;
    Ok(Parsed {
        members,
        notices,
        ..Parsed::default()
    })
}
