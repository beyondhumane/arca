use super::{Conflict, Progress, RarArchive};
use arca_core::{Entry, Error, Method, Result};
use rars::{ArchiveMemberDetail, ArchiveReadOptions, ReadCancellation};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, UNIX_EPOCH};

const MIB: u64 = 1024 * 1024;
const MAX_MEMBER: u64 = 4 * 1024 * MIB;
const MAX_TOTAL: u64 = 16 * 1024 * MIB;
const PREVIEW_LIMIT: u64 = 64 * MIB;

pub(super) fn options<'a>(
    password: Option<&'a str>,
    token: &'a ReadCancellation,
) -> ArchiveReadOptions<'a> {
    ArchiveReadOptions::with_optional_password(password.map(str::as_bytes))
        .with_cancellation(token)
        .with_max_header_count(100_000)
        .with_max_header_bytes(64 * MIB)
        .with_rar50_dictionary_size_limit(256 * MIB)
        .with_rar50_buffered_decode_limit(64 * MIB)
        .with_max_reader_workspace_bytes(512 * MIB)
        .with_max_member_output_bytes(MAX_MEMBER)
        .with_max_total_output_bytes(MAX_TOTAL)
}

pub(super) fn error(error: rars::Error) -> Error {
    use rars::error::ErrorKind;
    match error.kind() {
        ErrorKind::PasswordRequired => Error::PasswordRequired,
        ErrorKind::BadPassword => Error::BadPassword,
        ErrorKind::Cancelled => Error::Cancelled,
        ErrorKind::ResourceLimit => Error::Limit(error.to_string()),
        ErrorKind::UnsupportedFeature | ErrorKind::UnsupportedFormat => {
            Error::Unsupported(error.to_string())
        }
        ErrorKind::Io => Error::Io(io::Error::other(error.to_string())),
        _ => Error::Format(error.to_string()),
    }
}

pub(super) fn at_path(error: Error, path: &Path) -> Error {
    let context = |message| format!("RAR volume '{}': {message}", path.display());
    match error {
        Error::PasswordRequired | Error::BadPassword | Error::Cancelled => error,
        Error::Limit(message) => Error::Limit(context(message)),
        Error::Unsupported(message) => Error::Unsupported(context(message)),
        Error::Io(e) => Error::Io(io::Error::new(e.kind(), context(e.to_string()))),
        e => Error::Format(context(e.to_string())),
    }
}

fn controlled<T>(
    progress: &Progress<'_>,
    work: impl FnOnce(&ReadCancellation) -> Result<T>,
) -> Result<T> {
    let token = ReadCancellation::new();
    if !progress(0, 0, "RAR") {
        return Err(Error::Cancelled);
    }
    let result = std::thread::scope(|scope| {
        let (done, wait) = channel::<()>();
        let token = &token;
        scope.spawn(move || {
            while matches!(
                wait.recv_timeout(Duration::from_millis(100)),
                Err(RecvTimeoutError::Timeout)
            ) {
                if !progress(0, 0, "RAR") {
                    token.cancel();
                    break;
                }
            }
        });
        let result = work(token);
        drop(done);
        result
    });
    if token.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        result
    }
}

pub(super) fn open(
    path: &Path,
    password: Option<&str>,
    progress: &Progress<'_>,
) -> Result<RarArchive> {
    controlled(progress, |token| {
        let inner = super::volumes::Volumes::open(path, password, progress, token)?;
        let mut entries = Vec::new();
        let mut names = HashSet::new();
        let mut files = HashSet::new();
        let mut total = 0u64;
        for (index, member) in inner.members(token)?.into_iter().enumerate() {
            let meta = &member.meta;
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let mode = meta.file_attr & 0o170000;
            if meta.is_redirection
                || (meta.attr_source() == rars::AttrSource::Unix
                    && !matches!(mode, 0 | 0o100000 | 0o040000))
            {
                return Err(Error::Unsupported(
                    "RAR links and special files are not supported".into(),
                ));
            }
            let decoded = member.decoded_name(None).map_err(error)?;
            let name = std::str::from_utf8(&decoded)
                .map_err(|_| Error::Unsupported("RAR filename cannot be decoded as UTF-8".into()))?
                .replace('\\', "/");
            validate_name(&name)?;
            let normalized = arca_core::safe_name(&name)?
                .to_string_lossy()
                .to_lowercase();
            if !names.insert(normalized.clone()) {
                return Err(Error::Format(format!(
                    "duplicate RAR destination: '{name}'"
                )));
            }
            if !meta.is_directory {
                files.insert(normalized);
            }
            if meta.unpacked_size > MAX_MEMBER {
                return Err(Error::Limit(format!("RAR member '{name}' exceeds 4 GiB")));
            }
            total = total
                .checked_add(meta.unpacked_size)
                .filter(|n| *n <= MAX_TOTAL)
                .ok_or_else(|| Error::Limit("RAR output exceeds 16 GiB".into()))?;
            let crc32 = match member.detail {
                ArchiveMemberDetail::Rar15To40 { crc32, .. } => Some(crc32),
                ArchiveMemberDetail::Rar50Plus { crc32, .. } => crc32,
                _ => None,
            };
            entries.push(Entry {
                name,
                size: meta.unpacked_size,
                compressed_size: meta.packed_size,
                method: if meta.is_stored {
                    Method::Store
                } else {
                    Method::Rar
                },
                crc32,
                is_dir: meta.is_directory,
                mtime: meta
                    .modification_time()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .and_then(|t| i64::try_from(t.as_secs()).ok()),
                created: None,
                accessed: None,
                attributes: if meta.attr_source() == rars::AttrSource::Dos {
                    meta.file_attr as u8
                } else {
                    0
                },
                // RAR identity is a member index, never a ZIP byte offset.
                offset: index as u64,
                raw_name: meta.name.clone(),
                utf8: true,
                encrypted: meta.is_encrypted,
                zipcrypto: false,
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
        Ok(RarArchive { inner, entries })
    })
}

fn validate_name(name: &str) -> Result<()> {
    if name.starts_with('/')
        || name
            .chars()
            .any(|c| c.is_control() || ":<>\"|?*".contains(c))
    {
        return Err(Error::Format(format!("unsafe RAR path: '{name}'")));
    }
    arca_core::safe_name(name)?;
    for part in name.split('/').filter(|p| !p.is_empty()) {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let numbered = ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2"
                        | "3"
                        | "4"
                        | "5"
                        | "6"
                        | "7"
                        | "8"
                        | "9"
                        | "\u{b9}"
                        | "\u{b2}"
                        | "\u{b3}"
                )
            })
        });
        if part.ends_with(['.', ' '])
            || matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            || numbered
        {
            return Err(Error::Format(format!(
                "unsafe RAR path component: '{part}'"
            )));
        }
    }
    Ok(())
}

pub(super) fn test(
    archive: &RarArchive,
    password: Option<&str>,
    progress: &Progress<'_>,
) -> Result<()> {
    controlled(progress, |token| {
        rars::extract_volumes_to_with_options(
            &archive.inner.archives,
            options(password, token),
            |_| Ok(Box::new(io::sink())),
        )
        .map_err(|e| archive.inner.error(e))
    })
}

pub(super) fn read_entry(
    archive: &RarArchive,
    index: usize,
    password: Option<&str>,
) -> Result<Vec<u8>> {
    let entry = archive
        .entries
        .get(index)
        .ok_or_else(|| Error::Format("RAR entry not found".into()))?;
    if entry.size > PREVIEW_LIMIT {
        return Err(Error::Limit("RAR preview exceeds 64 MiB".into()));
    }
    let token = ReadCancellation::new();
    if let [single] = archive.inner.archives.as_slice() {
        return single
            .read_member_at_with_options(
                index,
                options(password, &token).with_max_member_output_bytes(PREVIEW_LIMIT),
            )
            .map_err(|e| archive.inner.error(e))?
            .ok_or_else(|| Error::Unsupported("RAR entry has no file contents".into()));
    }
    if entry.is_dir {
        return Err(Error::Unsupported("RAR entry has no file contents".into()));
    }
    let buffer = Rc::new(RefCell::new(PreviewData::default()));
    let mut current = 0;
    let result = rars::extract_volumes_to_with_options(
        &archive.inner.archives,
        options(password, &token),
        |_| {
            let selected = current == index;
            current += 1;
            if selected {
                Ok(Box::new(PreviewWriter(Rc::clone(&buffer))) as Box<dyn io::Write>)
            } else {
                Ok(Box::new(io::sink()) as Box<dyn io::Write>)
            }
        },
    );
    let mut buffer = buffer.borrow_mut();
    if buffer.exceeded {
        return Err(Error::Limit("RAR preview exceeds 64 MiB".into()));
    }
    result.map_err(|e| archive.inner.error(e))?;
    Ok(std::mem::take(&mut buffer.bytes))
}

#[derive(Default)]
struct PreviewData {
    bytes: Vec<u8>,
    exceeded: bool,
}

struct PreviewWriter(Rc<RefCell<PreviewData>>);

impl io::Write for PreviewWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = self.0.borrow_mut();
        if bytes.len() as u64 > PREVIEW_LIMIT - buffer.bytes.len() as u64 {
            buffer.exceeded = true;
            return Err(io::Error::other("RAR preview exceeds 64 MiB"));
        }
        buffer.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                let link = meta.file_type().is_symlink();
                #[cfg(windows)]
                let link = {
                    use std::os::windows::fs::MetadataExt;
                    link || meta.file_attributes() & 0x400 != 0
                };
                if link {
                    return Err(Error::Format(format!(
                        "RAR destination contains a link: '{}'",
                        ancestor.display()
                    )));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub(super) fn extract(
    archive: &RarArchive,
    dest: &Path,
    wanted: &[bool],
    password: Option<&str>,
    progress: &Progress<'_>,
    conflict: &dyn Fn(&Path) -> Conflict,
) -> Result<u64> {
    let selected = |i: usize| wanted.is_empty() || wanted.get(i).copied().unwrap_or(false);
    let staging = tempfile::Builder::new().prefix("arca-rar-").tempdir()?;
    controlled(progress, |token| {
        let mut index = 0;
        rars::extract_volumes_to_with_options(
            &archive.inner.archives,
            options(password, token),
            |member| {
                let current = index;
                index += 1;
                let entry = archive
                    .entries
                    .get(current)
                    .ok_or(rars::Error::InvalidHeader("RAR member order changed"))?;
                if !progress(current, archive.entries.len(), &entry.name) {
                    return Err(rars::Error::Cancelled);
                }
                if member.is_directory {
                    return Ok(Box::new(io::sink()) as Box<dyn io::Write>);
                }
                let sink: Box<dyn io::Write> = if selected(current) {
                    Box::new(File::create(staging.path().join(current.to_string()))?)
                } else {
                    Box::new(io::sink())
                };
                Ok(sink)
            },
        )
        .map_err(|e| archive.inner.error(e))?;
        Ok(())
    })?;
    if !progress(0, archive.entries.len(), "RAR") {
        return Err(Error::Cancelled);
    }
    let absolute = std::path::absolute(dest)?;
    let mut existing = absolute.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| Error::Format("missing destination root".into()))?;
    }
    let dest = existing.canonicalize()?.join(
        absolute
            .strip_prefix(existing)
            .map_err(|_| Error::Format("invalid destination".into()))?,
    );
    let key = |path: &Path| path.to_string_lossy().to_lowercase();
    let mut claimed = HashSet::new();
    for entry in &archive.entries {
        let path = dest.join(arca_core::safe_name(&entry.name)?);
        for ancestor in path.ancestors() {
            claimed.insert(key(ancestor));
        }
    }
    let mut targets = Vec::new();
    for (i, entry) in archive
        .entries
        .iter()
        .enumerate()
        .filter(|(i, _)| selected(*i))
    {
        let mut target = dest.join(arca_core::safe_name(&entry.name)?);
        reject_links(&target)?;
        let mut overwrite = false;
        if !entry.is_dir && target.exists() {
            match conflict(&target) {
                Conflict::Skip => continue,
                Conflict::Cancel => return Err(Error::Cancelled),
                Conflict::Overwrite => overwrite = true,
                Conflict::Rename => {
                    let parent = target
                        .parent()
                        .ok_or_else(|| Error::Format("missing destination parent".into()))?;
                    let stem = target.file_stem().unwrap_or_default().to_string_lossy();
                    let extension = target
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    target = (1..10_000)
                        .map(|n| parent.join(format!("{stem} ({n}){extension}")))
                        .find(|p| !p.exists() && !claimed.contains(&key(p)))
                        .ok_or_else(|| {
                            Error::Limit("cannot find an unused extraction name".into())
                        })?;
                }
            }
        }
        if target.exists() && target.is_dir() != entry.is_dir {
            return Err(Error::Format(format!(
                "file/directory conflict at '{}'",
                target.display()
            )));
        }
        claimed.insert(key(&target));
        targets.push((i, target, overwrite));
    }
    let mut bytes = 0;
    for (i, target, overwrite) in targets {
        let entry = &archive.entries[i];
        if !progress(i, archive.entries.len(), &entry.name) {
            return Err(Error::Cancelled);
        }
        reject_links(&target)?;
        if entry.is_dir {
            fs::create_dir_all(&target)?;
            continue;
        }
        let parent = target
            .parent()
            .ok_or_else(|| Error::Format("missing destination parent".into()))?;
        fs::create_dir_all(parent)?;
        // Publish only verified bytes, with a same-filesystem atomic replacement.
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        let mut input = File::open(staging.path().join(i.to_string()))?;
        bytes += io::copy(&mut input, &mut output)?;
        output.as_file().sync_all()?;
        reject_links(&target)?;
        if overwrite {
            output.persist(&target)
        } else {
            output.persist_noclobber(&target)
        }
        .map_err(|e| Error::Io(e.error))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_writer_caps_actual_output_before_appending() {
        use std::io::Write;
        let buffer = Rc::new(RefCell::new(PreviewData {
            bytes: vec![0; PREVIEW_LIMIT as usize - 1],
            exceeded: false,
        }));
        let mut writer = PreviewWriter(Rc::clone(&buffer));
        assert_eq!(writer.write(b"x").unwrap(), 1);
        assert_eq!(writer.write(b"").unwrap(), 0);
        assert!(writer.write(b"y").is_err());
        assert_eq!(buffer.borrow().bytes.len(), PREVIEW_LIMIT as usize);
        assert!(buffer.borrow().exceeded);
    }

    #[test]
    fn cancellation_reaches_long_running_backend_work_and_preserves_its_kind() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result = controlled::<()>(
            &|_, _, _| calls.fetch_add(1, Ordering::SeqCst) == 0,
            |token| {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while !token.is_cancelled() && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(Error::Format("decoder stopped after cancellation".into()))
            },
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[test]
    fn rejects_unsafe_and_ambiguous_names() {
        for name in [
            "../x",
            "/tmp/x",
            "C:/x",
            "a:stream",
            "CON.txt",
            "lpt1",
            "foo.",
            "foo ",
            "a/../../x",
            "x\0y",
        ] {
            assert!(validate_name(name).is_err(), "{name}");
        }
        assert!(validate_name("folder/file.txt").is_ok());
    }
}
