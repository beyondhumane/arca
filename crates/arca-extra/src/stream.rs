//! The compressed tarballs of a DEB and the compressed CPIO payload of an
//! RPM: a range of the file, decoded on the way.

use crate::{Range, BUF};
use arca_core::{Error, Format, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    None,
    Gzip,
    Stream(Format),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    Tar,
    Cpio,
}

#[derive(Clone, Debug)]
pub(crate) struct Spec {
    pub start: u64,
    pub len: u64,
    pub codec: Codec,
    pub layout: Layout,
}

/// Tells the codec from the first bytes of the data, whatever the name or
/// the package header claims.
pub(crate) fn sniff(magic: &[u8]) -> Result<Codec> {
    if magic.starts_with(&[0x1f, 0x8b]) {
        Ok(Codec::Gzip)
    } else if let Some(format) = arca_stream::sniff(magic) {
        Ok(Codec::Stream(format))
    } else if magic.starts_with(&[0x5d, 0, 0]) {
        Err(Error::Unsupported(
            "the contents are compressed with legacy LZMA, which Arca does not read".into(),
        ))
    } else {
        Ok(Codec::None)
    }
}

/// The first bytes of a range, at most ten, for [`sniff`].
pub(crate) fn magic(file: &mut (impl Read + Seek), start: u64, len: u64) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(start))?;
    let mut magic = Vec::with_capacity(10);
    file.take(len.min(10)).read_to_end(&mut magic)?;
    Ok(magic)
}

pub(crate) fn open(path: &Path, spec: &Spec) -> Result<Box<dyn Read + Send>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(spec.start))?;
    let raw = Range {
        inner: BufReader::with_capacity(BUF, file),
        left: spec.len,
    };
    Ok(match spec.codec {
        Codec::None => Box::new(raw),
        Codec::Gzip => Box::new(flate2::read::MultiGzDecoder::new(raw)),
        Codec::Stream(format) => arca_stream::decoder(format, raw)?,
    })
}
