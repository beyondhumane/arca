use super::{human, sevenz::is_link, CodecArg};
use arca_core::{Error, Level, Result};
use arca_rar::{create_limits, create_rar, CreateOptions, Source};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Ceilings applied while the inputs are walked, before any name is kept
/// or any directory is entered, so an oversized tree fails early instead of
/// being collected in full and refused by the writer afterwards.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    max_entries: usize,
    max_name_bytes: usize,
    max_total_bytes: u64,
}

impl Bounds {
    const WRITER: Self = Self {
        max_entries: create_limits::MAX_ENTRIES,
        max_name_bytes: create_limits::MAX_NAME_BYTES,
        max_total_bytes: create_limits::MAX_TOTAL_BYTES,
    };
}

/// Depth-first walk driven by an explicit stack. Every path is reserved
/// against `max_entries` before it is queued, counting the entries already
/// collected and those still pending, so neither a huge flat directory nor
/// many pending sibling lists can grow past the cap.
#[derive(Debug)]
struct Walk<'a> {
    bounds: Bounds,
    output: &'a Path,
    cancelled: &'a AtomicBool,
    sources: Vec<Source>,
    names: HashSet<String>,
    pending: Vec<PathBuf>,
    bytes: u64,
    files: u64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create(
    out: &Path,
    inputs: &[PathBuf],
    level: Level,
    codec: CodecArg,
    requested_threads: usize,
    password: Option<&str>,
    hide_names: bool,
) -> Result<()> {
    if cfg!(not(feature = "rar")) {
        return create_rar(out, &[], &CreateOptions::default(), &|_, _, _| true);
    }
    let options = options(level, codec, requested_threads, password, hide_names)?;
    let cancelled = interruption()?;
    let walk = collect(out, inputs, Bounds::WRITER, cancelled)?;
    let t0 = Instant::now();
    create_rar(out, &walk.sources, &options, &|_, _, _| {
        !cancelled.load(Ordering::SeqCst)
    })?;
    println!(
        "{}: {} files, {} -> {} in {:.3} s (sequential)",
        out.display(),
        walk.files,
        human(walk.bytes),
        human(fs::metadata(out)?.len()),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Every option the RAR writer cannot honor is refused here, before any
/// input is read, rather than ignored.
fn options(
    level: Level,
    codec: CodecArg,
    requested_threads: usize,
    password: Option<&str>,
    hide_names: bool,
) -> Result<CreateOptions> {
    if hide_names {
        return Err(Error::Unsupported(
            "--hide-names requires .7z; .rar creation does not encrypt".into(),
        ));
    }
    if password.is_some() {
        return Err(Error::Unsupported(
            ".rar creation does not support encryption; use .zip or .7z for a password".into(),
        ));
    }
    if requested_threads > 1 {
        return Err(Error::Unsupported(
            ".rar creation is sequential; leave --threads at 0 or 1".into(),
        ));
    }
    let level = match codec {
        CodecArg::Auto => level,
        CodecArg::Store => Level::Store,
        CodecArg::Deflate | CodecArg::Zstd | CodecArg::Lzma2 => {
            return Err(Error::Unsupported(
                ".rar creation uses the RAR compressor; --codec accepts only auto or store, \
                 pick the strength with --level"
                    .into(),
            ))
        }
    };
    Ok(CreateOptions { level })
}

/// Turns Ctrl-C (SIGINT) and SIGTERM into a flag that the walk and the
/// writer's progress callback read, so an interrupted creation ends with
/// `Error::Cancelled`, drops its staged file and never publishes. Installed
/// before any input is read. SIGKILL and power loss are not covered.
fn interruption() -> Result<&'static AtomicBool> {
    static CANCELLED: AtomicBool = AtomicBool::new(false);
    #[cfg(feature = "rar")]
    ctrlc::try_set_handler(|| CANCELLED.store(true, Ordering::SeqCst)).map_err(|e| {
        Error::Io(io::Error::other(format!(
            "cannot watch for interruption during RAR creation: {e}"
        )))
    })?;
    Ok(&CANCELLED)
}

fn collect<'a>(
    output: &'a Path,
    inputs: &[PathBuf],
    bounds: Bounds,
    cancelled: &'a AtomicBool,
) -> Result<Walk<'a>> {
    let mut walk = Walk {
        bounds,
        output,
        cancelled,
        sources: Vec::new(),
        names: HashSet::new(),
        pending: Vec::new(),
        bytes: 0,
        files: 0,
    };
    for input in inputs {
        let base = input.parent().unwrap_or(Path::new(""));
        walk.reserve(0)?;
        walk.pending.push(input.clone());
        while let Some(path) = walk.pending.pop() {
            walk.visit(&path, base)?;
        }
    }
    Ok(walk)
}

impl Walk<'_> {
    fn check_cancelled(&self) -> Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        Ok(())
    }

    fn reserve(&self, queued: usize) -> Result<()> {
        self.check_cancelled()?;
        if self.sources.len() + self.pending.len() + queued >= self.bounds.max_entries {
            return Err(Error::Limit(format!(
                "RAR archive would hold more than {} members",
                self.bounds.max_entries
            )));
        }
        Ok(())
    }

    fn enumerate(&self, dir: &Path) -> Result<Vec<OsString>> {
        let mut children = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            self.reserve(children.len())?;
            let child = entry.file_name();
            if child.len() > self.bounds.max_name_bytes {
                return Err(Error::Limit(format!(
                    "RAR member name of {} bytes exceeds {} bytes: '{}'",
                    child.len(),
                    self.bounds.max_name_bytes,
                    dir.join(&child).display()
                )));
            }
            children.push(child);
        }
        children.sort();
        Ok(children)
    }

    fn visit(&mut self, path: &Path, base: &Path) -> Result<()> {
        self.check_cancelled()?;
        let meta = fs::symlink_metadata(path).map_err(|e| {
            Error::Io(io::Error::new(
                e.kind(),
                format!("RAR source '{}': {e}", path.display()),
            ))
        })?;
        if is_link(&meta) {
            return Err(Error::Unsupported(format!(
                "RAR source '{}' is a symbolic link; links are not followed or stored",
                path.display()
            )));
        }
        if !meta.is_file() && !meta.is_dir() {
            return Err(Error::Unsupported(format!(
                "RAR source '{}' is not a regular file or directory",
                path.display()
            )));
        }
        if same_file(path, self.output) {
            return Err(Error::Format(format!(
                "RAR source '{}' is the archive being created",
                path.display()
            )));
        }
        let name = member_name(path, path.strip_prefix(base).unwrap_or(path))?;
        if name.len() > self.bounds.max_name_bytes {
            return Err(Error::Limit(format!(
                "RAR member name of {} bytes exceeds {} bytes: '{}'",
                name.len(),
                self.bounds.max_name_bytes,
                path.display()
            )));
        }
        if !name.is_empty() {
            if !self.names.insert(name.to_lowercase()) {
                return Err(Error::Format(format!(
                    "duplicate RAR member name: '{name}' (from '{}')",
                    path.display()
                )));
            }
            if meta.is_file() {
                self.bytes = self
                    .bytes
                    .checked_add(meta.len())
                    .filter(|n| *n <= self.bounds.max_total_bytes)
                    .ok_or_else(|| {
                        Error::Limit(format!(
                            "RAR archive input exceeds {} bytes",
                            self.bounds.max_total_bytes
                        ))
                    })?;
                self.files += 1;
            }
            self.sources.push(Source {
                path: path.to_path_buf(),
                name,
            });
        }
        if meta.is_dir() {
            let children = self.enumerate(path)?;
            self.pending
                .extend(children.into_iter().rev().map(|child| path.join(child)));
        }
        Ok(())
    }
}

/// The stored name for `relative`: its normal components joined with `/`.
/// `.` components (from inputs such as `.` or `./dir`) are dropped so the
/// writer never sees `./a.txt`; `..` and absolute components are refused
/// rather than normalized away.
fn member_name(path: &Path, relative: &Path) -> Result<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::Format(format!(
                    "RAR source '{}' has no name relative to its base ('..' and absolute \
                     components are not stored)",
                    path.display()
                )))
            }
        }
    }
    Ok(parts.join("/"))
}

/// Whether `path` is the output file under another spelling, so an aliased
/// source is refused with a clear message instead of being read.
fn same_file(path: &Path, output: &Path) -> bool {
    match (fs::canonicalize(path), fs::canonicalize(output)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static NEVER: AtomicBool = AtomicBool::new(false);

    fn small() -> Bounds {
        Bounds {
            max_entries: 3,
            max_name_bytes: 64,
            max_total_bytes: 10,
        }
    }

    #[test]
    fn defaults_match_the_writer_limits() {
        assert_eq!(Bounds::WRITER.max_entries, create_limits::MAX_ENTRIES);
        assert_eq!(Bounds::WRITER.max_name_bytes, create_limits::MAX_NAME_BYTES);
        assert_eq!(
            Bounds::WRITER.max_total_bytes,
            create_limits::MAX_TOTAL_BYTES
        );
    }

    #[test]
    fn directories_and_files_keep_their_relative_names() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir_all(input.join("sub/empty")).unwrap();
        fs::write(input.join("a.txt"), b"aa").unwrap();
        fs::write(input.join("sub/b.txt"), b"bbb").unwrap();
        let out = dir.path().join("out.rar");
        let walk = collect(&out, std::slice::from_ref(&input), Bounds::WRITER, &NEVER).unwrap();
        let names: Vec<&str> = walk.sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["in", "in/a.txt", "in/sub", "in/sub/b.txt", "in/sub/empty"]
        );
        assert_eq!(walk.files, 2);
        assert_eq!(walk.bytes, 5);
        assert!(walk.sources.iter().all(|s| s.path.starts_with(&input)));
    }

    #[test]
    fn entry_count_is_checked_before_the_vector_grows() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..4 {
            fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
        }
        let out = dir.path().join("out.rar");
        let err = collect(&out, &[dir.path().to_path_buf()], small(), &NEVER).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("more than 3 members")));
    }

    #[test]
    fn directory_enumeration_stops_at_the_entry_cap() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["c", "a", "b"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let out = dir.path().join("out.rar");
        let walk = collect(&out, &[], small(), &NEVER).unwrap();
        let names = walk.enumerate(dir.path()).unwrap();
        assert_eq!(names, ["a", "b", "c"]);

        fs::write(dir.path().join("d"), b"").unwrap();
        let err = walk.enumerate(dir.path()).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("more than 3 members")));
    }

    #[test]
    fn pending_entries_count_against_the_cap_during_enumeration() {
        let dir = tempfile::tempdir().unwrap();
        let flat = dir.path().join("flat");
        fs::create_dir_all(&flat).unwrap();
        fs::write(flat.join("x"), b"").unwrap();
        fs::write(flat.join("y"), b"").unwrap();
        let out = dir.path().join("out.rar");
        let mut walk = collect(&out, &[], small(), &NEVER).unwrap();
        walk.pending.push(dir.path().join("queued-1"));
        assert_eq!(walk.enumerate(&flat).unwrap().len(), 2);
        walk.pending.push(dir.path().join("queued-2"));
        let err = walk.enumerate(&flat).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("more than 3 members")));
    }

    #[test]
    fn nested_sibling_lists_cannot_compound_past_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        for sub in ["p", "q"] {
            fs::create_dir_all(input.join(sub)).unwrap();
            fs::write(input.join(sub).join("1"), b"").unwrap();
            fs::write(input.join(sub).join("2"), b"").unwrap();
        }
        let out = dir.path().join("out.rar");
        let bounds = Bounds {
            max_entries: 6,
            ..small()
        };
        let err = collect(&out, std::slice::from_ref(&input), bounds, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("more than 6 members")));
        let bounds = Bounds {
            max_entries: 7,
            ..small()
        };
        let walk = collect(&out, std::slice::from_ref(&input), bounds, &NEVER).unwrap();
        let names: Vec<&str> = walk.sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["in", "in/p", "in/p/1", "in/p/2", "in/q", "in/q/1", "in/q/2"]
        );
        assert!(walk.pending.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn deep_trees_do_not_recurse() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().join("in");
        let depth = if cfg!(target_os = "macos") { 400 } else { 2000 };
        for _ in 0..depth {
            deep.push("d");
        }
        fs::create_dir_all(&deep).unwrap();
        let out = dir.path().join("out.rar");
        let walk = collect(&out, &[dir.path().join("in")], Bounds::WRITER, &NEVER).unwrap();
        assert_eq!(walk.sources.len(), depth + 1);
    }

    #[test]
    fn name_bytes_are_checked_before_descending() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("abcdefghijklm");
        fs::create_dir_all(deep.join("x")).unwrap();
        let out = dir.path().join("out.rar");
        let bounds = Bounds {
            max_name_bytes: 12,
            ..small()
        };
        let err = collect(&out, &[deep], bounds, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("13 bytes exceeds 12 bytes")));
    }

    #[test]
    fn aggregate_size_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), [0u8; 6]).unwrap();
        fs::write(dir.path().join("b"), [0u8; 6]).unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(&out, &[dir.path().to_path_buf()], small(), &NEVER).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("exceeds 10 bytes")));
    }

    #[test]
    fn duplicate_and_case_colliding_names_fail_during_the_walk() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("one")).unwrap();
        fs::create_dir_all(dir.path().join("two")).unwrap();
        fs::write(dir.path().join("one/Same.txt"), b"1").unwrap();
        fs::write(dir.path().join("two/same.txt"), b"2").unwrap();
        let out = dir.path().join("out.rar");
        let inputs = [
            dir.path().join("one/Same.txt"),
            dir.path().join("two/same.txt"),
        ];
        let err = collect(&out, &inputs, Bounds::WRITER, &NEVER).unwrap_err();
        assert!(
            matches!(err, Error::Format(m) if m.contains("duplicate RAR member name: 'same.txt'"))
        );
        let twice = [dir.path().join("one"), dir.path().join("one")];
        let err = collect(&out, &twice, Bounds::WRITER, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Format(m) if m.contains("duplicate RAR member name: 'one'")));
    }

    #[cfg(unix)]
    #[test]
    fn links_are_refused_wherever_they_appear() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("real.txt"), b"x").unwrap();
        std::os::unix::fs::symlink("real.txt", input.join("link.txt")).unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(&out, std::slice::from_ref(&input), Bounds::WRITER, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Unsupported(m) if m.contains("symbolic link")));
        let err = collect(&out, &[input.join("link.txt")], Bounds::WRITER, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Unsupported(m) if m.contains("symbolic link")));
    }

    #[test]
    fn parent_components_are_refused_and_dot_components_dropped() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("in/sub")).unwrap();
        fs::write(dir.path().join("in/sub/a.txt"), b"a").unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(
            &out,
            &[dir.path().join("in/sub/..")],
            Bounds::WRITER,
            &NEVER,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Format(_)), "{err}");
        assert!(err.to_string().contains("'..'"), "{err}");
        assert_eq!(
            member_name(Path::new("x"), Path::new("./in/./sub/a.txt")).unwrap(),
            "in/sub/a.txt"
        );
        assert_eq!(member_name(Path::new("x"), Path::new(".")).unwrap(), "");
        assert!(member_name(Path::new("x"), Path::new("/in/a.txt")).is_err());
    }

    #[test]
    fn an_interruption_flag_stops_the_walk_with_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("in")).unwrap();
        fs::write(dir.path().join("in/a.txt"), b"a").unwrap();
        let out = dir.path().join("out.rar");
        let stop = AtomicBool::new(true);
        let err = collect(&out, &[dir.path().join("in")], Bounds::WRITER, &stop).unwrap_err();
        assert!(matches!(err, Error::Cancelled), "{err}");
    }

    #[test]
    fn missing_sources_name_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(&out, &[dir.path().join("absent")], Bounds::WRITER, &NEVER).unwrap_err();
        assert!(matches!(err, Error::Io(e) if e.to_string().contains("absent")));
    }
}
