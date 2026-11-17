//! Salvaging what can still be read from a damaged TAR.

use crate::{is_header, limits, without_current, TarReader, TarWriter, BLOCK};
use arca_core::{Error, Result};
use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom, Write};

const MAX_REPORTED: usize = 10_000;

/// A stretch of the archive that was given up on.
pub struct Lost {
    /// Where it starts in the TAR, after any decompression.
    pub offset: u64,
    /// The entry it belonged to, when that much could be read.
    pub name: Option<String>,
    pub reason: String,
}

#[derive(Default)]
pub struct Salvaged {
    /// The entries written, in order, as a reader will list them.
    pub kept: Vec<String>,
    pub lost: Vec<Lost>,
    pub unreported: u64,
    /// Why the source stopped giving bytes before the archive ended, which
    /// for a compressed TAR is damage in the compressed stream. Nothing past
    /// it can be read.
    pub stream: Option<String>,
}

impl Salvaged {
    fn lose(&mut self, offset: u64, name: Option<String>, reason: String) {
        if self.lost.len() < MAX_REPORTED {
            self.lost.push(Lost {
                offset,
                name,
                reason,
            });
        } else {
            self.unreported += 1;
        }
    }
}

/// Copies every entry of `source` whose headers check out and whose data is
/// all there into `to`, headers as they were. TAR keeps no checksum over the
/// data, so that is all that can be verified. After a header that does not
/// check out, the archive is searched block by block for the next one that
/// does, and reading carries on from there; a run of zeros in the middle is
/// stepped over the same way.
///
/// Each entry's data goes through `spool` before any of it reaches `to`, so
/// an entry cut short leaves nothing behind. A hard link whose target was
/// lost is left out too: it would point at nothing.
///
/// `progress` hears each entry's name with 0 and then the bytes copied every
/// few megabytes; false stops with [`Error::Cancelled`]. Failing to write
/// `to` or `spool` is an error; anything wrong with `source` is reported.
pub fn salvage<R: Read, W: Write, S: Read + Write + Seek>(
    source: R,
    to: &mut TarWriter<W>,
    spool: &mut S,
    progress: &mut dyn FnMut(&str, u64) -> bool,
) -> Result<Salvaged> {
    let mut src = Pushback::new(source);
    let mut report = Salvaged::default();
    let mut names: HashSet<String> = HashSet::new();
    let mut seen = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let base = src.at();
        let mut reader = TarReader::new(&mut src);
        reader.keep_headers();
        let trouble = loop {
            let e = match reader.next_entry() {
                Ok(Some(e)) => e,
                Ok(None) => break None,
                Err(error) => break Some((None, error)),
            };
            seen += 1;
            if seen > limits::MAX_ENTRIES {
                return Err(Error::Limit(format!(
                    "more than {} TAR entries",
                    limits::MAX_ENTRIES
                )));
            }
            let name = e.entry.name.clone();
            if !progress(&name, 0) {
                return Err(Error::Cancelled);
            }
            if let Some(target) = e.hard_link().map(without_current) {
                if !names.contains(target) {
                    let target = target.to_string();
                    if let Err(error) = reader.skip_data(&e) {
                        break Some((Some(name), error));
                    }
                    report.lose(
                        base + e.entry.offset,
                        Some(name),
                        format!("a hard link to '{target}', which was not recovered"),
                    );
                    continue;
                }
            }
            spool.seek(SeekFrom::Start(0))?;
            let mut stuck: Option<io::Error> = None;
            let copied = reader.with_data(&e, |data| {
                let mut n = 0u64;
                loop {
                    let got = match data.read(&mut buf) {
                        Ok(0) => return Ok(n),
                        Ok(got) => got,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(Error::Io(error)),
                    };
                    if let Err(error) = spool.write_all(&buf[..got]) {
                        stuck = Some(error);
                        return Err(Error::Cancelled);
                    }
                    n += got as u64;
                }
            });
            if let Some(error) = stuck {
                return Err(Error::Io(error));
            }
            let n = match copied {
                Ok(n) => n,
                Err(error) => break Some((Some(name), error)),
            };
            spool.seek(SeekFrom::Start(0))?;
            to.copy_header(&e, &e.entry.name, None)?;
            to.copy_data(&mut (&mut *spool).take(n), n, &name, progress)?;
            names.insert(without_current(&name).to_string());
            report.kept.push(without_current(&name).to_string());
        };
        drop(reader);
        if let Some((name, error)) = trouble {
            if matches!(error, Error::Cancelled) {
                return Err(error);
            }
            if let Some(why) = src.broken.take() {
                report.stream = Some(why);
                report.lose(src.at(), name, error.to_string());
                return Ok(report);
            }
            if src.ended {
                report.lose(src.at(), name, error.to_string());
                return Ok(report);
            }
            report.lose(src.at(), name, error.to_string());
        }
        if !resync(&mut src)? {
            if let Some(why) = src.broken.take() {
                report.stream = Some(why);
            }
            return Ok(report);
        }
    }
}

/// Moves to the next block that is a header with a valid checksum, and puts
/// it back to be read. False when the source ends first.
fn resync<R: Read>(src: &mut Pushback<R>) -> Result<bool> {
    let misaligned = (src.at() % BLOCK as u64) as usize;
    if misaligned != 0 {
        let mut skip = vec![0u8; BLOCK - misaligned];
        if src.fill(&mut skip) < skip.len() {
            return Ok(false);
        }
    }
    let mut block = [0u8; BLOCK];
    loop {
        if src.fill(&mut block) < BLOCK {
            return Ok(false);
        }
        if is_header(&block) {
            src.put_back(&block);
            return Ok(true);
        }
    }
}

/// A source that can be handed a block back, and that remembers why it
/// stopped: `ended` for a clean end, `broken` for a read that failed.
struct Pushback<R> {
    inner: R,
    front: Vec<u8>,
    taken: usize,
    read: u64,
    ended: bool,
    broken: Option<String>,
}

impl<R: Read> Pushback<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            front: Vec::new(),
            taken: 0,
            read: 0,
            ended: false,
            broken: None,
        }
    }

    fn at(&self) -> u64 {
        self.read - (self.front.len() - self.taken) as u64
    }

    fn put_back(&mut self, block: &[u8]) {
        let mut front = block.to_vec();
        front.extend_from_slice(&self.front[self.taken..]);
        self.front = front;
        self.taken = 0;
    }

    fn fill(&mut self, buf: &mut [u8]) -> usize {
        let mut filled = 0;
        while filled < buf.len() {
            match self.read(&mut buf[filled..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        filled
    }
}

impl<R: Read> Read for Pushback<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.taken < self.front.len() {
            let n = buf.len().min(self.front.len() - self.taken);
            buf[..n].copy_from_slice(&self.front[self.taken..self.taken + n]);
            self.taken += n;
            if self.taken == self.front.len() {
                self.front.clear();
                self.taken = 0;
            }
            return Ok(n);
        }
        if self.ended || self.broken.is_some() {
            return Ok(0);
        }
        loop {
            match self.inner.read(buf) {
                Ok(0) if !buf.is_empty() => {
                    self.ended = true;
                    return Ok(0);
                }
                Ok(n) => {
                    self.read += n as u64;
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.broken = Some(e.to_string());
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
