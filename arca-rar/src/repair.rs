use super::{Progress, RarArchive, Repaired};
use crate::reader::{self, controlled};
use arca_core::{Error, Result};
use rars::{Archive, ArchiveReader};
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

const MAX_IN_MEMORY: u64 = 1 << 30;

pub(super) fn repair(
    input: &Path,
    output: &Path,
    password: Option<&str>,
    progress: &Progress<'_>,
) -> Result<Repaired> {
    if output.exists() && fs::canonicalize(input)? == fs::canonicalize(output)? {
        return Err(Error::Format(
            "the repaired archive cannot replace the damaged one".into(),
        ));
    }
    let size = fs::metadata(input)?.len();
    let parent = match output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let staged = tempfile::Builder::new()
        .prefix(".arca-")
        .suffix(".rar.part")
        .tempfile_in(parent)?;
    let report = controlled(progress, |token| {
        let archive = ArchiveReader::read_reader_with_options(
            File::open(input)?,
            reader::options(password, token),
        )
        .map_err(|e| reader::at_path(reader::error(e), input))?;
        if is_volume(&archive) {
            return Err(Error::Unsupported(
                "a RAR volume set cannot be repaired one volume at a time".into(),
            ));
        }
        let mut sink = Paced {
            inner: BufWriter::new(staged.as_file().try_clone()?),
            progress,
            written: 0,
            total: size,
            stopped: false,
        };
        let report = match &archive {
            Archive::Rar50Plus(a) => a
                .repair_recovery_to_with_report(&mut sink, password.map(str::as_bytes))
                .map_err(not_repairable),
            _ if size > MAX_IN_MEMORY => Err(Error::Limit(
                "a RAR 4 archive over 1 GiB would be repaired in memory, which is refused".into(),
            )),
            _ => archive
                .repair_recovery_with_options(reader::options(password, token))
                .map_err(not_repairable)
                .and_then(|repaired| {
                    sink.write_all(&repaired.data)?;
                    Ok(repaired.report)
                }),
        };
        if sink.stopped {
            return Err(Error::Cancelled);
        }
        let report = report?;
        sink.inner.flush()?;
        sink.inner.get_ref().sync_all()?;
        Ok(report)
    })?;
    let done = Repaired {
        changed: report.changed,
        data_repaired: report.data_repaired,
    };
    if !report.changed {
        return Ok(done);
    }
    let check = RarArchive::open_with_progress(staged.path(), password, progress)?;
    if password.is_some() || !check.entries().iter().any(|e| e.encrypted) {
        check.test(password, progress)?;
    }
    drop(check);
    staged.persist(output).map_err(|e| Error::Io(e.error))?;
    Ok(done)
}

fn is_volume(archive: &Archive) -> bool {
    match archive {
        Archive::Rar13(a) => a.main.is_volume(),
        Archive::Rar15To40(a) => a.main.is_volume(),
        Archive::Rar50Plus(a) => a.main.is_volume(),
        _ => true,
    }
}

fn not_repairable(error: rars::Error) -> Error {
    match reader::error(error) {
        Error::Format(message) => {
            Error::Format(format!("cannot repair from the recovery record: {message}"))
        }
        other => other,
    }
}

struct Paced<'a, W: Write> {
    inner: W,
    progress: &'a Progress<'a>,
    written: u64,
    total: u64,
    stopped: bool,
}

impl<W: Write> Write for Paced<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let before = self.written >> 20;
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        if self.written >> 20 != before
            && !(self.progress)(
                (self.written >> 20) as usize,
                ((self.total >> 20) as usize).max(1),
                "",
            )
        {
            self.stopped = true;
            return Err(io::Error::other("cancelled"));
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
