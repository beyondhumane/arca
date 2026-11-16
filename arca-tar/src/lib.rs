#![forbid(unsafe_code)]

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
    size: Option<u64>,
    mtime: Option<i64>,
}

impl Longer {
    fn is_empty(&self) -> bool {
        self.name.is_none() && self.size.is_none() && self.mtime.is_none()
    }

    /// Reads pax records, each `<length> <key>=<value>\n` where the length
    /// counts the whole record, itself included.
    fn apply_pax(&mut self, mut data: &[u8]) -> Result<()> {
        let bad = || Error::Format("a malformed pax record".into());
        while !data.is_empty() {
            let space = data.iter().position(|&b| b == b' ').ok_or_else(bad)?;
            let length: usize = std::str::from_utf8(&data[..space])
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(bad)?;
            if length <= space + 1 || length > data.len() || data[length - 1] != b'\n' {
                return Err(bad());
            }
            let record = &data[space + 1..length - 1];
            let equals = record.iter().position(|&b| b == b'=').ok_or_else(bad)?;
            let (key, value) = (&record[..equals], &record[equals + 1..]);
            let value = std::str::from_utf8(value).map_err(|_| bad())?;
            match key {
                b"path" => self.name = Some(value.to_string()),
                b"size" => self.size = Some(value.parse().map_err(|_| bad())?),
                b"mtime" => {
                    let whole = value.split('.').next().unwrap_or(value);
                    self.mtime = Some(whole.parse().map_err(|_| bad())?);
                }
                _ => {}
            }
            data = &data[length..];
        }
        Ok(())
    }
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
}

pub struct TarReader<R: Read> {
    source: R,
    pos: u64,
    finished: bool,
}

impl<R: Read> TarReader<R> {
    pub fn new(source: R) -> Self {
        TarReader {
            source,
            pos: 0,
            finished: false,
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
            let mut longer = Longer::default();
            let header = loop {
                let mut header = [0u8; BLOCK];
                match self.source.read_exact(&mut header) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        self.finished = true;
                        return if longer.is_empty() {
                            Ok(None)
                        } else {
                            Err(dangling())
                        };
                    }
                    Err(e) => return Err(Error::Io(e)),
                }
                self.pos += BLOCK as u64;

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
                        self.payload(size, limits::MAX_NAME as u64 + 1)?;
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
        let is_dir = kind == b'5' || name.ends_with('/');

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
        Ok(Some(TarEntry {
            entry,
            data: if is_dir { 0 } else { size },
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
        mut data: R,
    ) -> Result<()> {
        let (prefix, base) = split_name(name)?;
        let mut header = [0u8; BLOCK];
        header[..base.len()].copy_from_slice(base.as_bytes());
        write_octal(&mut header[100..108], mode as u64);
        write_octal(&mut header[108..116], 0);
        write_octal(&mut header[116..124], 0);
        write_octal(&mut header[124..136], size);
        write_octal(&mut header[136..148], mtime.max(0) as u64);
        header[156] = if name.ends_with('/') { b'5' } else { b'0' };
        header[257..262].copy_from_slice(b"ustar");
        header[262] = 0;
        header[263..265].copy_from_slice(b"00");
        if !prefix.is_empty() {
            header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        }
        let sum = checksum(&header);
        let s = format!("{sum:06o}");
        header[148..154].copy_from_slice(s.as_bytes());
        header[154] = 0;
        header[155] = b' ';

        self.out.write_all(&header)?;
        let written = io::copy(&mut data, &mut self.out)?;
        if written != size {
            return Err(Error::Format(format!(
                "'{name}': {size} bytes declared but {written} written"
            )));
        }
        let padding = (BLOCK - (size as usize % BLOCK)) % BLOCK;
        if padding > 0 {
            self.out.write_all(&vec![0u8; padding])?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        self.out.write_all(&[0u8; BLOCK * 2])?;
        self.out.flush()?;
        Ok(self.out)
    }
}

fn split_name(name: &str) -> Result<(&str, &str)> {
    if name.len() <= 100 {
        return Ok(("", name));
    }
    let cut = name[..name.len().min(156)]
        .rfind('/')
        .ok_or_else(|| Error::Limit(format!("name of {} bytes with no split point", name.len())))?;
    let (p, b) = name.split_at(cut);
    let b = &b[1..];
    if p.len() > 155 || b.len() > 100 {
        return Err(Error::Limit(format!("name too long for ustar: {name}")));
    }
    Ok((p, b))
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
}
