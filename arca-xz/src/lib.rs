#![forbid(unsafe_code)]
//! XZ streams for Arca: a decoder with a dictionary limit, an encoder whose
//! threads fit in a memory budget, the index walk that gives the size without
//! decoding, and the look inside that tells a TAR.XZ from a plain `.xz`.

use arca_core::{Entry, Error, Format, Level, Method, Result};
use lzma_rust2::{LzmaOptions, XzOptions, XzReader, XzWriter, XzWriterMt};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;
use std::path::Path;

pub const MAGIC: [u8; 6] = [0xFD, b'7', b'z', b'X', b'Z', 0];
/// Largest LZMA2 dictionary the decoder accepts. `xz -9` needs 64 MiB.
pub const MEMORY_LIMIT_MIB: u32 = 1024;
/// What the compression threads may use between them.
pub const ENCODER_BUDGET_MIB: u64 = 2048;
const MAX_INDEX: u64 = 64 << 20;
const HEADER: u64 = 12;
const BUF: usize = 256 * 1024;

fn corrupt(what: &str) -> Error {
    Error::Format(format!("XZ: {what}"))
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn read_at<R: Read + Seek>(source: &mut R, at: u64, buf: &mut [u8]) -> Result<()> {
    source.seek(SeekFrom::Start(at))?;
    source.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => corrupt("the stream is truncated"),
        _ => Error::Io(e),
    })
}

fn varint(bytes: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for i in 0..9 {
        let byte = *bytes
            .get(*pos)
            .ok_or_else(|| corrupt("the index is truncated"))?;
        *pos += 1;
        if i > 0 && byte == 0 {
            return Err(corrupt("the index has a padded integer"));
        }
        value |= u64::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(corrupt("the index has an integer that is too long"))
}

/// Returns the bytes the blocks take on disk and the bytes they decode to.
fn parse_index(index: &[u8]) -> Result<(u64, u64)> {
    if index.len() < 8 || !index.len().is_multiple_of(4) || index[0] != 0 {
        return Err(corrupt("the index is missing or damaged"));
    }
    let (body, crc) = index.split_at(index.len() - 4);
    if crc32fast::hash(body) != le32(crc) {
        return Err(corrupt("the index checksum does not match"));
    }
    let mut pos = 1;
    let records = varint(body, &mut pos)?;
    if records > body.len() as u64 / 2 {
        return Err(corrupt("the index lists more blocks than it has room for"));
    }
    let (mut packed, mut unpacked) = (0u64, 0u64);
    for _ in 0..records {
        let unpadded = varint(body, &mut pos)?;
        let size = varint(body, &mut pos)?;
        if unpadded < 5 {
            return Err(corrupt("the index lists an impossible block"));
        }
        packed = unpadded
            .checked_add(3)
            .map(|v| v & !3)
            .and_then(|v| packed.checked_add(v))
            .ok_or_else(|| corrupt("the index sizes overflow"))?;
        unpacked = unpacked
            .checked_add(size)
            .ok_or_else(|| corrupt("the index sizes overflow"))?;
    }
    if body.len() - pos >= 4 || body[pos..].iter().any(|&b| b != 0) {
        return Err(corrupt("the index padding is damaged"));
    }
    Ok((packed, unpacked))
}

/// The decoded size every index declares, walking the streams from the end.
///
/// Only headers, footers and indexes are read, so this is cheap even for a
/// large file. The decoder checks the same figure against the real data.
pub fn uncompressed_size<R: Read + Seek>(source: &mut R) -> Result<u64> {
    let mut end = source.seek(SeekFrom::End(0))?;
    if !end.is_multiple_of(4) {
        return Err(corrupt(
            "the file size is not a multiple of four, so it is truncated or has trailing data",
        ));
    }
    let mut total = 0u64;
    let mut streams = 0u32;
    loop {
        let mut word = [0u8; 4];
        while end >= 4 {
            read_at(source, end - 4, &mut word)?;
            if word != [0; 4] {
                break;
            }
            end -= 4;
        }
        if streams > 0 && end == 0 {
            return Ok(total);
        }
        if end < 2 * HEADER {
            return Err(corrupt("the stream is truncated"));
        }
        let mut footer = [0u8; 12];
        read_at(source, end - HEADER, &mut footer)?;
        if &footer[10..] != b"YZ" {
            return Err(corrupt(
                "the stream footer is missing; the file is truncated or has trailing data",
            ));
        }
        if crc32fast::hash(&footer[4..10]) != le32(&footer[..4]) {
            return Err(corrupt("the stream footer checksum does not match"));
        }
        let backward = (u64::from(le32(&footer[4..8])) + 1) * 4;
        if backward > MAX_INDEX {
            return Err(Error::Limit(format!(
                "XZ index of {backward} bytes is larger than the {} MiB Arca reads",
                MAX_INDEX >> 20
            )));
        }
        let index_start = (end - HEADER)
            .checked_sub(backward)
            .ok_or_else(|| corrupt("the index points before the start of the file"))?;
        let mut index = vec![0u8; backward as usize];
        read_at(source, index_start, &mut index)?;
        let (packed, unpacked) = parse_index(&index)?;
        let start = index_start
            .checked_sub(packed)
            .and_then(|v| v.checked_sub(HEADER))
            .ok_or_else(|| corrupt("the index points before the start of the file"))?;
        let mut header = [0u8; 12];
        read_at(source, start, &mut header)?;
        if header[..6] != MAGIC {
            return Err(corrupt("the stream header is missing or damaged"));
        }
        if crc32fast::hash(&header[6..8]) != le32(&header[8..]) {
            return Err(corrupt("the stream header checksum does not match"));
        }
        if header[6..8] != footer[8..10] {
            return Err(corrupt("the stream header and footer disagree"));
        }
        total = total
            .checked_add(unpacked)
            .ok_or_else(|| corrupt("the index sizes overflow"))?;
        streams += 1;
        end = start;
        if end == 0 {
            return Ok(total);
        }
    }
}

fn describe(e: io::Error) -> io::Error {
    match e.kind() {
        io::ErrorKind::OutOfMemory => io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("the XZ stream needs a dictionary over Arca's {MEMORY_LIMIT_MIB} MiB limit"),
        ),
        io::ErrorKind::UnexpectedEof => {
            io::Error::new(io::ErrorKind::UnexpectedEof, "the XZ stream is truncated")
        }
        io::ErrorKind::Unsupported => io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported XZ feature: {e}"),
        ),
        io::ErrorKind::Interrupted => e,
        kind => io::Error::new(kind, format!("corrupt XZ data: {e}")),
    }
}

/// Decompresses every concatenated stream, checking each block's CRC32,
/// CRC64 or SHA-256 and, when the size is known, the index against the data.
pub struct Decoder<R: Read> {
    inner: XzReader<R>,
    expected: Option<u64>,
    produced: u64,
}

impl<R: Read> Decoder<R> {
    pub fn new(source: R) -> Self {
        Self {
            inner: XzReader::new_mem_limit(source, true, MEMORY_LIMIT_MIB * 1024),
            expected: None,
            produced: 0,
        }
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf).map_err(describe)?;
        self.produced += n as u64;
        if let Some(expected) = self.expected {
            if self.produced > expected || (n == 0 && !buf.is_empty() && self.produced != expected)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "corrupt XZ data: the index declares {expected} bytes, the blocks hold {}{}",
                        if self.produced > expected { "more than " } else { "" },
                        self.produced
                    ),
                ));
            }
        }
        Ok(n)
    }
}

/// Opens a file for decoding, after reading its indexes.
pub fn open(path: &Path) -> Result<Decoder<BufReader<File>>> {
    let mut file = File::open(path)?;
    let size = uncompressed_size(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    let mut decoder = Decoder::new(BufReader::with_capacity(BUF, file));
    decoder.expected = Some(size);
    Ok(decoder)
}

/// What a standalone `.xz` decompresses to. XZ keeps no name, so it comes from
/// the file: `notes.txt.xz` gives `notes.txt`, `a.txz` gives `a.tar`, and a name
/// without either suffix gets `.out` appended.
pub fn output_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match Format::split_name(&name) {
        Some((stem, Format::Xz)) if !stem.is_empty() => stem.to_string(),
        Some((stem, Format::TarXz)) if !stem.is_empty() => format!("{stem}.tar"),
        _ => format!("{name}.out"),
    }
}

/// The single entry of a standalone `.xz`. XZ stores no date, so there is none.
pub fn entry(path: &Path) -> Result<Entry> {
    let mut file = File::open(path)?;
    let size = uncompressed_size(&mut file)?;
    let name = output_name(path);
    Ok(Entry {
        raw_name: name.as_bytes().to_vec(),
        name,
        size,
        compressed_size: file.metadata()?.len(),
        method: Method::Lzma2,
        crc32: None,
        is_dir: false,
        mtime: None,
        created: None,
        accessed: None,
        attributes: 0,
        offset: 0,
        utf8: true,
        encrypted: false,
        zipcrypto: false,
    })
}

/// The format of a file, looking at its bytes when the name says XZ or says
/// nothing. A TAR header at the start of the decoded data makes it TAR.XZ
/// whatever the name; without one it is a standalone `.xz`, even if named
/// `.tar.xz`. Any other name is trusted as it is.
pub fn identify(path: &Path) -> Option<Format> {
    let named = Format::detect(path);
    if !matches!(named, None | Some(Format::Xz | Format::TarXz)) {
        return named;
    }
    let Ok(mut file) = File::open(path) else {
        return named;
    };
    let mut magic = [0u8; 6];
    if file.read_exact(&mut magic).is_err() || magic != MAGIC {
        return named;
    }
    if file.seek(SeekFrom::Start(0)).is_err() {
        return named.or(Some(Format::Xz));
    }
    let mut block = [0u8; 512];
    let mut filled = 0;
    let mut decoder = Decoder::new(BufReader::new(file));
    while filled < block.len() {
        match decoder.read(&mut block[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return named.or(Some(Format::Xz)),
        }
    }
    let empty_tar = named == Some(Format::TarXz)
        && filled == block.len()
        && block.iter().all(|&b| b == 0)
        && rest_is_empty_tar(&mut decoder);
    Some(
        if filled == block.len() && (arca_tar::is_header(&block) || empty_tar) {
            Format::TarXz
        } else {
            Format::Xz
        },
    )
}

// An empty tar is only its end marker: at least two zero blocks, padded with
// zero blocks up to one 10240-byte record. Any other run of zeros is a file.
const EMPTY_TAR_MAX: usize = 20 * 512;

fn rest_is_empty_tar(decoder: &mut impl Read) -> bool {
    let mut rest = Vec::new();
    let limit = (EMPTY_TAR_MAX - 512 + 1) as u64;
    if decoder.take(limit).read_to_end(&mut rest).is_err() {
        return false;
    }
    let total = 512 + rest.len();
    (1024..=EMPTY_TAR_MAX).contains(&total)
        && total.is_multiple_of(512)
        && rest.iter().all(|&b| b == 0)
}

/// The xz preset for an Arca level. Store is preset 0: XZ has no stored mode.
pub fn preset(level: Level) -> u32 {
    match level {
        Level::Store => 0,
        Level::Fast => 1,
        Level::Normal => 6,
        Level::Best => 9,
    }
}

fn dictionary(preset: u32) -> u64 {
    u64::from(LzmaOptions::with_preset(preset).dict_size)
}

fn block_size(preset: u32) -> u64 {
    (3 * dictionary(preset)).max(1 << 20)
}

// Encoder memory per preset as xz(1) documents it, plus the input block and
// its compressed copy that each thread holds.
fn worker_mib(preset: u32) -> u64 {
    const ENCODER_MIB: [u64; 10] = [3, 9, 17, 32, 48, 94, 94, 186, 370, 674];
    ENCODER_MIB[preset.min(9) as usize] + 2 * (block_size(preset) >> 20)
}

/// Threads the encoder uses for `requested` (0 means every core), capped so
/// that together they stay within [`ENCODER_BUDGET_MIB`].
pub fn workers(level: Level, requested: usize) -> usize {
    let requested = if requested == 0 {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    } else {
        requested
    };
    let fit = ENCODER_BUDGET_MIB / worker_mib(preset(level));
    (fit.max(1) as usize).min(requested).min(256)
}

enum Kind<W: Write> {
    Single(Box<XzWriter<W>>),
    Parallel {
        writer: Box<XzWriterMt<W>>,
        pending: u64,
        window: u64,
    },
}

/// One thread writes a single block, as `xz -T1` does. More threads split the
/// input into blocks of three dictionaries and compress them side by side,
/// waiting for each round to finish so memory stays bounded.
pub struct Encoder<W: Write> {
    kind: Kind<W>,
    workers: usize,
}

pub fn encoder<W: Write>(out: W, level: Level, threads: usize) -> Result<Encoder<W>> {
    let preset = preset(level);
    let mut options = XzOptions::with_preset(preset);
    let workers = workers(level, threads);
    let kind = if workers == 1 {
        Kind::Single(Box::new(XzWriter::new(out, options)?))
    } else {
        let block = block_size(preset);
        options.set_block_size(NonZeroU64::new(block));
        Kind::Parallel {
            writer: Box::new(XzWriterMt::new(out, options, workers as u32)?),
            pending: 0,
            window: block * workers as u64,
        }
    };
    Ok(Encoder { kind, workers })
}

impl<W: Write> Encoder<W> {
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Writes the index and footer. Without this the stream is incomplete.
    pub fn finish(self) -> Result<W> {
        Ok(match self.kind {
            Kind::Single(w) => w.finish()?,
            Kind::Parallel { writer, .. } => writer.finish()?,
        })
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.kind {
            Kind::Single(w) => w.write(buf),
            Kind::Parallel {
                writer,
                pending,
                window,
            } => {
                let n = writer.write(buf)?;
                *pending += n as u64;
                if *pending >= *window {
                    writer.flush()?;
                    *pending = 0;
                }
                Ok(n)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.kind {
            Kind::Single(w) => w.flush(),
            Kind::Parallel {
                writer, pending, ..
            } => {
                *pending = 0;
                writer.flush()
            }
        }
    }
}

/// Reads what is left of a stream so its last checks and index are verified.
pub fn drain<R: Read>(source: &mut R) -> Result<u64> {
    Ok(io::copy(source, &mut io::sink())?)
}

#[cfg(test)]
mod tests;
