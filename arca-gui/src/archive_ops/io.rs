//! Archive readers, extractors, and filesystem writers.

use crate::{archive_stem, detect, Format, Message};
use arca_core::{Codec, Entry, Level};
use arca_tar::{TarReader, TarWriter};
use arca_zip::ZipArchive;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

pub(crate) const BUF: usize = 256 * 1024;

pub(crate) fn open_source(archive: &Path, format: Format) -> std::io::Result<Box<dyn Read>> {
    let f = BufReader::with_capacity(BUF, File::open(archive)?);
    Ok(match format {
        Format::TarGz => Box::new(flate2::read::GzDecoder::new(f)),
        _ => Box::new(f),
    })
}

pub(crate) fn list_entries(
    archive: &Path,
    password: Option<&str>,
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> arca_core::Result<Vec<Entry>> {
    let Some(format) = detect(archive) else {
        return Err(arca_core::Error::Unsupported(format!(
            "unrecognized extension in '{}'",
            archive.display()
        )));
    };
    match format {
        Format::Rar => {
            let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
            Ok(a.entries().to_vec())
        }
        Format::Zip => Ok(ZipArchive::open(File::open(archive)?)?.entries().to_vec()),
        Format::SevenZ => Ok(
            arca_7z::SevenZArchive::open(File::open(archive)?, password)?
                .entries()
                .to_vec(),
        ),
        Format::Tar | Format::TarGz => {
            let mut r = TarReader::new(open_source(archive, format)?);
            let mut v = Vec::new();
            while let Some(e) = r.next_entry()? {
                v.push(e.entry.clone());
                r.skip_data(&e)?;
            }
            Ok(v)
        }
    }
}

pub(crate) fn check_access(
    archive: &Path,
    password: Option<&str>,
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> arca_core::Result<()> {
    match detect(archive) {
        Some(Format::Rar) => {
            let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
            if a.entries().iter().any(|e| e.encrypted) {
                let password = password.ok_or(arca_core::Error::PasswordRequired)?;
                a.test(Some(password), notify)
            } else {
                Ok(())
            }
        }
        Some(Format::SevenZ) => {
            let mut a = arca_7z::SevenZArchive::open(File::open(archive)?, password)?;
            if a.has_encrypted() && password.is_none() {
                return Err(arca_core::Error::PasswordRequired);
            }
            a.validate_encrypted(&mut |p| notify(p.entries_done, p.entries_total, p.name))
        }
        Some(Format::Zip) => {
            let a = ZipArchive::open(File::open(archive)?)?;
            if a.has_encrypted() {
                let password = password.ok_or(arca_core::Error::PasswordRequired)?;
                let entries = a.entries().to_vec();
                arca_zip::check_password(&mut File::open(archive)?, &entries, password)
            } else {
                Ok(())
            }
        }
        Some(_) => Ok(()),
        None => Err(arca_core::Error::Unsupported("unknown format".into())),
    }
}

// The icon the desktop shows for this kind of file, kept as a texture per
// extension. Without the cache a listing of 1513 entries would ask the shell
// 1513 times a frame; with it, once per kind for the life of the window.
//
// A `None` in the map is a remembered failure, so a kind the system has no
// answer for is not asked about again every frame.
// What the desktop calls this kind of file, cached by extension the way the
// icons are: the answer is the same for every .txt in the archive, and asking
// the shell fifteen hundred times for it would be fifteen hundred round trips
// to another thread while the list is being drawn.
// A folder of its own per archive, so two archives holding a file with the same
// name do not overwrite each other's copy. safe_name is what keeps an entry
// called "../../evil" from landing outside it.
/// Where the version before the last change is kept, so it can be put back.
pub(crate) fn undo_path(archive: &Path) -> PathBuf {
    let mut name = archive.as_os_str().to_os_string();
    name.push(".arca-undo");
    PathBuf::from(name)
}

/// Moves the archive out of the way instead of letting the new one overwrite
/// it, so that the change can be taken back.
///
/// A move, not a copy: the file stays on the volume it was already on and
/// nothing is read or written, so keeping the old version costs the time of a
/// directory entry however big the archive is. What it does cost is the space,
/// until the next change replaces it or the window closes.
pub(crate) fn step_aside(archive: &Path) -> std::io::Result<()> {
    let keep = undo_path(archive);
    if keep.exists() {
        fs::remove_file(&keep)?;
    }
    fs::rename(archive, &keep)
}

// One entry straight into memory, for looking at rather than for keeping.
//
// The same walk as `extract_one` without the file at the end of it: a viewer
// that wrote to the temporary folder on the way would have extracted the thing
// it was only supposed to show.
pub(crate) fn read_entry(
    archive: &Path,
    index: usize,
    out: &mut Vec<u8>,
    password: Option<&str>,
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> arca_core::Result<()> {
    let Some(format) = detect(archive) else {
        return Err(arca_core::Error::Unsupported("unknown format".into()));
    };
    match format {
        Format::SevenZ => {
            let mut a = arca_7z::SevenZArchive::open(File::open(archive)?, password)?;
            *out = a.read_entry(index, crate::VIEW_LIMIT, &mut |p| {
                notify(p.entries_done, p.entries_total, p.name)
            })?;
        }
        Format::Rar => {
            let a = arca_rar::RarArchive::open(archive, password)?;
            *out = a.read_entry(index, password)?;
        }
        Format::Zip => {
            let mut a = ZipArchive::open(File::open(archive)?)?;
            a.extract_to_with(index, out, password)?;
        }
        Format::Tar | Format::TarGz => {
            // A tar has no index, so the only way to one entry is through all
            // the ones before it.
            let mut r = TarReader::new(open_source(archive, format)?);
            let mut at = 0usize;
            while let Some(e) = r.next_entry()? {
                if at == index {
                    r.copy_data(&e, out)?;
                    return Ok(());
                }
                r.skip_data(&e)?;
                at += 1;
            }
            return Err(arca_core::Error::Format(
                "that entry is not in the archive any more".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn extract_one(
    archive: &Path,
    entry: &Entry,
    password: Option<&str>,
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> arca_core::Result<PathBuf> {
    let room = fs::canonicalize(std::env::temp_dir())?
        .join("Arca")
        .join(archive_stem(archive));
    if detect(archive) == Some(Format::Rar) {
        let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
        let index = usize::try_from(entry.offset)
            .map_err(|_| arca_core::Error::Format("invalid RAR entry index".into()))?;
        if a.entries().get(index).is_none_or(|e| e.name != entry.name) {
            return Err(arca_core::Error::Format(
                "RAR entry changed since listing".into(),
            ));
        }
        let mut wanted = vec![false; a.entries().len()];
        wanted[index] = true;
        a.extract(&room, &wanted, password, notify, &|_| {
            arca_rar::Conflict::Overwrite
        })?;
        return Ok(room.join(arca_core::safe_name(&entry.name)?));
    }
    let path = room.join(arca_core::safe_name(&entry.name)?);
    if detect(archive) == Some(Format::SevenZ) {
        let mut a = arca_7z::SevenZArchive::open(File::open(archive)?, password)?;
        let index = usize::try_from(entry.offset)
            .map_err(|_| arca_core::Error::Format("invalid entry index".into()))?;
        if a.entries().get(index).is_none_or(|e| e.name != entry.name) {
            return Err(arca_core::Error::Format(
                "7z entry changed since listing".into(),
            ));
        }
        let mut destination = arca_core::extraction::Destination::new(&room);
        a.extract(
            &[index],
            &mut |_, e, reader| {
                write_sevenz_entry(&mut destination, e, reader, &|_| Answer::Replace).map(|_| ())
            },
            &mut |p| notify(p.entries_done, p.entries_total, p.name),
        )?;
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let Some(format) = detect(archive) else {
        return Err(arca_core::Error::Unsupported("unknown format".into()));
    };
    let mut out = BufWriter::with_capacity(BUF, File::create(&path)?);
    match format {
        Format::SevenZ => unreachable!(),
        Format::Rar => return Err(arca_rar::read_only()),
        Format::Zip => {
            let mut source = BufReader::with_capacity(BUF, File::open(archive)?);
            arca_zip::extract_entry_with(&mut source, entry, &mut out, password)?;
        }
        Format::Tar | Format::TarGz => {
            // A tar has no index, so the only way to one entry is through all
            // the ones before it.
            let mut r = TarReader::new(open_source(archive, format)?);
            let mut found = false;
            while let Some(e) = r.next_entry()? {
                if e.entry.name == entry.name && !e.entry.is_dir {
                    r.copy_data(&e, &mut out)?;
                    found = true;
                    break;
                }
                r.skip_data(&e)?;
            }
            if !found {
                return Err(arca_core::Error::Format(format!(
                    "'{}' is not in the archive any more",
                    entry.name
                )));
            }
        }
    }
    out.flush()?;
    Ok(path)
}

// Whatever the desktop opens this kind of file with. The child is left to run
// on its own; the window does not wait for it and does not care what it was.
#[cfg(windows)]
pub(crate) fn launch_with_system(path: &Path) -> arca_core::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // The empty pair of quotes is the window title `start` insists on eating.
    // Without it a quoted path becomes the title and nothing opens. No /WAIT
    // either: that would leave a cmd sitting around until the viewer is closed.
    std::process::Command::new("cmd")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .map_err(arca_core::Error::Io)?;
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn launch_with_system(path: &Path) -> arca_core::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(path)
        .spawn()
        .map_err(arca_core::Error::Io)?;
    Ok(())
}

// A button with a picture on it, and a word next to the picture when the button
// is one of the ones worth naming. GPUI Kit supplies the button surface;
// icons and labels remain separate so each can be styled independently.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    Replace,
    ReplaceAll,
    Skip,
    SkipAll,
    Rename,
    RenameAll,
    Cancel,
}

// The worker asks the window and blocks until it answers. The "all" answers
// stick, so the question is asked once and not per file.
pub(crate) fn conflict_asker<'a>(
    tx: &'a Sender<Message>,
    replies: &'a std::sync::mpsc::Receiver<Answer>,
) -> impl Fn(&Path) -> Answer + 'a {
    let sticky = std::cell::Cell::new(None::<Answer>);
    move |path: &Path| {
        if let Some(a) = sticky.get() {
            return a;
        }
        if tx
            .send(Message::Conflict(path.display().to_string()))
            .is_err()
        {
            return Answer::Cancel;
        }
        let answer = replies.recv().unwrap_or(Answer::Cancel);
        if matches!(
            answer,
            Answer::ReplaceAll | Answer::SkipAll | Answer::RenameAll | Answer::Cancel
        ) {
            sticky.set(Some(answer));
        }
        answer
    }
}

// `claimed` holds the names this run has already handed out. A .zip decides
// every destination before writing anything, so `exists()` alone would give two
// entries with the same name the same free name.
pub(crate) fn free_name(path: &Path, claimed: &HashSet<PathBuf>) -> PathBuf {
    let dir = path.parent().map(PathBuf::from).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..10_000u32 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() && !claimed.contains(&candidate) {
            return candidate;
        }
    }
    path.to_path_buf()
}

// Returns None when the entry must be skipped, and Err on cancel.
pub(crate) fn dest_path(
    dest: &Path,
    name: &str,
    is_dir: bool,
    ask: &dyn Fn(&Path) -> Answer,
    claimed: &mut HashSet<PathBuf>,
) -> arca_core::Result<Option<PathBuf>> {
    let path = dest.join(arca_core::safe_name(name)?);
    if is_dir {
        fs::create_dir_all(&path)?;
        return Ok(None);
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    if !path.exists() && !claimed.contains(&path) {
        claimed.insert(path.clone());
        return Ok(Some(path));
    }
    let chosen = match ask(&path) {
        Answer::Replace | Answer::ReplaceAll => path,
        Answer::Skip | Answer::SkipAll => return Ok(None),
        Answer::Rename | Answer::RenameAll => {
            let renamed = free_name(&path, claimed);
            if renamed == path {
                return Err(arca_core::Error::Limit(
                    "no free extraction name available".into(),
                ));
            }
            renamed
        }
        Answer::Cancel => return Err(arca_core::Error::Format("cancelled".into())),
    };
    claimed.insert(chosen.clone());
    Ok(Some(chosen))
}

pub(crate) fn creation_output(
    out: &Path,
    ask: &dyn Fn(&Path) -> Answer,
) -> arca_core::Result<PathBuf> {
    if !out.exists() {
        return Ok(out.to_path_buf());
    }
    match ask(out) {
        Answer::Replace | Answer::ReplaceAll => Ok(out.to_path_buf()),
        Answer::Rename | Answer::RenameAll => {
            let renamed = free_name(out, &HashSet::new());
            if renamed == out {
                return Err(arca_core::Error::Limit(
                    "no free archive name available".into(),
                ));
            }
            Ok(renamed)
        }
        _ => Err(arca_core::Error::Cancelled),
    }
}

fn write_sevenz_entry(
    destination: &mut arca_core::extraction::Destination,
    entry: &Entry,
    reader: &mut dyn Read,
    ask: &dyn Fn(&Path) -> Answer,
) -> arca_core::Result<u64> {
    // This callback runs only after encrypted content has been validated.
    use arca_core::extraction::Conflict;
    destination
        .write_entry(
            &entry.name,
            entry.is_dir,
            reader,
            &mut |path| match ask(path) {
                Answer::Replace | Answer::ReplaceAll => Conflict::Overwrite,
                Answer::Skip | Answer::SkipAll => Conflict::Skip,
                Answer::Rename | Answer::RenameAll => Conflict::Rename,
                Answer::Cancel => Conflict::Cancel,
            },
        )
        .map(|written| written.unwrap_or(0))
}

// A .zip is random access: the central directory says where every entry starts,
// so one thread per core can each open the file and decompress a different one.
// A .tar is a single stream, and a .tar.gz a single gzip stream on top of it, so
// there is nothing to split there and that branch stays sequential.
//
// The directories and the overwrite questions are settled first, in one thread.
// Asking the window from several threads at once would put the same dialog on
// screen twice, and racing on which name is free gives a different result every
// run.
pub(crate) fn extract(
    archive: &Path,
    dest: &Path,
    wanted: &[bool],
    // Told how far along this is, and answers whether to carry on. False is
    // somebody pressing stop.
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
    ask: &dyn Fn(&Path) -> Answer,
    password: Option<&str>,
) -> arca_core::Result<u64> {
    let Some(format) = detect(archive) else {
        return Err(arca_core::Error::Unsupported("unknown format".into()));
    };
    if format == Format::Rar {
        let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
        return a.extract(dest, wanted, password, notify, &|path| match ask(path) {
            Answer::Replace | Answer::ReplaceAll => arca_rar::Conflict::Overwrite,
            Answer::Skip | Answer::SkipAll => arca_rar::Conflict::Skip,
            Answer::Rename | Answer::RenameAll => arca_rar::Conflict::Rename,
            Answer::Cancel => arca_rar::Conflict::Cancel,
        });
    }
    if format != Format::SevenZ {
        fs::create_dir_all(dest)?;
    }
    let mut bytes = 0u64;
    let mut claimed: HashSet<PathBuf> = HashSet::new();

    match format {
        Format::SevenZ => {
            let mut a = arca_7z::SevenZArchive::open(File::open(archive)?, password)?;
            let mut destination = arca_core::extraction::Destination::new(dest);
            let indices: Vec<_> = (0..a.len())
                .filter(|&i| wanted.is_empty() || wanted.get(i).copied().unwrap_or(false))
                .collect();
            a.extract(
                &indices,
                &mut |_, e, reader| {
                    bytes += write_sevenz_entry(&mut destination, e, reader, ask)?;
                    Ok(())
                },
                &mut |p| notify(p.entries_done, p.entries_total, p.name),
            )?;
        }
        Format::Rar => return Err(arca_rar::read_only()),
        Format::Zip => {
            let a = ZipArchive::open(File::open(archive)?)?;
            let mut jobs: Vec<(Entry, PathBuf)> = Vec::new();
            for (i, e) in a.entries().iter().enumerate() {
                if !wanted.is_empty() && !wanted.get(i).copied().unwrap_or(true) {
                    continue;
                }
                if let Some(path) = dest_path(dest, &e.name, e.is_dir, ask, &mut claimed)? {
                    jobs.push((e.clone(), path));
                }
            }
            drop(a);

            let total = jobs.len();
            let done = AtomicUsize::new(0);
            let written: Vec<u64> = jobs
                .par_iter()
                .map(|(e, path)| {
                    let mut source = BufReader::with_capacity(BUF, File::open(archive)?);
                    let mut f = BufWriter::with_capacity(BUF, File::create(path)?);
                    let w = arca_zip::extract_entry_with(&mut source, e, &mut f, password)?;
                    f.flush()?;
                    if !notify(done.fetch_add(1, Ordering::Relaxed) + 1, total, &e.name) {
                        return Err(arca_core::Error::Cancelled);
                    }
                    Ok(w)
                })
                .collect::<arca_core::Result<Vec<u64>>>()?;
            bytes = written.iter().sum();
            let _ = notify(total, total, "");
        }
        Format::Tar | Format::TarGz => {
            let mut r = TarReader::new(open_source(archive, format)?);
            let total = wanted.len();
            let mut i = 0usize;
            while let Some(e) = r.next_entry()? {
                if !notify(i, total, &e.entry.name) {
                    return Err(arca_core::Error::Cancelled);
                }
                if !wanted.is_empty() && !wanted.get(i).copied().unwrap_or(true) {
                    r.skip_data(&e)?;
                    i += 1;
                    continue;
                }
                match dest_path(dest, &e.entry.name, e.entry.is_dir, ask, &mut claimed)? {
                    Some(path) => {
                        let mut w = BufWriter::with_capacity(BUF, File::create(&path)?);
                        bytes += r.copy_data(&e, &mut w)?;
                        w.flush()?;
                    }
                    None => r.skip_data(&e)?,
                }
                i += 1;
            }
            let _ = notify(i, i, "");
        }
    }
    Ok(bytes)
}

pub(crate) fn test_archive(
    archive: &Path,
    only: Option<&HashSet<String>>,
    password: Option<&str>,
    // Told how far along this is, and answers whether to carry on. False is
    // somebody pressing stop.
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> arca_core::Result<(usize, Vec<String>)> {
    let Some(format) = detect(archive) else {
        return Err(arca_core::Error::Unsupported("unknown format".into()));
    };
    let mut good = 0usize;
    let mut bad = Vec::new();

    match format {
        Format::SevenZ => {
            let mut a = arca_7z::SevenZArchive::open(File::open(archive)?, password)?;
            let indices: Vec<_> = a
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, e)| only.is_none_or(|set| set.contains(&e.name)))
                .map(|(i, _)| i)
                .collect();
            if only.is_none() {
                a.test(&mut |p| notify(p.entries_done, p.entries_total, p.name))?;
                good = a.entries().iter().filter(|e| !e.is_dir).count();
            } else {
                a.extract(
                    &indices,
                    &mut |_, e, reader| {
                        std::io::copy(reader, &mut std::io::sink())?;
                        good += usize::from(!e.is_dir);
                        Ok(())
                    },
                    &mut |p| notify(p.entries_done, p.entries_total, p.name),
                )?;
            }
        }
        Format::Rar => {
            let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
            a.test(password, notify)?;
            good = a.entries().iter().filter(|e| !e.is_dir).count();
        }
        Format::Zip => {
            let mut a = ZipArchive::open(File::open(archive)?)?;
            let total = a.len();
            for i in 0..total {
                let name = a.entries()[i].name.clone();
                if !notify(i, total, &name) {
                    return Err(arca_core::Error::Cancelled);
                }
                if a.entries()[i].is_dir || only.is_some_and(|set| !set.contains(&name)) {
                    continue;
                }
                match a.extract_to_with(i, std::io::sink(), password) {
                    Ok(_) => good += 1,
                    Err(e) => bad.push(format!("{name}: {e}")),
                }
            }
            let _ = notify(total, total, "");
        }
        Format::Tar | Format::TarGz => {
            let mut r = TarReader::new(open_source(archive, format)?);
            let mut i = 0usize;
            while let Some(e) = r.next_entry()? {
                if !notify(i, i + 1, &e.entry.name) {
                    return Err(arca_core::Error::Cancelled);
                }
                if e.entry.is_dir || only.is_some_and(|set| !set.contains(&e.entry.name)) {
                    r.skip_data(&e)?;
                } else {
                    match r.copy_data(&e, &mut std::io::sink()) {
                        Ok(_) => good += 1,
                        Err(err) => bad.push(format!("{}: {err}", e.entry.name)),
                    }
                }
                i += 1;
            }
            let _ = notify(i, i, "");
        }
    }
    Ok((good, bad))
}

pub(crate) fn collect_files(inputs: &[PathBuf]) -> std::io::Result<Vec<(PathBuf, String)>> {
    collect_sources(inputs, false)
}

fn collect_sources(
    inputs: &[PathBuf],
    directories: bool,
) -> std::io::Result<Vec<(PathBuf, String)>> {
    fn walk(
        p: &Path,
        base: &Path,
        out: &mut Vec<(PathBuf, String)>,
        directories: bool,
    ) -> std::io::Result<()> {
        let meta = fs::symlink_metadata(p)?;
        let rel = p.strip_prefix(base).unwrap_or(p);
        let name = rel.to_string_lossy().replace('\\', "/");
        if meta.is_dir() {
            if directories && rel.components().any(|c| c != std::path::Component::CurDir) {
                out.push((p.to_path_buf(), format!("{name}/")));
            }
            let mut children: Vec<_> = fs::read_dir(p)?.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|d| d.file_name());
            for c in children {
                walk(&c.path(), base, out, directories)?;
            }
        } else if meta.is_file() {
            out.push((p.to_path_buf(), name));
        } else if directories {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "7z sources must be regular files or directories",
            ));
        }
        Ok(())
    }

    let mut v = Vec::new();
    for e in inputs {
        let base = e.parent().unwrap_or(Path::new(""));
        walk(e, base, &mut v, directories)?;
    }
    Ok(v)
}

/// Cuando se escribio por ultima vez un fichero, en segundos desde 1970.
///
/// Cero cuando el sistema no lo dice, que es lo que un zip entiende por dejar
/// ese hueco vacio de todas formas.
fn mtime_of(m: &fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn compress(
    out: &Path,
    inputs: &[PathBuf],
    format: Format,
    codec: Codec,
    level: Level,
    // Told how far along this is, and answers whether to carry on. False is
    // somebody pressing stop.
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
    encryption: (Option<&str>, bool),
) -> arca_core::Result<(u64, u64)> {
    let (password, hide_names) = encryption;
    if !format.can_write() || detect(out) == Some(Format::Rar) {
        return Err(arca_rar::read_only());
    }
    if password.is_some() && !matches!(format, Format::Zip | Format::SevenZ) {
        return Err(arca_core::Error::Unsupported(
            "encryption requires ZIP or 7z".into(),
        ));
    }
    if hide_names && format != Format::SevenZ {
        return Err(arca_core::Error::Unsupported(
            "hidden names require 7z".into(),
        ));
    }
    let files = collect_sources(inputs, format == Format::SevenZ)?;
    let total = files.len();
    let mut source_bytes = 0u64;

    match format {
        Format::SevenZ => {
            let mut sources = Vec::with_capacity(files.len());
            for (path, name) in &files {
                let meta = fs::metadata(path)?;
                if meta.is_file() {
                    source_bytes += meta.len();
                }
                sources.push(arca_7z::Source {
                    path: path.clone(),
                    name: name.clone(),
                });
            }
            arca_7z::create_7z(
                out,
                &sources,
                &arca_7z::CreateOptions {
                    level,
                    password,
                    hide_names,
                },
                &mut |p| notify(p.entries_done, p.entries_total, p.name),
            )?;
        }
        Format::Rar => return Err(arca_rar::read_only()),
        // Cada entrada de un zip se comprime por su cuenta, asi que esto entrega
        // la lista entera y deja que corra en todos los nucleos. Es la misma
        // llamada que hace la linea de ordenes: hay una, no dos.
        //
        // Ademas de repartir, ese camino usa libdeflate, que el de escribir
        // entrada a entrada no. Medido sobre 952 MB: 28,3 s y 841,6 MB haciendolo
        // uno detras de otro, 20,1 s y 835,8 MB por aqui. Menos tiempo y menos
        // tamano por el mismo trabajo.
        Format::Zip => {
            let mut sources = Vec::with_capacity(files.len());
            for (path, name) in &files {
                let meta = fs::metadata(path)?;
                source_bytes += meta.len();
                sources.push(arca_zip::Source {
                    path: path.clone(),
                    name: name.clone(),
                    size: meta.len(),
                    // La fecha que tenia el fichero. Escribiendo entrada a
                    // entrada iba un `None` aqui y todo lo que salia de la
                    // ventana se guardaba sin fecha, cosa que la consola nunca
                    // hizo.
                    mtime: mtime_of(&meta),
                    codec,
                    level,
                });
            }
            arca_zip::create_zip(out, &sources, 0, password, notify)?;
        }
        Format::Tar | Format::TarGz => {
            let raw = BufWriter::with_capacity(BUF, File::create(out)?);
            let sink: Box<dyn Write> = if format == Format::TarGz {
                Box::new(flate2::write::GzEncoder::new(
                    raw,
                    flate2::Compression::new(level.to_flate2()),
                ))
            } else {
                Box::new(raw)
            };
            let mut w = TarWriter::new(sink);
            for (i, (path, name)) in files.iter().enumerate() {
                if !notify(i, total, name) {
                    return Err(arca_core::Error::Cancelled);
                }
                let meta = fs::metadata(path)?;
                let f = BufReader::with_capacity(BUF, File::open(path)?);
                w.add(name, meta.len(), 0, 0o644, f)?;
                source_bytes += meta.len();
            }
            w.finish()?;
        }
    }
    let _ = notify(total, total, "");
    let final_size = fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    Ok((source_bytes, final_size))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    Beside,
    Subfolder,
}
