use super::{detect, detect_archive, printable, tar_source, BUF};
use arca_core::volumes::{refuse_existing, staged_new};
use arca_core::{Container, Error, Format, Level, Result};
use arca_tar::TarWriter;
use arca_zip::ZipArchive;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

// The repaired archive is written beside --out and only takes that name once
// it reads back, and only if nothing has it, least of all the damaged
// archive: that file may be the only copy of whatever could not be recovered.
pub(crate) fn run(archive: &Path, out: &Path, password: Option<&str>) -> Result<()> {
    refuse_existing(archive, out)?;
    // Looking inside a .tar.xz decodes its first header, which is what may be
    // damaged, so a name that says what the archive is wins.
    let format = match Format::detect(archive) {
        Some(named) if repairable(named) => named,
        _ => detect_archive(archive)?,
    };
    let writes = if format.container() == Container::Zip {
        Format::Zip
    } else {
        format
    };
    if detect(out)? != writes {
        return Err(Error::Unsupported(format!(
            "a repair writes the format it reads, so --out needs a .{} name",
            writes.extension()
        )));
    }
    match format.container() {
        Container::Zip => repair_zip(archive, out),
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => repair_tar(archive, out, format),
        Container::Rar => repair_rar(archive, out, password),
        Container::SevenZ => repair_7z(archive, out, password),
        _ => Err(Error::Unsupported(format!(
            "{} archives cannot be repaired",
            format.label()
        ))),
    }
}

fn repairable(format: Format) -> bool {
    matches!(
        format.container(),
        Container::Zip
            | Container::Tar
            | Container::TarGz
            | Container::TarXz
            | Container::TarBz2
            | Container::TarZst
            | Container::Rar
            | Container::SevenZ
    )
}

fn repair_zip(archive: &Path, out: &Path) -> Result<()> {
    let t0 = Instant::now();
    let report = staged_new(archive, out, |temp| {
        let source = BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
        let sink = BufWriter::with_capacity(BUF, File::create(temp)?);
        let (mut sink, report) = arca_zip::repair(source, sink)?;
        sink.flush()?;
        drop(sink);
        if ZipArchive::open(File::open(temp)?)?.len() != report.recovered.len() {
            return Err(Error::Format(
                "the rebuilt archive does not read back as written".into(),
            ));
        }
        for s in &report.skipped {
            eprintln!(
                "  skipped at offset {}: {}: {}",
                s.offset,
                printable(s.name.as_deref().unwrap_or("?")),
                printable(&s.reason)
            );
        }
        if report.unreported > 0 {
            eprintln!("  and {} more skipped", report.unreported);
        }
        for r in report.recovered.iter().filter(|r| !r.verified) {
            eprintln!(
                "  encrypted, copied without checking its contents: {}",
                printable(&r.name)
            );
        }
        if report.recovered.is_empty() {
            return Err(Error::Format(format!(
                "no entry of '{}' could be recovered",
                archive.display()
            )));
        }
        Ok(report)
    })?;
    println!(
        "{}: {} entries recovered, {} skipped in {:.3} s",
        out.display(),
        report.recovered.len(),
        report.skipped.len() as u64 + report.unreported,
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

fn spool_path(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".arca-spool");
    out.with_file_name(name)
}

enum Packed<W: Write> {
    Plain(W),
    Gz(flate2::write::GzEncoder<W>),
    Stream(arca_stream::Encoder<W>),
}

impl<W: Write> Packed<W> {
    fn new(out: W, format: Format) -> Result<Self> {
        Ok(match format {
            Format::Tar => Packed::Plain(out),
            Format::TarGz => Packed::Gz(flate2::write::GzEncoder::new(
                out,
                flate2::Compression::new(Level::NORMAL.to_flate2()),
            )),
            _ => Packed::Stream(arca_stream::encoder(out, format, Level::NORMAL, 0, None)?),
        })
    }

    fn finish(self) -> Result<W> {
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

fn read_back(temp: &Path, format: Format, kept: &[String]) -> Result<()> {
    let mut back = arca_tar::TarReader::new(tar_source(temp, format)?);
    let mut i = 0usize;
    while let Some(e) = back.next_entry()? {
        if kept.get(i) != Some(&e.entry.name) {
            return Err(Error::Format(
                "the rebuilt archive does not read back as written".into(),
            ));
        }
        back.skip_data(&e)?;
        i += 1;
    }
    arca_stream::drain(&mut back.into_inner())?;
    if i != kept.len() {
        return Err(Error::Format(
            "the rebuilt archive does not read back as written".into(),
        ));
    }
    Ok(())
}

fn repair_tar(archive: &Path, out: &Path, format: Format) -> Result<()> {
    let t0 = Instant::now();
    let report = staged_new(archive, out, |temp| {
        let spool = spool_path(temp);
        let result = (|| -> Result<arca_tar::Salvaged> {
            let source = salvage_source(archive, format)?;
            let mut held = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&spool)?;
            let file = BufWriter::with_capacity(BUF, File::create(temp)?);
            let mut w = TarWriter::new(Packed::new(file, format)?);
            let report = arca_tar::salvage(source, &mut w, &mut held, &mut |_, _| true)?;
            drop(held);
            let file = w.finish()?.finish()?;
            file.into_inner()
                .map_err(|e| Error::Io(e.into_error()))?
                .sync_all()?;
            if !report.kept.is_empty() {
                read_back(temp, format, &report.kept)?;
            }
            Ok(report)
        })();
        let _ = fs::remove_file(&spool);
        let report = result?;
        for l in &report.lost {
            eprintln!(
                "  lost at offset {}: {}: {}",
                l.offset,
                printable(l.name.as_deref().unwrap_or("?")),
                printable(&l.reason)
            );
        }
        if report.unreported > 0 {
            eprintln!("  and {} more lost", report.unreported);
        }
        if let Some(why) = &report.stream {
            if format == Format::Tar {
                eprintln!(
                    "  the archive could not be read past here: {}",
                    printable(why)
                );
            } else {
                eprintln!(
                    "  the {} stream is damaged ({}): nothing after the damage decodes, and since \
                     its checks come after the data they cover, the last entries kept before it \
                     are unchecked",
                    format.label(),
                    printable(why)
                );
            }
        }
        if report.kept.is_empty() {
            return Err(Error::Format(format!(
                "no entry of '{}' could be recovered",
                archive.display()
            )));
        }
        Ok(report)
    })?;
    println!(
        "{}: {} entries recovered, {} lost in {:.3} s",
        out.display(),
        report.kept.len(),
        report.lost.len() as u64 + report.unreported,
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

fn repair_rar(archive: &Path, out: &Path, password: Option<&str>) -> Result<()> {
    let t0 = Instant::now();
    let report = arca_rar::repair(archive, out, password, &|_, _, _| true)?;
    if !report.changed {
        println!(
            "{}: the recovery record finds nothing to repair; nothing was written",
            archive.display()
        );
        return Ok(());
    }
    println!(
        "{}: repaired from the recovery record{} in {:.3} s",
        out.display(),
        if report.data_repaired {
            ", damaged data rebuilt"
        } else {
            ""
        },
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

fn repair_7z(archive: &Path, out: &Path, password: Option<&str>) -> Result<()> {
    let t0 = Instant::now();
    let report = staged_new(archive, out, |temp| {
        let report = arca_7z::salvage_7z(archive, temp, password, Level::NORMAL, &mut |_| true)?;
        let rebuilt = arca_7z::SevenZArchive::open(BufReader::new(File::open(temp)?), password)?;
        let any_file = rebuilt.entries().iter().any(|e| !e.is_dir);
        drop(rebuilt);
        for (name, why) in &report.lost {
            eprintln!("  lost: {}: {}", printable(name), printable(why));
        }
        if !any_file {
            return Err(Error::Format(format!(
                "no file of '{}' could be recovered",
                archive.display()
            )));
        }
        Ok(report)
    })?;
    println!(
        "{}: {} entries recovered, {} lost in {:.3} s",
        out.display(),
        report.kept,
        report.lost.len(),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}
