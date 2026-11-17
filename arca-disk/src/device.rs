use arca_core::{Error, Format, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::part::Partition;

/// The disk an image holds, as one run of bytes. Regions the image does not
/// store read as zeros; nothing past `len` can be read.
pub(crate) trait Device {
    fn len(&self) -> u64;

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()>;

    fn sector(&self) -> u64 {
        512
    }

    /// Reads everything the container keeps a checksum for and checks it.
    fn check(&mut self, _carry_on: &dyn Fn() -> bool) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct Opened {
    pub device: Box<dyn Device>,
    /// The partitions the container itself lists, as a DMG does.
    pub partitions: Option<Vec<Partition>>,
    pub notices: Vec<String>,
}

pub(crate) fn open(path: &Path, format: Format) -> Result<Opened> {
    let plain = |device: Box<dyn Device>| Opened {
        device,
        partitions: None,
        notices: Vec::new(),
    };
    match format {
        Format::Img => Ok(plain(Box::new(Raw::open(path)?))),
        Format::Vhd => Ok(plain(crate::vhd::open(path)?)),
        Format::Vhdx => Ok(plain(crate::vhdx::open(path)?)),
        Format::Vmdk => Ok(plain(crate::vmdk::open(path)?)),
        Format::Dmg => crate::dmg::open(path),
        other => Err(Error::Unsupported(format!(
            "{} is not one of the formats arca-disk reads",
            other.label()
        ))),
    }
}

pub(crate) fn past_end() -> Error {
    Error::Format("a read past the end of the disk".into())
}

/// A file read at absolute positions, never past `len`.
pub(crate) struct Raw {
    file: File,
    len: u64,
    at: Option<u64>,
}

impl Raw {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Raw {
            file,
            len,
            at: None,
        })
    }

    pub fn with_len(mut self, len: u64) -> Self {
        self.len = self.len.min(len);
        self
    }

    pub fn bytes(&mut self, pos: u64, len: u64) -> Result<Vec<u8>> {
        if pos.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(Error::Format(
                "a structure runs past the end of the file".into(),
            ));
        }
        let mut out = vec![0u8; len as usize];
        self.read_at(pos, &mut out)?;
        Ok(out)
    }
}

impl Device for Raw {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.len {
            return Err(past_end());
        }
        if self.at != Some(pos) {
            self.file.seek(SeekFrom::Start(pos))?;
        }
        self.at = None;
        self.file.read_exact(buf).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                Error::Format("the image file ends early".into())
            } else {
                e.into()
            }
        })?;
        self.at = Some(end);
        Ok(())
    }
}

/// A part of a device: a partition, or the whole disk.
pub(crate) struct Volume<'a> {
    pub dev: &'a mut dyn Device,
    pub base: u64,
    pub len: u64,
}

impl Volume<'_> {
    pub fn read(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.len {
            return Err(Error::Format(
                "a structure runs past the end of the partition".into(),
            ));
        }
        self.dev.read_at(self.base + pos, buf)
    }

    /// `len` bytes at `pos`, which must lie inside the volume: the check
    /// comes before the allocation.
    pub fn bytes(&mut self, pos: u64, len: u64) -> Result<Vec<u8>> {
        if pos.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(Error::Format(
                "a structure runs past the end of the partition".into(),
            ));
        }
        let mut out = vec![0u8; len as usize];
        self.read(pos, &mut out)?;
        Ok(out)
    }
}

/// The volume as a seekable reader, for arca-iso.
pub(crate) struct VolumeReader<'a, 'b> {
    pub volume: &'a mut Volume<'b>,
    pub pos: u64,
}

impl Read for VolumeReader<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.volume.len.saturating_sub(self.pos);
        let n = (buf.len() as u64).min(left) as usize;
        if n == 0 {
            return Ok(0);
        }
        self.volume
            .read(self.pos, &mut buf[..n])
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for VolumeReader<'_, '_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => self.volume.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = target.ok_or_else(|| std::io::Error::other("seek out of range"))?;
        Ok(self.pos)
    }
}

pub(crate) fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

pub(crate) fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap_or([0; 4]))
}

pub(crate) fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap_or([0; 8]))
}

pub(crate) fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap_or([0; 4]))
}

pub(crate) fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap_or([0; 8]))
}

pub(crate) fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}
