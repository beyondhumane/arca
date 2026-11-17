#![forbid(unsafe_code)]
//! Single compressed streams for Arca: XZ, bzip2 and Zstandard, holding one
//! file or a TAR. This picks the codec for a format, looks inside a file to
//! tell which one it really is, and gives the one entry of a standalone stream.

mod bz2;
mod zst;

use arca_core::{Container, Entry, Error, Format, Level, Method, Result};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub use arca_xz::drain;

const BUF: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codec {
    Xz,
    Bzip2,
    Zstd,
}

fn codec(format: Format) -> Option<Codec> {
    match format.container() {
        Container::Xz | Container::TarXz => Some(Codec::Xz),
        Container::Bz2 | Container::TarBz2 => Some(Codec::Bzip2),
        Container::Zst | Container::TarZst => Some(Codec::Zstd),
        _ => None,
    }
}

impl Codec {
    fn single(self) -> Format {
        match self {
            Codec::Xz => Format::Xz,
            Codec::Bzip2 => Format::Bz2,
            Codec::Zstd => Format::Zst,
        }
    }

    fn tar(self) -> Format {
        match self {
            Codec::Xz => Format::TarXz,
            Codec::Bzip2 => Format::TarBz2,
            Codec::Zstd => Format::TarZst,
        }
    }

    fn method(self) -> Method {
        match self {
            Codec::Xz => Method::Lzma2,
            Codec::Bzip2 => Method::Bzip2,
            Codec::Zstd => Method::Zstd,
        }
    }

    fn sniff(magic: &[u8]) -> Option<Codec> {
        if magic.starts_with(&arca_xz::MAGIC) {
            Some(Codec::Xz)
        } else if bz2::is_magic(magic) {
            Some(Codec::Bzip2)
        } else if zst::is_magic(magic) {
            Some(Codec::Zstd)
        } else {
            None
        }
    }

    fn decoder(self, file: arca_core::volumes::Source) -> Result<Box<dyn Read + Send>> {
        Ok(match self {
            Codec::Xz => Box::new(arca_xz::Decoder::new(BufReader::new(file))),
            Codec::Bzip2 => Box::new(bz2::decoder(BufReader::new(file))),
            Codec::Zstd => zst::decoder(BufReader::new(file))?,
        })
    }
}

fn not_a_stream(format: Format) -> Error {
    Error::Unsupported(format!("{} is not a compressed stream", format.label()))
}

/// Whether `format` is one of the streams this crate reads and writes, on
/// its own or around a TAR.
pub fn handles(format: Format) -> bool {
    codec(format).is_some()
}

/// Opens a file for reading what it decodes to, after checking whatever the
/// format declares about itself up front: the XZ index, the bzip2 header,
/// the Zstandard frame headers. A declared size is held against the data.
pub fn open(path: &Path, format: Format) -> Result<Box<dyn Read + Send>> {
    match codec(format).ok_or_else(|| not_a_stream(format))? {
        Codec::Xz => Ok(Box::new(arca_xz::open(path)?)),
        Codec::Bzip2 => bz2::open(path),
        Codec::Zstd => zst::open(path),
    }
}

/// The standalone stream whose magic number `magic` starts with: XZ, BZ2 or
/// ZST, or `None` for anything else. Ten bytes are enough to tell.
pub fn sniff(magic: &[u8]) -> Option<Format> {
    Codec::sniff(magic).map(Codec::single)
}

/// Decodes one of these streams read from anywhere, such as a range inside
/// another file, with the checks the decoders make while they read.
pub fn decoder<R: Read + Send + 'static>(
    format: Format,
    source: R,
) -> Result<Box<dyn Read + Send>> {
    let source = BufReader::with_capacity(BUF, source);
    Ok(match codec(format).ok_or_else(|| not_a_stream(format))? {
        Codec::Xz => Box::new(arca_xz::Decoder::new(source)),
        Codec::Bzip2 => Box::new(bz2::decoder(source)),
        Codec::Zstd => zst::decoder(source)?,
    })
}

/// What a standalone stream decompresses to. None of the three keeps a name,
/// so it comes from the file: `notes.txt.bz2` gives `notes.txt`, `a.tzst`
/// gives `a.tar`, and a name without a known suffix gets `.out` appended.
pub fn output_name(path: &Path) -> String {
    let path = arca_core::volumes::whole_name(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match Format::split_name(&name) {
        Some((stem, format)) if !stem.is_empty() && format.stream_tar().is_some() => {
            stem.to_string()
        }
        Some((stem, format)) if !stem.is_empty() && handles(format) => format!("{stem}.tar"),
        _ => format!("{name}.out"),
    }
}

/// The single entry of a standalone stream. None of them stores a date. XZ
/// always records the size and Zstandard usually does; bzip2 never does, and
/// an unknown size is given as 0 rather than decoding the whole file to
/// count it.
pub fn entry(path: &Path, format: Format) -> Result<Entry> {
    let codec = codec(format).ok_or_else(|| not_a_stream(format))?;
    let mut file = arca_core::volumes::open(path)?;
    let size = match codec {
        Codec::Xz => arca_xz::uncompressed_size(&mut file)?,
        Codec::Bzip2 => {
            bz2::check_header(&mut file)?;
            0
        }
        Codec::Zstd => zst::declared_size(&mut file)?.unwrap_or(0),
    };
    let name = output_name(path);
    Ok(Entry {
        raw_name: name.as_bytes().to_vec(),
        name,
        size,
        compressed_size: file.len()?,
        method: codec.method(),
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

/// The format of a file, looking at its bytes when the name says it is one
/// of these streams or says nothing. The magic number picks the codec, and a
/// TAR header at the start of the decoded data makes it the TAR variant
/// whatever the name; without one it is a standalone stream, even if named
/// `.tar.bz2`. Any other name is trusted as it is, and a name that says
/// nothing at all is also looked at for the read-only formats of
/// [`Format::sniff`].
pub fn identify(path: &Path) -> Option<Format> {
    let named = Format::detect(path).or_else(|| split_format(path));
    if named.is_some_and(|format| !handles(format)) {
        return named;
    }
    let Ok(mut file) = arca_core::volumes::open(path) else {
        return named;
    };
    let mut magic = [0u8; 32];
    let mut got = 0;
    while got < magic.len() {
        match file.read(&mut magic[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return named,
        }
    }
    let Some(found) = Codec::sniff(&magic[..got]) else {
        return named.or_else(|| Format::sniff(&magic[..got]));
    };
    let fallback = named
        .filter(|format| codec(*format) == Some(found))
        .unwrap_or(found.single());
    if file.seek(SeekFrom::Start(0)).is_err() {
        return Some(fallback);
    }
    let Ok(mut decoder) = found.decoder(file) else {
        return Some(fallback);
    };
    let mut block = [0u8; 512];
    let mut filled = 0;
    while filled < block.len() {
        match decoder.read(&mut block[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Some(fallback),
        }
    }
    let empty_tar = named == Some(found.tar())
        && filled == block.len()
        && block.iter().all(|&b| b == 0)
        && rest_is_empty_tar(&mut decoder);
    Some(
        if filled == block.len() && (arca_tar::is_header(&block) || empty_tar) {
            found.tar()
        } else {
            found.single()
        },
    )
}

// A part of a split set is read as the archive it was cut from, named by
// what comes before the part number. RAR has volumes of its own, and an ISO
// and the formats of arca-extra are read by path, so none is joined this way.
fn split_format(path: &Path) -> Option<Format> {
    let (whole, _) = arca_core::volumes::split_part(path)?;
    Format::detect(&whole).filter(|format| {
        !matches!(
            format.container(),
            Container::Rar | Container::Iso | Container::Extra
        )
    })
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

enum Kind<W: Write> {
    Xz(arca_xz::Encoder<W>),
    Bzip2(bzip2::write::BzEncoder<W>),
    #[cfg(feature = "codecs-native")]
    Zstd(zstd::stream::write::Encoder<'static, W>),
}

/// Compresses into one of the stream formats. Nothing is complete until
/// [`Encoder::finish`] has written the end of the stream.
pub struct Encoder<W: Write> {
    kind: Kind<W>,
    workers: usize,
}

/// An encoder for `format` at `level`. `threads` is what was asked for, 0
/// meaning every core: XZ fits it in its memory budget, Zstandard uses it as
/// given and bzip2 always runs on one. `size`, when the input's length is
/// known up front, is recorded in the stream where the format has room for
/// it, so that listing does not need to decode.
pub fn encoder<W: Write>(
    out: W,
    format: Format,
    level: Level,
    threads: usize,
    size: Option<u64>,
) -> Result<Encoder<W>> {
    Ok(match codec(format).ok_or_else(|| not_a_stream(format))? {
        Codec::Xz => {
            let e = arca_xz::encoder(out, level, threads)?;
            let workers = e.workers();
            Encoder {
                kind: Kind::Xz(e),
                workers,
            }
        }
        Codec::Bzip2 => Encoder {
            kind: Kind::Bzip2(bz2::encoder(out, level)),
            workers: 1,
        },
        #[cfg(feature = "codecs-native")]
        Codec::Zstd => {
            let (e, workers) = zst::encoder(out, level, threads, size)?;
            Encoder {
                kind: Kind::Zstd(e),
                workers,
            }
        }
        #[cfg(not(feature = "codecs-native"))]
        Codec::Zstd => {
            let _ = (out, threads, size);
            return Err(zst::missing());
        }
    })
}

impl<W: Write> Encoder<W> {
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Writes the end of the stream and hands back what it was written to.
    pub fn finish(self) -> Result<W> {
        Ok(match self.kind {
            Kind::Xz(e) => e.finish()?,
            Kind::Bzip2(e) => e.finish()?,
            #[cfg(feature = "codecs-native")]
            Kind::Zstd(e) => e.finish()?,
        })
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.kind {
            Kind::Xz(e) => e.write(buf),
            Kind::Bzip2(e) => e.write(buf),
            #[cfg(feature = "codecs-native")]
            Kind::Zstd(e) => e.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.kind {
            Kind::Xz(e) => e.flush(),
            Kind::Bzip2(e) => e.flush(),
            #[cfg(feature = "codecs-native")]
            Kind::Zstd(e) => e.flush(),
        }
    }
}

#[cfg(test)]
mod tests;
