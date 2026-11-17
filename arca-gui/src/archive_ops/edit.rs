//! Changing an archive in place, whatever its format: a ZIP through arca-zip,
//! a TAR by copying its entries into a new one and compressing that the way
//! the old one was, a 7z by decoding and encoding again.
//!
//! Each writes the new archive at `out`, beside the original, and reads it
//! back in full before saying it worked; `rewrite_zip` only then steps the
//! original aside. Every handle on `out` is closed by the time these return.

use super::{mtime_of, open_source, Job, Paced, BUF};
use crate::{detect, Format};
use arca_core::{Entry, Error, Level, Result};
use arca_tar::{TarReader, TarWriter};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

type Notify<'a> = &'a (dyn Fn(usize, usize, &str) -> bool + Sync);

/// Whether entries can be added to, taken out of and renamed in an archive of
/// this format. Not the ZIP containers, which keep a signature or a layout,
/// and not a standalone stream, which holds one file and no names.
pub(crate) fn editable(format: Format) -> bool {
    matches!(
        format,
        Format::Zip | Format::Tar | Format::TarGz | Format::TarXz | Format::TarBz2 | Format::SevenZ
    ) || (format == Format::TarZst && cfg!(feature = "codecs-native"))
}

/// Whether `job`, when it changes an archive, can change that one: a new
/// password only in a ZIP, entries in any format [`editable`] takes.
pub(crate) fn can_edit(job: &Job) -> bool {
    match job {
        Job::Password { archive, .. } | Job::Comment { archive, .. } => {
            detect(archive) == Some(Format::Zip)
        }
        Job::Delete { archive, .. }
        | Job::Rename { archive, .. }
        | Job::Move { archive, .. }
        | Job::NewFolder { archive, .. }
        | Job::Add { archive, .. } => detect(archive).is_some_and(editable),
        _ => true,
    }
}

/// `archive` without the entries `keep` turns down.
pub(crate) fn remove_entries(
    archive: &Path,
    out: &Path,
    password: Option<&str>,
    keep: &dyn Fn(&Entry) -> bool,
    notify: Notify<'_>,
) -> Result<()> {
    match format_of(archive)? {
        Format::Zip => arca_zip::remove_entries(archive, out, password, keep, notify).map(drop),
        format => rewrite(
            archive,
            out,
            format,
            password,
            &|e| keep(e).then(|| e.name.clone()),
            &[],
            Level::NORMAL,
            notify,
        ),
    }
}

/// `archive` with every entry under the name `name` gives it.
pub(crate) fn rename_entries(
    archive: &Path,
    out: &Path,
    password: Option<&str>,
    name: &dyn Fn(&Entry) -> String,
    notify: Notify<'_>,
) -> Result<()> {
    match format_of(archive)? {
        Format::Zip => arca_zip::rename_entries(archive, out, password, name, notify).map(drop),
        format => rewrite(
            archive,
            out,
            format,
            password,
            &|e| Some(name(e)),
            &[],
            Level::NORMAL,
            notify,
        ),
    }
}

/// `archive` with `extra` in it, each replacing the entry that had its name.
///
/// A TAR or a 7z is compressed again as a whole, at the level asked for with
/// the files being added; a new folder alone carries no level of its own and
/// leaves the normal one.
pub(crate) fn add_entries(
    archive: &Path,
    out: &Path,
    password: Option<&str>,
    extra: &[arca_zip::Addition],
    notify: Notify<'_>,
) -> Result<()> {
    let format = format_of(archive)?;
    if format == Format::Zip {
        return arca_zip::add_entries(archive, out, password, extra, notify).map(drop);
    }
    let level = extra
        .iter()
        .find(|a| a.source.is_some())
        .map_or(Level::NORMAL, |a| a.level);
    rewrite(
        archive,
        out,
        format,
        password,
        &|e| Some(e.name.clone()),
        extra,
        level,
        notify,
    )
}

fn format_of(archive: &Path) -> Result<Format> {
    match detect(archive) {
        Some(format) if editable(format) => Ok(format),
        Some(format) if !format.can_write() => Err(format.read_only()),
        _ => Err(Error::Unsupported(format!(
            "'{}' cannot be changed in place",
            archive.display()
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite(
    archive: &Path,
    out: &Path,
    format: Format,
    password: Option<&str>,
    name: &dyn Fn(&Entry) -> Option<String>,
    extra: &[arca_zip::Addition],
    level: Level,
    notify: Notify<'_>,
) -> Result<()> {
    if format == Format::SevenZ {
        let add: Vec<arca_7z::Addition> = extra
            .iter()
            .map(|a| arca_7z::Addition {
                source: a.source.clone(),
                name: a.name.clone(),
            })
            .collect();
        let change = arca_7z::Change {
            name,
            add: &add,
            level,
        };
        return arca_7z::rewrite_7z(archive, out, password, &change, &mut |p| {
            notify(p.entries_done, p.entries_total, p.name)
        });
    }
    let taken: HashSet<&str> = extra.iter().map(|a| a.name.trim_end_matches('/')).collect();
    let name = |e: &Entry| name(e).filter(|n| !taken.contains(n.trim_end_matches('/')));
    let written = write_tar(archive, out, format, &name, extra, level, notify)?;
    verify_tar(out, format, &written, notify)
}

/// The compressor a TAR goes through on its way to the file.
pub(super) enum Packed<W: Write> {
    Plain(W),
    Gz(flate2::write::GzEncoder<W>),
    Stream(arca_stream::Encoder<W>),
}

impl<W: Write> Packed<W> {
    pub(super) fn new(out: W, format: Format, level: Level) -> Result<Self> {
        Ok(match format {
            Format::Tar => Packed::Plain(out),
            Format::TarGz => Packed::Gz(flate2::write::GzEncoder::new(
                out,
                flate2::Compression::new(level.to_flate2()),
            )),
            _ => Packed::Stream(arca_stream::encoder(out, format, level, 0, None)?),
        })
    }

    pub(super) fn finish(self) -> Result<W> {
        Ok(match self {
            Packed::Plain(w) => w,
            Packed::Gz(e) => e.finish()?,
            Packed::Stream(e) => e.finish()?,
        })
    }
}

impl<W: Write> Write for Packed<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Packed::Plain(w) => w.write(buf),
            Packed::Gz(e) => e.write(buf),
            Packed::Stream(e) => e.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Packed::Plain(w) => w.flush(),
            Packed::Gz(e) => e.flush(),
            Packed::Stream(e) => e.flush(),
        }
    }
}

/// Copies what is kept and appends `extra`, and gives back the names written.
fn write_tar(
    archive: &Path,
    out: &Path,
    format: Format,
    name: &dyn Fn(&Entry) -> Option<String>,
    extra: &[arca_zip::Addition],
    level: Level,
    notify: Notify<'_>,
) -> Result<Vec<String>> {
    let mut reader = TarReader::new(open_source(archive, format)?);
    let file = BufWriter::with_capacity(BUF, File::create(out)?);
    let mut w = TarWriter::new(Packed::new(file, format, level)?);
    let mut i = 0usize;
    let mut written =
        arca_tar::copy_entries(&mut reader, &mut w, &|e| name(&e.entry), &mut |n, at| {
            if at == 0 {
                i += 1;
            }
            notify(i - 1, i, n)
        })?;
    // The decompressor checks its trailer only once it is read to the end:
    // without this a damaged one would be copied into a new archive that
    // checks out.
    arca_stream::drain(&mut reader.into_inner())?;
    let total = i + extra.len();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    for a in extra {
        if !notify(i, total, &a.name) {
            return Err(Error::Cancelled);
        }
        match &a.source {
            None => w.add(&a.name, 0, now, 0o755, io::empty())?,
            Some(path) => {
                let meta = std::fs::metadata(path)?;
                let mut data = Paced::new(
                    BufReader::with_capacity(BUF, File::open(path)?),
                    &a.name,
                    notify,
                );
                let result = w.add(
                    &a.name,
                    meta.len(),
                    mtime_of(&meta),
                    arca_tar::mode_of(&meta),
                    &mut data,
                );
                data.check(result)?;
            }
        }
        written.push(a.name.clone());
        i += 1;
    }
    let file = w.finish()?.finish()?;
    file.into_inner()
        .map_err(|e| Error::Io(e.into_error()))?
        .sync_all()?;
    Ok(written)
}

/// Reads the new TAR back to its end, data and compression trailer included,
/// and holds its names to the ones written.
fn verify_tar(out: &Path, format: Format, written: &[String], notify: Notify<'_>) -> Result<()> {
    let mut reader = TarReader::new(open_source(out, format)?);
    let mut i = 0usize;
    while let Some(e) = reader.next_entry()? {
        if !notify(i, written.len(), &e.entry.name) {
            return Err(Error::Cancelled);
        }
        if written.get(i) != Some(&e.entry.name) {
            return Err(Error::Format(
                "the rewritten archive does not list what was written".into(),
            ));
        }
        let name = e.entry.name.as_str();
        reader.with_data(&e, |data| {
            let mut data = Paced::new(data, name, notify);
            let result = io::copy(&mut data, &mut io::sink()).map_err(Error::from);
            data.check(result)
        })?;
        i += 1;
    }
    arca_stream::drain(&mut reader.into_inner())?;
    if i != written.len() {
        return Err(Error::Format(
            "the rewritten archive does not list what was written".into(),
        ));
    }
    Ok(())
}
