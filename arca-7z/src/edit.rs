//! Rewriting an existing 7z with entries left out, renamed or added.
//!
//! A solid block is one compressed stream, so nothing in it can be copied
//! across on its own: every block that keeps an entry is decoded and encoded
//! again. The decoded block goes straight from the decoder into the encoder a
//! few kilobytes at a time, so memory stays at what one decoder and one encoder
//! hold, whatever the size of the block.

use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use arca_core::{Entry, Error, Level, Result};
use sevenz_rust2::{
    ArchiveEntry, ArchiveWriter, BlockDecoder, EncoderMethod as M, Password, SourceReader,
};

use crate::write::{header_methods, methods, new_entry, push_files, workers, Stage, Tracker};
use crate::{
    entry_path, upstream, Compression, CreateOptions, Phase, Progress, SevenZArchive, Source,
};

/// Something to put in: a file read from `source`, or with no source a
/// folder.
pub struct Addition {
    pub source: Option<PathBuf>,
    pub name: String,
}

/// What a rewrite changes.
pub struct Change<'a> {
    /// The name each entry keeps, or `None` to leave it out. Whether it ends
    /// in a slash stays as it was: 7-Zip writes folders without one, Arca with.
    pub name: &'a dyn Fn(&Entry) -> Option<String>,
    /// New entries. One whose name is already in the archive replaces the
    /// entry that had it. They are compressed the way the archive's first
    /// block was, when that is a method a new 7z can have, and with LZMA2
    /// otherwise.
    pub add: &'a [Addition],
    /// The level everything that is encoded again is compressed at. Each
    /// block keeps its method and filters, stored blocks stay stored, and
    /// [`Level::STORE`] stores everything.
    pub level: Level,
}

/// A block with coders this build cannot write again, written with the ones
/// it could keep and LZMA2 for the method it lost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recoded {
    /// Its entries, by the names they have now.
    pub entries: Vec<String>,
    /// The coders it lost, as 7z names them.
    pub lost: Vec<String>,
}

/// Writes `input` again at `output` with `change` applied, then reads all of
/// `output` back. An encrypted archive needs its password and keeps it, with
/// its names hidden again if they were; a partly encrypted one comes out
/// encrypted throughout. Every entry keeps its times and attributes. Returns
/// the blocks that could not keep their coders.
pub fn rewrite_7z(
    input: &Path,
    output: &Path,
    password: Option<&str>,
    change: &Change<'_>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<Vec<Recoded>> {
    let mut source = SevenZArchive::open(io::BufReader::new(fs::File::open(input)?), password)?;
    let password = password.filter(|p| !p.is_empty());
    if source.encrypted && password.is_none() {
        return Err(Error::PasswordRequired);
    }
    let options = CreateOptions {
        level: change.level,
        password: password.filter(|_| source.encrypted),
        hide_names: source.archive.header_encrypted,
        compression: compression_of(&source.archive),
    };
    let plan = plan(&source, change)?;
    let data = |i: usize| {
        let file = &source.archive.files[i];
        file.has_stream && file.size > 0
    };
    let mut added = Vec::with_capacity(change.add.len());
    for addition in change.add {
        let meta = match &addition.source {
            Some(path) => Some(fs::metadata(path)?),
            None => None,
        };
        added.push(meta);
    }
    let any_data = (0..plan.len()).any(|i| plan[i].is_some() && data(i))
        || added.iter().flatten().any(|m| m.is_file() && m.len() > 0);
    if options.password.is_some() && !options.hide_names && !any_data {
        return Err(Error::Unsupported(
            "encrypting only empty entries requires hide_names to validate the password".into(),
        ));
    }
    let total = plan.iter().flatten().count() + change.add.len();
    let mut expected = Vec::with_capacity(total);

    let mut writer = ArchiveWriter::new(BufWriter::new(fs::File::create(output)?))
        .map_err(|e| upstream(e, false))?;
    writer.set_encrypt_header(options.hide_names);
    let mut progress = Counter {
        notify,
        total,
        done: 0,
        bytes: 0,
    };
    let mut recoded = Vec::new();
    let mut copied = vec![false; source.archive.blocks.len()];
    for i in 0..plan.len() {
        match source.archive.stream_map.file_block_index[i] {
            Some(block) if !copied[block] => {
                copied[block] = true;
                recoded.extend(copy_block(
                    &mut source,
                    block,
                    &plan,
                    &options,
                    &mut writer,
                    &mut progress,
                    &mut expected,
                )?);
            }
            Some(_) => {}
            None => {
                if let Some(name) = &plan[i] {
                    let entry = source.archive.files[i].clone();
                    push_empty(&mut writer, entry, name, &mut progress, &mut expected)?;
                }
            }
        }
    }
    drop(source);

    let mut sources = Vec::new();
    let mut metadata = Vec::new();
    for (addition, meta) in change.add.iter().zip(added) {
        let name = addition.name.as_str();
        match meta {
            Some(meta) if meta.is_file() && meta.len() > 0 => {
                sources.push(Source {
                    path: addition.source.clone().unwrap_or_default(),
                    name: name.to_string(),
                });
                metadata.push(meta);
            }
            Some(meta) => {
                let entry = new_entry(name, &meta);
                push_empty(&mut writer, entry, name, &mut progress, &mut expected)?;
            }
            None => {
                let mut entry = ArchiveEntry::new_directory(name);
                if let Ok(time) = sevenz_rust2::NtTime::try_from(std::time::SystemTime::now()) {
                    entry.has_last_modified_date = true;
                    entry.last_modified_date = time;
                }
                push_empty(&mut writer, entry, name, &mut progress, &mut expected)?;
            }
        }
    }
    let full: Vec<usize> = (0..sources.len()).collect();
    let tracker = RefCell::new(Tracker {
        notify: &mut *progress.notify,
        total,
        done: progress.done,
        bytes: progress.bytes,
        cancelled: false,
        changed: false,
    });
    push_files(&mut writer, &sources, &metadata, &full, &options, &tracker)?;
    expected.extend(sources.iter().map(|s| s.name.clone()));

    writer.set_content_methods(header_methods(options.password)?);
    let file = writer
        .finish()?
        .into_inner()
        .map_err(|e| Error::Io(e.into_error()))?;
    file.sync_all()?;
    drop(file);

    verify(output, password, expected, notify)?;
    Ok(recoded)
}

/// What additions are compressed with: the method of the first block, if a
/// new 7z can have it, so that what goes into a stored archive is stored.
fn compression_of(archive: &sevenz_rust2::Archive) -> Compression {
    archive
        .blocks
        .iter()
        .flat_map(|block| block.coders.iter())
        .find_map(
            |coder| match Stage::of(coder.encoder_method_id(), coder.properties())? {
                Stage::Filter(_) => None,
                stage => Some(
                    Compression::ALL
                        .into_iter()
                        .find(|c| c.stage() == stage)
                        .unwrap_or_default(),
                ),
            },
        )
        .unwrap_or_default()
}

/// The coders to write `block` again with, in the order they are undone,
/// and the names of those that cannot be written: a method is replaced with
/// LZMA2, a filter left out.
fn chain_of(block: &sevenz_rust2::Block) -> (Vec<Stage>, Vec<String>) {
    let mut chain = Vec::new();
    let mut lost = Vec::new();
    for (_, coder) in block.ordered_coder_iter() {
        let id = coder.encoder_method_id();
        if id == M::ID_AES256_SHA256 {
            continue;
        }
        match Stage::of(id, coder.properties()) {
            Some(stage) => chain.push(stage),
            None => {
                lost.push(
                    M::by_id(id)
                        .map_or_else(|| format!("method {id:02X?}"), |m| m.name().to_string()),
                );
            }
        }
    }
    if !lost.is_empty() && !chain.iter().any(|s| !matches!(s, Stage::Filter(_))) {
        chain.insert(0, Stage::Lzma2);
    }
    (chain, lost)
}

/// The name each entry of `source` keeps, checked the way the reader will
/// check it: a usable path, and none twice.
fn plan<R: Read + io::Seek>(
    source: &SevenZArchive<R>,
    change: &Change<'_>,
) -> Result<Vec<Option<String>>> {
    let mut added = HashSet::new();
    for addition in change.add {
        if !added.insert(entry_path(addition.name.trim_end_matches('/'))?) {
            return Err(Error::Format("duplicate 7z entry name".into()));
        }
    }
    let mut taken = HashSet::new();
    let mut header_size = 0usize;
    let mut plan = Vec::with_capacity(source.entries.len());
    for entry in &source.entries {
        let name = match (change.name)(entry) {
            Some(name) => {
                let bare = name.trim_end_matches('/');
                let name = if entry.name.ends_with('/') {
                    format!("{bare}/")
                } else {
                    bare.to_string()
                };
                let path = entry_path(&name)?;
                if added.contains(&path) {
                    None
                } else if !taken.insert(path) {
                    return Err(Error::Format("duplicate 7z entry name".into()));
                } else {
                    Some(name)
                }
            }
            None => None,
        };
        if let Some(name) = &name {
            header_size = header_size.saturating_add(name.encode_utf16().count() * 2 + 128);
        }
        plan.push(name);
    }
    for addition in change.add {
        header_size = header_size.saturating_add(addition.name.encode_utf16().count() * 2 + 128);
    }
    if taken.len() + added.len() > 100_000 {
        return Err(Error::Limit("7z entry count".into()));
    }
    if header_size > 16 * 1024 * 1024 {
        return Err(Error::Limit("7z header size".into()));
    }
    Ok(plan)
}

/// The caller's progress, counted across every block and every addition.
struct Counter<'n> {
    notify: &'n mut dyn FnMut(Progress<'_>) -> bool,
    total: usize,
    done: usize,
    bytes: u64,
}

impl Counter<'_> {
    fn tell(&mut self, name: &str) -> Result<()> {
        let progress = Progress {
            phase: Phase::Create,
            entries_done: self.done,
            entries_total: self.total,
            bytes_done: self.bytes,
            name,
        };
        if (self.notify)(progress) {
            Ok(())
        } else {
            Err(Error::Cancelled)
        }
    }
}

/// An entry with no data: a folder, an empty file. It keeps every attribute
/// of `entry` and takes `name`.
fn push_empty<W: Write + io::Seek>(
    writer: &mut ArchiveWriter<W>,
    mut entry: ArchiveEntry,
    name: &str,
    progress: &mut Counter<'_>,
    expected: &mut Vec<String>,
) -> Result<()> {
    progress.tell(name)?;
    entry.name = name.to_string();
    entry.has_stream = false;
    writer
        .push_archive_entry::<io::Empty>(entry, None)
        .map_err(|e| upstream(e, false))?;
    progress.done += 1;
    expected.push(name.to_string());
    Ok(())
}

/// The entries of `block` that are kept, as one new block. The old one is
/// decoded once, start to end, and what is left out is read past.
fn copy_block<R: Read + io::Seek, W: Write + io::Seek>(
    source: &mut SevenZArchive<R>,
    block: usize,
    plan: &[Option<String>],
    options: &CreateOptions<'_>,
    writer: &mut ArchiveWriter<W>,
    progress: &mut Counter<'_>,
    expected: &mut Vec<String>,
) -> Result<Option<Recoded>> {
    let archive = &source.archive;
    let first = archive.stream_map.block_first_file_index[block];
    let mut pieces = Vec::new();
    let mut offset = 0u64;
    for i in (first..archive.files.len())
        .take_while(|&i| archive.stream_map.file_block_index[i] == Some(block))
    {
        let file = &archive.files[i];
        match &plan[i] {
            Some(_) if file.has_stream && file.size > 0 => pieces.push((i, offset)),
            Some(name) => push_empty(writer, file.clone(), name, progress, expected)?,
            None => {}
        }
        if file.has_stream {
            offset = offset
                .checked_add(file.size)
                .ok_or_else(|| Error::Format("7z block size overflow".into()))?;
        }
    }
    if pieces.is_empty() {
        return Ok(None);
    }
    let encrypted = archive.blocks[block]
        .coders
        .iter()
        .any(|c| c.encoder_method_id() == M::ID_AES256_SHA256);
    let (chain, lost) = chain_of(&archive.blocks[block]);
    let size = archive.blocks[block].get_unpack_size();
    writer.set_content_methods(methods(
        options.password,
        &chain,
        options.level,
        size,
        workers(options.level),
    )?);
    let no_password = Password::empty();
    let password = if encrypted {
        &source.password
    } else {
        &no_password
    };
    let reader = BlockDecoder::new(1, block, archive, password, &mut source.source)
        .into_reader()
        .map_err(|e| upstream(e, encrypted))?;
    let feed = RefCell::new(Feed {
        reader,
        at: 0,
        progress,
        encrypted,
        stopped: None,
    });
    let entries = pieces
        .iter()
        .map(|&(i, _)| {
            let mut entry = archive.files[i].clone();
            entry.name = plan[i].clone().unwrap_or_default();
            entry
        })
        .collect();
    let readers = pieces
        .iter()
        .map(|&(i, start)| {
            let file = &archive.files[i];
            SourceReader::new(Piece {
                feed: &feed,
                name: plan[i].as_deref().unwrap_or_default(),
                start,
                left: file.size,
                crc: file.has_crc.then_some(file.crc as u32),
                hasher: crc32fast::Hasher::new(),
                finished: false,
            })
        })
        .collect();
    let result = writer.push_archive_entries(entries, readers);
    let Feed {
        mut reader,
        stopped,
        ..
    } = feed.into_inner();
    if let Some(error) = stopped {
        return Err(error);
    }
    result.map_err(|e| upstream(e, false))?;
    // The block's own checksum covers entries that have none of their own,
    // and it is only checked once the block has been read to its end.
    if pieces.iter().any(|&(i, _)| !archive.files[i].has_crc) {
        io::copy(&mut reader, &mut io::sink()).map_err(|e| broken(e, encrypted))?;
    }
    let names: Vec<String> = pieces
        .iter()
        .map(|&(i, _)| plan[i].clone().unwrap_or_default())
        .collect();
    expected.extend(names.iter().cloned());
    Ok((!lost.is_empty()).then_some(Recoded {
        entries: names,
        lost,
    }))
}

fn broken(error: io::Error, encrypted: bool) -> Error {
    if encrypted {
        Error::PasswordOrCorrupt
    } else {
        Error::Format(format!("7z: {error}"))
    }
}

/// One decoded block, shared by the entries read out of it in turn.
struct Feed<'r, 'p, 'n> {
    reader: Box<dyn Read + 'r>,
    at: u64,
    progress: &'p mut Counter<'n>,
    encrypted: bool,
    stopped: Option<Error>,
}

impl Feed<'_, '_, '_> {
    fn stop(&mut self, error: Error) -> io::Error {
        let text = error.to_string();
        self.stopped = Some(error);
        io::Error::other(text)
    }

    fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = match self.reader.read(buf) {
            Ok(0) if !buf.is_empty() => {
                let error = if self.encrypted {
                    Error::PasswordOrCorrupt
                } else {
                    Error::Format("truncated 7z entry".into())
                };
                return Err(self.stop(error));
            }
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => 0,
            Err(e) => {
                let error = broken(e, self.encrypted);
                return Err(self.stop(error));
            }
        };
        self.at += n as u64;
        Ok(n)
    }
}

/// One entry of a block being copied: where it starts in the decoded block,
/// how much of it is left, and the checksum it has to come out with.
struct Piece<'f, 'r, 'p, 'n> {
    feed: &'f RefCell<Feed<'r, 'p, 'n>>,
    name: &'f str,
    start: u64,
    left: u64,
    crc: Option<u32>,
    hasher: crc32fast::Hasher,
    finished: bool,
}

impl Read for Piece<'_, '_, '_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.finished {
            return Ok(0);
        }
        let mut feed = self.feed.borrow_mut();
        if feed.stopped.is_some() {
            return Err(io::Error::other("stopped"));
        }
        if let Err(error) = feed.progress.tell(self.name) {
            return Err(feed.stop(error));
        }
        let buf_len = buf.len();
        while feed.at < self.start {
            let skip = usize::try_from(self.start - feed.at)
                .unwrap_or(usize::MAX)
                .min(buf_len);
            feed.read_some(&mut buf[..skip])?;
        }
        if self.left == 0 {
            self.finished = true;
            let crc = std::mem::take(&mut self.hasher).finalize();
            if self.crc.is_some_and(|expected| expected != crc) {
                let error = if feed.encrypted {
                    Error::PasswordOrCorrupt
                } else {
                    Error::Format(format!("checksum mismatch in '{}'", self.name))
                };
                return Err(feed.stop(error));
            }
            feed.progress.done += 1;
            return Ok(0);
        }
        let want = usize::try_from(self.left)
            .unwrap_or(usize::MAX)
            .min(buf_len);
        let n = feed.read_some(&mut buf[..want])?;
        self.hasher.update(&buf[..n]);
        self.left -= n as u64;
        feed.progress.bytes += n as u64;
        Ok(n)
    }
}

/// Reads all of `output` back, with the password it was written with, and
/// holds its names to the ones that were written.
fn verify(
    output: &Path,
    password: Option<&str>,
    mut expected: Vec<String>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<()> {
    let mut written = SevenZArchive::open(io::BufReader::new(fs::File::open(output)?), password)?;
    let mut names: Vec<String> = written.entries().iter().map(|e| e.name.clone()).collect();
    names.sort_unstable();
    expected.sort_unstable();
    if names != expected {
        return Err(Error::Format(
            "the rewritten 7z does not list what was written".into(),
        ));
    }
    written.test(notify)
}
