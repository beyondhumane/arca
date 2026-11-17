//! LHA/LZH archives through `delharc`: header levels 0 to 3, the lh0-lh7
//! methods and the LArc and PMarc ones. Every file's CRC-16 is checked.

use crate::{check_count, check_name, clean, entry, file_len, Data, Kind, Member, Parsed, BUF};
use arca_core::{Error, Method, Result};
use delharc::{CompressionMethod, LhaDecodeReader, LhaHeader};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

struct Counted<'a> {
    inner: &'a mut BufReader<File>,
    read: u64,
}

impl Read for Counted<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

fn mtime(header: &LhaHeader) -> Option<i64> {
    let parsed = header.parse_last_modified();
    if parsed.is_naive() {
        let raw = header.last_modified;
        arca_core::dos_to_unix((raw >> 16) as u16, raw as u16)
    } else {
        parsed.to_utc().map(|t| t.timestamp())
    }
}

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut file = BufReader::with_capacity(BUF, file);
    let mut members = Vec::new();
    let mut pos = 0u64;
    while pos < len {
        let at = pos;
        file.seek(SeekFrom::Start(at))?;
        let mut counted = Counted {
            inner: &mut file,
            read: 0,
        };
        let header = match LhaHeader::read(&mut counted) {
            Ok(Some(header)) => header,
            Ok(None) => break,
            Err(e) => return Err(Error::Format(format!("an LZH header is damaged: {e}"))),
        };
        let data = at + counted.read;
        let end = data
            .checked_add(header.compressed_size)
            .filter(|&end| end <= len)
            .ok_or_else(|| Error::Format("an LZH entry runs past the end of the archive".into()))?;
        let path = header.parse_pathname_to_str();
        check_name(path.as_bytes())?;
        let method = header.compression_method().ok();
        let unix = header.parse_unix_permissions().map(|p| u32::from(p.bits()));
        let (shown, kind) = match path.split_once('|') {
            Some((name, target)) if method == Some(CompressionMethod::Lhd) => {
                (clean(name), Kind::Symlink(target.to_string()))
            }
            _ if header.is_directory() => (clean(&path), Kind::Dir),
            _ => (clean(&path), Kind::File),
        };
        if !carry_on(&shown) {
            return Err(Error::Cancelled);
        }
        pos = end;
        if shown.is_empty() {
            continue;
        }
        check_count(&members)?;
        let size = if kind == Kind::File {
            header.original_size
        } else {
            0
        };
        let mut e = entry(shown, path.into_bytes(), size, mtime(&header));
        e.is_dir = kind == Kind::Dir;
        e.compressed_size = if kind == Kind::File {
            header.compressed_size
        } else {
            0
        };
        e.method = match method {
            Some(CompressionMethod::Lh0 | CompressionMethod::Lz4 | CompressionMethod::Pm0) => {
                Method::Store
            }
            _ => Method::Lzh,
        };
        e.attributes = (header.msdos_attrs.bits() & 0x27) as u8;
        members.push(Member {
            entry: e,
            data: if kind == Kind::File {
                Data::Lzh { header: at }
            } else {
                Data::None
            },
            kind,
            mode: unix.map_or(0, |mode| mode & 0o7777),
            sum: None,
        });
    }
    Ok(Parsed {
        members,
        ..Parsed::default()
    })
}

/// One file's data, decoded from its header on, with its CRC-16 checked
/// once the last byte is read.
pub(crate) struct Decoded<'a> {
    inner: LhaDecodeReader<&'a mut BufReader<File>>,
    left: u64,
    checked: bool,
}

pub(crate) fn decoder<'a>(file: &'a mut BufReader<File>, name: &str) -> Result<Decoded<'a>> {
    let inner = LhaDecodeReader::new(file)
        .map_err(|e| Error::Format(format!("the LZH header of '{name}' is damaged: {e}")))?;
    if !inner.is_decoder_supported() {
        return Err(Error::Unsupported(format!(
            "'{name}' is compressed with {}, which Arca does not read",
            String::from_utf8_lossy(&inner.header().compression)
        )));
    }
    Ok(Decoded {
        left: inner.header().original_size,
        inner,
        checked: false,
    })
}

impl Read for Decoded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            if self.left == 0 && !self.checked {
                self.checked = true;
                self.inner.crc_check().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "an LZH file fails its CRC-16")
                })?;
            }
            return Ok(0);
        }
        let want = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "an LZH file is truncated",
            ));
        }
        self.left -= n as u64;
        Ok(n)
    }
}
