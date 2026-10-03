use super::{human, resolve_conflict, CodecArg, OnConflict, BUF};
use arca_7z::{create_7z, CreateOptions, SevenZArchive, Source};
use arca_core::{Error, Level, Result};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub(super) fn create(
    out: &Path,
    inputs: &[PathBuf],
    level: Level,
    codec: CodecArg,
    password: Option<&str>,
    hide_names: bool,
) -> Result<()> {
    if !matches!(codec, CodecArg::Auto | CodecArg::Store | CodecArg::Lzma2) {
        return Err(Error::Unsupported(
            ".7z creation supports only auto, store or lzma2 codecs".into(),
        ));
    }
    if password == Some("") || hide_names && password.is_none() {
        return Err(Error::Format(
            ".7z encryption and --hide-names require a nonempty password".into(),
        ));
    }
    let options = CreateOptions {
        level: if codec == CodecArg::Store {
            Level::Store
        } else {
            level
        },
        password,
        hide_names,
    };
    let mut sources = Vec::new();
    let mut bytes = 0;
    let mut files = 0;
    for input in inputs {
        collect(
            input,
            input.parent().unwrap_or(Path::new("")),
            &mut sources,
            &mut bytes,
            &mut files,
        )?;
    }
    let t0 = Instant::now();
    create_7z(out, &sources, &options, &mut |_| true)?;
    println!(
        "{}: {files} files, {} -> {} in {:.3} s (sequential)",
        out.display(),
        human(bytes),
        human(fs::metadata(out)?.len()),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

fn collect(
    path: &Path,
    base: &Path,
    sources: &mut Vec<Source>,
    bytes: &mut u64,
    files: &mut u64,
) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if is_link(&meta) || !meta.is_file() && !meta.is_dir() {
        return Err(Error::Unsupported(format!(
            "7z source is not a regular file or directory: '{}'",
            path.display()
        )));
    }
    let mut name = path
        .strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if meta.is_dir() {
        name.push('/');
    } else {
        *bytes += meta.len();
        *files += 1;
    }
    if name != "./" {
        sources.push(Source {
            path: path.to_path_buf(),
            name,
        });
    }
    if meta.is_dir() {
        let mut children = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            collect(&child.path(), base, sources, bytes, files)?;
        }
    }
    Ok(())
}

fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if is_link(&meta) => {
                return Err(Error::Format(format!(
                    "refusing to extract through a symlink or reparse point: '{}'",
                    ancestor.display()
                )))
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

struct StagedFile {
    path: PathBuf,
    committed: bool,
}

impl StagedFile {
    fn new(parent: &Path, target: &Path) -> Result<(Self, File)> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..10_000 {
            let path = parent.join(format!(
                ".arca-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            if path == target {
                continue;
            }
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok((
                        Self {
                            path,
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

impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn extract(
    archive: &Path,
    dest: &Path,
    policy: OnConflict,
    password: Option<&str>,
) -> Result<()> {
    let t0 = Instant::now();
    let mut archive = SevenZArchive::open(File::open(archive)?, password)?;
    let indices = (0..archive.len()).collect::<Vec<_>>();
    let mut claimed = HashSet::new();
    let mut files = 0;
    let mut bytes = 0;
    archive.extract(
        &indices,
        &mut |_, entry, reader| {
            // Encrypted archives validate every block before the first callback.
            // No destination creation or truncation may move above this barrier.
            let path = dest.join(arca_core::safe_name(&entry.name)?);
            reject_links(&path)?;
            if entry.is_dir {
                fs::create_dir_all(&path)?;
                return Ok(());
            }
            let original = path.clone();
            let Some(path) = resolve_conflict(path, policy, &mut claimed) else {
                return Ok(());
            };
            if policy == OnConflict::Rename && path == original && path.exists() {
                return Err(Error::Limit("no free extraction name available".into()));
            }
            reject_links(&path)?;
            let parent = path
                .parent()
                .ok_or_else(|| Error::Format("missing destination parent".into()))?;
            fs::create_dir_all(parent)?;
            reject_links(parent)?;
            let (mut staged, file) = StagedFile::new(parent, &path)?;
            let mut writer = BufWriter::with_capacity(BUF, file);
            let written = io::copy(reader, &mut writer)?;
            writer.flush()?;
            drop(writer);
            // Replace the directory entry, not the contents of an existing hard link.
            reject_links(&path)?;
            fs::rename(&staged.path, &path)?;
            staged.committed = true;
            bytes += written;
            files += 1;
            Ok(())
        },
        &mut |_| true,
    )?;
    println!(
        "{files} files, {} written in {:.3} s",
        human(bytes),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}
