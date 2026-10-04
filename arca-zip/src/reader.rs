use std::io::{self, BufReader, Read, Seek, SeekFrom};

pub struct ExtractionReader<R> {
    inner: BufReader<R>,
    position: Option<u64>,
}

impl<R: Read + Seek> ExtractionReader<R> {
    pub fn with_capacity(capacity: usize, mut source: R) -> io::Result<Self> {
        let position = source.stream_position()?;
        Ok(Self {
            inner: BufReader::with_capacity(capacity, source),
            position: Some(position),
        })
    }
}

impl<R: Read> Read for ExtractionReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let result = self.inner.read(buf);
        match &result {
            Ok(n) => self.position = self.position.and_then(|p| p.checked_add(*n as u64)),
            Err(_) => self.position = None,
        }
        result
    }
}

impl<R: Read + Seek> Seek for ExtractionReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let current = self.stream_position()?;
        let target = match pos {
            SeekFrom::Start(target) => Some(target),
            SeekFrom::Current(delta) => current.checked_add_signed(delta),
            SeekFrom::End(_) => None,
        };
        // A failed seek may have moved the underlying cursor before failing.
        self.position = None;
        let result = if let Some((target, delta)) = target.and_then(|target| {
            i64::try_from(i128::from(target) - i128::from(current))
                .ok()
                .map(|delta| (target, delta))
        }) {
            self.inner.seek_relative(delta).map(|()| target)
        } else {
            self.inner.seek(pos)
        };
        if let Ok(position) = result {
            self.position = Some(position);
        }
        result
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        if let Some(position) = self.position {
            return Ok(position);
        }
        let position = self.inner.stream_position()?;
        self.position = Some(position);
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Counted {
        source: Cursor<Vec<u8>>,
        reads: usize,
        seeks: usize,
    }

    impl Read for Counted {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            self.source.read(buf)
        }
    }

    impl Seek for Counted {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.seeks += 1;
            self.source.seek(pos)
        }
    }

    #[test]
    fn preserves_forward_and_backward_seeks_inside_read_ahead() {
        let source = Counted {
            source: Cursor::new((0..100).collect()),
            reads: 0,
            seeks: 0,
        };
        let mut reader = ExtractionReader::with_capacity(32, source).unwrap();
        let mut byte = [0];
        reader.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [0]);
        for offset in [20, 4, 31, 0] {
            assert_eq!(reader.seek(SeekFrom::Start(offset)).unwrap(), offset);
            reader.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [offset as u8]);
        }
        assert_eq!(reader.inner.get_ref().reads, 1);
        assert_eq!(reader.inner.get_ref().seeks, 1);
        reader.seek(SeekFrom::Start(80)).unwrap();
        reader.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [80]);
        assert_eq!(reader.inner.get_ref().reads, 2);
        reader.seek(SeekFrom::Start(40)).unwrap();
        reader.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [40]);
        assert_eq!(reader.inner.get_ref().reads, 3);
    }

    #[test]
    fn handles_nonzero_positions_end_seeks_eof_and_failed_seeks() {
        let mut source = Cursor::new((0..100).collect::<Vec<u8>>());
        source.set_position(10);
        let mut reader = ExtractionReader::with_capacity(16, source).unwrap();
        let mut bytes = [0; 40];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, (10..50).collect::<Vec<_>>().as_slice());
        assert_eq!(reader.stream_position().unwrap(), 50);
        assert_eq!(reader.seek(SeekFrom::Current(-5)).unwrap(), 45);
        assert_eq!(reader.seek(SeekFrom::End(-3)).unwrap(), 97);
        assert_eq!(reader.read(&mut bytes).unwrap(), 3);
        assert_eq!(reader.read(&mut bytes).unwrap(), 0);
        assert_eq!(reader.stream_position().unwrap(), 100);
        assert!(reader.seek(SeekFrom::Current(-101)).is_err());
        reader.seek(SeekFrom::Start(2)).unwrap();
        reader.read_exact(&mut bytes[..1]).unwrap();
        assert_eq!(bytes[0], 2);
        assert_eq!(reader.seek(SeekFrom::Start(u64::MAX)).unwrap(), u64::MAX);
        assert!(reader.seek(SeekFrom::Current(1)).is_err());
        reader.seek(SeekFrom::Start(0)).unwrap();
        reader.read_exact(&mut bytes[..1]).unwrap();
        assert_eq!(bytes[0], 0);
    }
}
