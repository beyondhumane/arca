//! Read-only disk images: raw images, VHD, VHDX, VMDK and DMG. The disk's
//! MBR or GPT partitions are listed as folders, and the FAT, exFAT, NTFS,
//! HFS+ and ISO 9660 file systems in them are browsed. A partition whose
//! file system is not one of those is listed as one raw file. No writing
//! API is exposed.

use arca_core::extraction::{self, Destination, Refused};
use arca_core::{Entry, Error, Format, Result};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

mod device;
mod dmg;
mod exfat;
mod fat;
mod fs;
mod hfs;
mod ntfs;
mod part;
mod vhd;
mod vhdx;
mod vmdk;

use device::{Device, Volume};
use fs::{Kind, Member, Run};

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

/// What a link entry points to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link<'a> {
    /// A symbolic link and the path it holds.
    Symbolic(&'a str),
}

/// What an extraction wrote, and the links it left out.
#[derive(Debug, Default)]
pub struct Extracted {
    pub bytes: u64,
    pub files: usize,
    pub refused: Vec<Refused>,
}

pub struct DiskArchive {
    path: PathBuf,
    format: Format,
    members: Vec<Member>,
    entries: Vec<Entry>,
    notices: Vec<String>,
}

/// Whether this crate reads `format`.
pub fn handles(format: Format) -> bool {
    matches!(
        format,
        Format::Img | Format::Vhd | Format::Vhdx | Format::Vmdk | Format::Dmg
    )
}

pub fn read_only(format: Format) -> Error {
    format.read_only()
}

impl DiskArchive {
    pub fn open(path: &Path, format: Format) -> Result<Self> {
        Self::open_with_progress(path, format, &|_, _, _| true)
    }

    pub fn open_with_progress(
        path: &Path,
        format: Format,
        progress: &Progress<'_>,
    ) -> Result<Self> {
        let carry_on = || progress(0, 0, format.label());
        let opened = device::open(path, format)?;
        let mut dev = opened.device;
        let mut notices = opened.notices;
        let partitions = match opened.partitions {
            Some(listed) => Some(listed),
            None => part::scan(dev.as_mut(), &mut notices)?,
        }
        .filter(|found| !found.is_empty());
        let mut members = Vec::new();
        match partitions {
            Some(partitions) => {
                for (i, p) in partitions.iter().enumerate() {
                    if !carry_on() {
                        return Err(Error::Cancelled);
                    }
                    partition(dev.as_mut(), i, p, &carry_on, &mut members, &mut notices)?;
                }
            }
            None => {
                let len = dev.len();
                let mut vol = Volume {
                    dev: dev.as_mut(),
                    base: 0,
                    len,
                };
                match fs::detect(&mut vol)? {
                    Some(found) => {
                        let listing = fs::list(found, &mut vol, &carry_on)?;
                        notices.extend(listing.notices);
                        members.extend(listing.members);
                    }
                    None => {
                        notices.push(
                            "no partition table or known file system was found; the disk is listed as one raw image"
                                .into(),
                        );
                        members.push(raw(format!("{}.img", stem(path)), 0, len));
                    }
                }
            }
        }
        for (i, member) in members.iter_mut().enumerate() {
            member.entry.offset = i as u64;
        }
        Ok(DiskArchive {
            path: path.to_path_buf(),
            format,
            entries: members.iter().map(|m| m.entry.clone()).collect(),
            members,
            notices,
        })
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Things the listing leaves out on purpose or could not read: damaged
    /// partitions shown as raw images, compressed or encrypted files, links
    /// and folders whose records are broken.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    pub fn link(&self, index: usize) -> Option<Link<'_>> {
        match &self.members.get(index)?.kind {
            Kind::Symlink(target) => Some(Link::Symbolic(target)),
            Kind::File | Kind::Dir => None,
        }
    }

    /// Hands `visit` the data of every selected file (all of them when
    /// `wanted` is empty), in the order they lie on the disk. Folders and
    /// links are not visited. Returns how many files were visited.
    pub fn walk(
        &self,
        wanted: &[bool],
        progress: &Progress<'_>,
        visit: &mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
    ) -> Result<usize> {
        let mut picked: Vec<usize> = (0..self.members.len())
            .filter(|&i| selected(wanted, i) && self.members[i].kind == Kind::File)
            .collect();
        picked.sort_by_key(|&i| first_position(&self.members[i].runs));
        let total = picked.len();
        let mut dev = device::open(&self.path, self.format)?.device;
        for (done, &i) in picked.iter().enumerate() {
            let member = &self.members[i];
            let name = member.entry.name.as_str();
            if !progress(done, total, name) {
                return Err(Error::Cancelled);
            }
            let carry_on = || progress(done, total, name);
            let mut reader = RunReader {
                dev: dev.as_mut(),
                runs: &member.runs,
                next: 0,
                within: 0,
                left: member.entry.size,
                carry_on: &carry_on,
                failure: None,
            };
            let result = visit(i, &member.entry, &mut reader)
                .and_then(|()| io::copy(&mut reader, &mut io::sink()).map_err(Error::from));
            if let Some(failure) = reader.failure.take() {
                return Err(failure);
            }
            result?;
        }
        let _ = progress(total, total, "");
        Ok(total)
    }

    /// Reads every selected file and returns how many were read. With
    /// nothing selected, the checksums the container keeps for its data,
    /// such as those of a DMG's blocks, are checked as well.
    pub fn test(&self, wanted: &[bool], progress: &Progress<'_>) -> Result<usize> {
        if wanted.is_empty() {
            let mut dev = device::open(&self.path, self.format)?.device;
            dev.check(&|| progress(0, 0, self.format.label()))?;
        }
        self.walk(wanted, progress, &mut |_, _, reader| {
            io::copy(reader, &mut io::sink())?;
            Ok(())
        })
    }

    pub fn read_entry(&self, index: usize) -> Result<Vec<u8>> {
        let member = self.file(index)?;
        if member.entry.size > PREVIEW_LIMIT {
            return Err(Error::Limit("preview exceeds 64 MiB".into()));
        }
        let mut out = Vec::new();
        self.read_entry_to(index, &mut out, &|_, _, _| true)?;
        Ok(out)
    }

    /// Streams one file into `out` without the preview's size limit.
    pub fn read_entry_to(
        &self,
        index: usize,
        out: &mut dyn Write,
        progress: &Progress<'_>,
    ) -> Result<u64> {
        self.file(index)?;
        let mut wanted = vec![false; self.members.len()];
        wanted[index] = true;
        let mut written = 0;
        self.walk(&wanted, progress, &mut |_, _, reader| {
            written = io::copy(reader, out)?;
            Ok(())
        })?;
        Ok(written)
    }

    fn file(&self, index: usize) -> Result<&Member> {
        let member = self
            .members
            .get(index)
            .ok_or_else(|| Error::Format("entry not found".into()))?;
        match member.kind {
            Kind::File => Ok(member),
            Kind::Dir => Err(Error::Unsupported("the entry is a directory".into())),
            Kind::Symlink(_) => Err(Error::Unsupported("the entry is a symbolic link".into())),
        }
    }

    /// Folders first, then every file in disk order, then symbolic links,
    /// made only where their target stays inside `dest`. Each file goes
    /// through a temporary name, so a stop or a failure leaves no partial
    /// file behind.
    pub fn extract(
        &self,
        dest: &Path,
        wanted: &[bool],
        progress: &Progress<'_>,
        conflict: &dyn Fn(&Path) -> Conflict,
    ) -> Result<Extracted> {
        let mut destination = Destination::new(dest);
        let mut ask = |path: &Path| match conflict(path) {
            Conflict::Overwrite => extraction::Conflict::Overwrite,
            Conflict::Skip => extraction::Conflict::Skip,
            Conflict::Rename => extraction::Conflict::Rename,
            Conflict::Cancel => extraction::Conflict::Cancel,
        };
        for (i, member) in self.members.iter().enumerate() {
            if selected(wanted, i) && member.kind == Kind::Dir {
                destination.write_entry(
                    &member.entry.name,
                    true,
                    member.entry.mtime,
                    &mut io::empty(),
                    &mut ask,
                )?;
            }
        }
        let mut out = Extracted::default();
        self.walk(wanted, progress, &mut |_, entry, reader| {
            if let Some(bytes) =
                destination.write_entry(&entry.name, false, entry.mtime, reader, &mut ask)?
            {
                out.bytes += bytes;
                out.files += 1;
            }
            Ok(())
        })?;
        for (i, member) in self.members.iter().enumerate() {
            if let (true, Kind::Symlink(target)) = (selected(wanted, i), &member.kind) {
                destination.write_symlink(&member.entry.name, target)?;
            }
        }
        out.files += destination.make_links(&mut ask)?;
        out.refused = destination.refused().to_vec();
        Ok(out)
    }
}

fn partition(
    dev: &mut dyn Device,
    index: usize,
    p: &part::Partition,
    carry_on: &dyn Fn() -> bool,
    members: &mut Vec<Member>,
    notices: &mut Vec<String>,
) -> Result<()> {
    let mut vol = Volume {
        dev,
        base: p.start,
        len: p.len,
    };
    let found = match fs::detect(&mut vol) {
        Ok(found) => found,
        Err(e) => {
            notices.push(format!("partition {index} could not be read: {e}"));
            None
        }
    };
    let Some(found) = found else {
        members.push(raw(format!("{index}.img"), p.start, p.len));
        return Ok(());
    };
    let folder = format!("{index}.{}", found.tag());
    match fs::list(found, &mut vol, carry_on) {
        Ok(listing) => {
            members.push(Member {
                entry: fs::entry(folder.clone(), 0, true, None),
                kind: Kind::Dir,
                runs: Vec::new(),
            });
            notices.extend(
                listing
                    .notices
                    .into_iter()
                    .map(|n| format!("{folder}: {n}")),
            );
            for mut member in listing.members {
                member.entry.name = fs::join(&folder, &member.entry.name);
                member.entry.raw_name = member.entry.name.as_bytes().to_vec();
                member.entry.utf8 = true;
                members.push(member);
            }
            Ok(())
        }
        Err(Error::Cancelled) => Err(Error::Cancelled),
        Err(e @ Error::Limit(_)) => Err(e),
        Err(e) => {
            notices.push(format!(
                "partition {index} holds a damaged {} file system ({e}); it is listed as a raw image",
                found.label()
            ));
            members.push(raw(format!("{index}.img"), p.start, p.len));
            Ok(())
        }
    }
}

fn raw(name: String, at: u64, len: u64) -> Member {
    Member {
        entry: fs::entry(name, len, false, None),
        kind: Kind::File,
        runs: vec![Run::Disk { at, len }],
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| fs::usable(s))
        .unwrap_or_else(|| "disk".into())
}

fn selected(wanted: &[bool], i: usize) -> bool {
    wanted.is_empty() || wanted.get(i).copied().unwrap_or(false)
}

fn first_position(runs: &[Run]) -> u64 {
    runs.iter()
        .find_map(|r| match r {
            Run::Disk { at, .. } => Some(*at),
            _ => None,
        })
        .unwrap_or(0)
}

// Reads one file's runs in order, keeping why it stopped so the caller can
// report a stop or a damaged image instead of a bare I/O error.
struct RunReader<'a> {
    dev: &'a mut dyn Device,
    runs: &'a [Run],
    next: usize,
    within: u64,
    left: u64,
    carry_on: &'a dyn Fn() -> bool,
    failure: Option<Error>,
}

impl RunReader<'_> {
    fn fail(&mut self, e: Error) -> io::Error {
        let message = e.to_string();
        self.failure = Some(e);
        io::Error::other(message)
    }
}

impl Read for RunReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        if !(self.carry_on)() {
            return Err(self.fail(Error::Cancelled));
        }
        let Some(run) = self.runs.get(self.next) else {
            return Err(self.fail(Error::Format(
                "a file's data ends before its size: its allocation is damaged".into(),
            )));
        };
        let room = run.len() - self.within;
        let n = (buf.len().min(BUF) as u64).min(room).min(self.left) as usize;
        match run {
            Run::Disk { at, .. } => {
                if let Err(e) = self
                    .dev
                    .read_at(at.saturating_add(self.within), &mut buf[..n])
                {
                    return Err(self.fail(e));
                }
            }
            Run::Zero(_) => buf[..n].fill(0),
            Run::Inline(bytes) => {
                let from = self.within as usize;
                buf[..n].copy_from_slice(&bytes[from..from + n]);
            }
        }
        self.within += n as u64;
        self.left -= n as u64;
        if self.within == run.len() {
            self.next += 1;
            self.within = 0;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
