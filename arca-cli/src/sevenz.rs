use super::{human, CodecArg, OnConflict};
use arca_7z::{create_7z, CreateOptions, SevenZArchive, Source};
use arca_core::extraction::{Conflict, Destination};
use arca_core::{Error, Level, Result};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
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

pub(super) fn extract(
    archive: &Path,
    dest: &Path,
    policy: OnConflict,
    password: Option<&str>,
) -> Result<()> {
    let t0 = Instant::now();
    let mut archive = SevenZArchive::open(File::open(archive)?, password)?;
    let indices = (0..archive.len()).collect::<Vec<_>>();
    let mut destination = Destination::new(dest);
    let mut files = 0;
    let mut bytes = 0;
    archive.extract(
        &indices,
        &mut |_, entry, reader| {
            // Encrypted archives validate every block before the first callback.
            // No destination creation or truncation may move above this barrier.
            if let Some(written) = destination.write_entry(
                &entry.name,
                entry.is_dir,
                reader,
                &mut |_| match policy {
                    OnConflict::Overwrite => Conflict::Overwrite,
                    OnConflict::Skip => Conflict::Skip,
                    OnConflict::Rename => Conflict::Rename,
                },
            )? {
                bytes += written;
                files += 1;
            }
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
