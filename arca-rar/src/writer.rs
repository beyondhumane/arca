use super::{create_limits as limits, reader, CreateOptions, Progress, RarArchive, Source};
use arca_core::{Error, Format, Level, Result};
use rars::rar50::{
    write_streaming_archive_with_progress, ArchiveEntry, ArchiveExtras, FilterPolicy, WriterOptions,
};
use rars::{
    ArchiveVersion, EntrySource, ErrorKind, FeatureSet, WriteCancellation, WriteProgress,
    WriteProgressEvent, WriterResources,
};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER_ALLOWANCE: u64 = 128;
const DOS_ARCHIVE_ATTR: u64 = 0x20;
const RAR50_HOST_UNIX: u64 = 1;

/// What one source looked like when it was scanned. The opener compares the
/// file against this every time the writer reopens it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    len: u64,
    modified: Option<SystemTime>,
    identity: Option<(u64, u64)>,
}

impl Snapshot {
    fn of(meta: &fs::Metadata) -> Self {
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            identity: identity(meta),
        }
    }
}

#[cfg(unix)]
fn identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(unix)]
fn unix_mode(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode())
}

#[cfg(not(unix))]
fn unix_mode(_meta: &fs::Metadata) -> Option<u32> {
    None
}

struct Planned {
    name: String,
    path: PathBuf,
    is_dir: bool,
    snapshot: Snapshot,
    mode: Option<u32>,
}

pub(super) fn create(
    output: &Path,
    sources: &[Source],
    options: &CreateOptions,
    progress: &Progress<'_>,
) -> Result<()> {
    let parent = parent_dir(output)?;
    let planned = plan(output, &parent, sources, progress)?;
    let total = planned.len();
    let mut staging = tempfile::Builder::new()
        .prefix(".arca-")
        .suffix(".rar.part")
        .tempfile_in(&parent)?;
    let reporter = Reporter::new(progress, total);
    let resources = WriterResources::new(limits::WRITER_MEMORY_BYTES)
        .with_max_memory_bytes(limits::WRITER_MEMORY_BYTES)
        .with_max_spool_bytes(limits::MAX_SPOOL_BYTES)
        .with_max_prepared_header_bytes(limits::MAX_HEADER_BYTES)
        .with_max_preparation_bytes(limits::MAX_HEADER_BYTES)
        .with_temp_dir(&parent)
        .with_cancellation(reporter.token.clone());
    let entries = entries(&planned)?;
    reporter.poll();
    if reporter.is_cancelled() {
        return Err(Error::Cancelled);
    }
    write(
        staging.as_file_mut(),
        &entries,
        options,
        &resources,
        &reporter,
    )
    .map_err(|error| reporter.finish(error))?;
    if reporter.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let staging = staging.into_temp_path();
    verify(&staging, &planned, progress)?;
    if !progress(total, total, "") {
        return Err(Error::Cancelled);
    }
    publish(staging, output)
}

/// Streams the members into `file` through the RAR 5 writer's public
/// streaming entry point rather than `Builder`, which forces an automatic
/// filter search for non-solid archives. That search plans every member as a
/// whole and, in rars 0.10.0, reserves 0 bytes of the managed-memory ledger
/// for a zero-length member, so its spool charge fails. Without a filter
/// policy every member takes the streaming block path, whose workspace is
/// priced per batch, and the hard ledger holds for empty files and
/// directories too. The trade is that no data filter is tried for any level.
fn write(
    file: &mut File,
    entries: &[ArchiveEntry],
    options: &CreateOptions,
    resources: &WriterResources,
    reporter: &Reporter<'_>,
) -> rars::Result<()> {
    let level = match options.level {
        Level::Store => 0,
        Level::Fast => 1,
        Level::Normal => 3,
        Level::Best => 5,
    };
    let writer_options = WriterOptions::new(ArchiveVersion::Rar50, FeatureSet::store_only())
        .with_compression_level(level);
    let extras = ArchiveExtras::default().with_filter_policy(FilterPolicy::None);
    write_streaming_archive_with_progress(
        entries,
        writer_options,
        extras,
        resources,
        Some(reporter),
        file,
    )?;
    file.sync_all()?;
    Ok(())
}

fn parent_dir(output: &Path) -> Result<PathBuf> {
    if output.file_name().is_none() {
        return Err(Error::Format(format!(
            "'{}' is not a file name for a RAR archive",
            output.display()
        )));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    match fs::metadata(parent) {
        Ok(meta) if meta.is_dir() => Ok(parent.to_path_buf()),
        Ok(_) => Err(Error::Io(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("'{}' is not a directory", parent.display()),
        ))),
        Err(e) => Err(Error::Io(io::Error::new(
            e.kind(),
            format!("output directory '{}': {e}", parent.display()),
        ))),
    }
}

fn exists(output: &Path) -> Error {
    Error::Io(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "'{}' already exists; Arca creates new RAR archives only and never replaces a file",
            output.display()
        ),
    ))
}

fn plan(
    output: &Path,
    parent: &Path,
    sources: &[Source],
    progress: &Progress<'_>,
) -> Result<Vec<Planned>> {
    if sources.len() > limits::MAX_ENTRIES {
        return Err(Error::Limit(format!(
            "RAR archive would hold {} members; the limit is {}",
            sources.len(),
            limits::MAX_ENTRIES
        )));
    }
    if Format::detect(output) == Some(Format::Cbr) {
        return Err(Error::Unsupported(
            "CBR creation is not supported; create a .rar archive instead".into(),
        ));
    }
    if fs::symlink_metadata(output).is_ok() {
        return Err(exists(output));
    }
    let parent_identity = identity(&fs::metadata(parent)?);
    let total = sources.len();
    let mut planned = Vec::with_capacity(total);
    let mut names = HashSet::new();
    let mut files = HashSet::new();
    let mut header_bytes = 0u64;
    let mut total_bytes = 0u64;
    for (index, source) in sources.iter().enumerate() {
        if !progress(index, total, &source.name) {
            return Err(Error::Cancelled);
        }
        let name = validate_name(&source.name)?;
        header_bytes = header_bytes
            .checked_add(name.len() as u64 + HEADER_ALLOWANCE)
            .filter(|n| *n <= limits::MAX_HEADER_BYTES)
            .ok_or_else(|| Error::Limit("RAR header metadata exceeds 64 MiB".into()))?;
        let folded = name.to_lowercase();
        if !names.insert(folded.clone()) {
            return Err(Error::Format(format!(
                "duplicate RAR member name: '{name}'"
            )));
        }
        let meta = fs::symlink_metadata(&source.path).map_err(|e| {
            Error::Io(io::Error::new(
                e.kind(),
                format!("RAR source '{}': {e}", source.path.display()),
            ))
        })?;
        if meta.file_type().is_symlink() {
            return Err(Error::Unsupported(format!(
                "RAR source '{}' is a symbolic link; links are not followed or stored",
                source.path.display()
            )));
        }
        if !meta.is_file() && !meta.is_dir() {
            return Err(Error::Unsupported(format!(
                "RAR source '{}' is not a regular file or directory",
                source.path.display()
            )));
        }
        if meta.is_dir() && parent_identity.is_some() && identity(&meta) == parent_identity {
            return Err(Error::Format(format!(
                "RAR source '{}' is the output directory",
                source.path.display()
            )));
        }
        if meta.is_file() {
            if meta.len() > limits::MAX_MEMBER_BYTES {
                return Err(Error::Limit(format!("RAR member '{name}' exceeds 4 GiB")));
            }
            total_bytes = total_bytes
                .checked_add(meta.len())
                .filter(|n| *n <= limits::MAX_TOTAL_BYTES)
                .ok_or_else(|| Error::Limit("RAR archive input exceeds 16 GiB".into()))?;
            files.insert(folded);
        }
        planned.push(Planned {
            name,
            path: source.path.clone(),
            is_dir: meta.is_dir(),
            snapshot: Snapshot::of(&meta),
            mode: unix_mode(&meta),
        });
    }
    for name in &names {
        let mut parent = Path::new(name).parent();
        while let Some(path) = parent {
            if files.contains(path.to_string_lossy().as_ref()) {
                return Err(Error::Format(format!(
                    "RAR file/directory collision: '{name}'"
                )));
            }
            parent = path.parent();
        }
    }
    Ok(planned)
}

/// The archive name for `name`, or why it cannot be one. The rules are the
/// reader's, so the verification pass accepts every name that passes here.
fn validate_name(name: &str) -> Result<String> {
    if name.len() > limits::MAX_NAME_BYTES {
        return Err(Error::Limit(format!("name of {} bytes", name.len())));
    }
    let name = name.replace('\\', "/");
    if name.chars().any(|c| matches!(c, '\u{fffe}' | '\u{ffff}')) {
        return Err(Error::Format(format!("unsafe RAR path: '{name}'")));
    }
    reader::validate_name(&name)?;
    let path = arca_core::safe_name(&name)?;
    let mut parts = Vec::new();
    for part in path.components() {
        parts.push(
            part.as_os_str()
                .to_str()
                .ok_or_else(|| Error::Format(format!("unsafe RAR path: '{name}'")))?,
        );
    }
    Ok(parts.join("/"))
}

fn entries(planned: &[Planned]) -> Result<Vec<ArchiveEntry>> {
    planned
        .iter()
        .map(|plan| {
            let name =
                rars::validate_entry_name(plan.name.as_bytes().to_vec()).map_err(write_error)?;
            let mtime = plan
                .snapshot
                .modified
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|t| u32::try_from(t.as_secs()).ok());
            let (attributes, host_os) = match plan.mode {
                Some(mode) => (u64::from(mode), RAR50_HOST_UNIX),
                None => (DOS_ARCHIVE_ATTR, 0),
            };
            let source = if plan.is_dir {
                EntrySource::from_bytes(Vec::new())
            } else {
                source(plan)
            };
            Ok(ArchiveEntry::new(name, source)
                .with_directory(plan.is_dir)
                .with_mtime(mtime)
                .with_attributes(attributes)
                .with_host_os(host_os))
        })
        .collect()
}

/// Reopens the file for every pass and refuses to read it once it stops
/// being the file that was scanned: a different inode, size or mtime means
/// the input moved under the writer, including a hard link made to the
/// archive being written.
fn source(plan: &Planned) -> EntrySource {
    let path = plan.path.clone();
    let expected = plan.snapshot.clone();
    EntrySource::from_opener(expected.len, move || {
        let file = open_regular(&path)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(rars::Error::SourceChanged(
                "source is no longer a regular file",
            ));
        }
        if Snapshot::of(&meta) != expected {
            return Err(rars::Error::SourceChanged(
                "source file changed after it was scanned",
            ));
        }
        Ok(Box::new(file))
    })
}

/// Opens a source without following a final symbolic link and without
/// blocking on a FIFO, so a path swapped for a link or a pipe after the scan
/// is refused by the metadata check instead of followed or waited on.
fn open_regular(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(unix, windows)))]
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source is a symbolic link",
        ));
    }
    options.open(path).map_err(|e| {
        #[cfg(unix)]
        if e.raw_os_error() == Some(libc::ELOOP) {
            return io::Error::new(e.kind(), "source became a symbolic link");
        }
        e
    })
}

fn write_error(error: rars::Error) -> Error {
    match error.kind() {
        ErrorKind::Cancelled => Error::Cancelled,
        ErrorKind::ResourceLimit => Error::Limit(error.to_string()),
        ErrorKind::Io => Error::Io(io::Error::other(error.to_string())),
        ErrorKind::SourceChanged => Error::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("RAR source changed while writing: {error}"),
        )),
        ErrorKind::UnsupportedFeature | ErrorKind::UnsupportedFormat | ErrorKind::WriterFailure => {
            Error::Unsupported(format!("RAR writer: {error}"))
        }
        _ => Error::Format(format!("RAR writer: {error}")),
    }
}

/// Reopens the staged archive with the reader and decodes every member,
/// then checks the listing against what was asked for.
fn verify(staging: &Path, planned: &[Planned], progress: &Progress<'_>) -> Result<()> {
    let total = planned.len();
    let verifying = |_: usize, _: usize, _: &str| progress(total, total, "");
    let failed = |e: Error| match e {
        Error::Cancelled => Error::Cancelled,
        e => Error::Format(format!("the new RAR archive failed verification: {e}")),
    };
    let archive = RarArchive::open_with_progress(staging, None, &verifying).map_err(failed)?;
    archive.test(None, &verifying).map_err(failed)?;
    let entries = archive.entries();
    if entries.len() != planned.len() {
        return Err(failed(Error::Format(format!(
            "{} members written, {} expected",
            entries.len(),
            planned.len()
        ))));
    }
    for (entry, plan) in entries.iter().zip(planned) {
        let size = if plan.is_dir { 0 } else { plan.snapshot.len };
        if entry.name != plan.name || entry.is_dir != plan.is_dir || entry.size != size {
            return Err(failed(Error::Format(format!(
                "member '{}' does not match source '{}'",
                entry.name, plan.name
            ))));
        }
    }
    Ok(())
}

/// Gives the finished archive its name without ever exposing a partial one
/// or replacing a file that appeared since the plan. `persist_noclobber`
/// renames with `RENAME_NOREPLACE` where the kernel offers it, links the new
/// name and unlinks the staging name elsewhere on Unix, and moves without
/// `MOVEFILE_REPLACE_EXISTING` on Windows; each either creates the name
/// complete or fails, and a failure deletes the staging file.
fn publish(staging: tempfile::TempPath, output: &Path) -> Result<()> {
    staging.persist_noclobber(output).map_err(|failure| {
        if failure.error.kind() == io::ErrorKind::AlreadyExists {
            exists(output)
        } else {
            Error::Io(failure.error)
        }
    })
}

/// Bridges the writer's events to Arca's `(done, total, name)` callback and
/// turns a `false` answer into both a cancellation token (seen during resource
/// waits) and a cancelled flag (seen between chunks).
struct Reporter<'a> {
    progress: &'a Progress<'a>,
    total: usize,
    done: AtomicUsize,
    name: Mutex<String>,
    cancelled: AtomicBool,
    token: WriteCancellation,
}

impl<'a> Reporter<'a> {
    fn new(progress: &'a Progress<'a>, total: usize) -> Self {
        Self {
            progress,
            total,
            done: AtomicUsize::new(0),
            name: Mutex::new(String::new()),
            cancelled: AtomicBool::new(false),
            token: WriteCancellation::new(),
        }
    }

    fn poll(&self) {
        if self.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let name = self.name.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let done = self.done.load(Ordering::Relaxed).min(self.total);
        if !(self.progress)(done, self.total, &name) {
            self.cancelled.store(true, Ordering::Relaxed);
            self.token.cancel();
        }
    }

    fn finish(&self, error: rars::Error) -> Error {
        if self.is_cancelled() {
            Error::Cancelled
        } else {
            write_error(error)
        }
    }
}

impl WriteProgress for Reporter<'_> {
    fn report(&self, event: WriteProgressEvent<'_>) {
        match event {
            WriteProgressEvent::EntryStarted { name, .. } => {
                *self.name.lock().unwrap_or_else(|e| e.into_inner()) =
                    String::from_utf8_lossy(name).into_owned();
            }
            WriteProgressEvent::EntryFinished { .. } => {
                self.done.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        self.poll();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || self.token.is_cancelled()
    }
}
