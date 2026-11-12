use std::fmt;
use std::io;

mod format;
pub use format::{Container, Format};
pub mod extraction;

pub mod limits {
    pub const MAX_NAME: usize = 4096;
    pub const MAX_ENTRIES: u64 = 10_000_000;
    pub const MAX_EXTRA: usize = 65_535;
    pub const MAX_RATIO: u64 = 1000;
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Format(String),
    Limit(String),
    Integrity {
        name: String,
        expected: u32,
        found: u32,
    },
    // The AES authentication code did not match. Separate from Integrity
    // because there is no CRC to report here: the HMAC is what failed, and
    // the difference matters, since it also catches deliberate tampering.
    Tampered {
        name: String,
    },
    Unsupported(String),
    PasswordRequired,
    PasswordOrCorrupt,
    BadPassword,
    // Somebody pressed stop. Not a failure: nothing is wrong with the archive
    // and nothing needs reporting, so callers that clean up after themselves
    // can tell this apart from a real error and say so plainly.
    Cancelled,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Format(m) => write!(f, "malformed archive: {m}"),
            Error::Limit(m) => write!(f, "limit exceeded: {m}"),
            Error::Integrity {
                name,
                expected,
                found,
            } => write!(
                f,
                "integrity failure in '{name}': expected CRC {expected:08x}, found {found:08x}"
            ),
            Error::Tampered { name } => write!(
                f,
                "'{name}' did not pass its authentication code: the archive was altered after it was encrypted"
            ),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::PasswordRequired => write!(f, "a password is required for this archive"),
            Error::PasswordOrCorrupt => write!(f, "wrong password or corrupt encrypted archive"),
            Error::BadPassword => write!(f, "incorrect archive password"),
            Error::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Store,
    Deflate,
    Zstd,
    Lzma,
    Lzma2,
    Bzip2,
    Other7z,
    Rar,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Method::Store => "store",
            Method::Deflate => "deflate",
            Method::Zstd => "zstd",
            Method::Lzma => "lzma",
            Method::Lzma2 => "lzma2",
            Method::Bzip2 => "bzip2",
            Method::Other7z => "7z",
            Method::Rar => "rar",
        }
    }

    /// The ZIP method code, only for codecs supported by Arca's ZIP writer.
    pub fn code(self) -> Result<u16> {
        match self {
            Method::Store => Ok(0),
            Method::Deflate => Ok(8),
            Method::Zstd => Ok(93),
            _ => Err(Error::Unsupported(format!("{} in ZIP", self.name()))),
        }
    }
}

/// A compression level on the 0..=9 scale 7-Zip made familiar: 0 stores,
/// 1 is the fastest real compression and 9 the smallest output. Every
/// backend maps it to its own scale, so the same number means roughly the
/// same trade-off whatever the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(u8);

impl Level {
    pub const MIN: u8 = 0;
    pub const MAX: u8 = 9;
    pub const STORE: Level = Level(0);
    pub const FAST: Level = Level(1);
    pub const NORMAL: Level = Level(6);
    pub const BEST: Level = Level(9);

    /// The level for a number, clamped to the scale.
    pub fn new(n: u8) -> Level {
        Level(n.min(Self::MAX))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    pub fn to_flate2(self) -> u32 {
        u32::from(self.0)
    }

    /// El nivel equivalente en libdeflate, que tiene doce en vez de nueve.
    ///
    /// No son los mismos numeros que zlib aunque lo parezcan: medido sobre 327
    /// MB de datos reales, el 6 de libdeflate iguala en tamano al 9 de zlib y
    /// tarda un tercio. El 12 no tiene equivalente ahi -- es mas pequeno que
    /// cualquier cosa que zlib sepa hacer -- y por eso es el maximo: quien pide
    /// el maximo esta pidiendo tamano y sabe que va a esperar.
    pub fn to_libdeflate(self) -> i32 {
        [0, 1, 2, 3, 4, 5, 6, 8, 10, 12][usize::from(self.0)]
    }

    pub fn to_zstd(self) -> i32 {
        [1, 1, 2, 3, 4, 5, 6, 8, 10, 12][usize::from(self.0)]
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Level {
    type Err = String;

    /// A digit 0-9 or one of the names store, fast, normal, best.
    fn from_str(s: &str) -> std::result::Result<Level, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "store" => Ok(Level::STORE),
            "fast" => Ok(Level::FAST),
            "normal" => Ok(Level::NORMAL),
            "best" => Ok(Level::BEST),
            digits => digits
                .parse::<u8>()
                .ok()
                .filter(|n| *n <= Level::MAX)
                .map(Level)
                .ok_or_else(|| {
                    format!("'{s}' is not a level: use 0-9, store, fast, normal or best")
                }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Store,
    Deflate,
    Zstd,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub method: Method,
    pub crc32: Option<u32>,
    pub is_dir: bool,
    pub mtime: Option<i64>,
    // When the file was made and when it was last read. A zip only carries
    // these if the tool that wrote it bothered with an extra field for them,
    // which most do not, so both are usually nothing.
    pub created: Option<i64>,
    pub accessed: Option<i64>,
    // The DOS attribute byte out of the external attributes: read only,
    // hidden, system, archive. Zero when the archive was made somewhere that
    // has no such thing.
    pub attributes: u8,
    pub offset: u64,
    // The name as the archive spells it, before anybody decided what code page
    // it was written in, and whether the archive said outright that it is
    // UTF-8. Kept because that decision can be taken again: a zip that does not
    // flag its names says nothing about which page they are in.
    pub raw_name: Vec<u8>,
    pub utf8: bool,
    // The entry needs a password. For WinZip AES `method` holds the real
    // compressor, read out of the 0x9901 extra field rather than the method
    // field, which says 99 for every encrypted entry.
    pub encrypted: bool,
    // Only meaningful when `encrypted`: the old PKWARE cipher instead of AES.
    // Arca reads it and never writes it, so this is always false on anything it
    // produces.
    pub zipcrypto: bool,
}

impl Entry {
    pub fn ratio(&self) -> f64 {
        if self.size == 0 {
            return 0.0;
        }
        1.0 - (self.compressed_size as f64 / self.size as f64)
    }
}

pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Result<Self> {
        if pos > data.len() {
            return Err(Error::Format(format!(
                "offset {pos} past end of archive ({} bytes)",
                data.len()
            )));
        }
        Ok(Cursor { data, pos })
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| Error::Format(format!("overflow while reading {what}")))?;
        if end > self.data.len() {
            return Err(Error::Format(format!(
                "archive truncated while reading {what}: {} bytes missing",
                end - self.data.len()
            )));
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn u16le(&mut self, what: &str) -> Result<u16> {
        let b = self.take(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32le(&mut self, what: &str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64le(&mut self, what: &str) -> Result<u64> {
        let b = self.take(8, what)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn bytes(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        self.take(n, what)
    }

    pub fn skip(&mut self, n: usize, what: &str) -> Result<()> {
        self.take(n, what).map(|_| ())
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize()
}

/// The zone this machine is set to, read once. A machine whose zone cannot be
/// read is treated as being on UTC.
fn local_zone() -> &'static jiff::tz::TimeZone {
    static ZONE: std::sync::OnceLock<jiff::tz::TimeZone> = std::sync::OnceLock::new();
    ZONE.get_or_init(|| jiff::tz::TimeZone::try_system().unwrap_or(jiff::tz::TimeZone::UTC))
}

/// An instant as the wall clock here showed it, counted in seconds as if that
/// wall-clock time were UTC: the form `dos_to_unix` and a date on screen use.
pub fn to_local_wall(ts: i64) -> i64 {
    wall_in(ts, local_zone())
}

fn wall_in(ts: i64, zone: &jiff::tz::TimeZone) -> i64 {
    match jiff::Timestamp::from_second(ts) {
        Ok(at) => ts + i64::from(zone.to_offset(at).seconds()),
        Err(_) => ts,
    }
}

/// The instant a wall-clock time here stands for, the other way round from
/// `to_local_wall`. A time that happens twice when the clocks go back is the
/// first; one skipped when they go forward is read as the hour after.
fn instant_in(wall: i64, zone: &jiff::tz::TimeZone) -> i64 {
    let Ok(at) = jiff::Timestamp::from_second(wall) else {
        return wall;
    };
    let civil = jiff::tz::TimeZone::UTC.to_datetime(at);
    zone.to_ambiguous_timestamp(civil)
        .compatible()
        .map_or(wall, |instant| instant.as_second())
}

/// A ZIP's DOS date and time as an instant. They are wall-clock time where
/// the archive was written, which is taken to be the zone this machine is in:
/// it is what every other tool does, and the archive says nothing better.
pub fn dos_to_unix(date: u16, time: u16) -> Option<i64> {
    dos_to_unix_in(date, time, local_zone())
}

fn dos_to_unix_in(date: u16, time: u16, zone: &jiff::tz::TimeZone) -> Option<i64> {
    dos_wall(date, time).map(|wall| instant_in(wall, zone))
}

/// An instant as a DOS date and time, in the wall-clock time of this machine.
pub fn unix_to_dos(ts: i64) -> (u16, u16) {
    unix_to_dos_in(ts, local_zone())
}

fn unix_to_dos_in(ts: i64, zone: &jiff::tz::TimeZone) -> (u16, u16) {
    wall_to_dos(wall_in(ts, zone))
}

fn dos_wall(date: u16, time: u16) -> Option<i64> {
    let year = 1980i64 + ((date >> 9) & 0x7f) as i64;
    let month = ((date >> 5) & 0x0f) as i64;
    let day = (date & 0x1f) as i64;
    let h = ((time >> 11) & 0x1f) as i64;
    let m = ((time >> 5) & 0x3f) as i64;
    let s = ((time & 0x1f) * 2) as i64;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 23 || m > 59 || s > 59 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + m * 60 + s)
}

fn wall_to_dos(ts: i64) -> (u16, u16) {
    let days = ts.div_euclid(86_400);
    let rest = ts.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    let dos_year = (year - 1980).clamp(0, 127);
    let date = ((dos_year as u16) << 9) | ((month as u16) << 5) | day as u16;
    let time = (((rest / 3600) as u16) << 11)
        | ((((rest % 3600) / 60) as u16) << 5)
        | (((rest % 60) / 2) as u16);
    (date, time)
}

pub fn safe_name(name: &str) -> Result<std::path::PathBuf> {
    if name.len() > limits::MAX_NAME {
        return Err(Error::Limit(format!("name of {} bytes", name.len())));
    }
    if name.contains('\0') {
        return Err(Error::Format("name contains a null byte".into()));
    }
    let normalized = name.replace('\\', "/");
    let mut out = std::path::PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                return Err(Error::Format(format!(
                    "entry '{name}' tries to escape the destination directory"
                )))
            }
            p => {
                if p.len() >= 2 && p.as_bytes()[1] == b':' {
                    return Err(Error::Format(format!(
                        "entry '{name}' carries a drive letter"
                    )));
                }
                out.push(p);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(Error::Format(format!("empty name: '{name}'")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_truncates_without_panic() {
        let d = [1u8, 2, 3];
        let mut c = Cursor::new(&d);
        assert!(c.u16le("x").is_ok());
        assert!(matches!(c.u32le("y"), Err(Error::Format(_))));
    }

    #[test]
    fn cursor_rejects_offset_past_end() {
        let d = [0u8; 4];
        assert!(Cursor::at(&d, 99).is_err());
    }

    #[test]
    fn zip_slip_rejected() {
        assert!(safe_name("../../etc/passwd").is_err());
        assert!(safe_name("..\\..\\windows\\system32").is_err());
        assert!(safe_name("C:/tmp/x").is_err());
        assert!(safe_name("a/b/c.txt").is_ok());
        assert_eq!(
            safe_name("./a//b.txt").unwrap(),
            std::path::PathBuf::from("a/b.txt")
        );
    }

    #[test]
    fn dos_dates_round_trip() {
        let ts = 1_710_498_600i64;
        let (f, h) = unix_to_dos(ts);
        let back = dos_to_unix(f, h).unwrap();
        assert!((back - ts).abs() <= 2, "{back} vs {ts}");
    }

    // A DOS time is the wall clock where the archive was written. In Madrid
    // that is an hour ahead of UTC in winter and two in summer, and the same
    // DOS fields read in UTC are that much later.
    #[test]
    fn dos_times_are_wall_clock_time_in_the_local_zone() {
        let madrid = jiff::tz::TimeZone::get("Europe/Madrid").unwrap();
        let utc = jiff::tz::TimeZone::UTC;
        // 2024-01-15 12:00:00 and 2024-07-15 12:00:00 in UTC.
        let (winter, summer) = (1_705_320_000i64, 1_721_044_800i64);
        for (ts, offset) in [(winter, 3600), (summer, 7200)] {
            let (date, time) = unix_to_dos_in(ts, &madrid);
            assert_eq!((date, time), unix_to_dos_in(ts + offset, &utc));
            assert_eq!(dos_to_unix_in(date, time, &madrid), Some(ts));
            assert_eq!(dos_to_unix_in(date, time, &utc), Some(ts + offset));
        }
    }

    #[test]
    fn a_wall_clock_time_that_happens_twice_is_the_first() {
        let madrid = jiff::tz::TimeZone::get("Europe/Madrid").unwrap();
        // 2024-10-27 02:30 happened at 00:30 and again at 01:30 UTC.
        let wall = 1_729_996_200i64;
        assert_eq!(instant_in(wall, &madrid), wall - 7200);
        assert_eq!(wall_in(wall - 7200, &madrid), wall);
        assert_eq!(wall_in(wall - 3600, &madrid), wall);
    }

    #[test]
    fn a_time_out_of_range_is_left_as_it_is() {
        let madrid = jiff::tz::TimeZone::get("Europe/Madrid").unwrap();
        assert_eq!(wall_in(i64::MAX, &madrid), i64::MAX);
        assert_eq!(instant_in(i64::MIN, &madrid), i64::MIN);
        assert_eq!(dos_to_unix_in(0, 0, &madrid), None);
    }
}
