use crate::BUF;
use arca_core::{Error, Level, Result};
use flate2::bufread::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: [u8; 3] = [0x1f, 0x8b, 0x08];
const RESERVED_FLAGS: u8 = 0xe0;
const HEADER: usize = 10;

/// The ten bytes every member starts with: the magic number, Deflate as the
/// method, and flags with none of the reserved bits set.
pub(crate) fn is_magic(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER && bytes.starts_with(&MAGIC) && bytes[3] & RESERVED_FLAGS == 0
}

pub(crate) fn check_header<R: Read>(source: &mut R) -> Result<()> {
    let mut magic = [0u8; HEADER];
    let mut got = 0;
    while got < magic.len() {
        match source.read(&mut magic[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    if is_magic(&magic[..got]) {
        Ok(())
    } else if got > 0 && got < magic.len() && MAGIC.starts_with(&magic[..got.min(MAGIC.len())]) {
        Err(Error::Format("gzip: the stream is truncated".into()))
    } else {
        Err(Error::Format(
            "gzip: the stream header is missing or damaged".into(),
        ))
    }
}

fn describe(e: io::Error) -> io::Error {
    match e.kind() {
        io::ErrorKind::UnexpectedEof => {
            io::Error::new(io::ErrorKind::UnexpectedEof, "the gzip stream is truncated")
        }
        io::ErrorKind::Interrupted => e,
        kind => io::Error::new(kind, format!("corrupt gzip data: {e}")),
    }
}

/// Decodes every member, as `gzip`, `pigz` and `cat a.gz b.gz` write them,
/// checking each member's CRC-32 and length against its trailer. Anything
/// after the last member that is not another one is an error. Memory stays
/// small whatever the input: Deflate looks back at most 32 KiB, and flate2
/// refuses a header name or comment longer than 64 KiB.
pub(crate) struct Decoder<R: BufRead>(MultiGzDecoder<R>);

pub(crate) fn decoder<R: BufRead>(source: R) -> Decoder<R> {
    Decoder(MultiGzDecoder::new(source))
}

impl<R: BufRead> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(describe)
    }
}

pub(crate) fn open(path: &Path) -> Result<Box<dyn Read + Send>> {
    let mut file = arca_core::volumes::open(path)?;
    check_header(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(Box::new(decoder(BufReader::with_capacity(BUF, file))))
}

pub(crate) fn encoder<W: Write>(out: W, level: Level) -> GzEncoder<W> {
    GzEncoder::new(out, Compression::new(level.to_flate2()))
}
