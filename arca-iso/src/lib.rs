//! Read-only ISO 9660 images. No image-writing API is exposed.

use arca_core::{Entry, Error, Result};
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

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
        fs::create_dir_all(dest)?;
        let dest = dest.canonicalize()?;
        let mut claimed = HashSet::new();
        let mut jobs = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            if !selected(i) {
                continue;
            }
            let path = dest.join(arca_core::safe_name(&entry.name)?);
            reject_links(&path, &dest)?;
            if entry.is_dir {
                fs::create_dir_all(&path)?;
                continue;
            }
            if path.is_dir() {
                return Err(Error::Format(format!(
                    "file/directory conflict at '{}'",
                    path.display()
                )));
            }
            let (target, overwrite) = if path.exists() || claimed.contains(&path) {
                match conflict(&path) {
                    Conflict::Skip => continue,
                    Conflict::Cancel => return Err(Error::Cancelled),
                    Conflict::Overwrite => (path, true),
                    Conflict::Rename => (free_name(&path, &claimed)?, false),
                }
            } else {
                (path, false)
            };
            claimed.insert(target.clone());
            jobs.push((i, target, overwrite));
        }

        let total = jobs.len();
        let done = AtomicUsize::new(0);
        let written = jobs
            .par_iter()
            .map(|(i, target, overwrite)| {
                let entry = &self.entries[*i];
                let carry_on = || progress(done.load(Ordering::Relaxed), total, &entry.name);
                if !carry_on() {
                    return Err(Error::Cancelled);
                }
                let parent = target
                    .parent()
                    .ok_or_else(|| Error::Format("missing destination parent".into()))?;
                fs::create_dir_all(parent)?;
                let mut output = tempfile::Builder::new()
                    .prefix(".arca-")
                    .suffix(".tmp")
                    .tempfile_in(parent)?;
                let mut writer = BufWriter::with_capacity(BUF, output.as_file_mut());
                let bytes = copy(
                    &mut File::open(&self.path)?,
                    &self.extents[*i],
                    &mut writer,
                    &carry_on,
                )?;
                writer.flush()?;
                drop(writer);
                reject_links(target, &dest)?;
                if *overwrite {
                    output.persist(target)
                } else {
                    output.persist_noclobber(target)
                }
                .map_err(|e| Error::Io(e.error))?;
                done.fetch_add(1, Ordering::Relaxed);
                Ok(bytes)
            })
            .collect::<Result<Vec<u64>>>()?;
        let _ = progress(total, total, "");
        Ok(written.iter().sum())
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

// A link already in the destination tree would carry the write somewhere the
// entry name never pointed.
fn reject_links(path: &Path, root: &Path) -> Result<()> {
    for ancestor in path.ancestors().take_while(|a| *a != root) {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                let link = meta.file_type().is_symlink();
                #[cfg(windows)]
                let link = {
                    use std::os::windows::fs::MetadataExt;
                    link || meta.file_attributes() & 0x400 != 0
                };
                if link {
                    return Err(Error::Format(format!(
                        "ISO destination contains a link: '{}'",
                        ancestor.display()
                    )));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn free_name(path: &Path, claimed: &HashSet<PathBuf>) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Format("missing destination parent".into()))?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..10_000)
        .map(|n| parent.join(format!("{stem} ({n}){extension}")))
        .find(|p| !p.exists() && !claimed.contains(p))
        .ok_or_else(|| Error::Limit("cannot find an unused extraction name".into()))
}

#[cfg(test)]
mod tests;
