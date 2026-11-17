//! Archives cut into numbered parts the way 7-Zip's `-v` does it: the bytes
//! of one archive split at a fixed size into `name.ext.001`, `name.ext.002`
//! and so on, with nothing added. Reading joins them back into one seekable
//! source; writing cuts as it goes, under temporary names that only become
//! the real ones once the whole set is there.

use crate::{Error, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The most parts a set may have, which is also as far as four digits go.
pub const MAX_PARTS: u32 = 9999;

const STAGED: &str = ".arca-new";

/// The path of part `index` (counting from 1) of the set made from `base`.
pub fn part_path(base: &Path, index: u32) -> PathBuf {
    let mut name = base.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{index:03}"));
    base.with_file_name(name)
}

fn staged_part(base: &Path, index: u32) -> PathBuf {
    let mut name = part_path(base, index).into_os_string();
    name.push(STAGED);
    PathBuf::from(name)
}

fn staged_single(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(STAGED);
    out.with_file_name(name)
}

/// The number of a part named the way [`part_path`] writes it: three digits
/// up to 999, then the number as it is. `.000`, `.0005` and anything that is
/// not all digits are not parts.
fn part_number(suffix: &str) -> Option<u32> {
    if suffix.len() < 3 || suffix.len() > 5 || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = suffix.parse().ok()?;
    (n > 0 && format!("{n:03}") == suffix).then_some(n)
}

/// For a name like `backup.zip.002`, the name of the whole set,
/// `backup.zip`, and which part this is.
pub fn split_part(path: &Path) -> Option<(PathBuf, u32)> {
    let name = path.file_name()?.to_str()?;
    let (stem, suffix) = name.rsplit_once('.')?;
    let index = part_number(suffix)?;
    (!stem.is_empty()).then(|| (path.with_file_name(stem), index))
}

/// Whether `path` names a part of a split set.
pub fn is_part(path: &Path) -> bool {
    split_part(path).is_some()
}

/// The name a split part stands for, `backup.zip` for `backup.zip.001`,
/// and any other path as it is.
pub fn whole_name(path: &Path) -> PathBuf {
    split_part(path).map_or_else(|| path.to_path_buf(), |(base, _)| base)
}

fn broken(message: String) -> Error {
    Error::Format(message)
}

/// Every part of the set `path` belongs to, in order, with its size. All of
/// them have to be there, numbered from 1 without gaps, each one but the last
/// exactly as long as the first and the last no longer than that.
pub fn parts_of(path: &Path) -> Result<Vec<(PathBuf, u64)>> {
    let (base, opened) =
        split_part(path).ok_or_else(|| broken(format!("'{}' is not a part", path.display())))?;
    let name = base
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let dir = match base.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(rest) = file_name
            .to_str()
            .and_then(|n| n.strip_prefix(name.as_str()))
            .and_then(|n| n.strip_prefix('.'))
        else {
            continue;
        };
        if let Some(index) = part_number(rest) {
            if index > MAX_PARTS {
                return Err(Error::Limit(format!(
                    "'{}' has more than {MAX_PARTS} parts",
                    base.display()
                )));
            }
            found.push(index);
        }
    }
    found.sort_unstable();
    found.dedup();
    let mut parts = Vec::with_capacity(found.len());
    for (at, index) in found.iter().enumerate() {
        let expected = at as u32 + 1;
        if *index != expected {
            return Err(broken(format!(
                "part '{}' of the split archive is missing",
                part_path(&base, expected).display()
            )));
        }
        let path = part_path(&base, expected);
        let size = fs::metadata(&path)?.len();
        parts.push((path, size));
    }
    if found.last().is_none_or(|last| opened > *last) {
        return Err(broken(format!(
            "part '{}' of the split archive is missing",
            part_path(&base, found.len() as u32 + 1).display()
        )));
    }
    let first = parts[0].1;
    let last = parts.len() - 1;
    for (i, (path, size)) in parts.iter().enumerate() {
        let short = if i < last {
            *size != first
        } else {
            last > 0 && (*size == 0 || *size > first)
        };
        if short || (first == 0 && last > 0) {
            return Err(broken(format!(
                "part '{}' of the split archive has the wrong size: {size} bytes, \
                 where every part but the last has {first}",
                path.display()
            )));
        }
    }
    Ok(parts)
}

/// The parts of a split set read as the one archive they were cut from.
pub struct Joined {
    parts: Vec<PathBuf>,
    size: u64,
    len: u64,
    pos: u64,
    open: Option<(usize, File, u64)>,
}

impl Joined {
    pub fn open(path: &Path) -> Result<Self> {
        let found = parts_of(path)?;
        let size = found[0].1;
        let len = found.iter().map(|(_, n)| n).sum();
        let first = File::open(&found[0].0)?;
        Ok(Self {
            parts: found.into_iter().map(|(p, _)| p).collect(),
            size,
            len,
            pos: 0,
            open: Some((0, first, 0)),
        })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn parts(&self) -> &[PathBuf] {
        &self.parts
    }
}

impl Read for Joined {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        let index = ((self.pos / self.size) as usize).min(self.parts.len() - 1);
        let start = index as u64 * self.size;
        let within = self.pos - start;
        let part_len = if index + 1 == self.parts.len() {
            self.len - start
        } else {
            self.size
        };
        let want = (part_len - within).min(buf.len() as u64) as usize;
        if self.open.as_ref().is_none_or(|(i, _, _)| *i != index) {
            self.open = Some((index, File::open(&self.parts[index])?, 0));
        }
        let Some((_, file, at)) = self.open.as_mut() else {
            return Ok(0);
        };
        if *at != within {
            file.seek(SeekFrom::Start(within))?;
            *at = within;
        }
        let n = file.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "part '{}' of the split archive is shorter than it was when opened",
                    self.parts[index].display()
                ),
            ));
        }
        *at += n as u64;
        self.pos += n as u64;
        Ok(n)
    }
}

pub(crate) fn moved(from: u64, by: i64) -> io::Result<u64> {
    from.checked_add_signed(by).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "seek to a negative or overflowing position",
        )
    })
}

impl Seek for Joined {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::End(by) => moved(self.len, by)?,
            SeekFrom::Current(by) => moved(self.pos, by)?,
        };
        Ok(self.pos)
    }
}

/// An archive on disk, whether a single file or a split set.
pub enum Source {
    Single(File),
    Split(Joined),
}

/// Opens `path` for reading. A name like `backup.zip.002` opens the whole
/// set it belongs to, from its first byte.
pub fn open(path: &Path) -> Result<Source> {
    if is_part(path) {
        Ok(Source::Split(Joined::open(path)?))
    } else {
        Ok(Source::Single(File::open(path)?))
    }
}

impl Source {
    /// How many bytes the archive has, across all of its parts.
    pub fn len(&self) -> io::Result<u64> {
        match self {
            Source::Single(f) => Ok(f.metadata()?.len()),
            Source::Split(j) => Ok(j.len()),
        }
    }

    pub fn is_empty(&self) -> io::Result<bool> {
        self.len().map(|n| n == 0)
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Source::Single(f) => f.read(buf),
            Source::Split(j) => j.read(buf),
        }
    }
}

impl Seek for Source {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match self {
            Source::Single(f) => f.seek(to),
            Source::Split(j) => j.seek(to),
        }
    }
}

/// Writes a set of parts of `size` bytes each under their staged names,
/// opening the next part when the position reaches it and reopening an
/// earlier one when a seek goes back there.
pub struct SplitWriter {
    base: PathBuf,
    size: u64,
    limit: u32,
    pos: u64,
    parts: u32,
    open: Option<(u32, File, u64)>,
}

impl SplitWriter {
    fn new(base: &Path, size: u64, limit: u32) -> Self {
        Self {
            base: base.to_path_buf(),
            size,
            limit,
            pos: 0,
            parts: 0,
            open: None,
        }
    }

    fn file(&mut self, index: u32) -> io::Result<&mut (u32, File, u64)> {
        if index > self.limit {
            return Err(io::Error::other(format!(
                "a split archive cannot have more than {} parts; choose a larger volume size",
                self.limit
            )));
        }
        if self.open.as_ref().is_none_or(|(i, _, _)| *i != index) {
            self.open = None;
            while self.parts < index {
                if self.parts > 0 {
                    let previous = OpenOptions::new()
                        .write(true)
                        .open(staged_part(&self.base, self.parts))?;
                    if previous.metadata()?.len() < self.size {
                        previous.set_len(self.size)?;
                    }
                }
                File::create(staged_part(&self.base, self.parts + 1))?;
                self.parts += 1;
            }
            let file = OpenOptions::new()
                .write(true)
                .open(staged_part(&self.base, index))?;
            self.open = Some((index, file, 0));
        }
        self.open
            .as_mut()
            .ok_or_else(|| io::Error::other("no part open"))
    }
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let index = u32::try_from(self.pos / self.size + 1).unwrap_or(u32::MAX);
        let within = self.pos % self.size;
        let room = (self.size - within).min(buf.len() as u64) as usize;
        let (_, file, at) = self.file(index)?;
        if *at != within {
            file.seek(SeekFrom::Start(within))?;
            *at = within;
        }
        let n = file.write(&buf[..room])?;
        *at += n as u64;
        self.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.open.as_mut() {
            Some((_, file, _)) => file.flush(),
            None => Ok(()),
        }
    }
}

impl Seek for SplitWriter {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let end = match self.parts {
            0 => 0,
            n => {
                let last = fs::metadata(staged_part(&self.base, n))?.len();
                u64::from(n - 1) * self.size + last
            }
        };
        self.pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::End(by) => moved(end, by)?,
            SeekFrom::Current(by) => moved(self.pos, by)?,
        };
        Ok(self.pos)
    }
}

/// What a new archive is written into: one file, or parts of a fixed size.
/// Either way under staged names, which [`staged`] puts in place at the end.
pub enum Output {
    Single(File),
    Split(SplitWriter),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::Single(f) => f.write(buf),
            Output::Split(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::Single(f) => f.flush(),
            Output::Split(s) => s.flush(),
        }
    }
}

impl Seek for Output {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        match self {
            Output::Single(f) => f.seek(to),
            Output::Split(s) => s.seek(to),
        }
    }
}

fn staged_parts(base: &Path) -> Vec<PathBuf> {
    (1..)
        .map(|i| staged_part(base, i))
        .take_while(|p| p.exists())
        .collect()
}

fn discard(out: &Path, volume: Option<u64>) {
    match volume {
        None => {
            let _ = fs::remove_file(staged_single(out));
        }
        Some(_) => {
            for part in staged_parts(out) {
                let _ = fs::remove_file(part);
            }
        }
    }
}

fn commit(out: &Path, volume: Option<u64>) -> Result<u64> {
    if volume.is_none() {
        let temp = staged_single(out);
        let len = fs::metadata(&temp)?.len();
        fs::rename(&temp, out)?;
        return Ok(len);
    }
    let staged = staged_parts(out);
    if staged.is_empty() {
        File::create(staged_part(out, 1))?;
        return commit(out, volume);
    }
    let mut len = 0;
    for part in &staged {
        len += fs::metadata(part)?.len();
    }
    for (at, temp) in staged.iter().enumerate() {
        let index = at as u32 + 1;
        if let Err(e) = fs::rename(temp, part_path(out, index)) {
            for done in 1..index {
                let _ = fs::remove_file(part_path(out, done));
            }
            return Err(e.into());
        }
    }
    let mut stale = staged.len() as u32 + 1;
    while stale <= MAX_PARTS && part_path(out, stale).exists() {
        let _ = fs::remove_file(part_path(out, stale));
        stale += 1;
    }
    Ok(len)
}

/// Where a new archive at `out` ends up: `out` itself, or with a volume
/// size, its first part.
pub fn first_output(out: &Path, volume: Option<u64>) -> PathBuf {
    match volume {
        Some(_) => part_path(out, 1),
        None => out.to_path_buf(),
    }
}

/// Builds a new archive at `out`, or as parts of `volume` bytes named after
/// it, through `write`, and moves it into place only once `write` succeeds.
/// A failure or a stop leaves nothing behind under either name. An existing
/// first part is only replaced when `replace` says so, and replacing a longer
/// set removes the parts the new one does not reach. Answers with what
/// `write` gave back and how many bytes were written in all.
pub fn staged<T>(
    out: &Path,
    volume: Option<u64>,
    replace: bool,
    write: impl FnOnce(Output) -> Result<T>,
) -> Result<(T, u64)> {
    let output = match volume {
        None => Output::Single(File::create(staged_single(out))?),
        Some(0) => {
            return Err(Error::Unsupported(
                "the volume size must be at least one byte".into(),
            ))
        }
        Some(size) => {
            if !replace && part_path(out, 1).exists() {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "'{}' already exists; remove the old parts or pick another name",
                        part_path(out, 1).display()
                    ),
                )));
            }
            discard(out, volume);
            Output::Split(SplitWriter::new(out, size, MAX_PARTS))
        }
    };
    let result = write(output).and_then(|v| commit(out, volume).map(|len| (v, len)));
    if result.is_err() {
        discard(out, volume);
    }
    result
}

/// A volume size as 7-Zip's `-v` takes it: a number of bytes, or one ending
/// in `k`, `m` or `g` for KiB, MiB or GiB.
pub fn parse_size(text: &str) -> std::result::Result<u64, String> {
    let text = text.trim();
    let (digits, unit) = match text.char_indices().last() {
        Some((at, c)) if c.is_ascii_alphabetic() => (&text[..at], c.to_ascii_lowercase()),
        _ => (text, 'b'),
    };
    let shift = match unit {
        'b' => 0,
        'k' => 10,
        'm' => 20,
        'g' => 30,
        _ => return Err(format!("unknown unit in '{text}': use k, m or g")),
    };
    let n: u64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("'{text}' is not a size: write it like 100m, 700m or 4g"))?;
    if n == 0 {
        return Err("the volume size must be at least one byte".into());
    }
    n.checked_mul(1 << shift)
        .ok_or_else(|| format!("'{text}' is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_set(dir: &Path, name: &str, parts: &[&[u8]]) -> PathBuf {
        let base = dir.join(name);
        for (i, data) in parts.iter().enumerate() {
            fs::write(part_path(&base, i as u32 + 1), data).unwrap();
        }
        base
    }

    fn read_all(path: &Path) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        open(path)?.read_to_end(&mut v)?;
        Ok(v)
    }

    #[test]
    fn part_names_follow_seven_zip() {
        let base = Path::new("dir/a.zip");
        assert_eq!(part_path(base, 1), Path::new("dir/a.zip.001"));
        assert_eq!(part_path(base, 999), Path::new("dir/a.zip.999"));
        assert_eq!(part_path(base, 1000), Path::new("dir/a.zip.1000"));
        assert_eq!(
            split_part(Path::new("dir/a.zip.002")),
            Some((PathBuf::from("dir/a.zip"), 2))
        );
        assert_eq!(
            split_part(Path::new("a.zip.1000")),
            Some((PathBuf::from("a.zip"), 1000))
        );
        for name in [
            "a.zip",
            "a.zip.000",
            "a.zip.0005",
            "a.zip.01",
            "a.zip.1a3",
            ".001",
            "a.r00",
            "a.zip.",
        ] {
            assert_eq!(split_part(Path::new(name)), None, "{name}");
        }
        assert_eq!(whole_name(Path::new("x.tar.gz.003")), Path::new("x.tar.gz"));
        assert_eq!(whole_name(Path::new("x.tar.gz")), Path::new("x.tar.gz"));
    }

    #[test]
    fn sizes_parse_like_seven_zip() {
        assert_eq!(parse_size("100"), Ok(100));
        assert_eq!(parse_size("2k"), Ok(2048));
        assert_eq!(parse_size("100m"), Ok(100 << 20));
        assert_eq!(parse_size("4G"), Ok(4 << 30));
        for bad in ["", "0", "m", "10x", "-5m", "1.5m", "99999999999999999999g"] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
        assert!(parse_size("17179869184g").is_err());
    }

    #[test]
    fn any_part_opens_the_whole_set() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"abcd", b"efgh", b"ij"]);
        for i in 1..=3 {
            assert_eq!(read_all(&part_path(&base, i)).unwrap(), b"abcdefghij");
        }
        let mut s = open(&part_path(&base, 2)).unwrap();
        assert_eq!(s.len().unwrap(), 10);
        s.seek(SeekFrom::End(-3)).unwrap();
        let mut tail = Vec::new();
        s.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, b"hij");
        s.seek(SeekFrom::Start(3)).unwrap();
        let mut two = [0u8; 2];
        s.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"de");
        assert_eq!(s.seek(SeekFrom::Start(50)).unwrap(), 50);
        assert_eq!(s.read(&mut two).unwrap(), 0);
        assert!(s.seek(SeekFrom::Current(-100)).is_err());
        assert!(s.seek(SeekFrom::End(-11)).is_err());
        assert!(read_all(&dir.path().join("plain")).is_err());
    }

    #[test]
    fn a_single_part_is_a_set_of_one() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"xyz"]);
        assert_eq!(read_all(&part_path(&base, 1)).unwrap(), b"xyz");
        let empty = write_set(dir.path(), "e.bin", &[b""]);
        assert_eq!(read_all(&part_path(&empty, 1)).unwrap(), b"");
    }

    #[test]
    fn a_missing_part_is_named() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"ab", b"cd", b"ef"]);
        fs::remove_file(part_path(&base, 2)).unwrap();
        let error = read_all(&part_path(&base, 1)).unwrap_err().to_string();
        assert!(
            error.contains("a.bin.002") && error.contains("missing"),
            "{error}"
        );
        fs::remove_file(part_path(&base, 1)).unwrap();
        let error = read_all(&part_path(&base, 3)).unwrap_err().to_string();
        assert!(error.contains("a.bin.001"), "{error}");
    }

    #[test]
    fn opening_a_part_past_the_end_of_the_set_fails() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"ab", b"c"]);
        let error = read_all(&part_path(&base, 3)).unwrap_err().to_string();
        assert!(error.contains("a.bin.003"), "{error}");
    }

    #[test]
    fn short_and_long_parts_are_refused() {
        let dir = room();
        let base = write_set(dir.path(), "short.bin", &[b"abcd", b"ef", b"gh"]);
        let error = read_all(&part_path(&base, 1)).unwrap_err().to_string();
        assert!(
            error.contains("short.bin.002") && error.contains("size"),
            "{error}"
        );
        let base = write_set(dir.path(), "long.bin", &[b"ab", b"cd", b"efg"]);
        assert!(read_all(&part_path(&base, 1)).is_err());
        let base = write_set(dir.path(), "empty.bin", &[b"ab", b""]);
        assert!(read_all(&part_path(&base, 1)).is_err());
        let base = write_set(dir.path(), "zero.bin", &[b"", b""]);
        assert!(read_all(&part_path(&base, 1)).is_err());
    }

    #[test]
    fn similar_names_are_not_mixed_in() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"ab", b"c"]);
        fs::write(dir.path().join("a.bin.0003"), b"zz").unwrap();
        fs::write(dir.path().join("a.bin.003.arca-new"), b"zz").unwrap();
        fs::write(dir.path().join("xa.bin.003"), b"zz").unwrap();
        fs::write(dir.path().join("a.bin.004x"), b"zz").unwrap();
        assert_eq!(read_all(&part_path(&base, 1)).unwrap(), b"abc");
    }

    #[test]
    fn too_many_parts_are_refused() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"a"]);
        fs::write(part_path(&base, 10_000), b"a").unwrap();
        assert!(matches!(
            read_all(&part_path(&base, 1)),
            Err(Error::Limit(_))
        ));
    }

    #[test]
    fn a_part_that_shrinks_after_opening_is_an_error_not_a_panic() {
        let dir = room();
        let base = write_set(dir.path(), "a.bin", &[b"abcd", b"efgh", b"ij"]);
        let mut s = open(&part_path(&base, 1)).unwrap();
        fs::write(part_path(&base, 2), b"e").unwrap();
        let mut v = Vec::new();
        let error = s.read_to_end(&mut v).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    fn build(out: &Path, size: u64, data: &[u8]) -> Result<u64> {
        staged(out, Some(size), false, |mut w| {
            w.write_all(data)?;
            Ok(())
        })
        .map(|(_, len)| len)
    }

    #[test]
    fn writing_cuts_at_the_volume_size() {
        let dir = room();
        let out = dir.path().join("a.bin");
        assert_eq!(build(&out, 4, b"abcdefghij").unwrap(), 10);
        assert_eq!(fs::read(part_path(&out, 1)).unwrap(), b"abcd");
        assert_eq!(fs::read(part_path(&out, 2)).unwrap(), b"efgh");
        assert_eq!(fs::read(part_path(&out, 3)).unwrap(), b"ij");
        assert!(!part_path(&out, 4).exists());
        assert!(!out.exists());
        assert_eq!(read_all(&part_path(&out, 1)).unwrap(), b"abcdefghij");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn an_exact_multiple_has_no_empty_last_part() {
        let dir = room();
        let out = dir.path().join("a.bin");
        build(&out, 4, b"abcdefgh").unwrap();
        assert!(part_path(&out, 2).exists() && !part_path(&out, 3).exists());
        let empty = dir.path().join("e.bin");
        build(&empty, 4, b"").unwrap();
        assert_eq!(fs::read(part_path(&empty, 1)).unwrap(), b"");
    }

    #[test]
    fn seeking_back_rewrites_an_earlier_part() {
        let dir = room();
        let out = dir.path().join("a.bin");
        staged(&out, Some(3), false, |mut w| {
            w.seek(SeekFrom::Start(2))?;
            w.write_all(b"cdefgh")?;
            w.seek(SeekFrom::Start(0))?;
            w.write_all(b"ab")?;
            w.seek(SeekFrom::Start(4))?;
            w.write_all(b"EF")?;
            assert_eq!(w.seek(SeekFrom::End(0))?, 8);
            w.seek(SeekFrom::Current(2))?;
            w.write_all(b"k")?;
            assert!(w.seek(SeekFrom::Current(-100)).is_err());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            read_all(&part_path(&out, 1)).unwrap(),
            b"abcdEFgh\0\0k".to_vec()
        );
        assert_eq!(fs::read(part_path(&out, 1)).unwrap().len(), 3);
        assert_eq!(fs::read(part_path(&out, 4)).unwrap(), b"\0k");
    }

    #[test]
    fn seeking_past_a_part_before_writing_fills_the_gap() {
        let dir = room();
        let out = dir.path().join("a.bin");
        staged(&out, Some(4), false, |mut w| {
            w.seek(SeekFrom::Start(9))?;
            w.write_all(b"z")?;
            Ok(())
        })
        .unwrap();
        let mut expected = vec![0u8; 9];
        expected.push(b'z');
        assert_eq!(read_all(&part_path(&out, 1)).unwrap(), expected);
    }

    #[test]
    fn a_failure_or_a_stop_leaves_nothing_behind() {
        let dir = room();
        let out = dir.path().join("a.bin");
        let result = staged(&out, Some(2), false, |mut w| -> Result<()> {
            w.write_all(b"abcdefg")?;
            Err(Error::Cancelled)
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let result = staged(&out, None, false, |mut w| -> Result<()> {
            w.write_all(b"abc")?;
            Err(Error::Cancelled)
        });
        assert!(result.is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn running_past_the_part_limit_fails_and_cleans_up() {
        let dir = room();
        let out = dir.path().join("a.bin");
        let mut w = SplitWriter::new(&out, 2, 3);
        assert!(w.write_all(b"abcdef").is_ok());
        assert!(w.write_all(b"g").is_err());
        drop(w);
        discard(&out, Some(2));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_existing_set_is_kept_unless_replacing_and_then_trimmed() {
        let dir = room();
        let out = dir.path().join("a.bin");
        build(&out, 2, b"abcdefghij").unwrap();
        let error = build(&out, 2, b"xy").unwrap_err();
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(read_all(&part_path(&out, 1)).unwrap(), b"abcdefghij");
        staged(&out, Some(2), true, |mut w| {
            w.write_all(b"xyz")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(read_all(&part_path(&out, 1)).unwrap(), b"xyz");
        assert!(!part_path(&out, 3).exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn stale_staged_parts_from_an_earlier_run_are_not_committed() {
        let dir = room();
        let out = dir.path().join("a.bin");
        for i in 1..=4 {
            fs::write(staged_part(&out, i), b"old").unwrap();
        }
        build(&out, 2, b"abc").unwrap();
        assert_eq!(read_all(&part_path(&out, 1)).unwrap(), b"abc");
        assert!(!part_path(&out, 3).exists());
        assert!(!staged_part(&out, 3).exists());
    }

    #[test]
    fn a_single_output_is_staged_and_renamed() {
        let dir = room();
        let out = dir.path().join("a.bin");
        fs::write(&out, b"old").unwrap();
        let (_, len) = staged(&out, None, false, |mut w| {
            w.write_all(b"new!")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(len, 4);
        assert_eq!(fs::read(&out).unwrap(), b"new!");
        assert!(!staged_single(&out).exists());
        assert!(staged(&out, Some(0), true, |_| Ok(())).is_err());
    }
}
