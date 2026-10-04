use super::{human, sevenz::is_link, CodecArg};
use arca_core::{Error, Level, Result};
use arca_rar::{create_limits, create_rar, CreateOptions, Source};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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

#[derive(Debug)]
struct Walk<'a> {
    bounds: Bounds,
    output: &'a Path,
    sources: Vec<Source>,
    names: HashSet<String>,
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
    let walk = collect(out, inputs, Bounds::WRITER)?;
    let t0 = Instant::now();
    create_rar(out, &walk.sources, &options, &|_, _, _| true)?;
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

fn collect<'a>(output: &'a Path, inputs: &[PathBuf], bounds: Bounds) -> Result<Walk<'a>> {
    let mut walk = Walk {
        bounds,
        output,
        sources: Vec::new(),
        names: HashSet::new(),
        bytes: 0,
        files: 0,
    };
    for input in inputs {
        let base = input.parent().unwrap_or(Path::new(""));
        walk.visit(input, base)?;
    }
    Ok(walk)
}

impl Walk<'_> {
    fn visit(&mut self, path: &Path, base: &Path) -> Result<()> {
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
        let name = path
            .strip_prefix(base)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if name.len() > self.bounds.max_name_bytes {
            return Err(Error::Limit(format!(
                "RAR member name of {} bytes exceeds {} bytes: '{}'",
                name.len(),
                self.bounds.max_name_bytes,
                path.display()
            )));
        }
        if name != "." {
            if self.sources.len() >= self.bounds.max_entries {
                return Err(Error::Limit(format!(
                    "RAR archive would hold more than {} members",
                    self.bounds.max_entries
                )));
            }
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
            let mut children = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                self.visit(&child.path(), base)?;
            }
        }
        Ok(())
    }
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
        let walk = collect(&out, std::slice::from_ref(&input), Bounds::WRITER).unwrap();
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
        let err = collect(&out, &[dir.path().to_path_buf()], small()).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("more than 3 members")));
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
        let err = collect(&out, &[deep], bounds).unwrap_err();
        assert!(matches!(err, Error::Limit(m) if m.contains("13 bytes exceeds 12 bytes")));
    }

    #[test]
    fn aggregate_size_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), [0u8; 6]).unwrap();
        fs::write(dir.path().join("b"), [0u8; 6]).unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(&out, &[dir.path().to_path_buf()], small()).unwrap_err();
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
        let err = collect(&out, &inputs, Bounds::WRITER).unwrap_err();
        assert!(
            matches!(err, Error::Format(m) if m.contains("duplicate RAR member name: 'same.txt'"))
        );
        let twice = [dir.path().join("one"), dir.path().join("one")];
        let err = collect(&out, &twice, Bounds::WRITER).unwrap_err();
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
        let err = collect(&out, std::slice::from_ref(&input), Bounds::WRITER).unwrap_err();
        assert!(matches!(err, Error::Unsupported(m) if m.contains("symbolic link")));
        let err = collect(&out, &[input.join("link.txt")], Bounds::WRITER).unwrap_err();
        assert!(matches!(err, Error::Unsupported(m) if m.contains("symbolic link")));
    }

    #[test]
    fn missing_sources_name_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.rar");
        let err = collect(&out, &[dir.path().join("absent")], Bounds::WRITER).unwrap_err();
        assert!(matches!(err, Error::Io(e) if e.to_string().contains("absent")));
    }
}
