//! Self-extracting archives: a small program with a ZIP appended after it
//! and, at the very end, a fixed trailer that says where that ZIP starts and
//! how long it is. The program finds its archive by reading its own file
//! backwards from the end, so nothing about the program has to change. The
//! ZIP counts its offsets from the start of the file, as `zip -A` leaves a
//! self-extractor, so standard ZIP tools read the whole program as one.

use crate::volumes::moved;
use crate::{Error, Result};
use std::io::{self, Read, Seek, SeekFrom, Write};

/// The last eight bytes of a self-extractor.
pub const MAGIC: [u8; 8] = *b"ArcaSFX1";

/// Offset, length, flags and the magic number, in that order.
pub const TRAILER_LEN: u64 = 28;

/// The program talks through the console instead of opening windows.
pub const CONSOLE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trailer {
    pub offset: u64,
    pub len: u64,
    pub flags: u32,
}

fn not_attached() -> Error {
    Error::Format("no archive is attached to this program".into())
}

impl Trailer {
    pub fn to_bytes(self) -> [u8; TRAILER_LEN as usize] {
        let mut b = [0u8; TRAILER_LEN as usize];
        b[0..8].copy_from_slice(&self.offset.to_le_bytes());
        b[8..16].copy_from_slice(&self.len.to_le_bytes());
        b[16..20].copy_from_slice(&self.flags.to_le_bytes());
        b[20..28].copy_from_slice(&MAGIC);
        b
    }

    /// The trailer at the end of `file`, checked against the file's length:
    /// the archive it points at has to end exactly where the trailer begins.
    pub fn read<R: Read + Seek>(file: &mut R) -> Result<Self> {
        let total = file.seek(SeekFrom::End(0))?;
        if total < TRAILER_LEN {
            return Err(not_attached());
        }
        file.seek(SeekFrom::Start(total - TRAILER_LEN))?;
        let mut b = [0u8; TRAILER_LEN as usize];
        file.read_exact(&mut b)?;
        if b[20..28] != MAGIC {
            return Err(not_attached());
        }
        let word = |at: usize| {
            let mut w = [0u8; 8];
            w.copy_from_slice(&b[at..at + 8]);
            u64::from_le_bytes(w)
        };
        let mut f = [0u8; 4];
        f.copy_from_slice(&b[16..20]);
        let trailer = Self {
            offset: word(0),
            len: word(8),
            flags: u32::from_le_bytes(f),
        };
        if trailer.flags & !CONSOLE != 0 {
            return Err(Error::Unsupported(
                "this self-extractor was made by a newer Arca".into(),
            ));
        }
        let end = trailer
            .offset
            .checked_add(trailer.len)
            .and_then(|end| end.checked_add(TRAILER_LEN));
        if end != Some(total) {
            return Err(Error::Format(
                "the archive attached to this program is damaged or truncated".into(),
            ));
        }
        Ok(trailer)
    }

    pub fn console(self) -> bool {
        self.flags & CONSOLE != 0
    }
}

/// `len` bytes of `inner` from `start`, read as if they were a whole file.
pub struct Window<R> {
    inner: R,
    start: u64,
    len: u64,
    pos: u64,
    synced: bool,
}

impl<R: Read + Seek> Window<R> {
    pub fn new(inner: R, start: u64, len: u64) -> Self {
        Self {
            inner,
            start,
            len,
            pos: 0,
            synced: false,
        }
    }

    /// The archive a self-extractor carries, which is the whole file up to its
    /// trailer: the program in front is data the archive skips, and its
    /// offsets count from the start of the file.
    pub fn payload(mut file: R) -> Result<Self> {
        let trailer = Trailer::read(&mut file)?;
        Ok(Self::new(file, 0, trailer.offset + trailer.len))
    }
}

impl<R: Read + Seek> Read for Window<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        if !self.synced {
            let at = self.start.checked_add(self.pos).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "position out of range")
            })?;
            self.inner.seek(SeekFrom::Start(at))?;
            self.synced = true;
        }
        let want = (self.len - self.pos).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for Window<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::End(by) => moved(self.len, by)?,
            SeekFrom::Current(by) => moved(self.pos, by)?,
        };
        if pos != self.pos {
            self.synced = false;
        }
        self.pos = pos;
        Ok(pos)
    }
}

/// Writes a self-extractor into `out`: the program `stub`, the archive that
/// `payload` writes after it, and the trailer. `payload` gets `out` standing
/// right after the program, and the archive it writes there should count its
/// offsets from the start of the file, as `zip -A` leaves them, so that
/// standard tools read the whole program as that archive.
pub fn build<W, F>(mut out: W, stub: &[u8], flags: u32, payload: F) -> Result<W>
where
    W: Write + Seek,
    F: FnOnce(W) -> Result<W>,
{
    out.write_all(stub)?;
    let base = out.stream_position()?;
    let mut out = payload(out)?;
    let end = out.seek(SeekFrom::End(0))?;
    let len = end
        .checked_sub(base)
        .ok_or_else(|| Error::Format("the archive ended before it began".into()))?;
    out.write_all(
        &Trailer {
            offset: base,
            len,
            flags,
        }
        .to_bytes(),
    )?;
    Ok(out)
}

fn not_a_program() -> Error {
    Error::Format("the self-extractor stub is not a Windows program".into())
}

fn field<const N: usize>(exe: &[u8], at: usize) -> Result<[u8; N]> {
    let end = at.checked_add(N).ok_or_else(not_a_program)?;
    let mut b = [0u8; N];
    b.copy_from_slice(exe.get(at..end).ok_or_else(not_a_program)?);
    Ok(b)
}

/// Marks a Windows program as a console one, by its PE header's subsystem
/// field, which sits at the same place in 32 and 64-bit optional headers.
/// Anything but a well-formed windowed or console program is refused.
pub fn console_subsystem(exe: &mut [u8]) -> Result<()> {
    if field::<2>(exe, 0)? != *b"MZ" {
        return Err(not_a_program());
    }
    let pe = u32::from_le_bytes(field::<4>(exe, 0x3c)?) as usize;
    if field::<4>(exe, pe)? != *b"PE\0\0" {
        return Err(not_a_program());
    }
    let optional = pe.checked_add(24).ok_or_else(not_a_program)?;
    if !matches!(
        u16::from_le_bytes(field::<2>(exe, optional)?),
        0x10b | 0x20b
    ) {
        return Err(not_a_program());
    }
    let at = optional.checked_add(68).ok_or_else(not_a_program)?;
    if !matches!(u16::from_le_bytes(field::<2>(exe, at)?), 2 | 3) {
        return Err(not_a_program());
    }
    exe[at..at + 2].copy_from_slice(&3u16.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn made(stub: &[u8], payload: &[u8], flags: u32) -> Vec<u8> {
        build(Cursor::new(Vec::new()), stub, flags, |mut w| {
            w.write_all(payload)?;
            Ok(w)
        })
        .unwrap()
        .into_inner()
    }

    #[test]
    fn the_trailer_points_at_the_payload() {
        let file = made(b"program", b"archive bytes", CONSOLE);
        assert_eq!(file.len() as u64, 7 + 13 + TRAILER_LEN);
        let mut cursor = Cursor::new(file);
        let trailer = Trailer::read(&mut cursor).unwrap();
        assert_eq!((trailer.offset, trailer.len), (7, 13));
        assert!(trailer.console());
        let mut window = Window::payload(cursor).unwrap();
        let mut v = Vec::new();
        window.read_to_end(&mut v).unwrap();
        assert_eq!(v, b"programarchive bytes");
        window.seek(SeekFrom::End(-5)).unwrap();
        v.clear();
        window.read_to_end(&mut v).unwrap();
        assert_eq!(v, b"bytes");
        window.seek(SeekFrom::Start(100)).unwrap();
        assert_eq!(window.read(&mut [0u8; 4]).unwrap(), 0);
        assert!(window.seek(SeekFrom::Current(-1000)).is_err());
        assert!(window.seek(SeekFrom::End(-21)).is_err());
    }

    #[test]
    fn the_payload_is_written_at_the_positions_of_the_file() {
        let file = build(Cursor::new(Vec::new()), b"stub", 0, |mut w| {
            assert_eq!(w.stream_position()?, 4);
            w.write_all(b"abcdef")?;
            assert_eq!(w.seek(SeekFrom::Start(5))?, 5);
            w.write_all(b"X")?;
            assert_eq!(w.seek(SeekFrom::End(0))?, 10);
            w.seek(SeekFrom::End(0))?;
            Ok(w)
        })
        .unwrap()
        .into_inner();
        assert_eq!(&file[..10], b"stubaXcdef");
        let trailer = Trailer::read(&mut Cursor::new(file)).unwrap();
        assert_eq!((trailer.offset, trailer.len), (4, 6));
    }

    #[test]
    fn a_program_without_a_payload_says_so() {
        for bytes in [&b""[..], b"short", &[0u8; 100]] {
            let error = Trailer::read(&mut Cursor::new(bytes)).unwrap_err();
            assert!(error.to_string().contains("no archive"), "{error}");
        }
    }

    #[test]
    fn every_damaged_or_truncated_trailer_is_an_error() {
        let file = made(b"program", b"archive", 0);
        for cut in 0..file.len() {
            assert!(
                Trailer::read(&mut Cursor::new(&file[..cut])).is_err(),
                "{cut}"
            );
            if cut > 0 {
                assert!(
                    Trailer::read(&mut Cursor::new(&file[cut..])).is_err(),
                    "{cut}"
                );
            }
        }
        for at in 0..20 {
            for value in [0x00, 0x80, 0xff] {
                let mut bad = file.clone();
                let start = bad.len() - TRAILER_LEN as usize;
                if bad[start + at] == value {
                    continue;
                }
                bad[start + at] = value;
                assert!(
                    Trailer::read(&mut Cursor::new(&bad)).is_err(),
                    "{at} {value}"
                );
            }
        }
        let mut huge = made(b"", b"", 0);
        let start = huge.len() - TRAILER_LEN as usize;
        huge[start..start + 16].copy_from_slice(&[0xff; 16]);
        assert!(Trailer::read(&mut Cursor::new(&huge)).is_err());
    }

    fn pe(magic: u16, subsystem: u16) -> Vec<u8> {
        let mut exe = vec![0u8; 0x200];
        exe[0..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        exe[0x98..0x9a].copy_from_slice(&magic.to_le_bytes());
        exe[0x98 + 68..0x98 + 70].copy_from_slice(&subsystem.to_le_bytes());
        exe
    }

    #[test]
    fn a_windowed_program_becomes_a_console_one() {
        for magic in [0x10b, 0x20b] {
            let mut exe = pe(magic, 2);
            console_subsystem(&mut exe).unwrap();
            assert_eq!(&exe[0x98 + 68..0x98 + 70], &[3, 0]);
            assert_eq!(exe, pe(magic, 3));
            console_subsystem(&mut exe).unwrap();
        }
    }

    #[test]
    fn anything_but_a_program_is_refused_without_panicking() {
        let good = pe(0x20b, 2);
        for cut in 0..0x98 + 70 {
            assert!(
                console_subsystem(&mut good[..cut].to_vec()).is_err(),
                "{cut}"
            );
        }
        let mut wrong = good.clone();
        wrong[0] = b'Z';
        assert!(console_subsystem(&mut wrong).is_err());
        let mut wrong = good.clone();
        wrong[0x81] = b'X';
        assert!(console_subsystem(&mut wrong).is_err());
        assert!(console_subsystem(&mut pe(0x107, 2)).is_err());
        assert!(console_subsystem(&mut pe(0x20b, 10)).is_err());
        let mut far = good.clone();
        far[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(console_subsystem(&mut far).is_err());
        let mut near_end = good;
        near_end[0x3c..0x40].copy_from_slice(&0x1f0u32.to_le_bytes());
        assert!(console_subsystem(&mut near_end).is_err());
    }
}
