use crate::Notify;
use arca_core::hash::{Algorithm, Hasher};
use arca_core::{Container, Entry, Error, Format, Result};
use rayon::prelude::*;
use std::cell::RefCell;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const BUF: usize = 256 * 1024;

// Bytes between two looks at "carry on?", so a stop lands inside a large file.
const STEP: u64 = 4 << 20;

pub struct Digest {
    pub index: usize,
    pub name: String,
    pub sums: Result<Vec<String>>,
}

struct Paced<'a> {
    hasher: &'a mut Hasher,
    notify: Notify<'a>,
    at: (usize, usize, &'a str),
    since: u64,
    stopped: bool,
}

impl Write for Paced<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.since >= STEP {
            self.since = 0;
            let (done, total, name) = self.at;
            if !(self.notify)(done, total, name) {
                self.stopped = true;
                return Err(std::io::Error::other("cancelled"));
            }
        }
        self.hasher.update(data);
        self.since += data.len() as u64;
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn hash_with(
    algorithms: &[Algorithm],
    notify: Notify<'_>,
    at: (usize, usize, &str),
    fill: impl FnOnce(&mut dyn Write) -> Result<u64>,
) -> Result<Vec<String>> {
    let mut hasher = Hasher::new(algorithms);
    let mut out = Paced {
        hasher: &mut hasher,
        notify,
        at,
        since: 0,
        stopped: false,
    };
    let result = fill(&mut out);
    if out.stopped {
        return Err(Error::Cancelled);
    }
    result?;
    Ok(finish(hasher))
}

fn finish(hasher: Hasher) -> Vec<String> {
    hasher.finish().into_iter().map(|(_, hex)| hex).collect()
}

/// A failure that belongs to one entry goes with it; a stop ends the pass.
fn digest(index: usize, name: &str, sums: Result<Vec<String>>) -> Result<Digest> {
    match sums {
        Err(Error::Cancelled) => Err(Error::Cancelled),
        sums => Ok(Digest {
            index,
            name: name.to_string(),
            sums,
        }),
    }
}

pub fn digest_file(
    path: &Path,
    algorithms: &[Algorithm],
    notify: Notify<'_>,
    at: (usize, usize, &str),
) -> Result<Vec<String>> {
    hash_with(algorithms, notify, at, |out| {
        let mut file = BufReader::with_capacity(BUF, File::open(path)?);
        Ok(std::io::copy(&mut file, out)?)
    })
}

pub fn tar_source(archive: &Path, format: Format) -> Result<Box<dyn Read>> {
    if arca_stream::handles(format) {
        return Ok(arca_stream::open(archive, format)?);
    }
    let f = BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
    Ok(match format {
        Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(f)),
        _ => Box::new(f),
    })
}

struct Slot {
    sums: Rc<RefCell<Vec<Option<Hasher>>>>,
    at: usize,
}

impl Write for Slot {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if let Some(hasher) = self
            .sums
            .borrow_mut()
            .get_mut(self.at)
            .and_then(Option::as_mut)
        {
            hasher.update(data);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// A ZIP entry that cannot be read is a digest with its error; in the other
// formats one damaged entry fails the whole pass, because each entry is
// decoded from the stream before it.
pub fn digest_entries(
    archive: &Path,
    format: Format,
    entries: &[Entry],
    wanted: &[bool],
    password: Option<&str>,
    algorithms: &[Algorithm],
    notify: Notify<'_>,
) -> Result<Vec<Digest>> {
    let picked: Vec<usize> = (0..entries.len())
        .filter(|&i| wanted.get(i).copied().unwrap_or(false) && !entries[i].is_dir)
        .collect();
    let total = picked.len();
    let mut rows = Vec::with_capacity(total);
    match format.container() {
        Container::Zip => {
            let done = AtomicUsize::new(0);
            rows = picked
                .par_iter()
                .map(|&i| {
                    let entry = &entries[i];
                    let at = (done.load(Ordering::Relaxed), total, entry.name.as_str());
                    if !notify(at.0, total, &entry.name) {
                        return Err(Error::Cancelled);
                    }
                    let sums = hash_with(algorithms, notify, at, |out| {
                        let mut source =
                            BufReader::with_capacity(BUF, arca_core::volumes::open(archive)?);
                        arca_zip::extract_entry_with(&mut source, entry, out, password)
                    });
                    let row = digest(i, &entry.name, sums)?;
                    done.fetch_add(1, Ordering::Relaxed);
                    Ok(row)
                })
                .collect::<Result<Vec<_>>>()?;
        }
        Container::SevenZ => {
            let mut a = arca_7z::SevenZArchive::open(arca_core::volumes::open(archive)?, password)?;
            a.extract(
                &picked,
                &mut |i, entry, reader| {
                    let at = (rows.len(), total, entry.name.as_str());
                    let sums = hash_with(algorithms, notify, at, |out| {
                        Ok(std::io::copy(reader, out)?)
                    })?;
                    rows.push(digest(i, &entry.name, Ok(sums))?);
                    Ok(())
                },
                &mut |p| notify(p.entries_done, p.entries_total, p.name),
            )?;
        }
        Container::Rar => {
            let a = arca_rar::RarArchive::open_with_progress(archive, password, notify)?;
            let sums: Rc<RefCell<Vec<Option<Hasher>>>> = Rc::new(RefCell::new(
                (0..a.entries().len())
                    .map(|i| {
                        let pick = wanted.get(i).copied().unwrap_or(false)
                            && entries.get(i).is_some_and(|e| !e.is_dir);
                        pick.then(|| Hasher::new(algorithms))
                    })
                    .collect(),
            ));
            a.read_entries_to(password, notify, &mut |i| {
                sums.borrow().get(i)?.as_ref()?;
                let slot = Slot {
                    sums: Rc::clone(&sums),
                    at: i,
                };
                Some(Box::new(slot) as Box<dyn Write>)
            })?;
            let mut sums = sums.borrow_mut();
            for (i, entry) in a.entries().iter().enumerate() {
                if let Some(hasher) = sums.get_mut(i).and_then(Option::take) {
                    rows.push(digest(i, &entry.name, Ok(finish(hasher)))?);
                }
            }
        }
        Container::Iso => {
            let a = arca_iso::IsoArchive::open_with_progress(archive, notify)?;
            for (n, &i) in picked.iter().enumerate() {
                let Some(entry) = a.entries().get(i) else {
                    continue;
                };
                if !notify(n, total, &entry.name) {
                    return Err(Error::Cancelled);
                }
                let mut hasher = Hasher::new(algorithms);
                let sums = a
                    .read_entry_to(i, &mut hasher, &|_, _, name| notify(n, total, name))
                    .map(|_| finish(hasher));
                rows.push(digest(i, &entry.name, sums)?);
            }
        }
        Container::Xz | Container::Bz2 | Container::Zst => {
            if picked.contains(&0) {
                let name = arca_stream::output_name(archive);
                let sums = hash_with(algorithms, notify, (0, 1, &name), |out| {
                    Ok(std::io::copy(
                        &mut arca_stream::open(archive, format)?,
                        out,
                    )?)
                });
                rows.push(digest(0, &name, sums)?);
            }
        }
        Container::Tar
        | Container::TarGz
        | Container::TarXz
        | Container::TarBz2
        | Container::TarZst => {
            let mut r = arca_tar::TarReader::new(tar_source(archive, format)?);
            let mut i = 0usize;
            while let Some(e) = r.next_entry()? {
                let name = e.entry.name.clone();
                if !notify(rows.len(), total, &name) {
                    return Err(Error::Cancelled);
                }
                if wanted.get(i).copied().unwrap_or(false) && !e.entry.is_dir && e.is_plain() {
                    let at = (rows.len(), total, name.as_str());
                    let sums = hash_with(algorithms, notify, at, |out| {
                        r.with_data(&e, |data| Ok(std::io::copy(data, out)?))
                    })?;
                    rows.push(digest(i, &name, Ok(sums))?);
                } else {
                    r.skip_data(&e)?;
                }
                i += 1;
            }
            arca_stream::drain(&mut r.into_inner())?;
        }
    }
    let _ = notify(total, total, "");
    Ok(rows)
}
