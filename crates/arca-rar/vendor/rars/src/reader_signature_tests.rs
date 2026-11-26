use super::*;
use std::io::{self, Cursor, Seek, SeekFrom};

struct Observed {
    input: Cursor<Vec<u8>>,
    bytes: usize,
    refuse_after: Option<usize>,
}

impl Read for Observed {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.refuse_after.is_some_and(|end| self.bytes >= end) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let count = output.len().min(3);
        let count = self.input.read(&mut output[..count])?;
        self.bytes += count;
        Ok(count)
    }
}

#[test]
fn strong_standard_signatures_read_only_the_fixed_prefix() {
    for signature in [detect::RAR15_SIGNATURE.as_slice(), detect::RAR50_SIGNATURE] {
        let mut data = vec![42; 2 * 1024 * 1024];
        data[..signature.len()].copy_from_slice(signature);
        let mut source = Observed {
            input: Cursor::new(data),
            bytes: 0,
            refuse_after: Some(8),
        };
        let actual = read_archive_signature(&mut source, 2 * 1024 * 1024)
            .unwrap()
            .unwrap();
        assert_eq!(actual, detect_archive_family(signature).unwrap());
        assert_eq!(source.bytes, 8);
    }
}

#[test]
fn signature_prefix_retains_full_scan_precedence_and_short_inputs() {
    for bytes in [
        b"".as_slice(),
        b"Rar!\x1a\x07",
        b"Rar!\x1a\x07\x00",
        b"Rar!\x1a\x07\x01\x00",
        b"RE~^",
        b"RE~^payload Rar!\x1a\x07\x01\x00later",
        b"RE~^payload Rar!\x1a\x07\x00later",
        b"stub Rar!\x1a\x07\x01\x00later",
        b"stub RE~^payload",
        b"stub RE~^ Rar!\x1a\x07\x00payload",
        b"not an archive",
    ] {
        for len in 0..=bytes.len() {
            let expected = find_archive_start(&bytes[..len], SFX_SCAN_LIMIT);
            let actual = read_archive_signature(&mut &bytes[..len], len);
            assert_eq!(actual.unwrap(), expected);
        }
    }
}

#[test]
fn sfx_prefix_fallback_keeps_existing_scan_length_and_io_errors() {
    let mut bytes = vec![0; 300 * 1024];
    bytes.extend_from_slice(detect::RAR50_SIGNATURE);
    let mut source = Observed {
        input: Cursor::new(bytes.clone()),
        bytes: 0,
        refuse_after: None,
    };
    assert_eq!(
        read_archive_signature(&mut source, bytes.len()).unwrap(),
        find_archive_start(&bytes, SFX_SCAN_LIMIT)
    );
    assert_eq!(source.bytes, bytes.len());
    source.input.set_position(0);
    source.bytes = 0;
    source.refuse_after = Some(8);
    assert!(matches!(
        read_archive_signature(&mut source, bytes.len()),
        Err(Error::Io(_))
    ));
}

#[test]
fn reader_prefix_retains_cancellation_io_error_and_eof_ordering() {
    struct Callback {
        input: Cursor<Vec<u8>>,
        token: ReadCancellation,
        fail: bool,
        advertised_len: u64,
    }
    impl Read for Callback {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = output.len().min(3);
            let count = self.input.read(&mut output[..count])?;
            if self.input.position() == 8 {
                self.token.cancel();
                if self.fail {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
            }
            Ok(count)
        }
    }
    impl Seek for Callback {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            match position {
                SeekFrom::End(0) => self.input.seek(SeekFrom::Start(self.advertised_len)),
                position => self.input.seek(position),
            }
        }
    }
    for fail in [false, true] {
        let token = ReadCancellation::new();
        let reader = Callback {
            input: Cursor::new(detect::RAR50_SIGNATURE.to_vec()),
            token: token.clone(),
            fail,
            advertised_len: 32,
        };
        let error = ArchiveReader::read_reader_with_options(
            reader,
            ArchiveReadOptions::new().with_cancellation(&token),
        )
        .unwrap_err();
        assert!(token.is_cancelled());
        assert_eq!(
            error.kind(),
            if fail {
                ErrorKind::Io
            } else {
                ErrorKind::Cancelled
            }
        );
        if fail {
            assert!(
                matches!(error, Error::Io(ref source) if source.kind == io::ErrorKind::PermissionDenied)
            );
        }
    }
    let reader = Callback {
        input: Cursor::new(b"Rar!".to_vec()),
        token: ReadCancellation::new(),
        fail: false,
        advertised_len: 32,
    };
    let error = ArchiveReader::read_reader(reader).unwrap_err();
    assert!(matches!(error, Error::Io(ref source) if source.kind == io::ErrorKind::UnexpectedEof));
}
