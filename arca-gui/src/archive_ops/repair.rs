//! Repairing a damaged archive into a new file beside it. The damaged one is
//! only read, and the new one takes its name only once it reads back.

use super::edit::Packed;
use super::{open_source, BUF};
use crate::i18n::Strings;
use crate::{detect, fill, Format};
use arca_core::volumes::{refuse_existing, staged_new};
use arca_core::{Error, Level, Result};
use arca_tar::{TarReader, TarWriter};
use arca_zip::ZipArchive;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};

type Notify<'a> = &'a (dyn Fn(usize, usize, &str) -> bool + Sync);

/// Whether a damaged archive of this format can be repaired.
pub(crate) fn repairable(format: Format) -> bool {
    matches!(
        format,
        Format::Zip
            | Format::Tar
            | Format::TarGz
            | Format::TarXz
            | Format::TarBz2
            | Format::TarZst
            | Format::SevenZ
            | Format::Rar
    )
}

/// `name (repaired).zip` beside `archive`, or with a number after the word
/// when that is taken.
pub(crate) fn repaired_name(archive: &Path, word: &str) -> PathBuf {
    let whole = arca_core::volumes::whole_name(archive);
    let name = whole
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, suffix) = match Format::split_name(&name) {
        Some((stem, _)) => (
            name[..stem.len()].to_string(),
            name[stem.len()..].to_string(),
        ),
        None => (name.clone(), String::new()),
    };
    let dir = whole.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut n = 1;
    loop {
        let tag = if n == 1 {
            format!("{stem} ({word}){suffix}")
        } else {
            format!("{stem} ({word} {n}){suffix}")
        };
        let candidate = dir.join(tag);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

fn beside(out: &Path, tag: &str) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(tag);
    out.with_file_name(name)
}

/// Repairs `archive` into `out`. Gives back what to tell, and the file
/// written, which is nothing when there was nothing to repair.
pub(crate) fn repair(
    archive: &Path,
    out: &Path,
    password: Option<&str>,
    s: &'static Strings,
    notify: Notify<'_>,
) -> std::result::Result<(String, Option<PathBuf>), String> {
    let format = detect(archive)
        .filter(|f| repairable(*f))
        .ok_or_else(|| s.repair_cannot.to_string())?;
    refuse_existing(archive, out).map_err(|e| e.to_string())?;
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let done = |kept: usize, lost: u64| {
        fill(
            s.repair_done,
            &[
                ("name", &name),
                ("kept", &kept.to_string()),
                ("lost", &lost.to_string()),
            ],
        )
    };
    let text = match format {
        Format::Zip => {
            let (kept, lost, unchecked) =
                staged_new(archive, out, |temp| zip(archive, temp, notify))
                    .map_err(|e| e.to_string())?;
            let mut text = done(kept, lost);
            if unchecked > 0 {
                text.push_str(". ");
                text.push_str(&fill(s.repair_encrypted, &[("n", &unchecked.to_string())]));
            }
            text
        }
        Format::SevenZ => {
            let report = staged_new(archive, out, |temp| {
                arca_7z::salvage_7z(archive, temp, password, Level::NORMAL, &mut |p| {
                    notify(p.entries_done, p.entries_total, p.name)
                })
            })
            .map_err(|e| e.to_string())?;
            done(report.kept, report.lost.len() as u64)
        }
        Format::Rar => {
            let report =
                arca_rar::repair(archive, out, password, notify).map_err(|e| e.to_string())?;
            if !report.changed {
                return Ok((s.repair_nothing.to_string(), None));
            }
            fill(s.repair_rebuilt, &[("name", &name)])
        }
        format => {
            let report = staged_new(archive, out, |temp| tar(archive, temp, format, notify))
                .map_err(|e| e.to_string())?;
            let mut text = done(
                report.kept.len(),
                report.lost.len() as u64 + report.unreported,
            );
            if report.stream.is_some() {
                text.push_str(". ");
                text.push_str(s.repair_unchecked);
            }
            text
        }
    };
    Ok((text, Some(out.to_path_buf())))
}

fn zip(archive: &Path, temp: &Path, notify: Notify<'_>) -> Result<(usize, u64, usize)> {
    let source = BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
    let sink = BufWriter::with_capacity(BUF, File::create(temp)?);
    let (sink, report) = arca_zip::repair_with(source, sink, notify)?;
    sink.into_inner()
        .map_err(|e| Error::Io(e.into_error()))?
        .sync_all()?;
    if report.recovered.is_empty() {
        return Err(Error::Format("no entry could be recovered".into()));
    }
    let mut back = ZipArchive::open(File::open(temp)?)?;
    if back.len() != report.recovered.len() {
        return Err(Error::Format(
            "the rebuilt archive does not read back as written".into(),
        ));
    }
    for i in 0..back.len() {
        let e = &back.entries()[i];
        if !e.encrypted && !e.is_dir {
            back.extract_to(i, io::sink())?;
        }
    }
    let unchecked = report.recovered.iter().filter(|r| !r.verified).count();
    Ok((
        report.recovered.len(),
        report.skipped.len() as u64 + report.unreported,
        unchecked,
    ))
}

fn salvage_source(archive: &Path, format: Format) -> Result<Box<dyn Read>> {
    if arca_stream::handles(format) {
        return Ok(arca_stream::open_salvage(archive, format)?);
    }
    let file = BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
    Ok(match format {
        Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(file)),
        _ => Box::new(file),
    })
}

fn tar(
    archive: &Path,
    temp: &Path,
    format: Format,
    notify: Notify<'_>,
) -> Result<arca_tar::Salvaged> {
    let spool = beside(temp, ".spool");
    let result = (|| -> Result<arca_tar::Salvaged> {
        let source = salvage_source(archive, format)?;
        let mut held = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&spool)?;
        let file = BufWriter::with_capacity(BUF, File::create(temp)?);
        let mut w = TarWriter::new(Packed::new(file, format, Level::NORMAL)?);
        let mut i = 0usize;
        let report = arca_tar::salvage(source, &mut w, &mut held, &mut |name, at| {
            if at == 0 {
                i += 1;
            }
            notify(i - 1, i, name)
        })?;
        drop(held);
        w.finish()?
            .finish()?
            .into_inner()
            .map_err(|e| Error::Io(e.into_error()))?
            .sync_all()?;
        Ok(report)
    })();
    let _ = fs::remove_file(&spool);
    let report = result?;
    if report.kept.is_empty() {
        return Err(Error::Format("no entry could be recovered".into()));
    }
    let mut back = TarReader::new(open_source(temp, format)?);
    let mut i = 0usize;
    while let Some(e) = back.next_entry()? {
        if report.kept.get(i) != Some(&e.entry.name) {
            return Err(Error::Format(
                "the rebuilt archive does not read back as written".into(),
            ));
        }
        back.skip_data(&e)?;
        i += 1;
    }
    arca_stream::drain(&mut back.into_inner())?;
    if i != report.kept.len() {
        return Err(Error::Format(
            "the rebuilt archive does not read back as written".into(),
        ));
    }
    Ok(report)
}
