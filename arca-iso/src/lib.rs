//! Read-only ISO 9660 images. No image-writing API is exposed.

use arca_core::extraction::{self, Destination};
use arca_core::{Entry, Error, Result};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

mod parse;

use parse::Extent;

const BUF: usize = 256 * 1024;
const PREVIEW_LIMIT: u64 = 64 * 1024 * 1024;

pub type Progress<'a> = dyn Fn(usize, usize, &str) -> bool + Sync + 'a;

#[derive(Clone, Copy, Debug)]
pub enum Conflict {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}

pub struct IsoArchive {
    path: PathBuf,
    entries: Vec<Entry>,
    extents: Vec<Vec<Extent>>,
    notices: Vec<String>,
}

impl IsoArchive {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_progress(path, &|_, _, _| true)
    }

    pub fn open_with_progress(path: &Path, progress: &Progress<'_>) -> Result<Self> {
        let mut source = BufReader::with_capacity(BUF, File::open(path)?);
        let parsed = parse::parse(&mut source, &|| progress(0, 0, "ISO"))?;
        Ok(IsoArchive {
            path: path.to_path_buf(),
            entries: parsed.entries,
            extents: parsed.extents,
            notices: parsed.notices,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Things the listing leaves out on purpose: skipped links and special
    /// files, and file systems on the same image that are not read.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// ISO 9660 stores no checksums, so this proves every byte of every file
    /// can be read from the image, not that the bytes are the original ones.
    /// Reads every selected file (all of them when `wanted` is empty) and
    /// returns how many were read.
    pub fn test(&self, wanted: &[bool], progress: &Progress<'_>) -> Result<usize> {
        let mut source = File::open(&self.path)?;
        let total = self.entries.len();
        let mut tested = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            if !progress(i, total, &entry.name) {
                return Err(Error::Cancelled);
            }
            let selected = wanted.is_empty() || wanted.get(i).copied().unwrap_or(false);
            if entry.is_dir || !selected {
                continue;
            }
            tested += 1;
            copy(&mut source, &self.extents[i], &mut io::sink(), &|| {
                progress(i, total, &entry.name)
            })?;
        }
        let _ = progress(total, total, "");
        Ok(tested)
    }

    pub fn read_entry(&self, index: usize) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| Error::Format("ISO entry not found".into()))?;
        if entry.is_dir {
            return Err(Error::Unsupported("ISO entry is a directory".into()));
        }
        if entry.size > PREVIEW_LIMIT {
            return Err(Error::Limit("ISO preview exceeds 64 MiB".into()));
        }
        let mut out = Vec::with_capacity(entry.size as usize);
        copy(
            &mut File::open(&self.path)?,
            &self.extents[index],
            &mut out,
            &|| true,
        )?;
        Ok(out)
    }

    /// Each file is written to a temporary name next to its destination and
    /// renamed into place once complete, so a cancelled or failed run leaves
    /// no partial files behind. Files finished before the failure stay.
    pub fn extract(
        &self,
        dest: &Path,
        wanted: &[bool],
        progress: &Progress<'_>,
        conflict: &dyn Fn(&Path) -> Conflict,
    ) -> Result<u64> {
        let selected = |i: usize| wanted.is_empty() || wanted.get(i).copied().unwrap_or(false);
        let total = (0..self.entries.len())
            .filter(|&i| selected(i) && !self.entries[i].is_dir)
            .count();
        let mut destination = Destination::new(dest);
        let mut source = File::open(&self.path)?;
        let mut done = 0;
        let mut written = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            if !selected(i) {
                continue;
            }
            let carry_on = || progress(done, total, &entry.name);
            if !carry_on() {
                return Err(Error::Cancelled);
            }
            let mut reader = ExtentReader {
                source: &mut source,
                extents: &self.extents[i],
                next: 0,
                left: 0,
                carry_on: &carry_on,
                failure: None,
            };
            let result =
                destination.write_entry(&entry.name, entry.is_dir, &mut reader, &mut |path| {
                    match conflict(path) {
                        Conflict::Overwrite => extraction::Conflict::Overwrite,
                        Conflict::Skip => extraction::Conflict::Skip,
                        Conflict::Rename => extraction::Conflict::Rename,
                        Conflict::Cancel => extraction::Conflict::Cancel,
                    }
                });
            match result {
                Ok(bytes) => written += bytes.unwrap_or(0),
                Err(e) => return Err(reader.failure.take().unwrap_or(e)),
            }
            if !entry.is_dir {
                done += 1;
            }
        }
        let _ = progress(total, total, "");
        Ok(written)
    }
}

pub fn read_only() -> Error {
    Error::Unsupported("ISO is read-only; creating or modifying ISO images is disabled".into())
}

fn copy(
    source: &mut File,
    extents: &[Extent],
    out: &mut dyn Write,
    carry_on: &dyn Fn() -> bool,
) -> Result<u64> {
    let mut buf = vec![0u8; BUF];
    let mut written = 0u64;
    for extent in extents {
        source.seek(SeekFrom::Start(extent.start))?;
        let mut left = extent.len;
        while left > 0 {
            if !carry_on() {
                return Err(Error::Cancelled);
            }
            let want = left.min(BUF as u64) as usize;
            let got = source.read(&mut buf[..want])?;
            if got == 0 {
                return Err(Error::Format("image ended in the middle of a file".into()));
            }
            out.write_all(&buf[..got])?;
            left -= got as u64;
            written += got as u64;
        }
    }
    Ok(written)
}

// Streams one file's extents in order, keeping why it stopped so the caller
// can report a cancel or a short image instead of a bare I/O error.
struct ExtentReader<'a> {
    source: &'a mut File,
    extents: &'a [Extent],
    next: usize,
    left: u64,
    carry_on: &'a dyn Fn() -> bool,
    failure: Option<Error>,
}

impl Read for ExtentReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.left == 0 {
            let Some(extent) = self.extents.get(self.next) else {
                return Ok(0);
            };
            self.source.seek(SeekFrom::Start(extent.start))?;
            self.left = extent.len;
            self.next += 1;
        }
        if !(self.carry_on)() {
            self.failure = Some(Error::Cancelled);
            return Err(io::Error::other("cancelled"));
        }
        let want = self.left.min(buf.len() as u64) as usize;
        let got = self.source.read(&mut buf[..want])?;
        if got == 0 {
            self.failure = Some(Error::Format("image ended in the middle of a file".into()));
            return Err(io::Error::other("image ended in the middle of a file"));
        }
        self.left -= got as u64;
        Ok(got)
    }
}

#[cfg(test)]
mod tests;
