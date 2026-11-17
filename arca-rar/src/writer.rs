use super::{
    create_limits as limits, reader, volume_stem, CreateOptions, Progress, RarArchive, Source,
};
use arca_core::{Error, Format, Result};
use rars::rar50::{
    write_streaming_archive_with_progress, write_streaming_volumes_sized_with_progress,
    ArchiveEntry, ArchiveExtras, FilterPolicy, VolumeSink, WriterOptions,
};
use rars::{
    ArchiveVersion, EntrySource, ErrorKind, FeatureSet, WriteCancellation, WriteProgress,
    WriteProgressEvent, WriterResources,
};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
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
    options: &CreateOptions<'_>,
    progress: &Progress<'_>,
) -> Result<Vec<PathBuf>> {
    check_options(options, sources)?;
    let parent = parent_dir(output)?;
    let planned = plan(output, &parent, sources, options.volume.is_some(), progress)?;
    let total = planned.len();
    let reporter = Reporter::new(progress, total);
    let resources = WriterResources::new(limits::WRITER_MEMORY_BYTES)
        .with_max_memory_bytes(limits::WRITER_MEMORY_BYTES)
        .with_max_spool_bytes(limits::MAX_SPOOL_BYTES)
        .with_max_prepared_header_bytes(limits::MAX_HEADER_BYTES)
        .with_max_preparation_bytes(limits::MAX_HEADER_BYTES)
        .with_temp_dir(&parent)
        .with_cancellation(reporter.token.clone());
    let entries = entries(&planned, options.password)?;
    reporter.poll();
    if reporter.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if let Some(size) = options.volume {
        return create_set(
            output, &parent, &planned, &entries, options, size, &resources, &reporter,
        );
    }
    let mut staging = tempfile::Builder::new()
        .prefix(".arca-")
        .suffix(".rar.part")
        .tempfile_in(&parent)?;
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
    verify(&staging, &planned, options.password, progress)?;
    if !progress(total, total, "") {
        return Err(Error::Cancelled);
    }
    publish(staging, output)?;
    Ok(vec![output.to_path_buf()])
}

/// Every option is checked before any input is read.
fn check_options(options: &CreateOptions<'_>, sources: &[Source]) -> Result<()> {
    options.check()?;
    if options.volume.is_some() && sources.is_empty() {
        return Err(Error::Format(
            "a RAR volume set needs at least one member".into(),
        ));
    }
    Ok(())
}

/// The writer's settings for `options`. The filter policy is never
/// `FilterPolicy::Auto`: that search plans every member as a whole and, in
/// rars 0.10.0, reserves 0 bytes of the managed-memory ledger for a
/// zero-length member, so its spool charge fails. `FilterPolicy::Detect`
/// keeps every member on the streaming block path, whose workspace is priced
/// per batch, and stops at the size the reader decodes a filtered member in
/// memory, so Arca can always open what it writes.
fn settings<'a>(options: &CreateOptions<'a>) -> (WriterOptions, ArchiveExtras<'a>) {
    let mut features = FeatureSet::store_only();
    features.solid = options.solid;
    features.header_encryption = options.hide_names;
    let writer_options = WriterOptions::new(ArchiveVersion::Rar50, features)
        .with_compression_level(rar_level(options.level));
    let mut extras = ArchiveExtras::default()
        .with_filter_policy(filter_policy(options.level))
        .with_recovery_percent(options.recovery.map(u64::from));
    if options.hide_names {
        extras.header_password = options.password.map(str::as_bytes);
    }
    (writer_options, extras)
}

/// Streams the members into `file` through the RAR 5 writer's public
/// streaming entry point rather than `Builder`, which forces an automatic
/// filter search for non-solid archives.
fn write(
    file: &mut File,
    entries: &[ArchiveEntry],
    options: &CreateOptions<'_>,
    resources: &WriterResources,
    reporter: &Reporter<'_>,
) -> rars::Result<()> {
    let (writer_options, extras) = settings(options);
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

/// Writes, verifies and publishes a volume set. The volumes are written into
/// a private directory beside the output under their index, given their
/// final names there once the count is known, verified as a set from the
/// first one and only then published.
#[allow(clippy::too_many_arguments)]
fn create_set(
    output: &Path,
    parent: &Path,
    planned: &[Planned],
    entries: &[ArchiveEntry],
    options: &CreateOptions<'_>,
    size: u64,
    resources: &WriterResources,
    reporter: &Reporter<'_>,
) -> Result<Vec<PathBuf>> {
    let room = tempfile::Builder::new()
        .prefix(".arca-")
        .suffix(".rar.parts")
        .tempdir_in(parent)?;
    let mut sink = Parts {
        dir: room.path().to_path_buf(),
        files: Vec::new(),
        open: None,
        too_many: false,
    };
    let (writer_options, extras) = settings(options);
    let written = write_streaming_volumes_sized_with_progress(
        entries,
        writer_options,
        extras,
        size,
        &mut sink,
        resources,
        Some(reporter),
    );
    if sink.too_many {
        return Err(Error::Limit(format!(
            "the RAR set would need more than {} volumes of {size} bytes; choose a larger volume size",
            limits::MAX_VOLUMES
        )));
    }
    written.map_err(|error| reporter.finish(error))?;
    if reporter.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let names = volume_names(output, sink.files.len());
    let mut staged = Vec::with_capacity(names.len());
    for (file, name) in sink.files.iter().zip(&names) {
        let path = room.path().join(name.file_name().unwrap_or_default());
        fs::rename(file, &path)?;
        staged.push(path);
    }
    let first = staged
        .first()
        .ok_or_else(|| Error::Format("the RAR writer produced no volume".into()))?;
    verify(first, planned, options.password, reporter.progress)?;
    if !(reporter.progress)(reporter.total, reporter.total, "") {
        return Err(Error::Cancelled);
    }
    publish_set(&staged, &names)?;
    Ok(names)
}

/// The names WinRAR gives a set of `count` volumes for `output`. One volume
/// keeps the name asked for.
fn volume_names(output: &Path, count: usize) -> Vec<PathBuf> {
    if count == 1 {
        return vec![output.to_path_buf()];
    }
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = volume_stem(&name);
    let width = count.to_string().len();
    (1..=count)
        .map(|index| output.with_file_name(format!("{stem}.part{index:0width$}.rar")))
        .collect()
}

/// Whether `name` is a volume of the set whose stem is `stem`, in any width
/// or case, so a new set can never be mixed with the volumes of an old one.
fn is_volume_of(name: &str, stem: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some((prefix, digits)) = lower
        .strip_suffix(".rar")
        .and_then(|rest| rest.rsplit_once(".part"))
    else {
        return false;
    };
    prefix.eq_ignore_ascii_case(stem)
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Receives the volumes of a set as the writer finishes them, each in a new
/// file of the staging directory, flushed and synced before the next starts.
struct Parts {
    dir: PathBuf,
    files: Vec<PathBuf>,
    open: Option<File>,
    too_many: bool,
}

impl VolumeSink for Parts {
    fn start_volume(&mut self, index: u64) -> rars::Result<Box<dyn Write + Send>> {
        if index >= limits::MAX_VOLUMES as u64 {
            self.too_many = true;
            return Err(rars::Error::InvalidArgument("too many RAR volumes"));
        }
        let path = self.dir.join(format!("{index}.vol"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        self.open = Some(file.try_clone()?);
        self.files.push(path);
        Ok(Box::new(io::BufWriter::new(file)))
    }

    fn finish_volume(&mut self, _index: u64, len: u64) -> rars::Result<()> {
        let file = self
            .open
            .take()
            .ok_or(rars::Error::WriterFailure("RAR volume finished twice"))?;
        file.sync_all()?;
        if file.metadata()?.len() != len {
            return Err(rars::Error::WriterFailure(
                "RAR volume length does not match what was written",
            ));
        }
        Ok(())
    }
}

/// Publishes every volume of a set without replacing anything, the first one
/// last so a set cannot be opened before it is complete. If one cannot be
/// published, the ones already published are removed again.
fn publish_set(staged: &[PathBuf], names: &[PathBuf]) -> Result<()> {
    let order = (1..staged.len()).chain(std::iter::once(0));
    let mut published = Vec::with_capacity(names.len());
    for index in order {
        let result = tempfile::TempPath::try_from_path(&staged[index])
            .map_err(Error::Io)
            .and_then(|staging| publish(staging, &names[index]));
        if let Err(error) = result {
            for name in published {
                let _ = fs::remove_file(name);
            }
            return Err(error);
        }
        published.push(&names[index]);
    }
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
    volumes: bool,
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
    if volumes {
        let name = output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = volume_stem(&name);
        for found in fs::read_dir(parent)? {
            let found = found?;
            if is_volume_of(&found.file_name().to_string_lossy(), stem) {
                return Err(exists(&found.path()));
            }
        }
    }
    let parent_identity = identity(&fs::metadata(parent)?);
    let parent_real = fs::canonicalize(parent).ok();
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
        let is_parent = match parent_identity {
            Some(parent) => identity(&meta) == Some(parent),
            None => parent_real.is_some() && fs::canonicalize(&source.path).ok() == parent_real,
        };
        if meta.is_dir() && is_parent {
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

fn entries(planned: &[Planned], password: Option<&str>) -> Result<Vec<ArchiveEntry>> {
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
            let entry = ArchiveEntry::new(name, source)
                .with_directory(plan.is_dir)
                .with_mtime(mtime)
                .with_attributes(attributes)
                .with_host_os(host_os);
            Ok(match password {
                Some(password) => entry.with_password(password.as_bytes()),
                None => entry,
            })
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

// RAR has six levels where Arca has ten.
pub(super) fn rar_level(level: arca_core::Level) -> u8 {
    [0, 1, 1, 2, 2, 3, 3, 4, 4, 5][usize::from(level.get())]
}

fn filter_policy(level: arca_core::Level) -> FilterPolicy {
    if rar_level(level) >= 3 {
        FilterPolicy::Detect {
            max_member: limits::MAX_FILTERED_MEMBER_BYTES,
        }
    } else {
        FilterPolicy::None
    }
}

pub(super) fn write_error(error: rars::Error) -> Error {
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
fn verify(
    staging: &Path,
    planned: &[Planned],
    password: Option<&str>,
    progress: &Progress<'_>,
) -> Result<()> {
    let total = planned.len();
    let verifying = |_: usize, _: usize, _: &str| progress(total, total, "");
    let failed = |e: Error| match e {
        Error::Cancelled => Error::Cancelled,
        e => Error::Format(format!("the new RAR archive failed verification: {e}")),
    };
    let archive = RarArchive::open_with_progress(staging, password, &verifying).map_err(failed)?;
    archive.test(password, &verifying).map_err(failed)?;
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
