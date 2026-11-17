use arca_core::{Error, Result};
use std::path::Path;

use crate::device::{be32, be64, past_end, Device, Raw};

const UNUSED: u32 = u32::MAX;

pub(crate) fn open(path: &Path) -> Result<Box<dyn Device>> {
    let mut file = Raw::open(path)?;
    let len = file.len();
    let (footer, trailer) = footer(&mut file, len)?;
    let size = be64(&footer, 48);
    match be32(&footer, 60) {
        2 => {
            let data = len - trailer;
            Ok(Box::new(file.with_len(size.min(data))))
        }
        3 => Ok(Box::new(Dynamic::open(file, &footer, size)?)),
        4 => Err(Error::Unsupported(
            "this is a differencing VHD: its data is split with a parent disk Arca does not open"
                .into(),
        )),
        other => Err(Error::Format(format!("a VHD of disk type {other}"))),
    }
}

// The footer closes the file; a dynamic disk also keeps a copy at the start,
// read when the one at the end is damaged. Images from before Virtual PC 2004
// have a 511-byte footer.
fn footer(file: &mut Raw, len: u64) -> Result<(Vec<u8>, u64)> {
    let mut places = vec![];
    if len >= 512 {
        places.push((len - 512, 512));
    }
    if len >= 511 {
        places.push((len - 511, 511));
    }
    places.push((0, 0));
    for (at, trailer) in places {
        let size = if trailer == 511 { 511 } else { 512 };
        if at.saturating_add(size) > len {
            continue;
        }
        let mut footer = file.bytes(at, size)?;
        footer.resize(512, 0);
        if &footer[..8] == b"conectix" && valid(&footer) {
            if trailer == 0 && be32(&footer, 60) != 3 {
                continue;
            }
            return Ok((footer, if trailer == 0 { 512 } else { trailer }));
        }
    }
    Err(Error::Format(
        "no valid VHD footer: the file is not a VHD or its footer is damaged".into(),
    ))
}

fn valid(footer: &[u8]) -> bool {
    let sum = footer
        .iter()
        .enumerate()
        .filter(|(i, _)| !(64..68).contains(i))
        .fold(0u32, |sum, (_, &b)| sum.wrapping_add(u32::from(b)));
    !sum == be32(footer, 64)
}

struct Dynamic {
    file: Raw,
    size: u64,
    block: u64,
    bitmap: u64,
    table: Vec<u32>,
}

impl Dynamic {
    fn open(mut file: Raw, footer: &[u8], size: u64) -> Result<Self> {
        let at = be64(footer, 16);
        let header = file.bytes(at, 1024)?;
        if &header[..8] != b"cxsparse" {
            return Err(Error::Format(
                "the VHD's dynamic disk header is missing".into(),
            ));
        }
        let sum = header
            .iter()
            .enumerate()
            .filter(|(i, _)| !(36..40).contains(i))
            .fold(0u32, |sum, (_, &b)| sum.wrapping_add(u32::from(b)));
        if !sum != be32(&header, 36) {
            return Err(Error::Format(
                "the VHD's dynamic disk header is damaged".into(),
            ));
        }
        let table_at = be64(&header, 16);
        let entries = u64::from(be32(&header, 28));
        let block = u64::from(be32(&header, 32));
        if !block.is_power_of_two() || !(4096..=256 * 1024 * 1024).contains(&block) {
            return Err(Error::Format(format!("VHD blocks of {block} bytes")));
        }
        let raw = file.bytes(table_at, entries * 4)?;
        let table = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_be_bytes(*b))
            .collect();
        let bitmap = (block / 512).div_ceil(8).div_ceil(512) * 512;
        Ok(Dynamic {
            file,
            size: size.min(entries.saturating_mul(block)),
            block,
            bitmap,
            table,
        })
    }
}

impl Device for Dynamic {
    fn len(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.size {
            return Err(past_end());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let index = at / self.block;
            let within = at % self.block;
            let n = ((self.block - within) as usize).min(buf.len() - done);
            let part = &mut buf[done..done + n];
            match self.table.get(index as usize).copied().unwrap_or(UNUSED) {
                UNUSED => part.fill(0),
                sector => {
                    let start = u64::from(sector) * 512 + self.bitmap + within;
                    self.file.read_at(start, part)?;
                }
            }
            done += n;
        }
        Ok(())
    }
}
