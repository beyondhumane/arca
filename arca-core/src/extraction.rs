use crate::{safe_name, Error, Result};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, File, OpenOptions};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub enum Conflict {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}

/// Why a link an archive holds was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The target is absolute, leads out of the destination, or gets
    /// somewhere only through a link or through a folder that is not there.
    Outside,
    /// Making a symbolic link here takes privileges not everybody has.
    Unsupported,
    /// A hard link to a file this extraction did not write.
    NotExtracted,
    /// A folder already stands where the link would go.
    Occupied,
}

/// A link that was not made, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub name: String,
    pub target: String,
    pub why: Refusal,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let why = match self.why {
            Refusal::Outside => "its target is outside the destination folder",
            Refusal::Unsupported => "symbolic links cannot be created on this system",
            Refusal::NotExtracted => "it points to a file this extraction did not write",
            Refusal::Occupied => "a folder is already in its place",
        };
        write!(
            f,
            "link '{}' -> '{}' was not created: {why}",
            self.name, self.target
        )
    }
}

struct PendingLink {
    relative: PathBuf,
    name: String,
    target: String,
}

/// A lazily opened extraction destination. Constructing it does not write anything.
pub struct Destination {
    path: PathBuf,
    directories: Vec<Dir>,
    claimed: HashSet<PathBuf>,
    written: HashMap<PathBuf, PathBuf>,
    pending: Vec<PendingLink>,
    refused: Vec<Refused>,
}

impl Destination {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            directories: Vec::new(),
            claimed: HashSet::new(),
            written: HashMap::new(),
            pending: Vec::new(),
            refused: Vec::new(),
        }
    }

    /// The links left out so far.
    pub fn refused(&self) -> &[Refused] {
        &self.refused
    }

    /// The folders from the root down to `directory`, made where missing and
    /// each opened without following a link.
    fn directories(&mut self, directory: &Path) -> Result<Vec<Dir>> {
        // Keep every ancestor pinned, including on Windows where handles deny deletion.
        let mut directories = vec![self.root()?.try_clone()?];
        for component in directory.components() {
            if let Component::Normal(name) = component {
                directories.push(open_child(directories.last().unwrap(), Path::new(name))?);
            } else {
                return Err(Error::Format("invalid extraction directory".into()));
            }
        }
        Ok(directories)
    }

    /// The name to write `relative` under once any conflict is settled, or
    /// `None` when it is to be skipped.
    fn settle(
        &self,
        parent: &Dir,
        relative: &Path,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<Option<OsString>> {
        let mut name = relative.file_name().unwrap().to_os_string();
        let exists = exists_without_link(parent, Path::new(&name))?;
        if exists || self.claimed.contains(&claim_key(relative)) {
            match ask(&self.path.join(relative)) {
                Conflict::Overwrite => {}
                Conflict::Skip => return Ok(None),
                Conflict::Cancel => return Err(Error::Cancelled),
                Conflict::Rename => {
                    name = free_name(parent, relative, &self.claimed)?;
                }
            }
        }
        Ok(Some(name))
    }

    fn refuse(&mut self, name: &str, target: &str, why: Refusal) {
        self.refused.push(Refused {
            name: name.to_string(),
            target: target.to_string(),
            why,
        });
    }

    /// Queues a symbolic link at `name` holding `target`. Nothing is made
    /// until [`Destination::make_links`], so no file of the archive is ever
    /// written after, and through, a link of the same archive.
    pub fn write_symlink(&mut self, name: &str, target: &str) -> Result<()> {
        let relative = safe_name(name)?;
        self.pending.push(PendingLink {
            relative,
            name: name.to_string(),
            target: target.to_string(),
        });
        Ok(())
    }

    /// Makes the symbolic links [`Destination::write_symlink`] queued, once
    /// every other entry is written, and gives back how many were made. A
    /// link is made only on Unix and only when its target, read from where
    /// the link stands, stays inside the destination; any other is left out
    /// and listed in [`Destination::refused`].
    pub fn make_links(&mut self, ask: &mut dyn FnMut(&Path) -> Conflict) -> Result<usize> {
        let mut made = 0;
        for link in std::mem::take(&mut self.pending) {
            match self.make_symlink(&link.relative, &link.target, ask)? {
                Ok(true) => made += 1,
                Ok(false) => {}
                Err(why) => self.refuse(&link.name, &link.target, why),
            }
        }
        Ok(made)
    }

    fn make_symlink(
        &mut self,
        relative: &Path,
        target: &str,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<std::result::Result<bool, Refusal>> {
        let directories = self.directories(relative.parent().unwrap_or(Path::new("")))?;
        if !stays_inside(&directories, target) {
            return Ok(Err(Refusal::Outside));
        }
        if cfg!(not(unix)) {
            return Ok(Err(Refusal::Unsupported));
        }
        let parent = directories.last().unwrap();
        let mut name = relative.file_name().unwrap().to_os_string();
        match parent.symlink_metadata(&name) {
            Ok(meta) if meta.is_dir() => return Ok(Err(Refusal::Occupied)),
            Ok(_) => match ask(&self.path.join(relative)) {
                Conflict::Overwrite => {}
                Conflict::Skip => return Ok(Ok(false)),
                Conflict::Cancel => return Err(Error::Cancelled),
                Conflict::Rename => name = free_name(parent, relative, &self.claimed)?,
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        #[cfg(unix)]
        place(parent, Path::new(&name), |temp| {
            parent.symlink(target, temp)
        })?;
        let chosen = relative.with_file_name(&name);
        self.written.remove(&claim_key(&chosen));
        self.claimed.insert(claim_key(&chosen));
        Ok(Ok(true))
    }

    /// A hard link at `name` to the file this extraction wrote for the
    /// earlier entry `target`. Where the file system has no hard links the
    /// file is copied instead, with `mtime` and `mode` as for any file. A
    /// target this run did not write is left out and listed in
    /// [`Destination::refused`]. Returns whether anything was written.
    pub fn write_hard_link(
        &mut self,
        name: &str,
        target: &str,
        mtime: Option<i64>,
        mode: Option<u32>,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<bool> {
        let relative = safe_name(name)?;
        let source = safe_name(target)
            .ok()
            .and_then(|t| self.written.get(&claim_key(&t)).cloned());
        let Some(source) = source else {
            self.refuse(name, target, Refusal::NotExtracted);
            return Ok(false);
        };
        let sources = self.directories(source.parent().unwrap_or(Path::new("")))?;
        let (source_parent, source_name) = (sources.last().unwrap(), source.file_name().unwrap());
        if !is_plain_file(source_parent, source_name) {
            self.refuse(name, target, Refusal::NotExtracted);
            return Ok(false);
        }
        let directories = self.directories(relative.parent().unwrap_or(Path::new("")))?;
        let parent = directories.last().unwrap();
        let Some(chosen_name) = self.settle(parent, &relative, ask)? else {
            return Ok(false);
        };
        let chosen = relative.with_file_name(&chosen_name);
        self.claimed.insert(claim_key(&chosen));
        let linked = place(parent, Path::new(&chosen_name), |temp| {
            source_parent.hard_link(source_name, parent, temp)
        });
        if linked.is_err() {
            let mut file = source_parent.open(source_name)?;
            commit(parent, Path::new(&chosen_name), &mut file, mtime, mode)?;
        }
        self.written.insert(claim_key(&relative), chosen);
        Ok(true)
    }

    fn root(&mut self) -> Result<&Dir> {
        if self.directories.is_empty() {
            let absolute = std::path::absolute(&self.path)?;
            #[cfg(unix)]
            let absolute = as_the_disk_has_it(&absolute);
            let mut components = absolute.components().peekable();
            let mut anchor = PathBuf::new();
            while let Some(Component::Prefix(_) | Component::RootDir) = components.peek() {
                anchor.push(components.next().unwrap().as_os_str());
            }
            let mut directories =
                vec![Dir::open_ambient_dir(anchor, cap_std::ambient_authority())?];
            for component in components {
                match component {
                    Component::Normal(name) => {
                        directories.push(open_child(directories.last().unwrap(), Path::new(name))?);
                    }
                    Component::ParentDir if directories.len() > 1 => {
                        directories.pop();
                    }
                    Component::ParentDir | Component::CurDir => {}
                    _ => return Err(Error::Format("invalid extraction destination".into())),
                }
            }
            self.directories = directories;
        }
        Ok(self.directories.last().unwrap())
    }

    /// Returns `None` for a directory or a skipped file, otherwise the bytes written.
    pub fn write_entry(
        &mut self,
        name: &str,
        is_dir: bool,
        mtime: Option<i64>,
        reader: &mut dyn Read,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<Option<u64>> {
        self.write_file(name, is_dir, mtime, None, reader, ask)
    }

    /// `write_entry` for a format that records Unix permissions: the file
    /// also gets the execute bits `mode` has, see [`set_executable`].
    pub fn write_file(
        &mut self,
        name: &str,
        is_dir: bool,
        mtime: Option<i64>,
        mode: Option<u32>,
        reader: &mut dyn Read,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<Option<u64>> {
        let relative = safe_name(name)?;
        let directory = if is_dir {
            relative.as_path()
        } else {
            relative.parent().unwrap_or(Path::new(""))
        };
        let directories = self.directories(directory)?;
        if is_dir {
            return Ok(None);
        }
        let parent = directories.last().unwrap();
        let Some(name) = self.settle(parent, &relative, ask)? else {
            return Ok(None);
        };
        let chosen = relative.with_file_name(&name);
        self.claimed.insert(claim_key(&chosen));
        let bytes = commit(parent, Path::new(&name), reader, mtime, mode)?;
        self.written.insert(claim_key(&relative), chosen);
        Ok(Some(bytes))
    }
}

/// Writes `reader` under a temporary name in `parent` and renames it to
/// `name` only once it is whole.
fn commit(
    parent: &Dir,
    name: &Path,
    reader: &mut dyn Read,
    mtime: Option<i64>,
    mode: Option<u32>,
) -> Result<u64> {
    let (mut staged, file) = StagedFile::new(parent, name)?;
    let mut writer = BufWriter::with_capacity(256 * 1024, file);
    let bytes = io::copy(reader, &mut writer)?;
    writer.flush()?;
    let file = writer.into_inner().map_err(|e| e.into_error())?.into_std();
    set_mtime(&file, mtime);
    set_executable(&file, mode);
    exists_without_link(parent, name)?;
    // Rename replaces the directory entry without following it, even if it changed.
    parent.rename(&staged.name, parent, name)?;
    staged.committed = true;
    Ok(bytes)
}

/// Makes something under a temporary name with `make` and renames it to
/// `name`, which replaces whatever was there without following it.
fn place(parent: &Dir, name: &Path, make: impl Fn(&Path) -> io::Result<()>) -> Result<()> {
    for _ in 0..10_000 {
        let temp = temp_name();
        if Path::new(&temp) == name {
            continue;
        }
        match make(Path::new(&temp)) {
            Ok(()) => {
                let renamed = parent.rename(&temp, parent, name);
                // Renaming one hard link over another of the same file does
                // nothing and leaves the temporary name behind.
                let _ = parent.remove_file(&temp);
                return renamed.map_err(Into::into);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Limit(
        "cannot allocate an extraction temporary name".into(),
    ))
}

/// Whether a link at the end of `directories` holding `target` leads to
/// somewhere inside the destination, `directories[0]`. Every step but the
/// last must go up, or down into a folder that is there and is not a link,
/// so the check holds for what the system will do and not only on paper:
/// the folders exist when the link is made, and a link is never made where
/// a folder stands.
fn stays_inside(directories: &[Dir], target: &str) -> bool {
    if target.is_empty() || target.contains('\0') || target.starts_with('/') {
        return false;
    }
    let parts: Vec<&str> = target
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let mut depth = directories.len() - 1;
    let mut below: Vec<Dir> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if *part == ".." {
            if below.pop().is_none() {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            continue;
        }
        if i + 1 == parts.len() {
            break;
        }
        let here = below.last().unwrap_or(&directories[depth]);
        match here
            .open_dir_nofollow(part)
            .ok()
            .and_then(|d| not_reparse(d).ok())
        {
            Some(dir) => below.push(dir),
            None => return false,
        }
    }
    true
}

fn is_plain_file(parent: &Dir, name: &OsStr) -> bool {
    matches!(exists_without_link(parent, Path::new(name)), Ok(true))
        && parent.symlink_metadata(name).is_ok_and(|m| m.is_file())
}

/// The folder somebody asked to extract into, with the links on the way to it
/// followed: on macOS `/tmp` and `/var` are links, and on any Unix a home or
/// a mount point may be. The folder itself is not resolved, so one that is a
/// link is still refused when it is opened, and everything under it is still
/// opened without following links.
#[cfg(unix)]
fn as_the_disk_has_it(path: &Path) -> PathBuf {
    let Some(mut existing) = path.parent() else {
        return path.to_path_buf();
    };
    loop {
        if let Ok(real) = std::fs::canonicalize(existing) {
            return real.join(path.strip_prefix(existing).unwrap_or(Path::new("")));
        }
        match existing.parent() {
            Some(parent) => existing = parent,
            None => return path.to_path_buf(),
        }
    }
}

/// The form two paths share when the file system would take them for one
/// file. Windows and macOS compare names without regard to case by default, so
/// an archive holding `A.txt` and `a.txt` names one file there, and a run that
/// keeps track of the names it handed out has to compare them this way.
pub fn claim_key(path: &Path) -> PathBuf {
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// Writes `target` through a temporary file beside it, renamed over `target`
/// only once `fill` has succeeded. A wrong password, a corrupt entry or a stop
/// halfway leaves whatever was at `target` before exactly as it was, and no
/// temporary behind.
pub fn write_staged(
    target: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> Result<u64>,
) -> Result<u64> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = target.parent().unwrap_or(Path::new("."));
    let mut opened = None;
    for _ in 0..10_000 {
        let temp = dir.join(format!(
            ".arca-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(f) => {
                opened = Some((temp, f));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    let Some((temp, mut file)) = opened else {
        return Err(Error::Limit(
            "cannot allocate an extraction temporary file".into(),
        ));
    };
    let result = fill(&mut file);
    drop(file);
    let result = result.and_then(|n| {
        std::fs::rename(&temp, target)?;
        Ok(n)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Gives an extracted file the modification time its archive recorded.
///
/// Best effort: a file system that refuses the time still has every byte, and
/// failing the extraction over a date would lose more than it saves.
pub fn set_mtime(file: &std::fs::File, mtime: Option<i64>) {
    let Some(seconds) = mtime.and_then(|t| u64::try_from(t).ok()).filter(|t| *t > 0) else {
        return;
    };
    if let Some(when) = std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds)) {
        let _ = file.set_modified(when);
    }
}

/// Lets an extracted program run where the archive says it could: the execute
/// bits of `mode`, the Unix permissions the archive recorded, are added to
/// the file's. Nothing is ever taken away and nothing else is added, so the
/// most an archive can do is mark one of its own files executable. A no-op
/// where there are no such bits, and best effort like `set_mtime`.
pub fn set_executable(file: &std::fs::File, mode: Option<u32>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Some(execute) = mode.map(|m| m & 0o111).filter(|m| *m != 0) else {
            return;
        };
        if let Ok(meta) = file.metadata() {
            let mut permissions = meta.permissions();
            permissions.set_mode(permissions.mode() | execute);
            let _ = file.set_permissions(permissions);
        }
    }
    #[cfg(not(unix))]
    let _ = (file, mode);
}

fn open_child(parent: &Dir, name: &Path) -> Result<Dir> {
    let dir = match parent.open_dir_nofollow(name) {
        Ok(dir) => dir,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            match parent.create_dir(name) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            parent.open_dir_nofollow(name)?
        }
        Err(e) => return Err(e.into()),
    };
    not_reparse(dir)
}

fn not_reparse(dir: Dir) -> Result<Dir> {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if dir.dir_metadata()?.file_attributes() & 0x400 != 0 {
            return Err(Error::Format(
                "extraction directory is a reparse point".into(),
            ));
        }
    }
    Ok(dir)
}

fn exists_without_link(parent: &Dir, name: &Path) -> Result<bool> {
    match parent.symlink_metadata(name) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use cap_std::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(Error::Format("extraction target is a reparse point".into()));
                }
            }
            if meta.file_type().is_symlink() {
                return Err(Error::Format("extraction target is a symbolic link".into()));
            }
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn free_name(parent: &Dir, relative: &Path, claimed: &HashSet<PathBuf>) -> Result<OsString> {
    let stem = relative.file_stem().unwrap_or_default().to_string_lossy();
    let ext = relative
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..10_000 {
        let name = OsString::from(format!("{stem} ({n}){ext}"));
        let candidate = relative.with_file_name(&name);
        if !claimed.contains(&claim_key(&candidate))
            && !exists_without_link(parent, Path::new(&name))?
        {
            return Ok(name);
        }
    }
    Err(Error::Limit("no free extraction name available".into()))
}

struct StagedFile<'a> {
    parent: &'a Dir,
    name: OsString,
    committed: bool,
}

fn temp_name() -> OsString {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    OsString::from(format!(
        ".arca-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl<'a> StagedFile<'a> {
    fn new(parent: &'a Dir, target: &Path) -> Result<(Self, File)> {
        for _ in 0..10_000 {
            let name = temp_name();
            if Path::new(&name) == target {
                continue;
            }
            match parent.open_with(&name, OpenOptions::new().write(true).create_new(true)) {
                Ok(file) => {
                    return Ok((
                        Self {
                            parent,
                            name,
                            committed: false,
                        },
                        file,
                    ))
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Limit(
            "cannot allocate an extraction temporary file".into(),
        ))
    }
}

impl Drop for StagedFile<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.parent.remove_file(&self.name);
        }
    }
}

/// The files a new archive must not take in: itself, when it is written into
/// a folder being archived, and the temporary it is built under. Without this
/// an archive replacing an older one swallows the older one, or reads itself
/// while it grows.
pub struct OwnOutput {
    names: Vec<OsString>,
    dir: Option<PathBuf>,
}

impl OwnOutput {
    pub fn new(out: &Path) -> Self {
        let name = out.file_name().unwrap_or_default().to_os_string();
        let mut staged = name.clone();
        staged.push(".arca-new");
        let parent = match out.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        Self {
            names: vec![name, staged],
            dir: std::fs::canonicalize(parent).ok(),
        }
    }

    pub fn is_own(&self, path: &Path) -> bool {
        let Some(name) = path.file_name() else {
            return false;
        };
        let key = claim_key(Path::new(name));
        if !self.names.iter().any(|n| claim_key(Path::new(n)) == key) {
            return false;
        }
        let parent = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        match (&self.dir, std::fs::canonicalize(parent)) {
            (Some(dir), Ok(found)) => *dir == found,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_archive_written_into_its_own_input_does_not_take_itself_in() {
        let room = room();
        let out = room.path().join("sub").join("pack.zip");
        fs::create_dir_all(out.parent().unwrap()).unwrap();
        let own = OwnOutput::new(&out);
        assert!(own.is_own(&room.path().join("sub/./pack.zip")));
        assert!(own.is_own(&room.path().join("sub/pack.zip.arca-new")));
        assert!(!own.is_own(&room.path().join("pack.zip")));
        assert!(!own.is_own(&room.path().join("sub/other.zip")));
    }
    use std::fs;

    fn room() -> tempfile::TempDir {
        tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
    }

    #[test]
    fn a_staged_write_replaces_the_target_only_when_it_succeeds() {
        let room = room();
        let target = room.path().join("keep.txt");
        fs::write(&target, b"original").unwrap();
        let failed = write_staged(&target, |f| {
            f.write_all(b"half")?;
            Err(Error::Format("wrong password".into()))
        });
        assert!(failed.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        let written = write_staged(&target, |f| {
            f.write_all(b"new")?;
            Ok(3)
        });
        assert_eq!(written.unwrap(), 3);
        assert_eq!(fs::read(&target).unwrap(), b"new");
        let left: Vec<_> = fs::read_dir(room.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "no temporary may be left behind");
    }

    #[test]
    fn names_that_differ_only_in_case_are_one_claim_where_the_disk_says_so() {
        let same = claim_key(Path::new("Dir/A.txt")) == claim_key(Path::new("dir/a.txt"));
        assert_eq!(same, cfg!(any(windows, target_os = "macos")));
    }

    #[test]
    fn unused_and_invalid_destinations_do_not_write() {
        let room = room();
        let path = room.path().join("missing");
        let mut dest = Destination::new(&path);
        assert!(!path.exists());
        for name in ["", ".", "../outside", "a/../../outside"] {
            assert!(dest
                .write_entry(name, false, None, &mut io::empty(), &mut |_| {
                    Conflict::Overwrite
                })
                .is_err());
        }
        assert!(!path.exists());
    }

    #[test]
    fn conflicts_and_failed_reads_preserve_destinations() {
        let room = room();
        let mut dest = Destination::new(room.path());
        fs::write(room.path().join("file"), b"original").unwrap();
        assert_eq!(
            dest.write_entry("file", false, None, &mut &b"skip"[..], &mut |_| {
                Conflict::Skip
            })
            .unwrap(),
            None
        );
        assert!(matches!(
            dest.write_entry("file", false, None, &mut io::empty(), &mut |_| {
                Conflict::Cancel
            }),
            Err(Error::Cancelled)
        ));
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("broken input"))
            }
        }
        assert!(dest
            .write_entry("file", false, None, &mut Broken, &mut |_| {
                Conflict::Overwrite
            })
            .is_err());
        assert_eq!(fs::read(room.path().join("file")).unwrap(), b"original");
        assert_eq!(fs::read_dir(room.path()).unwrap().count(), 1);
        dest.write_entry("file", false, None, &mut &b"renamed"[..], &mut |_| {
            Conflict::Rename
        })
        .unwrap();
        assert_eq!(fs::read(room.path().join("file (1)")).unwrap(), b"renamed");
        fs::hard_link(room.path().join("file"), room.path().join("other-link")).unwrap();
        dest.write_entry("file", false, None, &mut &b"replacement"[..], &mut |_| {
            Conflict::Overwrite
        })
        .unwrap();
        assert_eq!(fs::read(room.path().join("file")).unwrap(), b"replacement");
        assert_eq!(
            fs::read(room.path().join("other-link")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn written_files_take_the_time_the_archive_recorded() {
        let room = room();
        let mut dest = Destination::new(room.path());
        let when = 1_600_000_000;
        dest.write_entry("dated", false, Some(when), &mut &b"x"[..], &mut |_| {
            Conflict::Overwrite
        })
        .unwrap();
        dest.write_entry("undated", false, None, &mut &b"x"[..], &mut |_| {
            Conflict::Overwrite
        })
        .unwrap();
        let modified = |name: &str| {
            fs::metadata(room.path().join(name))
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        };
        assert_eq!(modified("dated"), when as u64);
        assert!(modified("undated") > when as u64);
    }

    // A program in a tarball has to come out runnable, and a plain file must
    // not come out runnable just for having passed through an archive.
    #[cfg(unix)]
    #[test]
    fn programs_keep_their_execute_bits_and_nothing_more() {
        use std::os::unix::fs::PermissionsExt;
        let room = room();
        let mut dest = Destination::new(room.path());
        for (name, mode) in [
            ("program", Some(0o755)),
            ("plain", Some(0o644)),
            ("setuid", Some(0o4755)),
            ("unknown", None),
        ] {
            dest.write_file(name, false, None, mode, &mut &b"x"[..], &mut |_| {
                Conflict::Overwrite
            })
            .unwrap();
        }
        let mode = |name: &str| {
            fs::metadata(room.path().join(name))
                .unwrap()
                .permissions()
                .mode()
        };
        assert_eq!(mode("program") & 0o111, 0o111);
        assert_eq!(mode("plain") & 0o111, 0);
        assert_eq!(mode("unknown") & 0o111, 0);
        assert_eq!(mode("setuid") & 0o7000, 0);
    }

    #[test]
    fn pinned_root_cannot_be_redirected_before_directory_creation() {
        let room = room();
        let path = room.path().join("out");
        let moved = room.path().join("moved");
        let outside = room.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let mut dest = Destination::new(&path);
        dest.root().unwrap();
        #[cfg(unix)]
        {
            fs::rename(&path, &moved).unwrap();
            std::os::unix::fs::symlink(&outside, &path).unwrap();
        }
        #[cfg(windows)]
        assert!(fs::rename(&path, &moved).is_err());
        dest.write_entry(
            "nested/empty",
            true,
            None,
            &mut io::empty(),
            &mut |_| unreachable!(),
        )
        .unwrap();
        dest.write_entry(
            "nested/file",
            false,
            None,
            &mut &b"contents"[..],
            &mut |_| unreachable!(),
        )
        .unwrap();
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        #[cfg(unix)]
        assert_eq!(fs::read(moved.join("nested/file")).unwrap(), b"contents");
        #[cfg(windows)]
        assert_eq!(fs::read(path.join("nested/file")).unwrap(), b"contents");
    }

    #[cfg(unix)]
    #[test]
    fn substituted_child_is_not_followed_for_directory_creation() {
        let room = room();
        let outside = room.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let path = room.path().join("out");
        let mut dest = Destination::new(&path);
        dest.write_entry(
            "child",
            true,
            None,
            &mut io::empty(),
            &mut |_| unreachable!(),
        )
        .unwrap();
        fs::rename(path.join("child"), path.join("original")).unwrap();
        std::os::unix::fs::symlink(&outside, path.join("child")).unwrap();
        assert!(dest
            .write_entry(
                "child/new",
                true,
                None,
                &mut io::empty(),
                &mut |_| unreachable!()
            )
            .is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn destination_junctions_are_not_followed() {
        let room = room();
        let path = room.path().join("out");
        let outside = room.path().join("outside");
        fs::create_dir(&path).unwrap();
        fs::create_dir(&outside).unwrap();
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(path.join("child"))
            .arg(&outside)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let mut dest = Destination::new(&path);
        for is_dir in [true, false] {
            assert!(dest
                .write_entry(
                    "child/new",
                    is_dir,
                    None,
                    &mut &b"contents"[..],
                    &mut |_| unreachable!()
                )
                .is_err());
        }
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn parent_swap_during_copy_cannot_redirect_commit_or_cleanup() {
        for fail in [false, true] {
            let room = room();
            let path = room.path().join("out");
            let outside = room.path().join("outside");
            fs::create_dir(&path).unwrap();
            fs::create_dir(&outside).unwrap();
            fs::write(path.join("file"), b"original").unwrap();
            fs::write(outside.join("file"), b"outside").unwrap();
            struct Swap<'a> {
                path: &'a Path,
                outside: &'a Path,
                fail: bool,
                done: bool,
            }
            impl Read for Swap<'_> {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    if self.done {
                        return Ok(0);
                    }
                    self.done = true;
                    fs::rename(self.path, self.path.with_file_name("moved"))?;
                    std::os::unix::fs::symlink(self.outside, self.path)?;
                    if self.fail {
                        return Err(io::Error::other("cancelled after swap"));
                    }
                    buf[0] = b'x';
                    Ok(1)
                }
            }
            let mut reader = Swap {
                path: &path,
                outside: &outside,
                fail,
                done: false,
            };
            let result =
                Destination::new(&path).write_entry("file", false, None, &mut reader, &mut |_| {
                    Conflict::Overwrite
                });
            assert_eq!(result.is_err(), fail);
            assert_eq!(fs::read(outside.join("file")).unwrap(), b"outside");
            assert_eq!(fs::read_dir(outside).unwrap().count(), 1);
            let moved = room.path().join("moved");
            assert_eq!(fs::read_dir(&moved).unwrap().count(), 1);
            assert_eq!(
                fs::read(moved.join("file")).unwrap(),
                if fail { b"original".as_slice() } else { b"x" }
            );
        }
    }

    // The folder asked for may be reached through a link, as /tmp is on
    // macOS: that link is followed, and the files land where it leads.
    #[cfg(unix)]
    #[test]
    fn a_destination_reached_through_a_link_is_used() {
        let room = room();
        let real = room.path().join("real");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, room.path().join("link")).unwrap();
        let mut dest = Destination::new(&room.path().join("link").join("out"));
        dest.write_entry(
            "a.txt",
            false,
            None,
            &mut &b"contents"[..],
            &mut |_| unreachable!(),
        )
        .unwrap();
        assert_eq!(fs::read(real.join("out/a.txt")).unwrap(), b"contents");
    }

    // Only the folders above the one asked for are followed: that one, if it
    // is a link, is refused like any link under it.
    #[cfg(unix)]
    #[test]
    fn a_destination_that_is_itself_a_link_is_refused() {
        let room = room();
        let real = room.path().join("real");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, room.path().join("link")).unwrap();
        let mut dest = Destination::new(&room.path().join("link"));
        assert!(dest
            .write_entry(
                "a.txt",
                false,
                None,
                &mut &b"contents"[..],
                &mut |_| unreachable!(),
            )
            .is_err());
        assert_eq!(fs::read_dir(&real).unwrap().count(), 0);
    }

    fn file(dest: &mut Destination, name: &str, contents: &[u8]) {
        dest.write_entry(name, false, None, &mut &contents[..], &mut |_| {
            Conflict::Overwrite
        })
        .unwrap();
    }

    fn reasons(dest: &Destination) -> Vec<(&str, Refusal)> {
        dest.refused()
            .iter()
            .map(|r| (r.name.as_str(), r.why))
            .collect()
    }

    // A link that stays inside is made on Unix and pointed exactly where the
    // archive says; elsewhere it is reported, never written as a file.
    #[test]
    fn links_that_stay_inside_are_made_where_the_system_allows() {
        let room = room();
        let mut dest = Destination::new(room.path());
        dest.write_symlink("lib/link.so", "real.so").unwrap();
        dest.write_symlink("bin/tool", "../lib/./real.so").unwrap();
        dest.write_symlink("up", "lib/..").unwrap();
        dest.write_symlink("dangling", "missing").unwrap();
        file(&mut dest, "lib/real.so", b"code");
        let made = dest.make_links(&mut |_| unreachable!()).unwrap();
        if cfg!(unix) {
            assert_eq!(made, 4);
            assert!(dest.refused().is_empty());
            assert_eq!(fs::read(room.path().join("bin/tool")).unwrap(), b"code");
            assert_eq!(
                fs::read_link(room.path().join("lib/link.so")).unwrap(),
                Path::new("real.so")
            );
        } else {
            assert_eq!(made, 0);
            assert!(reasons(&dest)
                .iter()
                .all(|(_, why)| *why == Refusal::Unsupported));
            assert_eq!(dest.refused().len(), 4);
            assert!(!room.path().join("bin/tool").exists());
        }
    }

    #[test]
    fn absolute_and_escaping_targets_are_refused() {
        let room = room();
        let inner = room.path().join("out");
        let mut dest = Destination::new(&inner);
        for (name, target) in [
            ("abs", "/etc/passwd"),
            ("empty", ""),
            ("nul", "a\0b"),
            ("up", "../outside"),
            ("d/deep", "../../outside"),
            ("d/sneaky", "../d/../../outside"),
            ("drive", "C:/Windows/x"),
            ("missing", "nowhere/../../outside"),
        ] {
            dest.write_symlink(name, target).unwrap();
        }
        file(&mut dest, "d/f", b"x");
        assert_eq!(dest.make_links(&mut |_| unreachable!()).unwrap(), 0);
        assert_eq!(dest.refused().len(), 8);
        assert!(reasons(&dest)
            .iter()
            .all(|(_, why)| *why == Refusal::Outside));
        assert_eq!(fs::read_dir(room.path()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&inner).unwrap().count(), 1);
        assert!(dest.refused()[0].to_string().contains("/etc/passwd"));
        assert!(dest.write_symlink("../escape", "x").is_err());
    }

    // A link through another link can say one thing on paper and do another:
    // `sub/a -> ..` is inside, and so on paper is `b -> sub/a/../..`, which the
    // system reads as the folder above the destination.
    #[test]
    fn a_target_that_goes_through_a_link_is_refused() {
        let room = room();
        let mut dest = Destination::new(&room.path().join("out"));
        file(&mut dest, "sub/keep", b"x");
        dest.write_symlink("sub/a", "..").unwrap();
        dest.write_symlink("b", "sub/a/../..").unwrap();
        dest.make_links(&mut |_| unreachable!()).unwrap();
        let expected = if cfg!(unix) {
            vec![("b", Refusal::Outside)]
        } else {
            vec![("sub/a", Refusal::Unsupported), ("b", Refusal::Outside)]
        };
        assert_eq!(reasons(&dest), expected);
    }

    // The classic attack: a link to somewhere, then a file written through
    // it. Links are made last, so the file lands in a real folder of the
    // destination and the link, finding that folder in its place, is not made.
    #[test]
    fn a_file_is_never_written_through_a_link_of_the_same_archive() {
        let room = room();
        let outside = room.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let mut dest = Destination::new(&room.path().join("out"));
        dest.write_symlink("a", "sub").unwrap();
        dest.write_symlink("b", "sub").unwrap();
        file(&mut dest, "a/x", b"payload");
        file(&mut dest, "sub/keep", b"x");
        dest.make_links(&mut |_| unreachable!()).unwrap();
        let out = room.path().join("out");
        assert_eq!(fs::read(out.join("a/x")).unwrap(), b"payload");
        assert!(!out.join("sub/x").exists());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        let why = if cfg!(unix) {
            Refusal::Occupied
        } else {
            Refusal::Unsupported
        };
        assert_eq!(reasons(&dest)[0], ("a", why));
    }

    // A link left by an earlier run is not followed by a later one either.
    #[cfg(unix)]
    #[test]
    fn a_link_from_an_earlier_extraction_is_not_written_through() {
        let room = room();
        let outside = room.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let out = room.path().join("out");
        let mut first = Destination::new(&out);
        file(&mut first, "sub/keep", b"x");
        first.write_symlink("l", "sub").unwrap();
        assert_eq!(first.make_links(&mut |_| unreachable!()).unwrap(), 1);
        let mut second = Destination::new(&out);
        for (name, is_dir) in [("l/x", false), ("l/d", true), ("l", false)] {
            assert!(second
                .write_entry(name, is_dir, None, &mut &b"y"[..], &mut |_| {
                    Conflict::Overwrite
                })
                .is_err());
        }
        assert_eq!(fs::read_dir(out.join("sub")).unwrap().count(), 1);
    }

    #[test]
    fn existing_names_are_conflicts_for_links_too() {
        let room = room();
        let mut dest = Destination::new(room.path());
        file(&mut dest, "kept", b"original");
        file(&mut dest, "replaced", b"original");
        file(&mut dest, "real", b"real");
        dest.write_symlink("kept", "real").unwrap();
        dest.write_symlink("replaced", "real").unwrap();
        let mut asked = Vec::new();
        dest.make_links(&mut |path| {
            asked.push(path.file_name().unwrap().to_owned());
            if path.ends_with("kept") {
                Conflict::Skip
            } else {
                Conflict::Overwrite
            }
        })
        .unwrap();
        assert_eq!(fs::read(room.path().join("kept")).unwrap(), b"original");
        if cfg!(unix) {
            assert_eq!(asked, ["kept", "replaced"]);
            assert!(fs::symlink_metadata(room.path().join("replaced"))
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read(room.path().join("replaced")).unwrap(), b"real");
        } else {
            assert!(asked.is_empty());
        }
        let left: Vec<_> = fs::read_dir(room.path()).unwrap().collect();
        assert_eq!(left.len(), 3, "no temporary may be left behind");
    }

    #[test]
    fn hard_links_point_only_at_files_this_run_wrote() {
        let room = room();
        fs::write(room.path().join("before.txt"), b"not ours").unwrap();
        let mut dest = Destination::new(room.path());
        file(&mut dest, "dir/a.txt", b"shared");
        let mut ok = |_: &Path| Conflict::Overwrite;
        assert!(dest
            .write_hard_link("b.txt", "./dir/a.txt", None, None, &mut ok)
            .unwrap());
        assert!(dest
            .write_hard_link("c.txt", "b.txt", None, None, &mut ok)
            .unwrap());
        for (name, target) in [
            ("d.txt", "before.txt"),
            ("e.txt", "../outside"),
            ("f.txt", "missing"),
            ("g.txt", "dir"),
        ] {
            assert!(!dest
                .write_hard_link(name, target, None, None, &mut ok)
                .unwrap());
            assert!(!room.path().join(name).exists());
        }
        assert!(reasons(&dest)
            .iter()
            .all(|(_, why)| *why == Refusal::NotExtracted));
        assert_eq!(dest.refused().len(), 4);
        for name in ["b.txt", "c.txt"] {
            assert_eq!(fs::read(room.path().join(name)).unwrap(), b"shared");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let inode = |name: &str| fs::metadata(room.path().join(name)).unwrap().ino();
            assert_eq!(inode("dir/a.txt"), inode("b.txt"));
            assert_eq!(inode("dir/a.txt"), inode("c.txt"));
        }
        // A hard link to itself, answered Replace, leaves the file as it was.
        assert!(dest
            .write_hard_link("b.txt", "b.txt", None, None, &mut ok)
            .unwrap());
        assert_eq!(fs::read(room.path().join("b.txt")).unwrap(), b"shared");
        let left = fs::read_dir(room.path()).unwrap().count();
        assert_eq!(left, 4, "no temporary may be left behind");
    }

    #[test]
    fn a_hard_link_follows_the_name_its_target_was_renamed_to() {
        let room = room();
        fs::write(room.path().join("a.txt"), b"old").unwrap();
        let mut dest = Destination::new(room.path());
        dest.write_entry("a.txt", false, None, &mut &b"new"[..], &mut |_| {
            Conflict::Rename
        })
        .unwrap();
        assert!(dest
            .write_hard_link("b.txt", "a.txt", None, None, &mut |_| unreachable!())
            .unwrap());
        assert_eq!(fs::read(room.path().join("b.txt")).unwrap(), b"new");
        assert_eq!(fs::read(room.path().join("a.txt")).unwrap(), b"old");
    }
}
