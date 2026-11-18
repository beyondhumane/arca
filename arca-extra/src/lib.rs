//! Read-only CPIO, AR, DEB, RPM, CAB and LZH archives. No writing API is
//! exposed: Arca lists, tests, previews and extracts these formats and
//! nothing else.

use arca_core::extraction::{self, Destination, Refused};
use arca_core::{limits, Entry, Error, Format, Method, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

mod ar;
mod cab;
mod cpio;
mod deb;
mod lzh;
mod rpm;
mod stream;

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
    /// A hard link and the name of the entry whose data it shares.
    Hard(&'a str),
}

/// What an extraction wrote, and the links it left out.
#[derive(Debug, Default)]
pub struct Extracted {
    pub bytes: u64,
    pub files: usize,
    pub refused: Vec<Refused>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Dir,
    Symlink(String),
    Hard(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Data {
    None,
    Range { start: u64, len: u64 },
    Stream { stream: usize, ordinal: u64 },
    Cab { folder: usize, offset: u64 },
    Lzh { header: u64 },
}

#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub entry: Entry,
    pub kind: Kind,
    pub mode: u32,
    pub data: Data,
    /// The CPIO "crc" format's sum of every data byte, checked as it is read.
    pub sum: Option<u32>,
}

#[derive(Default)]
pub(crate) struct Parsed {
    pub members: Vec<Member>,
    pub notices: Vec<String>,
    pub streams: Vec<stream::Spec>,
    pub cab: Option<cab::Index>,
}

pub struct ExtraArchive {
    path: PathBuf,
    format: Format,
    members: Vec<Member>,
    entries: Vec<Entry>,
    notices: Vec<String>,
    streams: Vec<stream::Spec>,
    cab: Option<cab::Index>,
}

/// Whether this crate reads `format`.
pub fn handles(format: Format) -> bool {
    matches!(
        format,
        Format::Cpio | Format::Ar | Format::Deb | Format::Rpm | Format::Cab | Format::Lzh
    )
}

pub fn read_only(format: Format) -> Error {
    format.read_only()
}

impl ExtraArchive {
    pub fn open(path: &Path, format: Format) -> Result<Self> {
        Self::open_with_progress(path, format, &|_, _, _| true)
    }

    pub fn open_with_progress(
        path: &Path,
        format: Format,
        progress: &Progress<'_>,
    ) -> Result<Self> {
        let carry_on = |name: &str| progress(0, 0, name);
        let parsed = match format {
            Format::Cpio => cpio::open(path, &carry_on)?,
            Format::Ar => ar::open(path, &carry_on)?,
            Format::Deb => deb::open(path, &carry_on)?,
            Format::Rpm => rpm::open(path, &carry_on)?,
            Format::Cab => cab::open(path, &carry_on)?,
            Format::Lzh => lzh::open(path, &carry_on)?,
            other => {
                return Err(Error::Unsupported(format!(
                    "{} is not one of the formats arca-extra reads",
                    other.label()
                )))
            }
        };
        let mut members = parsed.members;
        for (i, member) in members.iter_mut().enumerate() {
            member.entry.offset = i as u64;
        }
        Ok(ExtraArchive {
            path: path.to_path_buf(),
            format,
            entries: members.iter().map(|m| m.entry.clone()).collect(),
            members,
            notices: parsed.notices,
            streams: parsed.streams,
            cab: parsed.cab,
        })
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Things the listing leaves out on purpose: device files, symbol tables,
    /// files that continue in another cabinet, and checks this reader does
    /// not make.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    pub fn link(&self, index: usize) -> Option<Link<'_>> {
        match &self.members.get(index)?.kind {
            Kind::Symlink(target) => Some(Link::Symbolic(target)),
            Kind::Hard(target) => Some(Link::Hard(target)),
            Kind::File | Kind::Dir => None,
        }
    }

    /// Hands `visit` the data of every selected file (all of them when
    /// `wanted` is empty), each read once, in the order the archive stores
    /// them. Directories and links are not visited. With nothing selected
    /// every compressed stream is also read to its end, so a damaged tail
    /// is found too. Returns how many files were visited.
    pub fn walk(
        &self,
        wanted: &[bool],
        progress: &Progress<'_>,
        visit: &mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
    ) -> Result<usize> {
        let picked: Vec<usize> = (0..self.members.len())
            .filter(|&i| selected(wanted, i) && self.members[i].kind == Kind::File)
            .collect();
        let mut walker = Walker {
            archive: self,
            total: picked.len(),
            done: 0,
            progress,
            visit,
        };
        if !self.streams.is_empty() {
            walker.streams(&picked, wanted.is_empty())?;
        } else if let Some(index) = &self.cab {
            walker.cab(index, &picked, wanted.is_empty())?;
        } else {
            walker.direct(&picked)?;
        }
        let done = walker.done;
        let _ = progress(done, done, "");
        Ok(done)
    }

    /// Reads every selected file and returns how many were read. Every
    /// checksum the format keeps is checked on the way: CAB block sums,
    /// LZH CRC-16s, the CPIO "crc" sums and the compressed streams' own.
    pub fn test(&self, wanted: &[bool], progress: &Progress<'_>) -> Result<usize> {
        self.walk(wanted, progress, &mut |_, _, reader| {
            io::copy(reader, &mut io::sink())?;
            Ok(())
        })
    }

    pub fn read_entry(&self, index: usize) -> Result<Vec<u8>> {
        let source = self.data_of(index)?;
        if self.members[source].entry.size > PREVIEW_LIMIT {
            return Err(Error::Limit("preview exceeds 64 MiB".into()));
        }
        let mut out = Vec::new();
        self.read_entry_to(index, &mut out, &|_, _, _| true)?;
        Ok(out)
    }

    /// Streams one file into `out` without the preview's size limit. A hard
    /// link gives the data of the file it shares it with.
    pub fn read_entry_to(
        &self,
        index: usize,
        out: &mut dyn Write,
        progress: &Progress<'_>,
    ) -> Result<u64> {
        let source = self.data_of(index)?;
        let mut wanted = vec![false; self.members.len()];
        wanted[source] = true;
        let mut written = 0;
        let visited = self.walk(&wanted, progress, &mut |_, _, reader| {
            written = io::copy(reader, out)?;
            Ok(())
        })?;
        if visited != 1 {
            return Err(Error::Format(format!(
                "'{}' was not found where the archive says it is",
                self.members[index].entry.name
            )));
        }
        Ok(written)
    }

    fn data_of(&self, index: usize) -> Result<usize> {
        let member = self
            .members
            .get(index)
            .ok_or_else(|| Error::Format("entry not found".into()))?;
        match &member.kind {
            Kind::File => Ok(index),
            Kind::Dir => Err(Error::Unsupported("the entry is a directory".into())),
            Kind::Symlink(_) => Err(Error::Unsupported("the entry is a symbolic link".into())),
            Kind::Hard(target) => self
                .members
                .iter()
                .position(|m| m.kind == Kind::File && m.entry.name == *target)
                .ok_or_else(|| {
                    Error::Format(format!(
                        "'{}' is a hard link to '{target}', which is not in the archive",
                        member.entry.name
                    ))
                }),
        }
    }

    /// Folders first, then every file as the archive stores it, then hard
    /// links to the files written, then symbolic links, made only where
    /// their target stays inside `dest`. Each file goes through a temporary
    /// name, so a stop or a failure leaves no partial file behind.
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
        let chosen = |i: usize| selected(wanted, i);
        for (i, member) in self.members.iter().enumerate() {
            if chosen(i) && member.kind == Kind::Dir {
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
        self.walk(wanted, progress, &mut |i, entry, reader| {
            let mode = Some(self.members[i].mode).filter(|&m| m != 0);
            if let Some(bytes) =
                destination.write_file(&entry.name, false, entry.mtime, mode, reader, &mut ask)?
            {
                out.bytes += bytes;
                out.files += 1;
            }
            Ok(())
        })?;
        for (i, member) in self.members.iter().enumerate() {
            if let (true, Kind::Hard(target)) = (chosen(i), &member.kind) {
                let mode = Some(member.mode).filter(|&m| m != 0);
                if destination.write_hard_link(
                    &member.entry.name,
                    target,
                    member.entry.mtime,
                    mode,
                    &mut ask,
                )? {
                    out.files += 1;
                }
            }
        }
        for (i, member) in self.members.iter().enumerate() {
            if let (true, Kind::Symlink(target)) = (chosen(i), &member.kind) {
                destination.write_symlink(&member.entry.name, target)?;
            }
        }
        out.files += destination.make_links(&mut ask)?;
        out.refused = destination.refused().to_vec();
        Ok(out)
    }
}

fn selected(wanted: &[bool], i: usize) -> bool {
    wanted.is_empty() || wanted.get(i).copied().unwrap_or(false)
}

struct Walker<'a, 'v> {
    archive: &'a ExtraArchive,
    total: usize,
    done: usize,
    progress: &'a Progress<'a>,
    visit: &'v mut dyn FnMut(usize, &Entry, &mut dyn Read) -> Result<()>,
}

impl Walker<'_, '_> {
    /// Hands one file's data to the visitor, then reads whatever the visitor
    /// left so that the checks at the end of the data still run.
    fn one(&mut self, index: usize, data: &mut dyn Read) -> Result<()> {
        let member = &self.archive.members[index];
        let name = member.entry.name.as_str();
        let (done, total, progress) = (self.done, self.total, self.progress);
        if !progress(done, total, name) {
            return Err(Error::Cancelled);
        }
        let carry_on = || progress(done, total, name);
        let mut summed = Summed {
            inner: data,
            sum: 0,
            left: member.entry.size,
        };
        let mut guard = Guard {
            inner: &mut summed,
            carry_on: &carry_on,
            stopped: false,
        };
        let result = (self.visit)(index, &member.entry, &mut guard)
            .and_then(|()| io::copy(&mut guard, &mut io::sink()).map_err(Error::from));
        if guard.stopped {
            return Err(Error::Cancelled);
        }
        result?;
        if let Some(expected) = member.sum {
            if summed.sum != expected {
                return Err(Error::Integrity {
                    name: name.to_string(),
                    expected,
                    found: summed.sum,
                });
            }
        }
        self.done += 1;
        Ok(())
    }

    fn direct(&mut self, picked: &[usize]) -> Result<()> {
        let mut file = BufReader::with_capacity(BUF, File::open(&self.archive.path)?);
        for &i in picked {
            match self.archive.members[i].data {
                Data::Range { start, len } => {
                    file.seek(SeekFrom::Start(start))?;
                    let name = &self.archive.members[i].entry.name;
                    let mut data = Exact::new(&mut file, len, name);
                    self.one(i, &mut data)?;
                }
                Data::Lzh { header } => {
                    file.seek(SeekFrom::Start(header))?;
                    let mut data = lzh::decoder(&mut file, &self.archive.members[i].entry.name)?;
                    self.one(i, &mut data)?;
                }
                _ => return Err(Error::Format("entry without data".into())),
            }
        }
        Ok(())
    }

    fn streams(&mut self, picked: &[usize], full: bool) -> Result<()> {
        for (s, spec) in self.archive.streams.iter().enumerate() {
            let mut here: HashMap<u64, usize> = HashMap::new();
            for &i in picked {
                if let Data::Stream { stream, ordinal } = self.archive.members[i].data {
                    if stream == s {
                        here.insert(ordinal, i);
                    }
                }
            }
            if here.is_empty() && !full {
                continue;
            }
            let source = stream::open(&self.archive.path, spec)?;
            match spec.layout {
                stream::Layout::Tar => self.tar(source, here, full)?,
                stream::Layout::Cpio => self.cpio(source, here, full)?,
            }
        }
        Ok(())
    }

    fn tar(
        &mut self,
        source: Box<dyn Read + Send>,
        mut here: HashMap<u64, usize>,
        full: bool,
    ) -> Result<()> {
        let mut reader = arca_tar::TarReader::new(source);
        let mut ordinal = 0u64;
        while let Some(e) = reader.next_entry()? {
            match here.remove(&ordinal) {
                Some(i) => reader.with_data(&e, |data| self.one(i, data))?,
                None => reader.skip_data(&e)?,
            }
            ordinal += 1;
            if here.is_empty() && !full {
                return Ok(());
            }
        }
        if full {
            arca_stream::drain(&mut reader.into_inner())?;
        }
        Ok(())
    }

    fn cpio(
        &mut self,
        source: Box<dyn Read + Send>,
        mut here: HashMap<u64, usize>,
        full: bool,
    ) -> Result<()> {
        let mut reader = cpio::Reader::new(source);
        let mut ordinal = 0u64;
        while let Some(h) = reader.next_header()? {
            match here.remove(&ordinal) {
                Some(i) => reader.with_data(&h, |data| self.one(i, data))?,
                None => reader.skip_data(&h)?,
            }
            ordinal += 1;
            if here.is_empty() && !full {
                return Ok(());
            }
        }
        if full {
            arca_stream::drain(&mut reader.into_inner())?;
        }
        Ok(())
    }

    fn cab(&mut self, index: &cab::Index, picked: &[usize], full: bool) -> Result<()> {
        let mut file = BufReader::with_capacity(BUF, File::open(&self.archive.path)?);
        let mut by_folder: Vec<Vec<(u64, usize)>> = vec![Vec::new(); index.folders.len()];
        for &i in picked {
            if let Data::Cab { folder, offset } = self.archive.members[i].data {
                by_folder[folder].push((offset, i));
            }
        }
        for (f, mut files) in by_folder.into_iter().enumerate() {
            if files.is_empty() && !full {
                continue;
            }
            files.sort_unstable();
            let folder = &index.folders[f];
            let mut reader = cab::FolderReader::new(&mut file, folder)?;
            for (offset, i) in files {
                if offset < reader.position() {
                    reader = cab::FolderReader::new(reader.into_inner(), folder)?;
                }
                reader.skip_to(offset)?;
                let member = &self.archive.members[i];
                let mut data = Exact::new(&mut reader, member.entry.size, &member.entry.name);
                self.one(i, &mut data)?;
            }
            if full && folder.decodable() {
                io::copy(&mut reader, &mut io::sink())?;
            }
        }
        Ok(())
    }
}

/// Exactly `left` bytes of `inner`: ending any earlier is an error.
pub(crate) struct Exact<'n, R> {
    inner: R,
    left: u64,
    name: &'n str,
}

impl<'n, R: Read> Exact<'n, R> {
    pub(crate) fn new(inner: R, left: u64, name: &'n str) -> Self {
        Exact { inner, left, name }
    }
}

impl<R: Read> Read for Exact<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let got = self.inner.read(&mut buf[..want])?;
        if got == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("the archive ends inside '{}'", self.name),
            ));
        }
        self.left -= got as u64;
        Ok(got)
    }
}

/// An owned [`Exact`] for the compressed streams, which must be `'static`.
pub(crate) struct Range<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Range<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let got = self.inner.read(&mut buf[..want])?;
        if got == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the archive is truncated",
            ));
        }
        self.left -= got as u64;
        Ok(got)
    }
}

struct Summed<'a> {
    inner: &'a mut dyn Read,
    sum: u32,
    left: u64,
}

impl Read for Summed<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let got = self.inner.read(buf)?;
        for &b in &buf[..got] {
            self.sum = self.sum.wrapping_add(u32::from(b));
        }
        self.left = self.left.saturating_sub(got as u64);
        Ok(got)
    }
}

// Asks whether to carry on before every read, and remembers a stop so it can
// be reported as one rather than as the I/O error it travels as.
struct Guard<'a> {
    inner: &'a mut dyn Read,
    carry_on: &'a dyn Fn() -> bool,
    stopped: bool,
}

impl Read for Guard<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !(self.carry_on)() {
            self.stopped = true;
            return Err(io::Error::other("cancelled"));
        }
        self.inner.read(buf)
    }
}

/// A name as the archive spells it, made relative: no `./` or `/` in front
/// and no `/` behind. The root folder itself comes out empty.
pub(crate) fn clean(name: &str) -> String {
    let mut rest = name;
    loop {
        if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix('/') {
            rest = r;
        } else {
            break;
        }
    }
    let rest = rest.trim_end_matches('/');
    if rest == "." {
        String::new()
    } else {
        rest.to_string()
    }
}

pub(crate) fn check_name(name: &[u8]) -> Result<()> {
    if name.len() > limits::MAX_NAME {
        return Err(Error::Limit(format!("a name of {} bytes", name.len())));
    }
    Ok(())
}

pub(crate) fn check_count(members: &[Member]) -> Result<()> {
    if members.len() as u64 >= limits::MAX_ENTRIES {
        return Err(Error::Limit("too many entries".into()));
    }
    Ok(())
}

pub(crate) fn entry(name: String, raw_name: Vec<u8>, size: u64, mtime: Option<i64>) -> Entry {
    Entry {
        name,
        size,
        compressed_size: size,
        method: Method::Store,
        crc32: None,
        is_dir: false,
        mtime,
        created: None,
        accessed: None,
        attributes: 0,
        offset: 0,
        raw_name,
        utf8: true,
        encrypted: false,
        zipcrypto: false,
        aes_strength: None,
    }
}

/// The member for a Unix file of type `mode & 0o170000`, or `None` for a
/// device, a pipe or a socket, which are never extracted.
pub(crate) fn unix_kind(mode: u32, link: Option<String>) -> Option<Kind> {
    match mode & 0o170000 {
        0o040000 => Some(Kind::Dir),
        0o120000 => Some(Kind::Symlink(link.unwrap_or_default())),
        0 | 0o100000 => Some(Kind::File),
        _ => None,
    }
}

pub(crate) fn skipped_notice(count: usize) -> String {
    format!("{count} device files, pipes or sockets are not listed")
}

pub(crate) fn file_len(file: &File) -> Result<u64> {
    Ok(file.metadata()?.len())
}

#[cfg(test)]
mod tests;
