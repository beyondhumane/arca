use crate::BUF;
use arca_core::{Error, Level, Result};
use bzip2::bufread::MultiBzDecoder;
use bzip2::write::BzEncoder;
use bzip2::Compression;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

const BLOCK: [u8; 6] = [0x31, 0x41, 0x59, 0x26, 0x53, 0x59];
const END: [u8; 6] = [0x17, 0x72, 0x45, 0x38, 0x50, 0x90];

/// `BZh`, a block size digit, then either the first block or, for an empty
/// stream, the end-of-stream marker.
pub(crate) fn is_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 10
        && bytes.starts_with(b"BZh")
        && (b'1'..=b'9').contains(&bytes[3])
        && (bytes[4..10] == BLOCK || bytes[4..10] == END)
}

pub(crate) fn check_header<R: Read>(source: &mut R) -> Result<()> {
    let mut magic = [0u8; 10];
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
    } else if got < magic.len() && b"BZh".starts_with(&magic[..got.min(3)]) {
        Err(Error::Format("bzip2: the stream is truncated".into()))
    } else {
        Err(Error::Format(
            "bzip2: the stream header is missing or damaged".into(),
        ))
    }
}

fn describe(e: io::Error) -> io::Error {
    match e.kind() {
        io::ErrorKind::UnexpectedEof => io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the bzip2 stream is truncated",
        ),
        io::ErrorKind::Interrupted => e,
        kind => io::Error::new(kind, format!("corrupt bzip2 data: {e}")),
    }
}

/// Decodes every concatenated stream, as `bunzip2` and `pbzip2` write them,
/// checking each block's CRC and each stream's. Anything after the last
/// stream that is not another one is an error. Memory stays under 4 MiB
/// whatever the input: bzip2 blocks are at most 900 kB.
pub(crate) struct Decoder<R: BufRead>(MultiBzDecoder<R>);

pub(crate) fn decoder<R: BufRead>(source: R) -> Decoder<R> {
    Decoder(MultiBzDecoder::new(source))
}

impl<R: BufRead> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(describe)
    }
}

pub(crate) fn open(path: &Path) -> Result<Box<dyn Read + Send>> {
    let mut file = File::open(path)?;
    check_header(&mut file)?;
    file.seek(SeekFrom::Start(0))?;
    Ok(Box::new(decoder(BufReader::with_capacity(BUF, file))))
}

/// bzip2 has nothing below its 100 kB blocks, so Store is level 1.
pub(crate) fn encoder<W: Write>(out: W, level: Level) -> BzEncoder<W> {
    BzEncoder::new(out, Compression::new(u32::from(level.get().clamp(1, 9))))
}
