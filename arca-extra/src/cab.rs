//! Microsoft cabinets: stored, MSZIP and LZX folders. Quantum folders are
//! listed but not decoded, and files that continue in another cabinet of a
//! set are left out.

use crate::{check_count, clean, entry, file_len, Data, Kind, Member, Parsed, BUF};
use arca_core::{Error, Method, Result};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 4] = b"MSCF";
const PREV: u16 = 0x1;
const NEXT: u16 = 0x2;
const RESERVE: u16 = 0x4;
const CONTINUED: u16 = 0xfffd;
const MAX_STRING: usize = 256;
pub(crate) const MAX_BLOCK: usize = 32 * 1024;
const WINDOW: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Compression {
    None,
    MsZip,
    Quantum,
    Lzx(lzxd::WindowSize),
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    header: u64,
    reserve: u8,
    packed: u16,
    size: u16,
    sum: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Folder {
    compression: Compression,
    blocks: Vec<Block>,
    total: u64,
}

impl Folder {
    pub(crate) fn decodable(&self) -> bool {
        self.compression != Compression::Quantum
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Index {
    pub folders: Vec<Folder>,
}

struct Cursor<'a> {
    file: &'a mut BufReader<File>,
}

impl Cursor<'_> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut b = [0u8; N];
        self.file.read_exact(&mut b).map_err(|_| truncated())?;
        Ok(b)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }

    fn skip(&mut self, n: u64) -> Result<()> {
        let by = i64::try_from(n).map_err(|_| truncated())?;
        self.file.seek_relative(by)?;
        Ok(())
    }

    fn string(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let b = self.u8()?;
            if b == 0 {
                return Ok(out);
            }
            if out.len() == MAX_STRING {
                return Err(Error::Format(
                    "a cabinet name is longer than 256 bytes".into(),
                ));
            }
            out.push(b);
        }
    }
}

fn truncated() -> Error {
    Error::Format("the cabinet is truncated".into())
}

fn compression(bits: u16) -> Result<Compression> {
    Ok(match bits & 0x0f {
        0 => Compression::None,
        1 => Compression::MsZip,
        2 => Compression::Quantum,
        3 => Compression::Lzx(match (bits >> 8) & 0x1f {
            15 => lzxd::WindowSize::KB32,
            16 => lzxd::WindowSize::KB64,
            17 => lzxd::WindowSize::KB128,
            18 => lzxd::WindowSize::KB256,
            19 => lzxd::WindowSize::KB512,
            20 => lzxd::WindowSize::MB1,
            21 => lzxd::WindowSize::MB2,
            other => {
                return Err(Error::Format(format!(
                    "an LZX folder declares a window of 2^{other} bytes"
                )))
            }
        }),
        other => {
            return Err(Error::Unsupported(format!(
                "cabinet compression type {other}"
            )))
        }
    })
}

fn method(compression: Compression) -> Method {
    match compression {
        Compression::None => Method::Store,
        Compression::MsZip => Method::Deflate,
        Compression::Quantum => Method::Quantum,
        Compression::Lzx(_) => Method::Lzx,
    }
}

// Code page names are read as Latin-1, which keeps every byte as one
// character; the flag says when they are UTF-8.
fn decode_name(raw: &[u8], utf8: bool) -> String {
    match std::str::from_utf8(raw) {
        Ok(text) if utf8 || text.is_ascii() => text.to_string(),
        _ if utf8 => String::from_utf8_lossy(raw).into_owned(),
        _ => raw.iter().map(|&b| char::from(b)).collect(),
    }
}

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut file = BufReader::with_capacity(BUF, file);
    let mut c = Cursor { file: &mut file };
    if &c.bytes::<4>()? != MAGIC {
        return Err(Error::Format("not a cabinet".into()));
    }
    c.skip(12)?;
    let files_at = u64::from(c.u32()?);
    c.skip(4)?;
    let (minor, major) = (c.u8()?, c.u8()?);
    if major != 1 {
        return Err(Error::Unsupported(format!(
            "cabinet version {major}.{minor}"
        )));
    }
    let folders = c.u16()?;
    let files = c.u16()?;
    let flags = c.u16()?;
    c.skip(4)?;
    let (mut folder_reserve, mut data_reserve) = (0u8, 0u8);
    if flags & RESERVE != 0 {
        let header_reserve = c.u16()?;
        folder_reserve = c.u8()?;
        data_reserve = c.u8()?;
        c.skip(u64::from(header_reserve))?;
    }
    for flag in [PREV, NEXT] {
        if flags & flag != 0 {
            c.string()?;
            c.string()?;
        }
    }
    let mut raw_folders = Vec::with_capacity(usize::from(folders));
    for _ in 0..folders {
        let start = u64::from(c.u32()?);
        let count = c.u16()?;
        let kind = c.u16()?;
        c.skip(u64::from(folder_reserve))?;
        raw_folders.push((start, count, kind));
    }
    let mut index = Index::default();
    let mut cut = Vec::new();
    for (start, count, kind) in raw_folders {
        let compression = compression(kind)?;
        let mut blocks = Vec::new();
        let mut total = 0u64;
        let mut at = start;
        let mut split = false;
        for _ in 0..count {
            if len.saturating_sub(at) < 8 + u64::from(data_reserve) {
                return Err(truncated());
            }
            c.file.seek(SeekFrom::Start(at))?;
            let sum = c.u32()?;
            let packed = c.u16()?;
            let size = c.u16()?;
            let data = at + 8 + u64::from(data_reserve);
            if u64::from(packed) > len - data {
                return Err(truncated());
            }
            if size == 0 {
                split = true;
                break;
            }
            if usize::from(size) > MAX_BLOCK {
                return Err(Error::Format(
                    "a cabinet block holds more than 32 KiB".into(),
                ));
            }
            if compression == Compression::None && packed != size {
                return Err(Error::Format("a stored cabinet block has two sizes".into()));
            }
            blocks.push(Block {
                header: at,
                reserve: data_reserve,
                packed,
                size,
                sum,
            });
            total += u64::from(size);
            at = data + u64::from(packed);
        }
        cut.push(split);
        index.folders.push(Folder {
            compression,
            blocks,
            total,
        });
    }
    c.file.seek(SeekFrom::Start(files_at))?;
    let mut members = Vec::new();
    let mut continued = 0usize;
    for _ in 0..files {
        let size = u64::from(c.u32()?);
        let offset = u64::from(c.u32()?);
        let folder = c.u16()?;
        let date = c.u16()?;
        let time = c.u16()?;
        let attributes = c.u16()?;
        let raw = c.string()?;
        let name = clean(&decode_name(&raw, attributes & 0x80 != 0).replace('\\', "/"));
        if !carry_on(&name) {
            return Err(Error::Cancelled);
        }
        let folder = usize::from(folder);
        if folder >= usize::from(CONTINUED) || (folder == 0 && flags & PREV != 0) {
            continued += 1;
            continue;
        }
        let Some(f) = index.folders.get(folder) else {
            return Err(Error::Format(
                "a cabinet file names a folder that is not there".into(),
            ));
        };
        if offset.checked_add(size).is_none_or(|end| end > f.total) {
            if cut[folder] {
                continued += 1;
                continue;
            }
            return Err(Error::Format(format!(
                "'{name}' runs past the end of its cabinet folder"
            )));
        }
        if name.is_empty() {
            return Err(Error::Format("a cabinet file without a name".into()));
        }
        check_count(&members)?;
        let mut e = entry(name, raw, size, arca_core::dos_to_unix(date, time));
        e.method = method(f.compression);
        e.attributes = (attributes & 0x27) as u8;
        members.push(Member {
            entry: e,
            kind: Kind::File,
            mode: 0,
            data: Data::Cab { folder, offset },
            sum: None,
        });
    }
    let mut notices = Vec::new();
    if continued > 0 {
        notices.push(format!(
            "{continued} files continue in another cabinet of the set and are not listed"
        ));
    }
    Ok(Parsed {
        members,
        notices,
        streams: Vec::new(),
        cab: Some(index),
    })
}

enum Decoder {
    Stored,
    MsZip(Vec<u8>),
    Lzx(Box<lzxd::Lzxd>),
}

/// A folder's data, decoded one block at a time from its start.
pub(crate) struct FolderReader<'a> {
    file: &'a mut BufReader<File>,
    folder: &'a Folder,
    decoder: Decoder,
    next: usize,
    block: Vec<u8>,
    at: usize,
    pos: u64,
}

impl<'a> FolderReader<'a> {
    pub(crate) fn new(file: &'a mut BufReader<File>, folder: &'a Folder) -> Result<Self> {
        let decoder = match folder.compression {
            Compression::None => Decoder::Stored,
            Compression::MsZip => Decoder::MsZip(Vec::new()),
            Compression::Lzx(window) => Decoder::Lzx(Box::new(lzxd::Lzxd::new(window))),
            Compression::Quantum => {
                return Err(Error::Unsupported(
                    "Quantum-compressed cabinets are not supported".into(),
                ))
            }
        };
        Ok(FolderReader {
            file,
            folder,
            decoder,
            next: 0,
            block: Vec::new(),
            at: 0,
            pos: 0,
        })
    }

    pub(crate) fn into_inner(self) -> &'a mut BufReader<File> {
        self.file
    }

    pub(crate) fn position(&self) -> u64 {
        self.pos
    }

    pub(crate) fn skip_to(&mut self, offset: u64) -> Result<()> {
        let mut left = offset - self.pos;
        while left > 0 {
            if self.at == self.block.len() && !self.load()? {
                return Err(truncated());
            }
            let n = (self.block.len() - self.at).min(usize::try_from(left).unwrap_or(usize::MAX));
            self.at += n;
            self.pos += n as u64;
            left -= n as u64;
        }
        Ok(())
    }

    fn load(&mut self) -> Result<bool> {
        let Some(block) = self.folder.blocks.get(self.next) else {
            return Ok(false);
        };
        self.next += 1;
        self.file.seek(SeekFrom::Start(block.header + 8))?;
        let mut data = vec![0u8; usize::from(block.reserve) + usize::from(block.packed)];
        self.file.read_exact(&mut data).map_err(|_| truncated())?;
        if block.sum != 0 {
            let mut header = Vec::with_capacity(4 + usize::from(block.reserve));
            header.extend_from_slice(&block.packed.to_le_bytes());
            header.extend_from_slice(&block.size.to_le_bytes());
            header.extend_from_slice(&data[..usize::from(block.reserve)]);
            let found = checksum(&header, checksum(&data[usize::from(block.reserve)..], 0));
            if found != block.sum {
                return Err(Error::Format(format!(
                    "cabinet block {} fails its checksum",
                    self.next - 1
                )));
            }
        }
        let data = &data[usize::from(block.reserve)..];
        let size = usize::from(block.size);
        self.block = match &mut self.decoder {
            Decoder::Stored => data.to_vec(),
            Decoder::MsZip(window) => mszip(data, size, window)?,
            Decoder::Lzx(lzx) => lzx
                .decompress_next(data, size)
                .map_err(|e| Error::Format(format!("corrupt LZX data: {e}")))?
                .to_vec(),
        };
        if self.block.len() != size {
            return Err(Error::Format(
                "a cabinet block decodes to a size other than the one it declares".into(),
            ));
        }
        self.at = 0;
        Ok(true)
    }
}

impl Read for FolderReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.at == self.block.len() && !self.load().map_err(io::Error::other)? {
            return Ok(0);
        }
        let n = buf.len().min(self.block.len() - self.at);
        buf[..n].copy_from_slice(&self.block[self.at..self.at + n]);
        self.at += n;
        self.pos += n as u64;
        Ok(n)
    }
}

// Each MSZIP block is a deflate stream of its own that may refer back into
// the 32 KiB before it, so that window is handed over as a dictionary.
fn mszip(data: &[u8], size: usize, window: &mut Vec<u8>) -> Result<Vec<u8>> {
    let body = data
        .strip_prefix(b"CK")
        .ok_or_else(|| Error::Format("an MSZIP block lacks its CK signature".into()))?;
    let mut inflate = flate2::Decompress::new(false);
    if !window.is_empty() {
        inflate
            .set_dictionary(window)
            .map_err(|e| Error::Format(format!("corrupt MSZIP data: {e}")))?;
    }
    let mut out = Vec::with_capacity(size + 1);
    inflate
        .decompress_vec(body, &mut out, flate2::FlushDecompress::Finish)
        .map_err(|e| Error::Format(format!("corrupt MSZIP data: {e}")))?;
    if out.len() > size {
        return Err(Error::Format(
            "an MSZIP block decodes to more than it declares".into(),
        ));
    }
    window.extend_from_slice(&out);
    if window.len() > WINDOW {
        window.drain(..window.len() - WINDOW);
    }
    Ok(out)
}

// The checksum MS-CAB defines: the data as little-endian words XORed
// together, the last one to three bytes taken most significant first.
fn checksum(data: &[u8], seed: u32) -> u32 {
    let mut sum = seed;
    let (words, rest) = data.as_chunks::<4>();
    for word in words {
        sum ^= u32::from_le_bytes(*word);
    }
    let tail = rest.iter().fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
    sum ^ tail
}

#[cfg(test)]
pub(crate) fn checksum_for_tests(data: &[u8], seed: u32) -> u32 {
    checksum(data, seed)
}
