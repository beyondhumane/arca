//! Unix `ar` archives with GNU (`//` name table) and BSD (`#1/`) long names.
//! Symbol tables are not files and are not listed.

use crate::{check_count, check_name, entry, file_len, Data, Kind, Member, Parsed, BUF};
use arca_core::{Error, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: &[u8; 8] = b"!<arch>\n";
const THIN: &[u8; 8] = b"!<thin>\n";
const HEADER: u64 = 60;
const MAX_TABLE: u64 = 16 * 1024 * 1024;

pub(crate) struct Raw {
    pub name: Vec<u8>,
    pub start: u64,
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
}

pub(crate) fn members(
    file: &mut BufReader<File>,
    len: u64,
    carry_on: &dyn Fn(&str) -> bool,
) -> Result<Vec<Raw>> {
    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut magic)
        .map_err(|_| Error::Format("not an ar archive".into()))?;
    if &magic == THIN {
        return Err(Error::Unsupported(
            "thin ar archives hold only references to other files".into(),
        ));
    }
    if &magic != MAGIC {
        return Err(Error::Format("not an ar archive".into()));
    }
    let mut pos = MAGIC.len() as u64;
    let mut table: Vec<u8> = Vec::new();
    let mut out: Vec<Raw> = Vec::new();
    while pos < len {
        if len - pos < HEADER {
            return Err(truncated());
        }
        let mut header = [0u8; HEADER as usize];
        file.seek(SeekFrom::Start(pos))?;
        file.read_exact(&mut header)?;
        if &header[58..60] != b"`\n" {
            return Err(Error::Format("an ar member header is damaged".into()));
        }
        let size = decimal(&header[48..58])?;
        let mut start = pos + HEADER;
        if size > len - start {
            return Err(truncated());
        }
        let mut data_size = size;
        let field = trim(&header[..16]);
        pos = start + size + (size & 1);
        let name: Vec<u8> = match field {
            b"/" | b"/SYM64/" => continue,
            b"//" => {
                if size > MAX_TABLE {
                    return Err(Error::Limit("an ar name table over 16 MiB".into()));
                }
                table = vec![0u8; size as usize];
                file.seek(SeekFrom::Start(start))?;
                file.read_exact(&mut table)?;
                continue;
            }
            _ if field.starts_with(b"__.SYMDEF") => continue,
            _ if field.starts_with(b"#1/") => {
                let n = decimal(&field[3..])?;
                if n > size {
                    return Err(Error::Format("an ar name is longer than its member".into()));
                }
                if n > arca_core::limits::MAX_NAME as u64 {
                    return Err(Error::Limit(format!("a name of {n} bytes")));
                }
                let mut name = vec![0u8; n as usize];
                file.seek(SeekFrom::Start(start))?;
                file.read_exact(&mut name)?;
                while name.last() == Some(&0) {
                    name.pop();
                }
                start += n;
                data_size -= n;
                if name.starts_with(b"__.SYMDEF") {
                    continue;
                }
                name
            }
            _ if field.len() > 1
                && field[0] == b'/'
                && field[1..].iter().all(u8::is_ascii_digit) =>
            {
                let at = decimal(&field[1..])?;
                let rest = usize::try_from(at)
                    .ok()
                    .and_then(|at| table.get(at..))
                    .ok_or_else(|| {
                        Error::Format("an ar long name points outside its table".into())
                    })?;
                let end = rest
                    .iter()
                    .position(|&b| b == b'\n' || b == 0)
                    .unwrap_or(rest.len());
                let name = &rest[..end];
                name.strip_suffix(b"/").unwrap_or(name).to_vec()
            }
            _ => field.strip_suffix(b"/").unwrap_or(field).to_vec(),
        };
        check_name(&name)?;
        if name.is_empty() {
            return Err(Error::Format("an ar member without a name".into()));
        }
        let label = String::from_utf8_lossy(&name).into_owned();
        if !carry_on(&label) {
            return Err(Error::Cancelled);
        }
        if out.len() as u64 >= arca_core::limits::MAX_ENTRIES {
            return Err(Error::Limit("too many entries".into()));
        }
        out.push(Raw {
            name,
            start,
            size: data_size,
            mtime: decimal(&header[16..28]).unwrap_or(0) as i64,
            mode: octal(&header[40..48]).unwrap_or(0) as u32,
        });
    }
    Ok(out)
}

fn trim(field: &[u8]) -> &[u8] {
    let end = field
        .iter()
        .rposition(|&b| b != b' ')
        .map_or(0, |last| last + 1);
    &field[..end]
}

fn number(field: &[u8], radix: u32) -> Result<u64> {
    let text = std::str::from_utf8(trim(field))
        .map_err(|_| Error::Format("an ar header holds a number that is not one".into()))?;
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, radix)
        .map_err(|_| Error::Format("an ar header holds a number that is not one".into()))
}

fn decimal(field: &[u8]) -> Result<u64> {
    number(field, 10)
}

fn octal(field: &[u8]) -> Result<u64> {
    number(field, 8)
}

fn truncated() -> Error {
    Error::Format("the ar archive is truncated".into())
}

pub(crate) fn open(path: &Path, carry_on: &dyn Fn(&str) -> bool) -> Result<Parsed> {
    let file = File::open(path)?;
    let len = file_len(&file)?;
    let mut file = BufReader::with_capacity(BUF, file);
    let mut members = Vec::new();
    for raw in self::members(&mut file, len, carry_on)? {
        check_count(&members)?;
        let name = String::from_utf8_lossy(&raw.name).into_owned();
        members.push(Member {
            entry: entry(name, raw.name, raw.size, Some(raw.mtime)),
            kind: Kind::File,
            mode: raw.mode & 0o7777,
            data: Data::Range {
                start: raw.start,
                len: raw.size,
            },
            sum: None,
        });
    }
    Ok(Parsed {
        members,
        ..Parsed::default()
    })
}
