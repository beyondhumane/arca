#![cfg(feature = "rar")]

use arca_core::Error;
use arca_rar::RarArchive;

fn read_vint(data: &[u8], at: &mut usize) -> u64 {
    let mut value = 0;
    for shift in (0..63).step_by(7) {
        let byte = data[*at];
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
    }
    panic!("invalid fixture integer")
}

fn vint(mut n: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while n >= 128 {
        bytes.push(n as u8 | 0x80);
        n >>= 7;
    }
    bytes.push(n as u8);
    bytes
}

fn rewrite_headers(mut edit: impl FnMut(u64, &mut Vec<u8>)) -> Vec<u8> {
    let input = include_bytes!("fixtures/stored.rar");
    let mut output = input[..8].to_vec();
    let mut cursor = 8;
    while cursor < input.len() {
        cursor += 4;
        let size = read_vint(input, &mut cursor) as usize;
        let end = cursor + size;
        let mut head = input[cursor..end].to_vec();
        let mut at = 0;
        let kind = read_vint(&head, &mut at);
        let flags = read_vint(&head, &mut at);
        if flags & 1 != 0 {
            read_vint(&head, &mut at);
        }
        let packed = if flags & 2 != 0 {
            read_vint(&head, &mut at) as usize
        } else {
            0
        };
        edit(kind, &mut head);
        let mut header = vint(head.len() as u64);
        header.extend(head);
        output.extend(arca_core::crc32(&header).to_le_bytes());
        output.extend(header);
        output.extend_from_slice(&input[end..end + packed]);
        cursor = end + packed;
    }
    output
}

fn rename(data: &mut Vec<u8>, old: &str, new: &str) {
    if let Some(at) = data.windows(old.len()).position(|w| w == old.as_bytes()) {
        assert_eq!(data[at - 1] as usize, old.len());
        assert!(new.len() < 128);
        data[at - 1] = new.len() as u8;
        data.splice(at..at + old.len(), new.bytes());
    }
}

fn opening(data: &[u8]) -> Result<RarArchive, Error> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crafted.rar");
    std::fs::write(&path, data).unwrap();
    RarArchive::open(&path, None)
}

#[test]
fn rejects_archive_paths_instead_of_sanitizing_them() {
    for name in [
        "../escape.txt",
        "/absolute.txt",
        "C:/drive.txt",
        "a:stream",
        "CON.txt",
        "nul",
        "a\\..\\evil",
        "bad\0name",
    ] {
        let data = rewrite_headers(|kind, header| {
            if kind == 2 {
                rename(header, "first.txt", name);
            }
        });
        assert!(matches!(opening(&data), Err(Error::Format(_))), "{name}");
    }
}

#[test]
fn rejects_duplicate_names_and_file_directory_collisions() {
    for name in ["first.txt", "FIRST.TXT", "first.txt/child"] {
        let data = rewrite_headers(|kind, header| {
            if kind == 2 {
                rename(header, "folder/second.txt", name);
            }
        });
        assert!(matches!(opening(&data), Err(Error::Format(_))), "{name}");
    }
}

#[test]
fn rejects_bombs_from_declared_output_before_decoding() {
    let data = rewrite_headers(|kind, header| {
        if kind != 2 {
            return;
        }
        let mut at = 0;
        read_vint(header, &mut at);
        let flags = read_vint(header, &mut at);
        if flags & 1 != 0 {
            read_vint(header, &mut at);
        }
        if flags & 2 != 0 {
            read_vint(header, &mut at);
        }
        let file_flags = read_vint(header, &mut at);
        if file_flags & 1 != 0 {
            return;
        }
        let start = at;
        read_vint(header, &mut at);
        header.splice(start..at, vint(5 * 1024 * 1024 * 1024));
    });
    assert!(matches!(opening(&data), Err(Error::Limit(_))));
}

#[test]
fn arbitrary_and_mutated_input_never_panics() {
    let data = include_bytes!("fixtures/plain.rar");
    for at in 0..data.len() {
        let mut mutated = data.to_vec();
        mutated[at] ^= 0xff;
        if let Ok(a) = opening(&mutated) {
            let _ = a.test(None, &|_, _, _| true);
        }
    }
    for size in [0, 1, 16, 64, 1024] {
        assert!(opening(&vec![0xff; size]).is_err());
    }
}

#[test]
fn rejects_unix_special_files_from_archive_attributes() {
    for mode in [0o120777u64, 0o010644, 0o020644, 0o060644, 0o140644] {
        let data = rewrite_headers(|kind, header| {
            if kind != 2 {
                return;
            }
            let mut at = 0;
            read_vint(header, &mut at);
            let flags = read_vint(header, &mut at);
            if flags & 1 != 0 {
                read_vint(header, &mut at);
            }
            if flags & 2 != 0 {
                read_vint(header, &mut at);
            }
            let file_flags = read_vint(header, &mut at);
            if file_flags & 1 != 0 {
                return;
            }
            read_vint(header, &mut at);
            let start = at;
            read_vint(header, &mut at);
            header.splice(start..at, vint(mode));
        });
        assert!(
            matches!(opening(&data), Err(Error::Unsupported(_))),
            "{mode:o}"
        );
    }
}

#[test]
fn missing_optional_crc32_is_not_invented_as_zero() {
    let data = rewrite_headers(|kind, header| {
        if kind != 2 {
            return;
        }
        let mut at = 0;
        read_vint(header, &mut at);
        let flags = read_vint(header, &mut at);
        if flags & 1 != 0 {
            read_vint(header, &mut at);
        }
        if flags & 2 != 0 {
            read_vint(header, &mut at);
        }
        let flag_at = at;
        let file_flags = read_vint(header, &mut at);
        if file_flags & 4 == 0 {
            return;
        }
        assert!(file_flags < 128);
        header[flag_at] &= !4;
        read_vint(header, &mut at);
        read_vint(header, &mut at);
        if file_flags & 2 != 0 {
            at += 4;
        }
        header.drain(at..at + 4);
    });
    let archive = opening(&data).unwrap();
    assert!(archive.entries().iter().all(|e| e.crc32.is_none()));
    archive.test(None, &|_, _, _| true).unwrap();
    let index = archive
        .entries()
        .iter()
        .position(|e| e.name == "first.txt")
        .unwrap();
    assert_eq!(
        archive.read_entry(index, None).unwrap(),
        b"Arca RAR fixture alpha\n".repeat(64)
    );
}

#[test]
fn dictionary_and_preview_limits_fail_before_publication() {
    for limit in ["dictionary", "preview"] {
        let data = rewrite_headers(|kind, header| {
            if kind != 2 {
                return;
            }
            let mut at = 0;
            read_vint(header, &mut at);
            let flags = read_vint(header, &mut at);
            if flags & 1 != 0 {
                read_vint(header, &mut at);
            }
            if flags & 2 != 0 {
                read_vint(header, &mut at);
            }
            let flags = read_vint(header, &mut at);
            if flags & 1 != 0 {
                return;
            }
            let size_at = at;
            read_vint(header, &mut at);
            if limit == "preview" {
                header.splice(size_at..at, vint(65 * 1024 * 1024));
                return;
            }
            read_vint(header, &mut at);
            if flags & 2 != 0 {
                at += 4;
            }
            if flags & 4 != 0 {
                at += 4;
            }
            let start = at;
            read_vint(header, &mut at);
            // A valid compressed-method claim with a 512 MiB dictionary (limit: 256 MiB).
            header.splice(start..at, vint((12 << 10) | (3 << 7)));
        });
        let archive = opening(&data).unwrap();
        if limit == "preview" {
            assert!(matches!(archive.read_entry(0, None), Err(Error::Limit(_))));
        } else {
            let out = tempfile::tempdir().unwrap();
            let dest = out.path().join("out");
            let error = archive
                .extract(&dest, &[], None, &|_, _, _| true, &|_| {
                    arca_rar::Conflict::Overwrite
                })
                .unwrap_err();
            assert!(matches!(error, Error::Limit(_)), "{error}");
            assert!(!dest.exists());
        }
    }
}
