#![forbid(unsafe_code)]

mod copy;
mod repair;

pub use copy::copy_entries;
pub use repair::{salvage, Lost, Salvaged};

use arca_core::{limits, Entry, Error, Method, Result};
use std::io::{self, Read, Write};

const BLOCK: usize = 512;

fn octal(field: &[u8], what: &str) -> Result<u64> {
    let s: Vec<u8> = field
        .iter()
        .copied()
        .take_while(|&b| b != 0)
        .filter(|&b| b != b' ')
        .collect();
    if s.is_empty() {
        return Ok(0);
    }
    let mut v: u64 = 0;
    for b in s {
        if !(b'0'..=b'7').contains(&b) {
            return Err(Error::Format(format!("{what}: invalid octal")));
        }
        v = v
            .checked_mul(8)
            .and_then(|x| x.checked_add((b - b'0') as u64))
            .ok_or_else(|| Error::Limit(format!("{what}: overflow")))?;
    }
    Ok(v)
}

/// A numeric field, in octal or, with the top bit of its first byte set, in the
/// base-256 GNU tar uses for sizes over 8 GiB and times before 1970.
fn number(field: &[u8], what: &str) -> Result<i64> {
    let overflow = || Error::Limit(format!("{what}: overflow"));
    match field.first() {
        Some(&first) if first & 0x80 != 0 => {
            // Big-endian, in two's complement over the whole field when the
            // second bit says negative, with the marker bit aside otherwise.
            if field.len() > 15 {
                return Err(overflow());
            }
            let negative = first & 0x40 != 0;
            let mut v: i128 = 0;
            for (i, &byte) in field.iter().enumerate() {
                let byte = if i == 0 && !negative {
                    byte & 0x7f
                } else {
                    byte
                };
                v = v * 256 + i128::from(byte);
            }
            if negative {
                v -= 1i128 << (8 * field.len());
            }
            i64::try_from(v).map_err(|_| overflow())
        }
        _ => i64::try_from(octal(field, what)?).map_err(|_| overflow()),
    }
}

fn unsigned(field: &[u8], what: &str) -> Result<u64> {
    u64::try_from(number(field, what)?).map_err(|_| Error::Format(format!("{what}: negative")))
}

/// The most a pax header may hold. Real ones are a few hundred bytes; this is
/// room for a long path and every attribute tar knows, and no more.
const PAX_LIMIT: u64 = 64 * 1024;

/// What the extension headers in front of an entry say about it.
#[derive(Default)]
struct Longer {
    name: Option<String>,
    link: Option<String>,
    size: Option<u64>,
    mtime: Option<i64>,
    comment: Option<String>,
}

impl Longer {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.link.is_none()
            && self.size.is_none()
            && self.mtime.is_none()
            && self.comment.is_none()
    }

    fn apply_pax(&mut self, data: &[u8]) -> Result<()> {
        each_pax(data, |key, value, _| {
            let value = std::str::from_utf8(value).map_err(|_| bad_pax())?;
            match key {
                b"path" => self.name = Some(value.to_string()),
                b"linkpath" => self.link = Some(value.to_string()),
                b"comment" => self.comment = Some(value.to_string()),
                b"size" => self.size = Some(value.parse().map_err(|_| bad_pax())?),
                b"mtime" => {
                    let whole = value.split('.').next().unwrap_or(value);
                    self.mtime = Some(whole.parse().map_err(|_| bad_pax())?);
                }
                _ => {}
            }
            Ok(())
        })
    }
}

fn bad_pax() -> Error {
    Error::Format("a malformed pax record".into())
}

/// Walks pax records, each `<length> <key>=<value>\n` where the length counts
/// the whole record, itself included. `f` is given the key, the value and the
/// whole record.
fn each_pax(mut data: &[u8], mut f: impl FnMut(&[u8], &[u8], &[u8]) -> Result<()>) -> Result<()> {
    while !data.is_empty() {
        let space = data.iter().position(|&b| b == b' ').ok_or_else(bad_pax)?;
        let length: usize = std::str::from_utf8(&data[..space])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(bad_pax)?;
        if length <= space + 1 || length > data.len() || data[length - 1] != b'\n' {
            return Err(bad_pax());
        }
        let record = &data[space + 1..length - 1];
        let equals = record.iter().position(|&b| b == b'=').ok_or_else(bad_pax)?;
        f(&record[..equals], &record[equals + 1..], &data[..length])?;
        data = &data[length..];
    }
    Ok(())
}

/// A name without the `./` in front of it, however many times it is there.
fn without_current(name: &str) -> &str {
    let mut name = name;
    while let Some(rest) = name.strip_prefix("./") {
        name = rest.trim_start_matches('/');
    }
    if name == "." {
        ""
    } else {
        name
    }
}

fn dangling() -> Error {
    Error::Format("an extension header with no entry after it".into())
}

fn write_octal(dest: &mut [u8], value: u64) {
    let width = dest.len() - 1;
    let s = format!("{value:0width$o}", width = width);
    let b = s.as_bytes();
    let n = b.len().min(width);
    dest[..n].copy_from_slice(&b[b.len() - n..]);
    dest[width] = 0;
}

fn checksum(header: &[u8; BLOCK]) -> u32 {
    let mut s: u32 = 0;
    for (i, &b) in header.iter().enumerate() {
        s += if (148..156).contains(&i) {
            32
        } else {
            b as u32
        };
    }
    s
}

fn text_field(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// The permissions to record for a file going into a tar: its own on Unix, so
/// a program stays runnable once extracted, and an ordinary file's elsewhere.
pub fn mode_of(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644
    }
}

/// The path a symbolic link going into a tar holds, read from the link
/// itself and never followed. `None` for anything that is not a link, for a
/// target that is not UTF-8, and everywhere but Unix, where a link needs
/// privileges to make and so is left out as it always was.
pub fn link_target(path: &std::path::Path, meta: &std::fs::Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        if !meta.file_type().is_symlink() {
            return None;
        }
        let target = std::fs::read_link(path)
            .ok()?
            .into_os_string()
            .into_string()
            .ok()?;
        (!target.is_empty() && target.len() <= limits::MAX_NAME).then_some(target)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, meta);
        None
    }
}

/// Whether a 512-byte block is a TAR header with a valid checksum.
pub fn is_header(block: &[u8]) -> bool {
    let Ok(header) = <&[u8; BLOCK]>::try_from(block) else {
        return false;
    };
    if header.iter().all(|&b| b == 0) {
        return false;
    }
    octal(&header[148..156], "checksum")
        .is_ok_and(|declared| declared == u64::from(checksum(header)))
}

pub struct TarEntry {
    pub entry: Entry,
    pub data: u64,
    /// The Unix permission bits, `0o755` for a program. Zero when the field
    /// cannot be read, which only ever costs the execute bit.
    pub mode: u32,
    /// The typeflag: `0` or NUL a file, `5` a folder, `1` a hard link, `2` a
    /// symbolic link, `3` to `6` devices and pipes, `V` a volume label.
    pub kind: u8,
    /// What a link points to: for a symbolic link the path it holds, for a
    /// hard link the name of an earlier entry. `None` for anything else.
    pub link: Option<String>,
    /// What a pax `comment` record says about the entry.
    pub comment: Option<String>,
    headers: Option<Vec<u8>>,
}

impl TarEntry {
    /// Whether extracting the entry means writing a file or making a folder.
    /// Links, devices, pipes and volume labels have no data of their own, and
    /// writing them as files would leave empty files where a tar puts links.
    pub fn is_plain(&self) -> bool {
        self.entry.is_dir || matches!(self.kind, b'0' | 0 | b'7')
    }

    /// The path a symbolic link holds.
    pub fn symlink(&self) -> Option<&str> {
        self.link
            .as_deref()
            .filter(|_| self.kind == b'2' && !self.entry.is_dir)
    }

    /// The earlier entry a hard link names.
    pub fn hard_link(&self) -> Option<&str> {
        self.link
            .as_deref()
            .filter(|_| self.kind == b'1' && !self.entry.is_dir)
    }
}

pub struct TarReader<R: Read> {
    source: R,
    pos: u64,
    finished: bool,
    keep: bool,
    raw: Vec<u8>,
}

/// The most header bytes kept for one entry: a whole pax header, a long name
/// and a long link, with room to spare. An entry with more than that in front
/// of it is copied from what its headers were read to say instead.
const RAW_LIMIT: usize = 4 * PAX_LIMIT as usize;

impl<R: Read> TarReader<R> {
    pub fn new(source: R) -> Self {
        TarReader {
            source,
            pos: 0,
            finished: false,
            keep: false,
            raw: Vec::new(),
        }
    }

    /// Keeps the header blocks of every entry read from here on, so that
    /// [`copy_entries`] can write them out again as they were.
    pub fn keep_headers(&mut self) {
        self.keep = true;
    }

    fn record(&mut self, bytes: &[u8]) {
        if self.keep && self.raw.len() <= RAW_LIMIT {
            self.raw.extend_from_slice(bytes);
        }
    }

    /// The source, positioned after the last block the reader consumed.
    pub fn into_inner(self) -> R {
        self.source
    }

    /// The next file or folder. The headers GNU tar and pax put in front of
    /// one to give it a longer name, a larger size or an exact time are read
    /// and applied here, never returned as entries of their own.
    pub fn next_entry(&mut self) -> Result<Option<TarEntry>> {
        if self.finished {
            return Ok(None);
        }
        let (header, longer, name) = loop {
            self.raw.clear();
            let mut longer = Longer::default();
            let header = loop {
                let mut header = [0u8; BLOCK];
                // A tar that simply stops where a header would start is read as
                // ended, as tar itself does; one that stops inside a header, or
                // whose decompressor reports it cut short, is not.
                if fill(&mut self.source, &mut header)? == 0 {
                    self.finished = true;
                    return if longer.is_empty() {
                        Ok(None)
                    } else {
                        Err(dangling())
                    };
                }
                self.pos += BLOCK as u64;
                self.record(&header);

                if header.iter().all(|&b| b == 0) {
                    self.finished = true;
                    return if longer.is_empty() {
                        Ok(None)
                    } else {
                        Err(dangling())
                    };
                }

                let declared = octal(&header[148..156], "checksum")? as u32;
                let actual = checksum(&header);
                if declared != actual {
                    return Err(Error::Format(format!(
                        "invalid header checksum: {declared} declared, {actual} computed"
                    )));
                }
                let size = unsigned(&header[124..136], "size")?;
                match header[156] {
                    b'L' => {
                        let name = self.payload(size, limits::MAX_NAME as u64 + 1)?;
                        longer.name = Some(text_field(&name));
                    }
                    b'K' => {
                        let link = self.payload(size, limits::MAX_NAME as u64 + 1)?;
                        longer.link = Some(text_field(&link));
                    }
                    b'x' => {
                        let records = self.payload(size, PAX_LIMIT)?;
                        longer.apply_pax(&records)?;
                    }
                    // The global header every `git archive` tarball starts with.
                    // Nothing in it names a file, so it is stepped over.
                    b'g' => self.payload(size, PAX_LIMIT).map(|_| ())?,
                    _ => break header,
                }
            };

            let name = match longer.name.take() {
                Some(name) => name,
                None => {
                    let base_name = text_field(&header[0..100]);
                    let prefix = if &header[257..262] == b"ustar" {
                        text_field(&header[345..500])
                    } else {
                        String::new()
                    };
                    if prefix.is_empty() {
                        base_name
                    } else {
                        format!("{prefix}/{base_name}")
                    }
                }
            };
            if name.len() > limits::MAX_NAME {
                return Err(Error::Limit("name too long".into()));
            }
            // `tar -C folder .` names everything `./…` and adds `./` for the
            // folder itself. The prefix says nothing, and the folder is wherever
            // the archive is extracted to, so it is not an entry of its own.
            let bare = without_current(&name);
            if bare.is_empty() {
                if header[156] == b'5' || name.ends_with('/') || name == "." {
                    let size = unsigned(&header[124..136], "size")?;
                    self.skip(longer.size.unwrap_or(size))?;
                    continue;
                }
                return Err(Error::Format("an entry without a name".into()));
            }
            break (header, longer, bare.to_string());
        };

        let size = match longer.size {
            Some(size) => size,
            None => unsigned(&header[124..136], "size")?,
        };
        let mtime = match longer.mtime {
            Some(mtime) => mtime,
            None => number(&header[136..148], "mtime")?,
        };
        let kind = header[156];
        if matches!(kind, b'S' | b'M') {
            return Err(Error::Unsupported(format!(
                "'{name}' is a GNU tar {} entry",
                if kind == b'S' {
                    "sparse"
                } else {
                    "multivolume"
                }
            )));
        }
        let is_dir = kind == b'5' || name.ends_with('/');
        let link = match longer.link {
            Some(link) => Some(link),
            None if matches!(kind, b'1' | b'2') => Some(text_field(&header[157..257])),
            None => None,
        };
        if link.as_ref().is_some_and(|l| l.len() > limits::MAX_NAME) {
            return Err(Error::Limit("link target too long".into()));
        }

        let entry = Entry {
            // A tar name is bytes too, but this reader has always taken them as
            // UTF-8 and no tar carries a flag to argue with.
            raw_name: name.as_bytes().to_vec(),
            utf8: true,
            name,
            size,
            compressed_size: size,
            method: Method::Store,
            crc32: None,
            is_dir,
            mtime: Some(mtime),
            // A tar header has a creation and an access time in the pax and
            // GNU extensions only, neither of which this reader speaks yet.
            created: None,
            accessed: None,
            attributes: 0,
            offset: self.pos,
            encrypted: false,
            zipcrypto: false,
        };
        let headers =
            (self.keep && self.raw.len() <= RAW_LIMIT).then(|| std::mem::take(&mut self.raw));
        Ok(Some(TarEntry {
            entry,
            data: if is_dir { 0 } else { size },
            mode: octal(&header[100..108], "mode").map_or(0, |m| (m & 0o7777) as u32),
            kind,
            link,
            comment: longer.comment,
            headers,
        }))
    }

    /// The data of an extension header, read whole and with its padding. Its
    /// size comes from the archive, so it is held to `limit` before anything
    /// is set aside for it.
    fn payload(&mut self, size: u64, limit: u64) -> Result<Vec<u8>> {
        if size > limit {
            return Err(Error::Limit(format!("an extension header of {size} bytes")));
        }
        let padded = size.div_ceil(BLOCK as u64) * BLOCK as u64;
        let mut data = vec![0u8; padded as usize];
        self.source.read_exact(&mut data).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                Error::Format("an extension header is truncated".into())
            } else {
                Error::Io(e)
            }
        })?;
        self.pos += padded;
        self.record(&data);
        data.truncate(size as usize);
        Ok(data)
    }

    /// Steps over `size` bytes of data and their padding.
    fn skip(&mut self, size: u64) -> Result<()> {
        let padded = size.div_ceil(BLOCK as u64) * BLOCK as u64;
        let n = io::copy(&mut (&mut self.source).take(padded), &mut io::sink())?;
        self.pos += n;
        if n < padded {
            return Err(Error::Io(truncated()));
        }
        Ok(())
    }

    pub fn copy_data<W: Write>(&mut self, e: &TarEntry, dest: &mut W) -> Result<u64> {
        let n = io::copy(&mut (&mut self.source).take(e.data), dest)?;
        self.pos += n;
        if n != e.data {
            return Err(Error::Io(truncated()));
        }
        let padding = (BLOCK - (e.data as usize % BLOCK)) % BLOCK;
        if padding > 0 {
            let mut discard = vec![0u8; padding];
            self.source.read_exact(&mut discard)?;
            self.pos += padding as u64;
        }
        Ok(n)
    }

    pub fn skip_data(&mut self, e: &TarEntry) -> Result<()> {
        self.copy_data(e, &mut io::sink()).map(|_| ())
    }

    /// Hands `f` the entry's data, which fails instead of ending early when
    /// the archive is cut short. Whatever `f` leaves unread is skipped.
    pub fn with_data<T>(
        &mut self,
        e: &TarEntry,
        f: impl FnOnce(&mut dyn Read) -> Result<T>,
    ) -> Result<T> {
        let padding = (BLOCK - (e.data as usize % BLOCK)) % BLOCK;
        let mut data = Exact {
            source: &mut self.source,
            left: e.data,
            padding,
        };
        let out = f(&mut data)?;
        io::copy(&mut data, &mut io::sink())?;
        self.pos += e.data + padding as u64;
        Ok(out)
    }
}

// The padding is read before the end of the data is reported, so a reader
// that commits on end of data never commits an entry whose padding is cut.
struct Exact<'a, R: Read> {
    source: &'a mut R,
    left: u64,
    padding: usize,
}

impl<R: Read> Read for Exact<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            if self.padding > 0 {
                let mut discard = [0u8; BLOCK];
                self.source
                    .read_exact(&mut discard[..self.padding])
                    .map_err(|_| truncated())?;
                self.padding = 0;
            }
            return Ok(0);
        }
        let max = buf
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.source.read(&mut buf[..max])?;
        if n == 0 {
            return Err(truncated());
        }
        self.left -= n as u64;
        Ok(n)
    }
}

fn truncated() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "the TAR entry is truncated")
}

/// Reads a whole block, or nothing at all: gives back 0 when the source ends
/// before the block starts and an error when it ends, or fails, inside it.
fn fill<R: Read>(source: &mut R, block: &mut [u8; BLOCK]) -> Result<usize> {
    let mut filled = 0;
    while filled < BLOCK {
        match source.read(&mut block[filled..]) {
            Ok(0) if filled == 0 => return Ok(0),
            Ok(0) => {
                return Err(Error::Format("the TAR archive ends inside a header".into()));
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(Error::Io(e)),
        }
    }
    Ok(filled)
}

pub struct TarWriter<W: Write> {
    out: W,
}

impl<W: Write> TarWriter<W> {
    pub fn new(out: W) -> Self {
        TarWriter { out }
    }

    pub fn add<R: Read>(
        &mut self,
        name: &str,
        size: u64,
        mtime: i64,
        mode: u32,
        data: R,
    ) -> Result<()> {
        let kind = if name.ends_with('/') { b'5' } else { b'0' };
        self.put(name, size, mtime, mode, kind, "")?;
        // No more than the size just recorded, whatever the source holds now.
        let written = io::copy(&mut data.take(size), &mut self.out)?;
        if written != size {
            return Err(Error::Format(format!(
                "'{name}': {size} bytes declared but {written} written"
            )));
        }
        self.pad(size)
    }

    /// A link, which has no data of its own: `kind` is `b'2'` for a symbolic
    /// link holding `target`, `b'1'` for a hard link to the earlier entry
    /// named `target`.
    pub fn add_link(&mut self, name: &str, target: &str, mtime: i64, kind: u8) -> Result<()> {
        if !matches!(kind, b'1' | b'2') {
            return Err(Error::Format(format!("'{name}' is not a link")));
        }
        if target.is_empty() || target.contains('\0') {
            return Err(Error::Format(format!("'{name}' has no usable link target")));
        }
        self.put(name, 0, mtime, 0o777, kind, target)
    }

    /// The headers of one entry, a pax one first when the ustar header cannot
    /// hold all it records.
    fn put(
        &mut self,
        name: &str,
        size: u64,
        mtime: i64,
        mode: u32,
        kind: u8,
        link: &str,
    ) -> Result<()> {
        if name.len() > limits::MAX_NAME {
            return Err(Error::Limit(format!("name of {} bytes", name.len())));
        }
        if link.len() > limits::MAX_NAME {
            return Err(Error::Limit(format!("link target of {} bytes", link.len())));
        }
        // What the ustar header cannot hold goes in a pax header in front of
        // it, which every tar since 2001 reads: a name past its 255 bytes, a
        // link target past 100, a size from 8 GiB up, a time before 1970 or
        // past 2242.
        let mut pax = Vec::new();
        let (prefix, base) = match split_name(name) {
            Some(parts) => parts,
            None => {
                pax.extend(pax_record("path", name));
                ("", cut_name(name, 100))
            }
        };
        if link.len() > 100 {
            pax.extend(pax_record("linkpath", link));
        }
        if size > OCTAL_11 {
            pax.extend(pax_record("size", &size.to_string()));
        }
        if !(0..=OCTAL_11 as i64).contains(&mtime) {
            pax.extend(pax_record("mtime", &mtime.to_string()));
        }
        if !pax.is_empty() {
            let header = ustar_header(
                &format!("PaxHeaders/{}", cut_name(base, 89)),
                "",
                "",
                pax.len() as u64,
                mtime,
                0o644,
                b'x',
            );
            self.out.write_all(&header)?;
            self.out.write_all(&pax)?;
            self.pad(pax.len() as u64)?;
        }
        self.out.write_all(&ustar_header(
            base,
            prefix,
            cut_name(link, 100),
            size,
            mtime,
            mode,
            kind,
        ))?;
        Ok(())
    }

    fn pad(&mut self, size: u64) -> Result<()> {
        let padding = (BLOCK - (size % BLOCK as u64) as usize) % BLOCK;
        if padding > 0 {
            self.out.write_all(&[0u8; BLOCK][..padding])?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        self.out.write_all(&[0u8; BLOCK * 2])?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// The largest number eleven octal digits hold: 8 GiB less one byte.
const OCTAL_11: u64 = 0o777_7777_7777;

/// A ustar header. Numbers too large for their octal field are written in
/// the base-256 form GNU tar reads, beside the pax record that says the same.
fn ustar_header(
    base: &str,
    prefix: &str,
    link: &str,
    size: u64,
    mtime: i64,
    mode: u32,
    kind: u8,
) -> [u8; BLOCK] {
    let mut header = [0u8; BLOCK];
    header[..base.len()].copy_from_slice(base.as_bytes());
    write_octal(&mut header[100..108], u64::from(mode & 0o7777));
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    if size > OCTAL_11 {
        header[124] = 0x80;
        header[128..136].copy_from_slice(&size.to_be_bytes());
    } else {
        write_octal(&mut header[124..136], size);
    }
    write_octal(
        &mut header[136..148],
        mtime.clamp(0, OCTAL_11 as i64) as u64,
    );
    header[156] = kind;
    header[157..157 + link.len()].copy_from_slice(link.as_bytes());
    header[257..262].copy_from_slice(b"ustar");
    header[263..265].copy_from_slice(b"00");
    header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    seal(&mut header);
    header
}

/// Writes a header's checksum into it, the way GNU tar does: six octal
/// digits, a NUL and a space.
fn seal(header: &mut [u8; BLOCK]) {
    let sum = checksum(header);
    header[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
    header[154] = 0;
    header[155] = b' ';
}

/// A name cut to at most `max` bytes without splitting a character.
fn cut_name(name: &str, max: usize) -> &str {
    let mut end = name.len().min(max);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// `<length> <key>=<value>\n`, where the length counts the whole record,
/// its own digits included.
fn pax_record(key: &str, value: &str) -> Vec<u8> {
    let body = key.len() + value.len() + 3;
    let mut length = body + 1;
    while length != body + length.to_string().len() {
        length = body + length.to_string().len();
    }
    format!("{length} {key}={value}\n").into_bytes()
}

/// The name split into ustar's prefix and name fields, at a slash, or `None`
/// when it does not fit them. The split is on bytes, so a name of any script
/// is cut only at its ASCII slash.
fn split_name(name: &str) -> Option<(&str, &str)> {
    if name.len() <= 100 {
        return Some(("", name));
    }
    let bytes = name.as_bytes();
    let cut = bytes[..bytes.len().min(156)]
        .iter()
        .rposition(|&b| b == b'/')?;
    let (p, b) = (&name[..cut], &name[cut + 1..]);
    (p.len() <= 155 && b.len() <= 100 && !b.is_empty()).then_some((p, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = b"test payload".repeat(100);
        let mut w = TarWriter::new(Vec::new());
        w.add(
            "dir/f.txt",
            data.len() as u64,
            1_700_000_000,
            0o644,
            &data[..],
        )
        .unwrap();
        let buf = w.finish().unwrap();
        assert_eq!(buf.len() % BLOCK, 0);

        let mut r = TarReader::new(&buf[..]);
        let e = r.next_entry().unwrap().unwrap();
        assert_eq!(e.entry.name, "dir/f.txt");
        assert_eq!(e.entry.size, data.len() as u64);
        let mut out = Vec::new();
        r.copy_data(&e, &mut out).unwrap();
        assert_eq!(out, data);
        assert!(r.next_entry().unwrap().is_none());
    }

    #[test]
    fn bad_checksum_is_detected() {
        let mut w = TarWriter::new(Vec::new());
        w.add("a.txt", 3, 0, 0o644, &b"abc"[..]).unwrap();
        let mut buf = w.finish().unwrap();
        buf[10] ^= 0xFF;
        let mut r = TarReader::new(&buf[..]);
        assert!(r.next_entry().is_err());
    }

    #[test]
    fn header_detection_needs_a_valid_checksum() {
        let mut w = TarWriter::new(Vec::new());
        w.add("a.txt", 1, 0, 0o644, &b"a"[..]).unwrap();
        let tar = w.finish().unwrap();
        assert!(is_header(&tar[..BLOCK]));
        assert!(!is_header(&tar[..BLOCK - 1]));
        assert!(!is_header(&[0u8; BLOCK]));
        let mut bad = tar[..BLOCK].to_vec();
        bad[0] ^= 1;
        assert!(!is_header(&bad));
    }

    #[test]
    fn with_data_reads_exactly_and_reports_truncation() {
        let mut w = TarWriter::new(Vec::new());
        w.add("a.txt", 5, 0, 0o644, &b"hello"[..]).unwrap();
        w.add("b.txt", 3, 0, 0o644, &b"abc"[..]).unwrap();
        let tar = w.finish().unwrap();

        let mut r = TarReader::new(&tar[..]);
        let a = r.next_entry().unwrap().unwrap();
        let first = r
            .with_data(&a, |d| {
                let mut two = [0u8; 2];
                d.read_exact(&mut two)?;
                Ok(two)
            })
            .unwrap();
        assert_eq!(&first, b"he");
        let b = r.next_entry().unwrap().unwrap();
        let mut out = Vec::new();
        r.with_data(&b, |d| Ok(io::copy(d, &mut out)?)).unwrap();
        assert_eq!(out, b"abc");
        assert!(r.next_entry().unwrap().is_none());

        let mut r = TarReader::new(&tar[..BLOCK + 2]);
        let a = r.next_entry().unwrap().unwrap();
        let mut out = Vec::new();
        let error = r.with_data(&a, |d| Ok(io::copy(d, &mut out)?)).unwrap_err();
        assert!(error.to_string().contains("truncated"), "{error}");

        let mut r = TarReader::new(&tar[..BLOCK + 10]);
        let a = r.next_entry().unwrap().unwrap();
        let mut committed = false;
        let error = r
            .with_data(&a, |d| {
                io::copy(d, &mut io::sink())?;
                committed = true;
                Ok(())
            })
            .unwrap_err();
        assert!(!committed);
        assert!(error.to_string().contains("truncated"), "{error}");
    }

    #[test]
    fn garbage_does_not_panic() {
        for seed in 0u32..1000 {
            let n = (seed as usize % 1500) + 1;
            let data: Vec<u8> = (0..n)
                .map(|i| ((seed.wrapping_mul(2_246_822_519) >> (i % 24)) & 0xFF) as u8)
                .collect();
            let mut r = TarReader::new(&data[..]);
            let _ = r.next_entry();
        }
    }

    /// A header of `kind` for `name` holding `size` bytes, as GNU tar and
    /// pax write their extension headers.
    fn header(name: &str, kind: u8, size: u64) -> Vec<u8> {
        let mut h = [0u8; BLOCK];
        h[..name.len()].copy_from_slice(name.as_bytes());
        write_octal(&mut h[100..108], 0o644);
        write_octal(&mut h[124..136], size);
        write_octal(&mut h[136..148], 1_700_000_000);
        h[156] = kind;
        h[257..262].copy_from_slice(b"ustar");
        let sum = checksum(&h);
        write_octal(&mut h[148..155], u64::from(sum));
        h[155] = b' ';
        h.to_vec()
    }

    fn padded(data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        out.resize(data.len().div_ceil(BLOCK) * BLOCK, 0);
        out
    }

    fn pax(records: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (key, value) in records {
            let body = format!(" {key}={value}\n");
            let mut length = body.len() + 1;
            while format!("{length}{body}").len() != length {
                length += 1;
            }
            out.extend_from_slice(format!("{length}{body}").as_bytes());
        }
        out
    }

    type Listed = (String, u64, i64, Vec<u8>);

    fn names(tar: &[u8]) -> Result<Vec<Listed>> {
        let mut r = TarReader::new(tar);
        let mut out = Vec::new();
        while let Some(e) = r.next_entry()? {
            let mut data = Vec::new();
            r.copy_data(&e, &mut data)?;
            out.push((e.entry.name, e.entry.size, e.entry.mtime.unwrap(), data));
        }
        Ok(out)
    }

    fn end() -> Vec<u8> {
        vec![0u8; BLOCK * 2]
    }

    #[test]
    fn the_permissions_of_an_entry_are_read_and_a_bad_field_costs_only_them() {
        let with_mode = |field: &[u8]| {
            let mut h = header("tool", b'0', 0);
            h[100..108].copy_from_slice(field);
            h[148..156].copy_from_slice(b"        ");
            let sum = checksum(<&[u8; BLOCK]>::try_from(&h[..]).unwrap());
            write_octal(&mut h[148..155], u64::from(sum));
            h.extend(end());
            let mut r = TarReader::new(&h[..]);
            r.next_entry().unwrap().unwrap().mode
        };
        assert_eq!(with_mode(b"0000755\0"), 0o755);
        assert_eq!(with_mode(b"0104755\0"), 0o4755);
        assert_eq!(with_mode(b"07x5\0\0\0\0"), 0);
    }

    #[test]
    fn a_gnu_long_name_names_the_entry_after_it() {
        let long = format!("{}/{}.txt", "d".repeat(120), "f".repeat(40));
        let mut tar = header("././@LongLink", b'L', long.len() as u64 + 1);
        tar.extend(padded(format!("{long}\0").as_bytes()));
        tar.extend(header("short-cut-name", b'0', 2));
        tar.extend(padded(b"hi"));
        tar.extend(header("././@LongLink", b'K', 10));
        tar.extend(padded(b"target.txt"));
        tar.extend(header("link", b'2', 0));
        tar.extend(end());
        let got = names(&tar).unwrap();
        assert_eq!(got[0].0, long);
        assert_eq!(got[0].3, b"hi");
        assert_eq!(got[1].0, "link");
        assert_eq!(got.len(), 2);
        assert_eq!(
            links(&tar).unwrap()[1],
            ("link".into(), Some("target.txt".into()))
        );
    }

    /// A header of `kind` for `name` pointing at `target`, which may fill the
    /// whole linkname field and so carry no NUL.
    fn link_header(name: &str, kind: u8, target: &[u8]) -> Vec<u8> {
        let mut h = header(name, kind, 0);
        h[157..157 + target.len()].copy_from_slice(target);
        h[148..156].copy_from_slice(b"        ");
        let sum = checksum(<&[u8; BLOCK]>::try_from(&h[..]).unwrap());
        write_octal(&mut h[148..155], u64::from(sum));
        h
    }

    fn links(tar: &[u8]) -> Result<Vec<(String, Option<String>)>> {
        let mut r = TarReader::new(tar);
        let mut out = Vec::new();
        while let Some(e) = r.next_entry()? {
            r.skip_data(&e)?;
            out.push((e.entry.name, e.link));
        }
        Ok(out)
    }

    #[test]
    fn link_targets_are_read_from_the_header_gnu_and_pax() {
        let full = "t".repeat(100);
        let long = format!("{}/{}", "d".repeat(150), "e".repeat(80));
        let mut tar = link_header("sym", b'2', b"../lib/x.so");
        tar.extend(link_header("hard", b'1', b"./a.txt"));
        tar.extend(link_header("full", b'2', full.as_bytes()));
        tar.extend(header("plain", b'0', 0));
        let records = pax(&[("linkpath", &long)]);
        tar.extend(header("PaxHeaders/x", b'x', records.len() as u64));
        tar.extend(padded(&records));
        tar.extend(link_header("paxed", b'2', b"cut"));
        tar.extend(end());
        let got = links(&tar).unwrap();
        let expected = [
            ("sym", Some("../lib/x.so")),
            ("hard", Some("./a.txt")),
            ("full", Some(full.as_str())),
            ("plain", None),
            ("paxed", Some(long.as_str())),
        ];
        for (got, (name, link)) in got.iter().zip(expected) {
            assert_eq!(got.0, name);
            assert_eq!(got.1.as_deref(), link);
        }
        let mut r = TarReader::new(&tar[..]);
        let sym = r.next_entry().unwrap().unwrap();
        assert_eq!(sym.symlink(), Some("../lib/x.so"));
        assert_eq!(sym.hard_link(), None);
        r.skip_data(&sym).unwrap();
        let hard = r.next_entry().unwrap().unwrap();
        assert_eq!((hard.symlink(), hard.hard_link()), (None, Some("./a.txt")));
    }

    #[test]
    fn broken_link_targets_are_errors_not_panics() {
        // A long link name with no entry after it, then cut inside its data.
        let mut alone = header("././@LongLink", b'K', 7);
        alone.extend(padded(b"target\0"));
        assert!(links(&alone).is_err());
        let mut then_end = alone.clone();
        then_end.extend(end());
        assert!(links(&then_end).is_err());
        let mut cut = header("././@LongLink", b'K', 600);
        cut.extend(vec![b'a'; 100]);
        assert!(links(&cut).is_err());
        // Larger than any name may be, in either extension.
        let mut huge = header("././@LongLink", b'K', 100_000);
        huge.extend(end());
        assert!(matches!(links(&huge), Err(Error::Limit(_))));
        let records = pax(&[("linkpath", &"x".repeat(limits::MAX_NAME + 1))]);
        let mut tar = header("PaxHeaders/x", b'x', records.len() as u64);
        tar.extend(padded(&records));
        tar.extend(link_header("l", b'2', b"y"));
        tar.extend(end());
        assert!(matches!(links(&tar), Err(Error::Limit(_))));
        // Not UTF-8 in a pax record, and cut in the middle of the header.
        let mut tar = header("PaxHeaders/x", b'x', 14);
        tar.extend(padded(b"14 linkpath=\xff\n"));
        tar.extend(link_header("l", b'2', b"y"));
        tar.extend(end());
        assert!(links(&tar).is_err());
        let whole = link_header("l", b'2', b"target");
        assert!(links(&whole[..300]).is_err());
        // Bytes that are not UTF-8 in the header field are kept as best they can be.
        let mut odd = link_header("l", b'2', b"\xff\xfe");
        odd.extend(end());
        assert_eq!(
            links(&odd).unwrap()[0].1.as_deref(),
            Some("\u{fffd}\u{fffd}")
        );
    }

    #[test]
    fn links_round_trip_with_short_long_and_non_ascii_targets() {
        let long = format!("../{}/{}", "日本".repeat(30), "ñ".repeat(40));
        assert!(long.len() > 100);
        let mut w = TarWriter::new(Vec::new());
        w.add("a.txt", 1, 0, 0o644, &b"a"[..]).unwrap();
        w.add_link("short", "a.txt", 1_700_000_000, b'2').unwrap();
        w.add_link("long", &long, 1_700_000_000, b'2').unwrap();
        w.add_link("hard", "a.txt", 1_700_000_000, b'1').unwrap();
        w.add_link("bad", "", 0, b'2').unwrap_err();
        w.add_link("bad", "a.txt", 0, b'0').unwrap_err();
        let tar = w.finish().unwrap();
        let got = links(&tar).unwrap();
        assert_eq!(got.len(), 4);
        assert_eq!(got[1], ("short".into(), Some("a.txt".into())));
        assert_eq!(got[2], ("long".into(), Some(long.clone())));
        assert_eq!(got[3], ("hard".into(), Some("a.txt".into())));
        let mut r = TarReader::new(&tar[..]);
        let file = r.next_entry().unwrap().unwrap();
        r.skip_data(&file).unwrap();
        let short = r.next_entry().unwrap().unwrap();
        assert_eq!(
            (short.kind, short.entry.size, short.is_plain()),
            (b'2', 0, false)
        );
    }

    #[test]
    fn pax_records_override_the_header_and_global_ones_are_skipped() {
        let long = format!("{}/{}.bin", "p".repeat(150), "q".repeat(30));
        let global = pax(&[("comment", "made by git archive")]);
        let mut tar = header("pax_global_header", b'g', global.len() as u64);
        tar.extend(padded(&global));
        let local = pax(&[("path", &long), ("mtime", "1234567890.25"), ("size", "3")]);
        tar.extend(header("PaxHeaders/x", b'x', local.len() as u64));
        tar.extend(padded(&local));
        tar.extend(header("cut", b'0', 0));
        tar.extend(padded(b"abc"));
        tar.extend(end());
        let got = names(&tar).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, long);
        assert_eq!((got[0].1, got[0].2), (3, 1_234_567_890));
        assert_eq!(got[0].3, b"abc");
    }

    #[test]
    fn a_pax_comment_is_read_and_a_global_one_is_not_taken_for_the_entry() {
        let global = pax(&[("comment", "made by git archive")]);
        let mut tar = header("pax_global_header", b'g', global.len() as u64);
        tar.extend(padded(&global));
        tar.extend(header("plain", b'0', 0));
        let local = pax(&[("comment", "nota en línea")]);
        tar.extend(header("PaxHeaders/x", b'x', local.len() as u64));
        tar.extend(padded(&local));
        tar.extend(header("noted", b'0', 0));
        tar.extend(end());
        let mut r = TarReader::new(&tar[..]);
        let first = r.next_entry().unwrap().unwrap();
        assert_eq!(first.comment, None);
        let second = r.next_entry().unwrap().unwrap();
        assert_eq!(second.comment.as_deref(), Some("nota en línea"));

        let mut dangling = header("PaxHeaders/x", b'x', local.len() as u64);
        dangling.extend(padded(&local));
        assert!(TarReader::new(&dangling[..]).next_entry().is_err());
        let bad = pax(&[("comment", "x")]);
        let mut broken = header("PaxHeaders/x", b'x', bad.len() as u64 + 4);
        broken.extend(padded(&bad));
        broken.extend(header("after", b'0', 0));
        assert!(TarReader::new(&broken[..]).next_entry().is_err());
    }

    #[test]
    fn the_folder_tar_was_run_in_is_not_an_entry() {
        let mut tar = header("./", b'5', 0);
        tar.extend(header("./a.txt", b'0', 1));
        tar.extend(padded(b"a"));
        tar.extend(header("././b/", b'5', 0));
        tar.extend(end());
        let got = names(&tar).unwrap();
        let got: Vec<&str> = got.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(got, ["a.txt", "b/"]);
        let mut nameless = header("", b'0', 0);
        nameless.extend(end());
        assert!(names(&nameless).is_err());
    }

    #[test]
    fn base_256_numbers_are_read() {
        let mut field = [0u8; 12];
        field[0] = 0x80;
        field[11] = 5;
        field[7] = 1;
        assert_eq!(number(&field, "size").unwrap(), (1i64 << 32) + 5);
        let negative = [0xffu8; 12];
        assert_eq!(number(&negative, "mtime").unwrap(), -1);
        assert!(unsigned(&negative, "size").is_err());
        let mut huge = [0u8; 12];
        huge[0] = 0x80;
        huge[1] = 0x7f;
        assert!(number(&huge, "size").is_err());
    }

    #[test]
    fn broken_extension_headers_are_errors_not_panics() {
        let name = b"long-name.txt\0";
        // A long name with nothing after it.
        let mut alone = header("././@LongLink", b'L', name.len() as u64);
        alone.extend(padded(name));
        assert!(names(&alone).is_err());
        let mut then_end = alone.clone();
        then_end.extend(end());
        assert!(names(&then_end).is_err());
        // Cut in the middle of its own data.
        let mut cut = header("././@LongLink", b'L', 600);
        cut.extend(vec![b'a'; 100]);
        assert!(names(&cut).is_err());
        // Larger than any name may be.
        let mut huge = header("././@LongLink", b'L', 100_000);
        huge.extend(end());
        assert!(matches!(names(&huge), Err(Error::Limit(_))));
        // Pax records that do not add up.
        for records in [
            &b"99 path=x\n"[..],
            b"9 pathx\n",
            b"x path=y\n",
            b"10 path=xy",
            b"3 a\n",
        ] {
            let mut tar = header("PaxHeaders/x", b'x', records.len() as u64);
            tar.extend(padded(records));
            tar.extend(header("a.txt", b'0', 0));
            tar.extend(end());
            assert!(names(&tar).is_err(), "{}", String::from_utf8_lossy(records));
        }
    }

    // A name of any script that is too long for ustar goes in a pax record and
    // reads back whole; one that ustar can split is cut only at a slash, never
    // inside a character.
    #[test]
    fn long_and_non_ascii_names_round_trip() {
        let split = format!("{}/{}.txt", "ñ".repeat(77), "é".repeat(30));
        assert!(split.len() > 100 && split.len() < 256);
        let long = format!("{}/{}", "日本".repeat(60), "ñ".repeat(80));
        let one_part = "x".repeat(300);
        let mut w = TarWriter::new(Vec::new());
        for name in [&split, &long, &one_part] {
            w.add(name, 3, 1_700_000_000, 0o755, &b"abc"[..]).unwrap();
        }
        let tar = w.finish().unwrap();
        let got = names(&tar).unwrap();
        let got: Vec<&str> = got.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(got, [split.as_str(), long.as_str(), one_part.as_str()]);
        let mut r = TarReader::new(&tar[..]);
        assert_eq!(r.next_entry().unwrap().unwrap().mode, 0o755);
    }

    // A size from 8 GiB up does not fit eleven octal digits; it is written in
    // base 256 and in a pax record, and reads back from either.
    #[test]
    fn sizes_and_times_past_ustar_fields_are_kept() {
        let huge = (8u64 << 30) + 5;
        let header = ustar_header("big", "", "", huge, -1, 0o644, b'0');
        assert_eq!(unsigned(&header[124..136], "size").unwrap(), huge);
        assert!(is_header(&header));
        let record = pax_record("size", &huge.to_string());
        let mut longer = Longer::default();
        longer.apply_pax(&record).unwrap();
        assert_eq!(longer.size, Some(huge));
        // Lengths that cross a digit boundary still count themselves.
        for value in ["x".repeat(4), "x".repeat(92), "x".repeat(993)] {
            let record = pax_record("path", &value);
            let digits = record.iter().position(|&b| b == b' ').unwrap();
            let length: usize = std::str::from_utf8(&record[..digits])
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(length, record.len());
        }
        let mut w = TarWriter::new(Vec::new());
        w.add("old.txt", 1, -86_400, 0o644, &b"a"[..]).unwrap();
        let tar = w.finish().unwrap();
        assert_eq!(names(&tar).unwrap()[0].2, -86_400);
    }

    // A source that grew after its size was taken writes the size recorded,
    // not what it holds now, so the archive stays readable.
    #[test]
    fn a_source_that_grew_writes_only_its_recorded_size() {
        let mut w = TarWriter::new(Vec::new());
        w.add("grew.txt", 3, 0, 0o644, &b"abcdef"[..]).unwrap();
        w.add("next.txt", 1, 0, 0o644, &b"z"[..]).unwrap();
        let tar = w.finish().unwrap();
        let got = names(&tar).unwrap();
        assert_eq!(got[0].3, b"abc");
        assert_eq!(got[1].0, "next.txt");
        let mut w = TarWriter::new(Vec::new());
        assert!(w.add("short.txt", 5, 0, 0o644, &b"ab"[..]).is_err());
    }

    // A tar that stops where a header would start is ended; one that stops
    // inside a header or inside an entry's data is cut, even when the data
    // is a whole number of blocks and so has no padding to trip over.
    #[test]
    fn truncation_is_an_error_and_a_missing_end_is_not() {
        let mut w = TarWriter::new(Vec::new());
        w.add("a.bin", 1024, 0, 0o644, &[7u8; 1024][..]).unwrap();
        let tar = w.finish().unwrap();
        let entry = BLOCK + 1024;
        assert_eq!(names(&tar[..entry]).unwrap().len(), 1);
        assert!(names(&tar[..entry + 100]).is_err());
        assert!(names(&tar[..BLOCK + 512]).is_err());
        assert!(names(&tar[..300]).is_err());
    }

    // Links, devices and pipes are listed but are not files to write, and a
    // sparse or multivolume entry is refused rather than written wrong.
    #[test]
    fn entry_kinds_are_told_apart() {
        let mut tar = header("link", b'2', 0);
        tar.extend(header("hard", b'1', 0));
        tar.extend(header("file", b'0', 0));
        tar.extend(header("dir/", b'5', 0));
        tar.extend(end());
        let mut r = TarReader::new(&tar[..]);
        let mut plain = Vec::new();
        while let Some(e) = r.next_entry().unwrap() {
            plain.push((e.entry.name.clone(), e.is_plain()));
        }
        assert_eq!(
            plain,
            [
                ("link".to_string(), false),
                ("hard".to_string(), false),
                ("file".to_string(), true),
                ("dir/".to_string(), true),
            ]
        );
        for &kind in b"SM" {
            let mut tar = header("odd", kind, 0);
            tar.extend(end());
            assert!(matches!(names(&tar), Err(Error::Unsupported(_))));
        }
    }
}
