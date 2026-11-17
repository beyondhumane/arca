use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use arca_core::{Error, Level, Result};
use sevenz_rust2::{
    encoder_options::{AesEncoderOptions, Lzma2Options},
    ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, Password, SourceReader,
};

use crate::{entry_path, upstream, Phase, Progress};

pub struct Source {
    pub path: PathBuf,
    pub name: String,
}

pub struct CreateOptions<'a> {
    pub level: Level,
    pub password: Option<&'a str>,
    pub hide_names: bool,
}

impl Default for CreateOptions<'_> {
    fn default() -> Self {
        Self {
            level: Level::NORMAL,
            password: None,
            hide_names: false,
        }
    }
}

pub fn create_7z(
    output: &Path,
    sources: &[Source],
    options: &CreateOptions<'_>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    create_7z_into(
        io::BufWriter::new(temporary.as_file_mut()),
        sources,
        options,
        notify,
    )?
    .flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(output).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

/// [`create_7z`] into any seekable writer, which it hands back once the
/// archive is complete, so that it can go somewhere other than one file.
pub fn create_7z_into<W: Write + io::Seek>(
    out: W,
    sources: &[Source],
    options: &CreateOptions<'_>,
    notify: &mut dyn FnMut(Progress<'_>) -> bool,
) -> Result<W> {
    if sources.len() > 100_000 {
        return Err(Error::Limit("7z entry count".into()));
    }
    if options.hide_names && options.password.is_none_or(str::is_empty) {
        return Err(Error::Format(
            "hiding 7z names requires a nonempty password".into(),
        ));
    }
    if options.password.is_some_and(str::is_empty) {
        return Err(Error::Format(
            "7z encryption requires a nonempty password".into(),
        ));
    }
    let mut names = std::collections::HashSet::new();
    let mut metadata = Vec::with_capacity(sources.len());
    let mut header_size = 0usize;
    for source in sources {
        if !names.insert(entry_path(&source.name)?) {
            return Err(Error::Format("duplicate 7z entry name".into()));
        }
        header_size = header_size
            .checked_add(source.name.encode_utf16().count() * 2 + 128)
            .ok_or_else(|| Error::Limit("7z header size".into()))?;
        if header_size > 16 * 1024 * 1024 {
            return Err(Error::Limit("7z header size".into()));
        }
        let meta = fs::symlink_metadata(&source.path)?;
        if !meta.is_file() && !meta.is_dir() {
            return Err(Error::Unsupported(
                "7z source is not a regular file or directory".into(),
            ));
        }
        metadata.push(meta);
    }
    if options.password.is_some()
        && !options.hide_names
        && !metadata.iter().any(|m| m.is_file() && m.len() > 0)
    {
        return Err(Error::Unsupported(
            "encrypting only empty entries requires hide_names to validate the password".into(),
        ));
    }
    if !notify(Progress {
        phase: Phase::Create,
        entries_done: 0,
        entries_total: sources.len(),
        bytes_done: 0,
        name: "",
    }) {
        return Err(Error::Cancelled);
    }
    let mut writer = ArchiveWriter::new(out).map_err(|e| upstream(e, false))?;
    writer.set_encrypt_header(options.hide_names);
    let entry_of = |index: usize| {
        let (source, meta) = (&sources[index], &metadata[index]);
        let mut entry = if meta.is_dir() {
            ArchiveEntry::new_directory(&source.name)
        } else {
            ArchiveEntry::new_file(&source.name)
        };
        if let Ok(time) = meta.modified().and_then(|t| {
            sevenz_rust2::NtTime::try_from(t)
                .map_err(|_| io::Error::other("timestamp outside 7z range"))
        }) {
            entry.has_last_modified_date = true;
            entry.last_modified_date = time;
        }
        entry
    };
    // Folders and empty files have no data and go in first, outside any block.
    let (empty, full): (Vec<usize>, Vec<usize>) =
        (0..sources.len()).partition(|&i| metadata[i].is_dir() || metadata[i].len() == 0);
    for &index in &empty {
        if !notify(Progress {
            phase: Phase::Create,
            entries_done: index,
            entries_total: sources.len(),
            bytes_done: 0,
            name: &sources[index].name,
        }) {
            return Err(Error::Cancelled);
        }
        let mut entry = entry_of(index);
        entry.has_stream = false;
        writer
            .push_archive_entry::<io::Empty>(entry, None)
            .map_err(|e| upstream(e, false))?;
    }
    let tracker = std::cell::RefCell::new(Tracker {
        notify,
        total: sources.len(),
        done: empty.len(),
        bytes: 0,
        cancelled: false,
        changed: false,
    });
    let workers = workers(options.level);
    for block in solid_blocks(sources, &metadata, &full) {
        writer.set_content_methods(methods(options, block.executables, workers)?);
        let entries = block.files.iter().map(|&i| entry_of(i)).collect();
        let readers = block
            .files
            .iter()
            .map(|&i| {
                SourceReader::new(Lazy {
                    path: &sources[i].path,
                    name: &sources[i].name,
                    expected: metadata[i].len(),
                    file: None,
                    read: 0,
                    tracker: &tracker,
                })
            })
            .collect();
        let result = writer.push_archive_entries(entries, readers);
        let state = tracker.borrow();
        if state.cancelled {
            return Err(Error::Cancelled);
        }
        if state.changed {
            return Err(Error::Io(io::Error::other(
                "7z source changed size during creation",
            )));
        }
        drop(state);
        result.map_err(|e| upstream(e, false))?;
    }
    let Tracker { notify, bytes, .. } = tracker.into_inner();
    // A header must never reuse the last file's AES salt and IV.
    writer.set_content_methods(methods(options, false, 1)?);
    let mut out = writer.finish()?;
    out.flush()?;
    if !notify(Progress {
        phase: Phase::Create,
        entries_done: sources.len(),
        entries_total: sources.len(),
        bytes_done: bytes,
        name: "",
    }) {
        return Err(Error::Cancelled);
    }
    Ok(out)
}

/// The most a solid block holds. Reading one entry decodes everything before
/// it in its block, so a block of the whole archive would make opening one
/// file of a large archive as slow as extracting all of it.
const SOLID_BYTES: u64 = 256 << 20;

/// Extensions of x86 and x64 machine code, which the BCJ filter makes much
/// easier to compress by turning relative call targets into absolute ones.
/// 7-Zip picks them the same way, by name.
const EXECUTABLE: [&str; 9] = [
    "exe", "dll", "sys", "ocx", "cpl", "scr", "drv", "efi", "mui",
];

struct Block {
    files: Vec<usize>,
    executables: bool,
}

/// Groups the files with data into solid blocks, executables apart so that
/// only they go through the filter, each group sorted by extension and then
/// by name so that similar files sit next to each other in the stream.
fn solid_blocks(sources: &[Source], metadata: &[fs::Metadata], files: &[usize]) -> Vec<Block> {
    let extension = |i: usize| {
        Path::new(&sources[i].name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    let (mut executables, mut rest): (Vec<usize>, Vec<usize>) = files
        .iter()
        .partition(|&&i| EXECUTABLE.contains(&extension(i).as_str()));
    let mut blocks = Vec::new();
    for (group, filtered) in [(&mut rest, false), (&mut executables, true)] {
        group.sort_by_cached_key(|&i| (extension(i), sources[i].name.to_lowercase()));
        let mut block = Vec::new();
        let mut size = 0u64;
        for &i in group.iter() {
            if !block.is_empty() && size + metadata[i].len() > SOLID_BYTES {
                blocks.push(Block {
                    files: std::mem::take(&mut block),
                    executables: filtered,
                });
                size = 0;
            }
            size += metadata[i].len();
            block.push(i);
        }
        if !block.is_empty() {
            blocks.push(Block {
                files: block,
                executables: filtered,
            });
        }
    }
    blocks
}

/// LZMA2 threads for `level`, as many cores as fit in the same memory budget
/// the .xz encoder keeps to.
pub fn workers(level: Level) -> usize {
    arca_xz::workers(level, 0)
}

fn methods(
    options: &CreateOptions<'_>,
    executables: bool,
    workers: usize,
) -> Result<Vec<EncoderConfiguration>> {
    let mut methods = Vec::new();
    if let Some(password) = options.password {
        let mut iv = [0; 16];
        let mut salt = [0; 16];
        getrandom::fill(&mut iv)
            .and_then(|()| getrandom::fill(&mut salt))
            .map_err(|e| Error::Io(io::Error::other(e.to_string())))?;
        methods.push(
            AesEncoderOptions {
                password: Password::from(password),
                iv,
                salt,
                num_cycles_power: 19,
            }
            .into(),
        );
    }
    let level = u32::from(options.level.get());
    if level == 0 {
        methods.push(EncoderMethod::COPY.into());
        return Ok(methods);
    }
    let preset = arca_xz::preset(options.level);
    methods.push(if workers > 1 {
        Lzma2Options::from_level_mt(level, workers as u32, arca_xz::block_size(preset)).into()
    } else {
        Lzma2Options::from_level(level).into()
    });
    // The last method is the first applied to the data.
    if executables {
        methods.push(EncoderMethod::BCJ_X86_FILTER.into());
    }
    Ok(methods)
}

/// What every file of a solid block shares while the block is written: the
/// caller's progress, and whether reading stopped and why.
struct Tracker<'n> {
    notify: &'n mut dyn FnMut(Progress<'_>) -> bool,
    total: usize,
    done: usize,
    bytes: u64,
    cancelled: bool,
    changed: bool,
}

/// A file of a solid block, opened when the block first reads from it so that
/// a block of thousands of files never holds thousands of handles.
struct Lazy<'a, 'n> {
    path: &'a Path,
    name: &'a str,
    expected: u64,
    file: Option<fs::File>,
    read: u64,
    tracker: &'a std::cell::RefCell<Tracker<'n>>,
}

impl Read for Lazy<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut tracker = self.tracker.borrow_mut();
        let progress = Progress {
            phase: Phase::Create,
            entries_done: tracker.done,
            entries_total: tracker.total,
            bytes_done: tracker.bytes,
            name: self.name,
        };
        if !(tracker.notify)(progress) {
            tracker.cancelled = true;
            return Err(io::Error::other("cancelled"));
        }
        if self.file.is_none() {
            self.file = Some(fs::File::open(self.path)?);
        }
        let size = match self.file.as_mut() {
            Some(file) => file.read(buffer)?,
            None => 0,
        };
        self.read += size as u64;
        tracker.bytes += size as u64;
        if size == 0 {
            if self.read != self.expected {
                tracker.changed = true;
                return Err(io::Error::other("7z source changed size during creation"));
            }
            self.file = None;
            tracker.done += 1;
        } else if self.read > self.expected {
            tracker.changed = true;
            return Err(io::Error::other("7z source changed size during creation"));
        }
        Ok(size)
    }
}
