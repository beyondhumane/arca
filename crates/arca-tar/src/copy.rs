//! Copying the entries of one tar into another, with their headers as they
//! were, so that taking one entry out or renaming it leaves the rest alone.

use crate::{
    cut_name, each_pax, limits, number, pax_record, seal, split_name, unsigned, ustar_header,
    without_current, TarEntry, TarReader, TarWriter, BLOCK,
};
use arca_core::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};

/// Bytes of one entry between two calls to `progress`.
const STEP: u64 = 4 << 20;

/// Copies every entry of `from` into `to` under the name `name` gives it, or
/// leaves it out when that is `None`. An entry keeps its headers as they were
/// when its name stays: owner, group, permissions, times, link target, device
/// numbers and any pax record. A renamed one keeps all of that but its name.
/// The `./` entry a reader steps over is copied as it was, and a global pax
/// header in front of an entry left out moves to the next entry written.
///
/// A hard link names an earlier entry, so it follows that entry when it is
/// renamed, and the copy is refused when that entry is left out: the link
/// would be left pointing at nothing. Names and targets are compared once
/// `.`, `..` and repeated slashes are resolved.
///
/// `progress` hears the name of each entry before it is copied, with 0, and
/// then every few megabytes of its data the bytes copied so far. False stops
/// the copy with [`Error::Cancelled`]. Gives back the names written, as a
/// reader will list them.
pub fn copy_entries<R: Read, W: Write>(
    from: &mut TarReader<R>,
    to: &mut TarWriter<W>,
    name: &dyn Fn(&TarEntry) -> Option<String>,
    progress: &mut dyn FnMut(&str, u64) -> bool,
) -> Result<Vec<String>> {
    from.keep_headers();
    let mut gone: HashSet<String> = HashSet::new();
    let mut moved: HashMap<String, String> = HashMap::new();
    let mut written = Vec::new();
    let mut carried = Vec::new();
    while let Some(e) = from.next_entry()? {
        carried.extend(from.take_prelude());
        if !progress(&e.entry.name, 0) {
            return Err(Error::Cancelled);
        }
        let Some(new) = name(&e) else {
            from.skip_data(&e)?;
            gone.insert(normal(&e.entry.name));
            carried.extend(globals(&e));
            continue;
        };
        let link = match e.hard_link().map(normal) {
            Some(target) if gone.contains(&target) => {
                return Err(Error::Unsupported(format!(
                    "'{}' is a hard link to '{target}', which is being left out",
                    e.entry.name
                )));
            }
            Some(target) => moved.get(&target).cloned(),
            None => None,
        };
        if new != e.entry.name {
            moved.insert(normal(&e.entry.name), new.clone());
        }
        to.out.write_all(&std::mem::take(&mut carried))?;
        to.copy_header(&e, &new, link.as_deref())?;
        from.with_data(&e, |data| to.copy_data(data, e.data, &new, progress))?;
        written.push(without_current(&new).to_string());
    }
    carried.extend(from.take_prelude());
    to.out.write_all(&carried)?;
    Ok(written)
}

/// A path with `.` and empty parts dropped and each `..` taking the part
/// before it away, as the entry it names would be found on disk.
fn normal(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// The global pax headers among the headers of `e`, as they were.
fn globals(e: &TarEntry) -> Vec<u8> {
    let Some(raw) = &e.headers else {
        return Vec::new();
    };
    segments(raw)
        .map(|all| {
            all.iter()
                .filter(|s| s.header[156] == b'g')
                .flat_map(|s| s.whole.iter().copied())
                .collect()
        })
        .unwrap_or_default()
}

impl<W: Write> TarWriter<W> {
    /// The headers of `e`, read by a reader keeping them: as they were when
    /// `name` is the entry's own and `link` is `None`, and otherwise with the
    /// new name and, for a hard link, the new target.
    pub(crate) fn copy_header(
        &mut self,
        e: &TarEntry,
        name: &str,
        link: Option<&str>,
    ) -> Result<()> {
        match &e.headers {
            Some(raw) if name == e.entry.name && link.is_none() => {
                self.out.write_all(raw)?;
                Ok(())
            }
            Some(raw) => {
                let headers = renamed(raw, name, link)?;
                self.out.write_all(&headers)?;
                Ok(())
            }
            None => {
                let target = link.or(e.link.as_deref()).unwrap_or("");
                let mtime = e.entry.mtime.unwrap_or(0);
                self.put(name, e.data, mtime, e.mode, e.kind, target)
            }
        }
    }

    /// Exactly `size` bytes of data from `data`, then the padding.
    pub(crate) fn copy_data(
        &mut self,
        data: &mut dyn Read,
        size: u64,
        name: &str,
        progress: &mut dyn FnMut(&str, u64) -> bool,
    ) -> Result<()> {
        let mut buf = vec![0u8; 64 * 1024];
        let (mut done, mut since) = (0u64, 0u64);
        while done < size {
            let want = buf
                .len()
                .min(usize::try_from(size - done).unwrap_or(usize::MAX));
            let n = match data.read(&mut buf[..want]) {
                Ok(0) => return Err(Error::Io(crate::truncated())),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::Io(e)),
            };
            self.out.write_all(&buf[..n])?;
            done += n as u64;
            since += n as u64;
            if since >= STEP {
                since = 0;
                if !progress(name, done) {
                    return Err(Error::Cancelled);
                }
            }
        }
        self.pad(size)
    }
}

/// One header block, and the data of an extension header after it.
struct Segment<'a> {
    header: &'a [u8; BLOCK],
    payload: &'a [u8],
    whole: &'a [u8],
}

fn unreadable() -> Error {
    Error::Format("the kept TAR headers do not read back".into())
}

/// The headers as the reader kept them: extension headers, each with its
/// data, and the entry's own header last.
fn segments(raw: &[u8]) -> Result<Vec<Segment<'_>>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < raw.len() {
        let header: &[u8; BLOCK] = raw
            .get(at..at.saturating_add(BLOCK))
            .and_then(|b| b.try_into().ok())
            .ok_or_else(unreadable)?;
        let extension = matches!(header[156], b'L' | b'K' | b'x' | b'g');
        let size = if extension {
            usize::try_from(unsigned(&header[124..136], "size")?).map_err(|_| unreadable())?
        } else {
            0
        };
        let end = size
            .checked_next_multiple_of(BLOCK)
            .and_then(|padded| at.checked_add(BLOCK)?.checked_add(padded))
            .ok_or_else(unreadable)?;
        let whole = raw.get(at..end).ok_or_else(unreadable)?;
        out.push(Segment {
            header,
            payload: &whole[BLOCK..BLOCK + size],
            whole,
        });
        at = end;
        if !extension && at != raw.len() {
            return Err(unreadable());
        }
    }
    match out.last() {
        Some(last) if !matches!(last.header[156], b'L' | b'K' | b'x' | b'g') => Ok(out),
        _ => Err(unreadable()),
    }
}

/// The headers in `raw` with the entry named `name` and, when given, linked
/// to `link`. Everything else is kept: a global pax header as it was, and the
/// records of the entry's own pax header but the ones for the name and link,
/// which are written again only when the ustar fields cannot hold them.
fn renamed(raw: &[u8], name: &str, link: Option<&str>) -> Result<Vec<u8>> {
    if name.len() > limits::MAX_NAME {
        return Err(Error::Limit(format!("name of {} bytes", name.len())));
    }
    if link.is_some_and(|l| l.is_empty() || l.contains('\0') || l.len() > limits::MAX_NAME) {
        return Err(Error::Format(format!("'{name}' has no usable link target")));
    }
    let segments = segments(raw)?;
    let Some((main, extensions)) = segments.split_last() else {
        return Err(unreadable());
    };
    let mut out = Vec::with_capacity(raw.len() + BLOCK);
    let mut records = Vec::new();
    for s in extensions {
        match s.header[156] {
            b'L' => {}
            b'K' if link.is_some() => {}
            b'x' => each_pax(s.payload, |key, _, record| {
                if key != b"path" && !(link.is_some() && key == b"linkpath") {
                    records.extend_from_slice(record);
                }
                Ok(())
            })?,
            _ => out.extend_from_slice(s.whole),
        }
    }
    let mut header = *main.header;
    // Only POSIX ustar has a prefix field there; GNU tar keeps times and
    // sparse maps in the same bytes, which must not be overwritten.
    let posix = &header[257..263] == b"ustar\0";
    header[..100].fill(0);
    if posix {
        header[345..500].fill(0);
    }
    let fits = if posix {
        split_name(name)
    } else {
        (name.len() <= 100).then_some(("", name))
    };
    let base = match fits {
        Some((prefix, base)) => {
            header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
            base
        }
        None => {
            records.extend(pax_record("path", name));
            cut_name(name, 100)
        }
    };
    header[..base.len()].copy_from_slice(base.as_bytes());
    if let Some(link) = link {
        header[157..257].fill(0);
        if link.len() > 100 {
            records.extend(pax_record("linkpath", link));
        }
        let cut = cut_name(link, 100);
        header[157..157 + cut.len()].copy_from_slice(cut.as_bytes());
    }
    seal(&mut header);
    if !records.is_empty() {
        let mtime = number(&header[136..148], "mtime").unwrap_or(0);
        out.extend_from_slice(&ustar_header(
            &format!("PaxHeaders/{}", cut_name(base, 89)),
            "",
            "",
            records.len() as u64,
            mtime,
            0o644,
            b'x',
        ));
        let padded = records.len().next_multiple_of(BLOCK);
        records.resize(padded, 0);
        out.extend_from_slice(&records);
    }
    out.extend_from_slice(&header);
    Ok(out)
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
