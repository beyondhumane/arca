#![cfg(feature = "rar")]

use arca_core::Error;
use arca_rar::{Conflict, RarArchive};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn copy_set(dir: &Path, names: &[&str]) {
    for (i, name) in names.iter().enumerate() {
        fs::copy(
            fixture(&format!("volume.part{}.rar", i + 1)),
            dir.join(name),
        )
        .unwrap();
    }
}

const NAMES: [&str; 4] = [
    "volume.part1.rar",
    "volume.part2.rar",
    "volume.part3.rar",
    "volume.part4.rar",
];

#[test]
fn standalone_metadata_does_not_discover_volume_like_names_or_unrelated_siblings() {
    for source in ["plain.rar", "headers.rar"] {
        for name in [
            "book.rar",
            "book.part10.rar",
            "book.part00.rar",
            "book.part999999999999999999999.rar",
            "book.r00",
        ] {
            let dir = tempfile::tempdir().unwrap();
            for sibling in ["book.rar", "book.r00", "book.part01.rar", "book.part1.rar"] {
                fs::write(dir.path().join(sibling), b"unrelated").unwrap();
            }
            let selected = dir.path().join(name);
            fs::copy(fixture(source), &selected).unwrap();
            let password = (source == "headers.rar").then_some("arca-test-only");
            let archive = RarArchive::open(&selected, password).unwrap();
            assert_eq!(archive.entries().len(), 4);
            archive.test(password, &|_, _, _| true).unwrap();
            let out = dir.path().join("out");
            archive
                .extract(&out, &[], password, &|_, _, _| true, &|_| {
                    Conflict::Overwrite
                })
                .unwrap();
            assert_eq!(
                fs::read(out.join("first.txt")).unwrap(),
                b"Arca RAR fixture alpha\n".repeat(64)
            );
        }
    }
}

fn failure_leaves_destination_untouched(path: &Path, affected: &str) {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out");
    for existing in [false, true] {
        if existing {
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("first.txt"), b"keep me").unwrap();
        }
        let error = RarArchive::open(path, None)
            .and_then(|a| a.extract(&dest, &[], None, &|_, _, _| true, &|_| Conflict::Overwrite))
            .expect_err("damaged set accepted");
        assert!(error.to_string().contains(affected), "{error}");
        if existing {
            assert_eq!(fs::read(dest.join("first.txt")).unwrap(), b"keep me");
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
        } else {
            assert!(!dest.exists());
        }
    }
}

#[test]
fn discovers_padded_case_insensitive_and_legacy_names_from_any_part() {
    for names in [
        [
            "Volume.part01.RAR",
            "volume.part02.rar",
            "volume.part03.rar",
            "volume.part04.rar",
        ],
        ["volume.RAR", "volume.r00", "volume.R01", "volume.r02"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        copy_set(dir.path(), &names);
        for name in names {
            let a = RarArchive::open(&dir.path().join(name), None).unwrap();
            assert_eq!(a.entries().len(), 4);
            a.test(None, &|_, _, _| true).unwrap();
        }
    }
}

#[test]
fn missing_volumes_are_named_and_other_directories_are_not_searched() {
    for missing in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        copy_set(dir.path(), &NAMES);
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::rename(
            dir.path().join(NAMES[missing]),
            dir.path().join("nested").join(NAMES[missing]),
        )
        .unwrap();
        failure_leaves_destination_untouched(
            &dir.path().join(NAMES[if missing == 3 { 0 } else { 3 }]),
            NAMES[missing],
        );
    }
}

#[test]
fn rejects_wrong_duplicate_out_of_order_and_corrupt_headers() {
    for case in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        copy_set(dir.path(), &NAMES);
        let path = dir.path().join(NAMES[1]);
        match case {
            0 => {
                fs::copy(fixture("plain.rar"), &path).unwrap();
            }
            1 => {
                fs::copy(fixture(NAMES[0]), &path).unwrap();
            }
            2 => {
                fs::copy(fixture(NAMES[2]), &path).unwrap();
            }
            3 => {
                fs::copy(&path, dir.path().join("volume.part02.rar")).unwrap();
            }
            _ => {
                let mut data = fs::read(&path).unwrap();
                data[8] ^= 1;
                fs::write(&path, data).unwrap();
            }
        }
        failure_leaves_destination_untouched(&dir.path().join(NAMES[3]), NAMES[1]);
    }
}

#[test]
fn every_later_volume_truncation_fails_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    copy_set(dir.path(), &NAMES);
    let out = dir.path().join("out");
    for name in &NAMES[1..] {
        let path = dir.path().join(name);
        let data = fs::read(&path).unwrap();
        let parsed = rars::rar50::Archive::parse(&data).unwrap();
        let Some(rars::rar50::Block::End(end)) = parsed.blocks.last() else {
            panic!("fixture has no end")
        };
        let required = end.block.header_range.end;
        // Volume padding after the end header is optional, not archive data.
        for n in 0..required {
            fs::write(&path, &data[..n]).unwrap();
            let error = RarArchive::open(&dir.path().join(NAMES[0]), None)
                .and_then(|a| a.extract(&out, &[], None, &|_, _, _| true, &|_| Conflict::Overwrite))
                .err()
                .unwrap_or_else(|| {
                    panic!("truncated set accepted: {name} prefix {n}/{}", data.len())
                });
            assert!(error.to_string().contains(name), "prefix {n}: {error}");
            assert!(!out.exists());
        }
        fs::write(&path, &data[..required]).unwrap();
        RarArchive::open(&dir.path().join(NAMES[0]), None)
            .unwrap()
            .test(None, &|_, _, _| true)
            .unwrap();
        fs::write(path, data).unwrap();
    }
}

#[test]
fn corrupt_later_payload_keeps_prior_members_staged() {
    for name in &NAMES[1..] {
        let dir = tempfile::tempdir().unwrap();
        copy_set(dir.path(), &NAMES);
        let path = dir.path().join(name);
        let mut data = fs::read(&path).unwrap();
        let at = data.windows(4).rposition(|bytes| bytes == b"Arca").unwrap();
        data[at] ^= 1;
        fs::write(&path, data).unwrap();
        let a = RarArchive::open(&dir.path().join(NAMES[0]), None).unwrap();
        assert!(a.test(None, &|_, _, _| true).is_err());
        failure_leaves_destination_untouched(&dir.path().join(NAMES[0]), name);
    }
}

#[test]
fn cancellation_spans_discovery_parsing_and_staging() {
    for phase in ["RAR volume discovery", NAMES[1]] {
        assert!(matches!(
            RarArchive::open_with_progress(&fixture(NAMES[3]), None, &|_, _, name| !name
                .contains(phase)),
            Err(Error::Cancelled)
        ));
    }
    let a = RarArchive::open(&fixture(NAMES[3]), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out");
    assert!(matches!(
        a.extract(
            &dest,
            &[],
            None,
            &|_, _, name| name != "folder/second.txt",
            &|_| Conflict::Overwrite
        ),
        Err(Error::Cancelled)
    ));
    assert!(!dest.exists());
}

#[cfg(unix)]
#[test]
fn sibling_symlinks_cannot_redirect_discovery_outside_the_selected_directory() {
    let dir = tempfile::tempdir().unwrap();
    copy_set(dir.path(), &NAMES);
    fs::remove_file(dir.path().join(NAMES[1])).unwrap();
    std::os::unix::fs::symlink(fixture(NAMES[1]), dir.path().join(NAMES[1])).unwrap();
    failure_leaves_destination_untouched(&dir.path().join(NAMES[0]), NAMES[1]);
}

#[cfg(unix)]
#[test]
fn selected_symlinks_are_rejected_before_parsing_or_discovery() {
    for target in ["plain.rar", NAMES[3]] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("selected.part4.rar");
        std::os::unix::fs::symlink(fixture(target), &path).unwrap();
        failure_leaves_destination_untouched(&path, "selected.part4.rar");
    }
}

#[cfg(windows)]
#[test]
#[ignore = "requires Windows symbolic-link permission; run on a developer-mode or elevated host"]
fn windows_reparse_inputs_cannot_redirect_selected_or_discovered_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let selected = dir.path().join("selected.rar");
    std::os::windows::fs::symlink_file(fixture("plain.rar"), &selected).unwrap();
    failure_leaves_destination_untouched(&selected, "selected.rar");
    copy_set(dir.path(), &NAMES);
    fs::remove_file(dir.path().join(NAMES[1])).unwrap();
    std::os::windows::fs::symlink_file(fixture(NAMES[1]), dir.path().join(NAMES[1])).unwrap();
    failure_leaves_destination_untouched(&dir.path().join(NAMES[3]), NAMES[1]);
}

#[test]
fn solid_split_selection_preserves_decoder_history() {
    let a = RarArchive::open(&fixture("solid_multivol.part06.rar"), None).unwrap();
    assert_eq!(a.entries().len(), 2);
    a.test(None, &|_, _, _| true).unwrap();
    assert_eq!(a.read_entry(0, None).unwrap(), b"AAAAAAAA\n");
    let bytes = a.read_entry(1, None).unwrap();
    assert_eq!(bytes.len(), 65_536);
    assert_eq!(arca_core::crc32(&bytes), 0xddc9_5682);
    let dir = tempfile::tempdir().unwrap();
    a.extract(dir.path(), &[false, true], None, &|_, _, _| true, &|_| {
        Conflict::Overwrite
    })
    .unwrap();
    assert_eq!(fs::read(dir.path().join("bigtext_64k.bin")).unwrap(), bytes);
    assert!(!dir.path().join("tiny.txt").exists());
}

#[test]
fn encrypted_split_streams_preserve_password_errors_and_staging() {
    for name in [
        "encrypted_multivol.part3.rar",
        "header_encrypted_stored_multivol.part3.rar",
    ] {
        let path = fixture(name);
        let a = RarArchive::open(&path, Some("password")).unwrap();
        a.test(Some("password"), &|_, _, _| true).unwrap();
        assert_eq!(a.entries().len(), 1);
        let bytes = a.read_entry(0, Some("password")).unwrap();
        assert_eq!(bytes.len() as u64, a.entries()[0].size);
        assert_eq!(bytes.len(), 4096);
        assert_eq!(
            arca_core::crc32(&bytes),
            if name.starts_with("header_") {
                0xa087_a9af
            } else {
                0xb9c5_4415
            }
        );
        if name.starts_with("header_") {
            assert!(matches!(
                RarArchive::open(&path, None),
                Err(Error::PasswordRequired)
            ));
            assert!(matches!(
                RarArchive::open(&path, Some("wrong")),
                Err(Error::BadPassword)
            ));
        }
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        for password in [None, Some("wrong")] {
            let error = RarArchive::open(&path, password)
                .and_then(|a| {
                    a.extract(&dest, &[], password, &|_, _, _| true, &|_| {
                        Conflict::Overwrite
                    })
                })
                .unwrap_err();
            assert!(
                if password.is_none() {
                    matches!(error, Error::PasswordRequired)
                } else {
                    matches!(error, Error::BadPassword)
                },
                "{name}: {error}"
            );
            assert!(!dest.exists());
        }
    }
}

#[test]
fn legacy_encrypted_splits_require_consistent_first_volume_flags() {
    let dir = tempfile::tempdir().unwrap();
    for ext in ["rar", "r00"] {
        fs::copy(
            fixture(&format!("encrypted_multivol_rar300.{ext}")),
            dir.path().join(format!("legacy.{ext}")),
        )
        .unwrap();
    }
    let later = dir.path().join("legacy.r00");
    assert!(RarArchive::open(&later, Some("password"))
        .err()
        .unwrap()
        .to_string()
        .contains("first-volume flag"));
    // The upstream legacy fixture marks BOTH parts as the first volume.
    // Normalize only that main-header bit and its CRC to test the decoder too.
    let mut bytes = fs::read(&later).unwrap();
    bytes[11] &= !1;
    let size = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
    let crc = arca_core::crc32(&bytes[9..7 + size]) as u16;
    bytes[7..9].copy_from_slice(&crc.to_le_bytes());
    fs::write(&later, bytes).unwrap();
    let a = RarArchive::open(&later, None).unwrap();
    let bytes = a.read_entry(0, Some("password")).unwrap();
    assert_eq!(bytes.len(), 65_536);
    assert_eq!(arca_core::crc32(&bytes), 0xddc9_5682);
    for password in [None, Some("wrong")] {
        let out = dir.path().join("out");
        let error = a
            .extract(&out, &[], password, &|_, _, _| true, &|_| {
                Conflict::Overwrite
            })
            .unwrap_err();
        assert!(
            if password.is_none() {
                matches!(error, Error::PasswordRequired)
            } else {
                matches!(error, Error::BadPassword)
            },
            "{error}"
        );
        assert!(!out.exists());
    }
}

fn vint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while value >= 128 {
        out.push(value as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
    out
}

fn header(body: &[u8]) -> Vec<u8> {
    let mut encoded = vint(body.len() as u64);
    encoded.extend(body);
    let mut out = arca_core::crc32(&encoded).to_le_bytes().to_vec();
    out.extend(encoded);
    out
}

fn synthetic_volume(number: u8, next: bool, count: usize, size: u64) -> Vec<u8> {
    let mut data = b"Rar!\x1a\x07\x01\x00".to_vec();
    data.extend(header(&[1, 0, 3, number]));
    for index in 0..count {
        let name = format!("file-{number}-{index}");
        let mut body = vec![2, 2, 0, 0];
        body.extend(vint(size));
        body.extend([0, 0, 1, name.len() as u8]);
        body.extend(name.bytes());
        data.extend(header(&body));
    }
    data.extend(header(&[5, 4, u8::from(next)]));
    data
}

#[test]
fn header_counts_non_file_bytes_and_output_limits_apply_to_the_whole_set() {
    for limit in ["headers", "bytes", "output"] {
        let dir = tempfile::tempdir().unwrap();
        let volumes = if limit == "output" { 5 } else { 3 };
        for i in 0..volumes {
            let path = dir.path().join(format!("set.part{}.rar", i + 1));
            let count = if limit == "headers" { 40_000 } else { 1 };
            let size = if limit == "output" {
                4 * 1024 * 1024 * 1024
            } else {
                0
            };
            fs::write(&path, synthetic_volume(i, i + 1 < volumes, count, size)).unwrap();
            if limit == "bytes" {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(24 * 1024 * 1024)
                    .unwrap();
            }
        }
        let error = RarArchive::open(&dir.path().join("set.part1.rar"), None)
            .err()
            .expect("aggregate limit accepted");
        assert!(matches!(error, Error::Limit(_)), "{limit}: {error}");
    }
}

#[test]
fn selected_later_volume_is_charged_once_against_aggregate_limits() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..2 {
        let path = dir.path().join(format!("set.part{}.rar", i + 1));
        fs::write(&path, synthetic_volume(i, i == 0, 1, 0)).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(24 * 1024 * 1024)
            .unwrap();
    }
    let archive = RarArchive::open(&dir.path().join("set.part2.rar"), None).unwrap();
    assert_eq!(archive.entries().len(), 2);
}

#[test]
fn rejects_metadata_mismatch_even_with_valid_header_checksums() {
    for mismatch in ["name", "size", "split", "codec"] {
        let dir = tempfile::tempdir().unwrap();
        copy_set(dir.path(), &NAMES);
        let path = dir.path().join(NAMES[1]);
        let mut data = fs::read(&path).unwrap();
        let parsed = rars::rar50::Archive::parse(&data).unwrap();
        let first = parsed.files().next().unwrap();
        let block = &first.block;
        let mut body = data[block.header_range.clone()].to_vec();
        match mismatch {
            "name" => {
                let index = body
                    .windows(9)
                    .position(|bytes| bytes == b"first.txt")
                    .unwrap();
                body[index] = b'x';
            }
            // This independent fixture's common flags, extra length and packed
            // length occupy six bytes, followed by file flags and unpacked size.
            "size" => {
                body[7] ^= 1;
            }
            "split" => {
                body[1] &= !8;
            }
            _ => {
                let offset = body
                    .windows(9)
                    .position(|bytes| bytes == b"first.txt")
                    .unwrap();
                body[offset - 3] ^= 128;
            }
        }
        data.splice(block.offset..block.header_range.end, header(&body));
        fs::write(path, data).unwrap();
        failure_leaves_destination_untouched(&dir.path().join(NAMES[0]), NAMES[1]);
    }
}
