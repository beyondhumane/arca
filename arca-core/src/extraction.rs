use crate::{safe_name, Error, Result};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, File, OpenOptions};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub enum Conflict {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}

/// A lazily opened extraction destination. Constructing it does not write anything.
pub struct Destination {
    path: PathBuf,
    directories: Vec<Dir>,
    claimed: HashSet<PathBuf>,
}

impl Destination {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            directories: Vec::new(),
            claimed: HashSet::new(),
        }
    }

    fn root(&mut self) -> Result<&Dir> {
        if self.directories.is_empty() {
            let absolute = std::path::absolute(&self.path)?;
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
        reader: &mut dyn Read,
        ask: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Result<Option<u64>> {
        let relative = safe_name(name)?;
        let directory = if is_dir {
            relative.as_path()
        } else {
            relative.parent().unwrap_or(Path::new(""))
        };
        // Keep every ancestor pinned, including on Windows where handles deny deletion.
        let mut directories = vec![self.root()?.try_clone()?];
        for component in directory.components() {
            if let Component::Normal(name) = component {
                directories.push(open_child(directories.last().unwrap(), Path::new(name))?);
            } else {
                return Err(Error::Format("invalid extraction directory".into()));
            }
        }
        if is_dir {
            return Ok(None);
        }
        let parent = directories.last().unwrap();
        let mut name = relative.file_name().unwrap().to_os_string();
        let exists = exists_without_link(parent, Path::new(&name))?;
        if exists || self.claimed.contains(&relative) {
            match ask(&self.path.join(&relative)) {
                Conflict::Overwrite => {}
                Conflict::Skip => return Ok(None),
                Conflict::Cancel => return Err(Error::Cancelled),
                Conflict::Rename => {
                    name = free_name(parent, &relative, &self.claimed)?;
                }
            }
        }
        let chosen = relative.with_file_name(&name);
        self.claimed.insert(chosen);
        let (mut staged, file) = StagedFile::new(parent, Path::new(&name))?;
        let mut writer = BufWriter::with_capacity(256 * 1024, file);
        let bytes = io::copy(reader, &mut writer)?;
        writer.flush()?;
        drop(writer);
        exists_without_link(parent, Path::new(&name))?;
        // Rename replaces the directory entry without following it, even if it changed.
        parent.rename(&staged.name, parent, &name)?;
        staged.committed = true;
        Ok(Some(bytes))
    }
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
        if !claimed.contains(&candidate) && !exists_without_link(parent, Path::new(&name))? {
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

impl<'a> StagedFile<'a> {
    fn new(parent: &'a Dir, target: &Path) -> Result<(Self, File)> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..10_000 {
            let name = OsString::from(format!(
                ".arca-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn room() -> tempfile::TempDir {
        tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
    }

    #[test]
    fn unused_and_invalid_destinations_do_not_write() {
        let room = room();
        let path = room.path().join("missing");
        let mut dest = Destination::new(&path);
        assert!(!path.exists());
        for name in ["", ".", "../outside", "a/../../outside"] {
            assert!(dest
                .write_entry(name, false, &mut io::empty(), &mut |_| Conflict::Overwrite)
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
            dest.write_entry("file", false, &mut &b"skip"[..], &mut |_| Conflict::Skip)
                .unwrap(),
            None
        );
        assert!(matches!(
            dest.write_entry("file", false, &mut io::empty(), &mut |_| Conflict::Cancel),
            Err(Error::Cancelled)
        ));
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("broken input"))
            }
        }
        assert!(dest
            .write_entry("file", false, &mut Broken, &mut |_| Conflict::Overwrite)
            .is_err());
        assert_eq!(fs::read(room.path().join("file")).unwrap(), b"original");
        assert_eq!(fs::read_dir(room.path()).unwrap().count(), 1);
        dest.write_entry("file", false, &mut &b"renamed"[..], &mut |_| {
            Conflict::Rename
        })
        .unwrap();
        assert_eq!(fs::read(room.path().join("file (1)")).unwrap(), b"renamed");
        fs::hard_link(room.path().join("file"), room.path().join("other-link")).unwrap();
        dest.write_entry("file", false, &mut &b"replacement"[..], &mut |_| {
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
            &mut io::empty(),
            &mut |_| unreachable!(),
        )
        .unwrap();
        dest.write_entry(
            "nested/file",
            false,
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
        dest.write_entry("child", true, &mut io::empty(), &mut |_| unreachable!())
            .unwrap();
        fs::rename(path.join("child"), path.join("original")).unwrap();
        std::os::unix::fs::symlink(&outside, path.join("child")).unwrap();
        assert!(dest
            .write_entry("child/new", true, &mut io::empty(), &mut |_| unreachable!())
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
                Destination::new(&path)
                    .write_entry("file", false, &mut reader, &mut |_| Conflict::Overwrite);
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
}
