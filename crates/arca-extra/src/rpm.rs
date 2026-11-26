//! RPM packages: a lead, a signature header, the main header and a CPIO
//! payload, usually compressed. The listing is the payload.

use crate::stream::{self, Layout, Spec};
use crate::{cpio, file_len, Data, Parsed, BUF};
use arca_core::{Error, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const LEAD: u64 = 96;
const LEAD_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];
const HEADER_MAGIC: [u8; 4] = [0x8e, 0xad, 0xe8, 0x01];
// rpm's own ceilings for one header: 64 Ki tags and 256 MiB of data.
const MAX_TAGS: u64 = 0xffff;
const MAX_DATA: u64 = 0x0fff_ffff;
const PAYLOAD_FORMAT: u32 = 1124;
const STRING: u32 = 6;

struct Header {
    tags: u64,
    data: u64,
}

fn header(file: &mut BufReader<File>, at: u64, len: u64) -> Result<Header> {
    if len.saturating_sub(at) < 16 {
        return Err(truncated());
    }
    let mut head = [0u8; 16];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut head)?;
    if head[..4] != HEADER_MAGIC {
        return Err(Error::Format("an RPM header is missing or damaged".into()));
    }
    let word = |i: usize| {
        u64::from(u32::from_be_bytes([
            head[i],
            head[i + 1],
            head[i + 2],
            head[i + 3],
        ]))
    };
    let (tags, data) = (word(8), word(12));
    if tags > MAX_TAGS || data > MAX_DATA {
        return Err(Error::Limit("an RPM header larger than rpm allows".into()));
    }
    if 16 + tags * 16 + data > len - at {
        return Err(truncated());
    }
    Ok(Header { tags, data })
}

impl Header {
    fn size(&self) -> u64 {
        16 + self.tags * 16 + self.data
    }
}

fn payload_format(file: &mut BufReader<File>, at: u64, h: &Header) -> Result<Option<String>> {
    let mut index = vec![0u8; (h.tags * 16) as usize];
    file.seek(SeekFrom::Start(at + 16))?;
    file.read_exact(&mut index)?;
    for tag in index.as_chunks::<16>().0 {
        let word = |i: usize| u32::from_be_bytes([tag[i], tag[i + 1], tag[i + 2], tag[i + 3]]);
        if word(0) != PAYLOAD_FORMAT || word(4) != STRING {
            continue;
        }
        let offset = u64::from(word(8));
        if offset >= h.data {
            return Err(Error::Format("an RPM tag points outside its header".into()));
        }
        let mut value = Vec::new();
        file.seek(SeekFrom::Start(at + 16 + h.tags * 16 + offset))?;
        (&mut *file)
            .take((h.data - offset).min(64))
            .read_to_end(&mut value)?;
        let end = value.iter().position(|&b| b == 0).unwrap_or(value.len());
        return Ok(Some(String::from_utf8_lossy(&value[..end]).into_owned()));
    }
    Ok(None)
}

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut file = BufReader::with_capacity(BUF, file);
    let mut lead = [0u8; LEAD as usize];
    if file.read_exact(&mut lead).is_err() || lead[..4] != LEAD_MAGIC {
        return Err(Error::Format("not an RPM package".into()));
    }
    if !matches!(lead[4], 3 | 4) {
        return Err(Error::Unsupported(format!(
            "RPM format version {}",
            lead[4]
        )));
    }
    let signature = header(&mut file, LEAD, len)?;
    let main_at = LEAD + signature.size().div_ceil(8) * 8;
    let main = header(&mut file, main_at, len)?;
    if let Some(format) = payload_format(&mut file, main_at, &main)? {
        if format != "cpio" {
            return Err(Error::Unsupported(format!(
                "an RPM payload in {format} format"
            )));
        }
    }
    let start = main_at + main.size();
    let magic = stream::magic(&mut file, start, len - start)?;
    let spec = Spec {
        start,
        len: len - start,
        codec: stream::sniff(&magic)?,
        layout: Layout::Cpio,
    };
    let mut reader = cpio::Reader::new(stream::open(path, &spec)?);
    let (members, mut notices) = cpio::list(
        &mut reader,
        None,
        &|ordinal, _, _| Data::Stream { stream: 0, ordinal },
        carry_on,
    )?;
    notices.push("the package's signatures and header digests are not checked".into());
    Ok(Parsed {
        members,
        notices,
        streams: vec![spec],
        cab: None,
    })
}

fn truncated() -> Error {
    Error::Format("the RPM package is truncated".into())
}
