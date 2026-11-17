use std::{
    fs,
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

use arca_core::{Error, Level, Result};
use sevenz_rust2::{
    encoder_options::{
        AesEncoderOptions, DeflateOptions, DeltaOptions, Lzma2Options, LzmaOptions, PpmdOptions,
    },
    ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, Password, SourceReader,
};

use crate::{entry_path, upstream, Phase, Progress};

pub struct Source {
    pub path: PathBuf,
    pub name: String,
}

/// What the data of a new 7z is compressed with. Every one of them is a
/// method 7-Zip itself reads; Zstandard is left out because it does not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compression {
    #[default]
    Lzma2,
    Lzma,
    Ppmd,
    Bzip2,
    Deflate,
    Store,
}

impl Compression {
    pub const ALL: [Compression; 6] = [
        Compression::Lzma2,
        Compression::Lzma,
        Compression::Ppmd,
        Compression::Bzip2,
        Compression::Deflate,
        Compression::Store,
    ];

    pub fn name(self) -> &'static str {
        self.stage().name()
    }

    /// Whether this build can write it: BZip2 comes with `codecs-native`.
    pub fn available(self) -> bool {
        self != Compression::Bzip2 || cfg!(feature = "codecs-native")
    }

    /// The threads it compresses on at `level`. Only LZMA2 uses more than one.
    pub fn threads(self, level: Level) -> usize {
        if self == Compression::Lzma2 && level != Level::STORE {
            workers(level)
        } else {
            1
        }
    }

    pub(crate) fn stage(self) -> Stage {
        match self {
            Compression::Lzma2 => Stage::Lzma2,
            Compression::Lzma => Stage::Lzma,
            Compression::Ppmd => Stage::Ppmd,
            Compression::Bzip2 => Stage::Bzip2,
            Compression::Deflate => Stage::Deflate,
            Compression::Store => Stage::Copy,
        }
    }
}

pub struct CreateOptions<'a> {
    pub level: Level,
    pub password: Option<&'a str>,
    pub hide_names: bool,
    /// Ignored at [`Level::STORE`], which always stores.
    pub compression: Compression,
}

impl Default for CreateOptions<'_> {
    fn default() -> Self {
        Self {
            level: Level::NORMAL,
            password: None,
            hide_names: false,
            compression: Compression::Lzma2,
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
    if !options.compression.available() {
        return Err(Error::Unsupported(format!(
            "{} in 7z needs a build with codecs-native",
            options.compression.name()
        )));
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
    let entry_of = |index: usize| new_entry(&sources[index].name, &metadata[index]);
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
    push_files(&mut writer, sources, &metadata, &full, options, &tracker)?;
    let Tracker { notify, bytes, .. } = tracker.into_inner();

    writer.set_content_methods(header_methods(options.password)?);
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

/// The entry for a file or folder going in under `name`, with its time.
pub(crate) fn new_entry(name: &str, meta: &fs::Metadata) -> ArchiveEntry {
    let mut entry = if meta.is_dir() {
        ArchiveEntry::new_directory(name)
    } else {
        ArchiveEntry::new_file(name)
    };
    if let Ok(time) = meta.modified().and_then(|t| {
        sevenz_rust2::NtTime::try_from(t)
            .map_err(|_| io::Error::other("timestamp outside 7z range"))
    }) {
        entry.has_last_modified_date = true;
        entry.last_modified_date = time;
    }
    entry
}

/// The files of `sources` picked by `full`, which all have data, in solid
/// blocks read from the disk as they are compressed.
pub(crate) fn push_files<W: Write + Seek>(
    writer: &mut ArchiveWriter<W>,
    sources: &[Source],
    metadata: &[fs::Metadata],
    full: &[usize],
    options: &CreateOptions<'_>,
    tracker: &std::cell::RefCell<Tracker<'_>>,
) -> Result<()> {
    let workers = workers(options.level);
    let main = options.compression.stage();
    let filter = main != Stage::Copy && options.level != Level::STORE;
    for block in solid_blocks(sources, metadata, full, filter.then_some(tracker))? {
        let mut chain = vec![main];
        chain.extend(block.filter.map(Stage::Filter));
        writer.set_content_methods(methods(
            options.password,
            &chain,
            options.level,
            block.size,
            workers,
        )?);
        let entries = block
            .files
            .iter()
            .map(|&i| new_entry(&sources[i].name, &metadata[i]))
            .collect();
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
                    tracker,
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
    Ok(())
}

/// The most a solid block holds. Reading one entry decodes everything before
/// it in its block, so a block of the whole archive would make opening one
/// file of a large archive as slow as extracting all of it.
const SOLID_BYTES: u64 = 256 << 20;

/// Extensions of x86 and x64 programs, which get the x86 filter when their
/// first bytes say nothing about the machine they are for.
const EXECUTABLE: [&str; 9] = [
    "exe", "dll", "sys", "ocx", "cpl", "scr", "drv", "efi", "mui",
];

/// A filter that goes over the data before it is compressed. The branch
/// filters turn the relative targets of calls and jumps into absolute ones,
/// which repeat; Delta stores each byte as its difference from the one a
/// sample earlier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Filter {
    X86,
    Arm,
    ArmThumb,
    Arm64,
    Ppc,
    Sparc,
    Ia64,
    Riscv,
    Delta(u16),
}

/// One coder of a block, in the order a decoder undoes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Copy,
    Lzma,
    Lzma2,
    Ppmd,
    Bzip2,
    Deflate,
    Zstd,
    Filter(Filter),
}

impl Stage {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Stage::Copy => "Copy",
            Stage::Lzma => "LZMA",
            Stage::Lzma2 => "LZMA2",
            Stage::Ppmd => "PPMd",
            Stage::Bzip2 => "BZip2",
            Stage::Deflate => "Deflate",
            Stage::Zstd => "Zstandard",
            Stage::Filter(Filter::X86) => "BCJ",
            Stage::Filter(Filter::Arm) => "ARM",
            Stage::Filter(Filter::ArmThumb) => "ARMT",
            Stage::Filter(Filter::Arm64) => "ARM64",
            Stage::Filter(Filter::Ppc) => "PPC",
            Stage::Filter(Filter::Sparc) => "SPARC",
            Stage::Filter(Filter::Ia64) => "IA64",
            Stage::Filter(Filter::Riscv) => "RISCV",
            Stage::Filter(Filter::Delta(_)) => "Delta",
        }
    }

    /// The stage of a coder an archive names, if this build can encode it.
    pub(crate) fn of(id: &[u8], properties: &[u8]) -> Option<Stage> {
        use EncoderMethod as M;
        Some(match id {
            M::ID_COPY => Stage::Copy,
            M::ID_LZMA => Stage::Lzma,
            M::ID_LZMA2 => Stage::Lzma2,
            M::ID_PPMD => Stage::Ppmd,
            M::ID_BZIP2 if cfg!(feature = "codecs-native") => Stage::Bzip2,
            M::ID_DEFLATE => Stage::Deflate,
            M::ID_ZSTD if cfg!(feature = "codecs-native") => Stage::Zstd,
            M::ID_BCJ_X86 => Stage::Filter(Filter::X86),
            M::ID_BCJ_ARM => Stage::Filter(Filter::Arm),
            M::ID_BCJ_ARM_THUMB => Stage::Filter(Filter::ArmThumb),
            M::ID_BCJ_ARM64 if properties.is_empty() => Stage::Filter(Filter::Arm64),
            M::ID_BCJ_PPC => Stage::Filter(Filter::Ppc),
            M::ID_BCJ_SPARC => Stage::Filter(Filter::Sparc),
            M::ID_BCJ_IA64 => Stage::Filter(Filter::Ia64),
            M::ID_BCJ_RISCV if properties.is_empty() => Stage::Filter(Filter::Riscv),
            M::ID_DELTA => Stage::Filter(Filter::Delta(
                properties.first().map_or(1, |&b| u16::from(b) + 1),
            )),
            _ => return None,
        })
    }
}

/// The filter 7-Zip would pick for a file that starts with `head`, out of
/// the program headers of Windows, Linux and macOS and the format of WAV
/// audio, or `None` for a head that is none of those. RISC-V code gets no
/// filter, since 7-Zip has only read that one since version 24.
pub(crate) fn sniff(head: &[u8]) -> Option<Option<Filter>> {
    let u16le = |at: usize| Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?));
    let u32le = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?));
    if head.len() >= 512 && u16le(0)? == 0x5A4D {
        let pe = usize::try_from(u32le(0x3C)?).ok()?;
        if pe & 7 != 0 || u32le(pe)? != 0x4550 || u16le(pe + 20)? > 1 << 10 {
            return None;
        }
        if !matches!(u16le(pe + 24)?, 0x10B | 0x20B) {
            return None;
        }
        return Some(match u16le(pe + 4)? {
            0x014C | 0x8664 => Some(Filter::X86),
            0xAA64 => Some(Filter::Arm64),
            0x01C0 | 0x01C2 => Some(Filter::Arm),
            0x01C4 => Some(Filter::ArmThumb),
            0x0200 => Some(Filter::Ia64),
            _ => None,
        });
    }
    if head.len() >= 512 && head.starts_with(b"\x7fELF") {
        if !matches!(head[4], 1 | 2) || !matches!(head[5], 1 | 2) || head[6] != 1 {
            return None;
        }
        let be = head[5] == 2;
        let pair = [head[0x12], head[0x13]];
        let machine = if be {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        };
        return Some(match machine {
            3 | 6 | 62 => Some(Filter::X86),
            2 | 18 | 43 => Some(Filter::Sparc),
            20 | 21 if be => Some(Filter::Ppc),
            40 if !be => Some(Filter::Arm),
            183 if !be => Some(Filter::Arm64),
            _ => None,
        });
    }
    if head.len() >= 512 {
        let be = match u32le(0)? {
            0xCEFA_EDFE | 0xCFFA_EDFE => true,
            0xFEED_FACE | 0xFEED_FACF => false,
            _ => return sniff_wav(head).map(Some),
        };
        let word = |at: usize| {
            let bytes: [u8; 4] = head.get(at..at + 4)?.try_into().ok()?;
            Some(if be {
                u32::from_be_bytes(bytes)
            } else {
                u32::from_le_bytes(bytes)
            })
        };
        if word(16)? > 1 << 18 || word(20)? > 1 << 24 {
            return None;
        }
        return Some(match word(4)? {
            7 | 0x0100_0007 => Some(Filter::X86),
            12 if !be => Some(Filter::Arm),
            0x0100_000C if !be => Some(Filter::Arm64),
            14 if be => Some(Filter::Sparc),
            18 | 0x0100_0012 if be => Some(Filter::Ppc),
            _ => None,
        });
    }
    sniff_wav(head).map(Some)
}

/// Delta at the distance of one sample frame, for PCM audio with a data
/// chunk among the first few.
fn sniff_wav(head: &[u8]) -> Option<Filter> {
    let u16le = |at: usize| Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?));
    let u32le = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?));
    if head.len() < 0x2C || &head[..4] != b"RIFF" || &head[8..16] != b"WAVEfmt " {
        return None;
    }
    let format = u32le(0x10)?;
    if !(0x10..=0x12).contains(&format) || u16le(0x14)? != 1 {
        return None;
    }
    let bits = u16le(0x22)?;
    let delta = u16le(0x16)?.checked_mul(bits >> 3)?;
    if bits & 7 != 0 || !(1..=256).contains(&delta) {
        return None;
    }
    let mut at = 0x14 + format as usize;
    for _ in 0..10 {
        let size = u32le(at + 4)?;
        if u32le(at)? == u32::from_le_bytes(*b"data") {
            return Some(Filter::Delta(delta));
        }
        if size > 1 << 16 {
            return None;
        }
        at += size as usize + 8;
    }
    None
}

/// The filter for the file at `path`: what its first bytes call for, or
/// for a program that does not say, x86 by its extension. A file that
/// cannot be read gets none here and fails where it is compressed.
fn filter_of(path: &Path, name: &str, len: u64) -> Option<Filter> {
    let mut head = [0u8; 4096];
    let mut read = 0;
    if len >= 0x2C {
        if let Ok(file) = fs::File::open(path) {
            let mut file = file.take(head.len() as u64);
            while read < head.len() {
                match file.read(&mut head[read..]) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => read += n,
                }
            }
        }
    }
    sniff(&head[..read]).unwrap_or_else(|| {
        let extension = Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        EXECUTABLE
            .contains(&extension.as_str())
            .then_some(Filter::X86)
    })
}

struct Block {
    files: Vec<usize>,
    filter: Option<Filter>,
    size: u64,
}

/// Groups the files with data into solid blocks, one group for each filter
/// so that a filter only goes over the files it is for, each group sorted
/// by extension and then by name so that similar files sit next to each
/// other in the stream. Without a `tracker` to tell how far looking has got,
/// nothing is looked at.
fn solid_blocks(
    sources: &[Source],
    metadata: &[fs::Metadata],
    files: &[usize],
    tracker: Option<&std::cell::RefCell<Tracker<'_>>>,
) -> Result<Vec<Block>> {
    let extension = |i: usize| {
        Path::new(&sources[i].name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    let mut groups: Vec<(Option<Filter>, Vec<usize>)> = Vec::new();
    for &i in files {
        let kind = match tracker {
            Some(tracker) => {
                let mut tracker = tracker.borrow_mut();
                let progress = Progress {
                    phase: Phase::Create,
                    entries_done: tracker.done,
                    entries_total: tracker.total,
                    bytes_done: tracker.bytes,
                    name: &sources[i].name,
                };
                if !(tracker.notify)(progress) {
                    return Err(Error::Cancelled);
                }
                filter_of(&sources[i].path, &sources[i].name, metadata[i].len())
            }
            None => None,
        };
        match groups.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, group)) => group.push(i),
            None => groups.push((kind, vec![i])),
        }
    }
    groups.sort_by_key(|(kind, _)| *kind);
    let mut blocks = Vec::new();
    for (kind, mut group) in groups {
        group.sort_by_cached_key(|&i| (extension(i), sources[i].name.to_lowercase()));
        let mut block = Vec::new();
        let mut size = 0u64;
        for &i in group.iter() {
            if !block.is_empty() && size + metadata[i].len() > SOLID_BYTES {
                blocks.push(Block {
                    files: std::mem::take(&mut block),
                    filter: kind,
                    size,
                });
                size = 0;
            }
            size += metadata[i].len();
            block.push(i);
        }
        if !block.is_empty() {
            blocks.push(Block {
                files: block,
                filter: kind,
                size,
            });
        }
    }
    Ok(blocks)
}

/// LZMA2 threads for `level`, as many cores as fit in the same memory budget
/// the .xz encoder keeps to.
pub fn workers(level: Level) -> usize {
    arca_xz::workers(level, 0)
}

/// The largest PPMd model Arca writes, 7-Zip's at its own level 9. The
/// reader takes models up to 256 MiB, so whatever Arca writes it can read.
pub const PPMD_MEMORY_MAX: u32 = 192 << 20;

/// PPMd's model order and memory for `level`, as 7-Zip sets them, with the
/// memory cut down to what `size` bytes can fill: a model larger than
/// sixteen times the data costs memory, here and in every decoder, and
/// gains nothing.
pub fn ppmd_parameters(level: Level, size: u64) -> (u32, u32) {
    const ORDERS: [u32; 10] = [3, 4, 4, 5, 5, 6, 8, 16, 24, 32];
    let level = level.get().min(9);
    let mut memory = if level >= 9 {
        PPMD_MEMORY_MAX
    } else {
        1 << (level + 19)
    };
    if u64::from(memory / 16) > size {
        if let Some(fit) = (16..=31)
            .map(|bit| 1u32 << bit)
            .find(|&m| size <= u64::from(m / 16))
        {
            memory = memory.min(fit);
        }
    }
    (ORDERS[usize::from(level)], memory)
}

/// The LZMA dictionary for `level`, no larger than the smallest power of two
/// that holds `size` bytes: a dictionary longer than the data finds nothing
/// more, and the decoder has to allocate all of it.
fn dictionary(level: Level, size: u64) -> u32 {
    let full = arca_xz::dictionary(u32::from(level.get()));
    let fit = size
        .max(4096)
        .checked_next_power_of_two()
        .unwrap_or(u64::MAX);
    u32::try_from(full.min(fit)).unwrap_or(u32::MAX)
}

/// The coders of one block: AES first when there is a password, then
/// `chain` as it is undone when reading. `size` is how much data the block
/// holds, or at least how much it may. At [`Level::STORE`] everything is
/// stored, with no filter.
pub(crate) fn methods(
    password: Option<&str>,
    chain: &[Stage],
    level: Level,
    size: u64,
    workers: usize,
) -> Result<Vec<EncoderConfiguration>> {
    let mut methods = Vec::new();
    if let Some(password) = password {
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
    if level == Level::STORE {
        methods.push(EncoderMethod::COPY.into());
        return Ok(methods);
    }
    let preset = u32::from(level.get());
    for &stage in chain {
        methods.push(match stage {
            Stage::Copy => EncoderMethod::COPY.into(),
            Stage::Lzma2 => {
                let mut options = if workers > 1 {
                    Lzma2Options::from_level_mt(
                        preset,
                        workers as u32,
                        arca_xz::block_size(arca_xz::preset(level)),
                    )
                } else {
                    Lzma2Options::from_level(preset)
                };
                options.set_dictionary_size(dictionary(level, size));
                options.into()
            }
            Stage::Lzma => {
                let mut options = LzmaOptions::from_level(preset);
                options.set_dictionary_size(dictionary(level, size));
                options.into()
            }
            Stage::Ppmd => {
                let (order, memory) = ppmd_parameters(level, size);
                PpmdOptions::from_order_memory_size(order, memory).into()
            }
            Stage::Deflate => DeflateOptions::from_level(level.to_flate2()).into(),
            #[cfg(feature = "codecs-native")]
            Stage::Bzip2 => sevenz_rust2::encoder_options::Bzip2Options::from_level(preset).into(),
            #[cfg(feature = "codecs-native")]
            Stage::Zstd => {
                sevenz_rust2::encoder_options::ZstandardOptions::from_level(level.to_zstd() as u32)
                    .into()
            }
            #[cfg(not(feature = "codecs-native"))]
            Stage::Bzip2 | Stage::Zstd => {
                return Err(Error::Unsupported(format!(
                    "{} in 7z needs a build with codecs-native",
                    stage.name()
                )))
            }
            Stage::Filter(filter) => match filter {
                Filter::X86 => EncoderMethod::BCJ_X86_FILTER.into(),
                Filter::Arm => EncoderMethod::BCJ_ARM_FILTER.into(),
                Filter::ArmThumb => EncoderMethod::BCJ_ARM_THUMB_FILTER.into(),
                Filter::Arm64 => EncoderMethod::BCJ_ARM64_FILTER.into(),
                Filter::Ppc => EncoderMethod::BCJ_PPC_FILTER.into(),
                Filter::Sparc => EncoderMethod::BCJ_SPARC_FILTER.into(),
                Filter::Ia64 => EncoderMethod::BCJ_IA64_FILTER.into(),
                Filter::Riscv => EncoderMethod::BCJ_RISCV_FILTER.into(),
                Filter::Delta(distance) => DeltaOptions::from_distance(u32::from(distance)).into(),
            },
        });
    }
    Ok(methods)
}

/// What the header goes with: only a password's AES matters there, and it
/// must never reuse the last block's salt and IV.
pub(crate) fn header_methods(password: Option<&str>) -> Result<Vec<EncoderConfiguration>> {
    methods(password, &[], Level::STORE, 0, 1)
}

/// What every file of a solid block shares while the block is written: the
/// caller's progress, and whether reading stopped and why.
pub(crate) struct Tracker<'n> {
    pub(crate) notify: &'n mut dyn FnMut(Progress<'_>) -> bool,
    pub(crate) total: usize,
    pub(crate) done: usize,
    pub(crate) bytes: u64,
    pub(crate) cancelled: bool,
    pub(crate) changed: bool,
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

#[cfg(test)]
mod tests {
    use super::{sniff, Filter};

    fn heads() -> Vec<Vec<u8>> {
        let mut pe = vec![0u8; 4096];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0xAA64u16.to_le_bytes());
        pe[0x98..0x9A].copy_from_slice(&0x10Bu16.to_le_bytes());
        let mut elf = vec![0u8; 600];
        elf[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        elf[0x12..0x14].copy_from_slice(&20u16.to_be_bytes());
        let mut macho = vec![0u8; 600];
        macho[..4].copy_from_slice(&[0xCE, 0xFA, 0xED, 0xFE]);
        macho[4] = 12;
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0".to_vec();
        wav.extend([0; 8]);
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data\0\0\0\0");
        vec![pe, elf, macho, wav]
    }

    #[test]
    fn heads_are_recognized_and_damaged_ones_never_panic() {
        let expected = [Filter::Arm64, Filter::Ppc, Filter::Arm, Filter::Delta(4)];
        for (head, filter) in heads().iter().zip(expected) {
            assert_eq!(sniff(head), Some(Some(filter)));
            for end in 0..head.len().min(700) {
                let _ = sniff(&head[..end]);
            }
            for at in 0..head.len().min(700) {
                for byte in [0, 0x7f, 0xff] {
                    let mut bad = head.clone();
                    bad[at] = byte;
                    let _ = sniff(&bad);
                }
            }
        }
        let mut far = heads()[0].clone();
        far[0x3C..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(sniff(&far), None);
        let mut looping = heads()[3].clone();
        looping[0x24..0x28].copy_from_slice(b"LIST");
        looping[0x28..0x2C].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(sniff(&looping), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(&[0xCA, 0xFE, 0xBA, 0xBE].repeat(200)), None);
    }
}
