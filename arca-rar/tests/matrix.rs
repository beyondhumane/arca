#![cfg(feature = "rar")]

use arca_core::Error;
use arca_rar::{Conflict, RarArchive};
use std::fs;
use std::path::{Path, PathBuf};

const PASSWORD: &str = "arca-test-only";

#[test]
fn historical_generations_verify_preview_and_extract() {
    for (case, counts) in [("rar13", 2), ("rar15", 1), ("rar20", 1), ("rar20-solid", 2)] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generations")
            .join(case)
            .join("set.rar");
        let archive = RarArchive::open(&path, None).unwrap();
        assert_eq!(archive.entries().len(), counts);
        archive.test(None, &|_, _, _| true).unwrap();
        let dest = tempfile::tempdir().unwrap();
        archive
            .extract(dest.path(), &[], None, &|_, _, _| true, &|_| {
                Conflict::Overwrite
            })
            .unwrap();
        for (index, entry) in archive.entries().iter().enumerate() {
            let contents = fs::read(dest.path().join(&entry.name)).unwrap();
            assert_eq!(contents, archive.read_entry(index, None).unwrap());
            assert_eq!(contents.len() as u64, entry.size);
        }
        if case == "rar13" {
            assert_eq!(
                fs::read(dest.path().join("HELLO.TXT")).unwrap(),
                b"Hello, RAR 1.402 fixture world.\r\n"
            );
            assert_eq!(
                fs::read(dest.path().join("TINY.TXT")).unwrap(),
                b"AAAAAAAA\r\n"
            );
        } else if case == "rar20" {
            assert_eq!(
                fs::read(dest.path().join("PLAIN.TXT")).unwrap(),
                b"Hello text not audio.\r\n".repeat(100)
            );
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/matrix")
}

fn expected(name: &str) -> Vec<u8> {
    match name {
        "first.txt" => b"Arca independent matrix\n".repeat(64),
        "nested/caf\u{e9}-\u{65e5}\u{672c}.txt" => b"Arca Unicode filename\n".repeat(16),
        "empty.txt" => Vec::new(),
        "dictionary.bin" => b"0123456789ABCDEF".repeat(33 * 1024 * 1024 / 16),
        "noise.bin" => {
            let mut state = 0x4152_4341u32;
            (0..8192)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect()
        }
        other => panic!("unexpected member {other}"),
    }
}

#[test]
fn independent_matrix_metadata_preview_verify_and_extract_from_every_volume() {
    let mut sets = 0;
    for case in fs::read_dir(root()).unwrap() {
        let case = case.unwrap().path();
        if !case.is_dir() {
            continue;
        }
        sets += 1;
        let name = case.file_name().unwrap().to_str().unwrap();
        let password = (name.contains("content") || name.contains("headers")).then_some(PASSWORD);
        for volume in fs::read_dir(&case).unwrap() {
            let path = volume.unwrap().path();
            let archive = RarArchive::open(&path, password)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                archive.entries().len(),
                if name == "rar5-dict64" { 1 } else { 4 }
            );
            archive.test(password, &|_, _, _| true).unwrap();
            let dest = tempfile::tempdir().unwrap();
            archive
                .extract(dest.path(), &[], password, &|_, _, _| true, &|_| {
                    Conflict::Overwrite
                })
                .unwrap();
            for (i, entry) in archive.entries().iter().enumerate() {
                let bytes = expected(&entry.name);
                assert!(!entry.is_dir);
                assert_eq!(entry.size, bytes.len() as u64);
                if !entry.encrypted {
                    assert_eq!(entry.crc32, Some(arca_core::crc32(&bytes)));
                }
                assert_eq!(archive.read_entry(i, password).unwrap(), bytes);
                assert_eq!(fs::read(dest.path().join(&entry.name)).unwrap(), bytes);
            }
        }
    }
    assert_eq!(sets, 18);
}

#[test]
fn independent_encryption_preserves_password_categories_and_destinations() {
    for name in [
        "rar5-content",
        "rar5-headers",
        "rar5-content-split",
        "rar5-headers-split",
        "rar4-content",
        "rar4-headers",
        "rar4-legacy-content",
        "rar4-legacy-headers",
    ] {
        let case = root().join(name);
        let path = if case.join("set.rar").exists() {
            case.join("set.rar")
        } else {
            case.join("set.part1.rar")
        };
        for password in [None, Some("wrong")] {
            let dest = tempfile::tempdir().unwrap();
            fs::write(dest.path().join("first.txt"), b"existing").unwrap();
            let error = RarArchive::open(&path, password)
                .and_then(|a| {
                    a.extract(dest.path(), &[], password, &|_, _, _| true, &|_| {
                        Conflict::Overwrite
                    })
                })
                .unwrap_err();
            assert!(
                if password.is_none() {
                    matches!(error, Error::PasswordRequired)
                } else if name.starts_with("rar4-") && name.contains("headers") {
                    // RAR4 headers have no password check; random decrypted lengths can fail parsing first.
                    matches!(error, Error::BadPassword | Error::Format(_))
                } else {
                    matches!(error, Error::BadPassword)
                },
                "{name}: {error}"
            );
            assert_eq!(
                fs::read(dest.path().join("first.txt")).unwrap(),
                b"existing"
            );
            assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 1);
        }
    }
}

#[test]
fn bounded_late_volume_truncation_and_mutation_never_publish_unverified_members() {
    for case in [
        "rar5-solid-split",
        "rar5-content-split",
        "rar5-headers-split",
        "rar4-legacy-content",
    ] {
        let source = root().join(case);
        let password = (case.contains("content") || case.contains("headers")).then_some(PASSWORD);
        let first = if case.starts_with("rar4") {
            "set.rar"
        } else {
            "set.part1.rar"
        };
        let last = if case.starts_with("rar4") {
            "set.r01"
        } else {
            "set.part3.rar"
        };
        let dir = tempfile::tempdir().unwrap();
        for path in fs::read_dir(&source).unwrap() {
            let path = path.unwrap().path();
            fs::copy(&path, dir.path().join(path.file_name().unwrap())).unwrap();
        }
        let bytes = fs::read(source.join(last)).unwrap();
        let stride = (bytes.len() / 32).max(1);
        for at in (0..bytes.len()).step_by(stride) {
            for mutate in [false, true] {
                let mut changed = bytes.clone();
                if mutate {
                    changed[at] ^= 0xff;
                } else {
                    changed.truncate(at);
                }
                fs::write(dir.path().join(last), changed).unwrap();
                let dest = tempfile::tempdir().unwrap();
                fs::write(dest.path().join("marker"), b"untouched").unwrap();
                let result = RarArchive::open(&dir.path().join(first), password).and_then(|a| {
                    a.extract(dest.path(), &[], password, &|_, _, _| true, &|_| {
                        Conflict::Overwrite
                    })
                });
                if result.is_err() {
                    assert_eq!(
                        fs::read_dir(dest.path()).unwrap().count(),
                        1,
                        "{case} at {at}"
                    );
                } else {
                    // Some mutations affect unused padding/metadata, not archive contents.
                    for entry in [
                        "first.txt",
                        "noise.bin",
                        "nested/caf\u{e9}-\u{65e5}\u{672c}.txt",
                        "empty.txt",
                    ] {
                        assert_eq!(
                            fs::read(dest.path().join(entry)).unwrap_or_else(|error| panic!(
                                "{case}, offset {at}, mutate={mutate}, {entry}: {error}"
                            )),
                            expected(entry)
                        );
                    }
                }
                assert_eq!(fs::read(dest.path().join("marker")).unwrap(), b"untouched");
            }
        }
    }
}
