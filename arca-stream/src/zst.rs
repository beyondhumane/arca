use arca_core::{Error, Result};
use std::io::{self, Read, Seek, SeekFrom};

const MAGIC: u32 = 0xFD2F_B528;
const SKIPPABLE: u32 = 0x184D_2A50;
const MAX_BLOCK: u64 = 128 << 10;

fn corrupt(what: &str) -> Error {
    Error::Format(format!("Zstandard: {what}"))
}

pub(crate) fn is_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && {
        let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        word == MAGIC || word & 0xFFFF_FFF0 == SKIPPABLE
    }
}

fn read_at<R: Read + Seek>(source: &mut R, at: u64, buf: &mut [u8]) -> Result<()> {
    source.seek(SeekFrom::Start(at))?;
    source.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => corrupt("the stream is truncated"),
        _ => Error::Io(e),
    })
}

fn le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0u64, |value, &b| (value << 8) | u64::from(b))
}

/// Walks the frames without decoding them, reading only frame and block
/// headers, and returns the sum of the sizes the frames declare. `None` when
/// some frame does not declare one, as a stream written to a pipe does not.
/// Anything that is not a frame, a block running past the end of the file, or
/// a file with no frame at all is an error.
pub(crate) fn declared_size<R: Read + Seek>(source: &mut R) -> Result<Option<u64>> {
    let end = source.seek(SeekFrom::End(0))?;
    let mut pos = 0u64;
    let mut total = Some(0u64);
    let mut frames = 0u64;
    let past = |at: u64, by: u64| {
        at.checked_add(by)
            .filter(|&next| next <= end)
            .ok_or_else(|| corrupt("the stream is truncated"))
    };
    while pos < end {
        let mut word = [0u8; 4];
        read_at(source, pos, &mut word)?;
        let magic = u32::from_le_bytes(word);
        if magic & 0xFFFF_FFF0 == SKIPPABLE {
            read_at(source, pos + 4, &mut word)?;
            pos = past(pos, 8 + u64::from(u32::from_le_bytes(word)))?;
            continue;
        }
        if magic != MAGIC {
            return Err(corrupt(if frames == 0 {
                "the frame header is missing or damaged"
            } else {
                "there is data after the last frame that is not a frame"
            }));
        }
        frames += 1;
        let mut descriptor = [0u8; 1];
        read_at(source, pos + 4, &mut descriptor)?;
        let d = descriptor[0];
        if d & 0x08 != 0 {
            return Err(corrupt("a frame header sets a reserved bit"));
        }
        let single_segment = d & 0x20 != 0;
        let window = usize::from(!single_segment);
        let dict = [0, 1, 2, 4][usize::from(d & 3)];
        let fcs = match d >> 6 {
            0 => usize::from(single_segment),
            1 => 2,
            2 => 4,
            _ => 8,
        };
        let mut header = [0u8; 14];
        let header = &mut header[..1 + window + dict + fcs];
        read_at(source, pos + 4, header)?;
        total = match fcs {
            0 => None,
            _ => {
                let at = 1 + window + dict;
                let size = le(&header[at..]) + if fcs == 2 { 256 } else { 0 };
                match total {
                    Some(sum) => Some(
                        sum.checked_add(size)
                            .ok_or_else(|| corrupt("the frame sizes overflow"))?,
                    ),
                    None => None,
                }
            }
        };
        pos = past(pos, 4 + header.len() as u64)?;
        loop {
            let mut block = [0u8; 3];
            read_at(source, pos, &mut block)?;
            let block = le(&block);
            let size = block >> 3;
            let body = match (block >> 1) & 3 {
                1 => 1,
                3 => return Err(corrupt("a block has the reserved type")),
                _ => size,
            };
            if size > MAX_BLOCK {
                return Err(corrupt("a block is larger than 128 KiB"));
            }
            pos = past(pos, 3 + body)?;
            if block & 1 != 0 {
                break;
            }
        }
        if d & 0x04 != 0 {
            pos = past(pos, 4)?;
        }
    }
    if frames == 0 {
        return Err(corrupt("there is no frame in the file"));
    }
    Ok(total)
}

#[cfg(feature = "codecs-native")]
mod native {
    use super::declared_size;
    use crate::BUF;
    use arca_core::{Level, Result};
    use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
    use std::path::Path;

    /// The largest window the decoder accepts, as a power of two: 1 GiB, the
    /// same ceiling as the XZ dictionary. zstd itself stops at 128 MiB unless
    /// told otherwise, and only `--long` or `--ultra` frames ask for more.
    pub(crate) const WINDOW_LOG_MAX: u32 = 30;

    fn describe(e: io::Error) -> io::Error {
        match e.kind() {
            io::ErrorKind::UnexpectedEof => io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the Zstandard stream is truncated",
            ),
            io::ErrorKind::Interrupted => e,
            _ if e.to_string().contains("memory") => io::Error::new(
                io::ErrorKind::OutOfMemory,
                format!(
                    "the Zstandard stream needs a window over Arca's {} MiB limit",
                    1u64 << (WINDOW_LOG_MAX - 20)
                ),
            ),
            kind => io::Error::new(kind, format!("corrupt Zstandard data: {e}")),
        }
    }

    /// Decodes every frame in turn, skipping skippable ones, with zstd checking
    /// each frame's checksum when it has one and its declared size when it
    /// has one. `expected` holds the sum of those sizes against the output.
    struct Decoder<R: BufRead> {
        inner: zstd::stream::read::Decoder<'static, R>,
        expected: Option<u64>,
        produced: u64,
    }

    impl<R: BufRead> Read for Decoder<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.inner.read(buf).map_err(describe)?;
            self.produced += n as u64;
            if let Some(expected) = self.expected {
                if self.produced > expected
                    || (n == 0 && !buf.is_empty() && self.produced != expected)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "corrupt Zstandard data: the frames declare {expected} bytes, they hold {}{}",
                            if self.produced > expected { "more than " } else { "" },
                            self.produced
                        ),
                    ));
                }
            }
            Ok(n)
        }
    }

    fn decoder_with<R: BufRead + Send + 'static>(
        source: R,
        expected: Option<u64>,
    ) -> Result<Box<dyn Read + Send>> {
        let mut inner = zstd::stream::read::Decoder::with_buffer(source)?;
        inner.window_log_max(WINDOW_LOG_MAX)?;
        Ok(Box::new(Decoder {
            inner,
            expected,
            produced: 0,
        }))
    }

    pub(crate) fn decoder<R: BufRead + Send + 'static>(source: R) -> Result<Box<dyn Read + Send>> {
        decoder_with(source, None)
    }

    pub(crate) fn open(path: &Path) -> Result<Box<dyn Read + Send>> {
        let mut file = arca_core::volumes::open(path)?;
        let size = declared_size(&mut file)?;
        file.seek(SeekFrom::Start(0))?;
        decoder_with(BufReader::with_capacity(BUF, file), size)
    }

    /// Threads as asked, 0 meaning every core. One means zstd compresses on
    /// the calling thread; more run as zstd's own workers.
    pub(crate) fn workers(requested: usize) -> usize {
        let requested = if requested == 0 {
            std::thread::available_parallelism().map_or(1, |n| n.get())
        } else {
            requested
        };
        requested.min(200)
    }

    pub(crate) fn encoder<W: Write>(
        out: W,
        level: Level,
        threads: usize,
        size: Option<u64>,
    ) -> Result<(zstd::stream::write::Encoder<'static, W>, usize)> {
        let workers = workers(threads);
        let mut e = zstd::stream::write::Encoder::new(out, level.to_zstd())?;
        e.include_checksum(true)?;
        if workers > 1 {
            e.multithread(workers as u32)?;
        }
        e.set_pledged_src_size(size)?;
        Ok((e, workers))
    }
}

#[cfg(feature = "codecs-native")]
pub(crate) use native::{decoder, encoder, open};

#[cfg(not(feature = "codecs-native"))]
pub(crate) fn missing() -> Error {
    Error::Unsupported("this binary was built without Zstandard".into())
}

#[cfg(not(feature = "codecs-native"))]
pub(crate) fn decoder<R>(_: R) -> Result<Box<dyn Read + Send>> {
    Err(missing())
}

#[cfg(not(feature = "codecs-native"))]
pub(crate) fn open(_: &std::path::Path) -> Result<Box<dyn Read + Send>> {
    Err(missing())
}
