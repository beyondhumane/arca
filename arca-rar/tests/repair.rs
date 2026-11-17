#![cfg(feature = "rar")]

use arca_core::Error;
use arca_rar::RarArchive;
use rars::builder::Builder;
use rars::ArchiveVersion;
use std::fs;

fn payload() -> Vec<u8> {
    (0..64 * 1024u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8)
        .collect()
}

fn built(recovery: Option<u64>, comment: Option<&[u8]>) -> Vec<u8> {
    let mut b = Builder::new(ArchiveVersion::Rar50)
        .store(true)
        .recovery_percent(recovery)
        .comment(comment.map(<[u8]>::to_vec));
    b.add_bytes(b"data.bin".to_vec(), payload(), None, None)
        .unwrap();
    b.to_bytes().unwrap()
}

fn damaged(bytes: &[u8]) -> Vec<u8> {
    let at = bytes
        .windows(16)
        .position(|w| w == &payload()[4096..4112])
        .unwrap();
    let mut bad = bytes.to_vec();
    for b in &mut bad[at..at + 16] {
        *b ^= 0x5A;
    }
    bad
}

#[test]
fn damaged_data_comes_back_from_the_recovery_record() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bad.rar");
    let output = dir.path().join("good.rar");
    let original = built(Some(10), None);
    let bad = damaged(&original);
    fs::write(&input, &bad).unwrap();
    let broken = RarArchive::open(&input, None).unwrap();
    assert!(broken.test(None, &|_, _, _| true).is_err());

    let report = arca_rar::repair(&input, &output, None, &|_, _, _| true).unwrap();
    assert!(report.changed && report.data_repaired);
    assert_eq!(fs::read(&input).unwrap(), bad);
    let fixed = RarArchive::open(&output, None).unwrap();
    assert_eq!(fixed.read_entry(0, None).unwrap(), payload());
}

#[test]
fn repair_refuses_its_input_a_missing_record_and_garbage() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("bad.rar");
    fs::write(&input, damaged(&built(Some(10), None))).unwrap();
    assert!(arca_rar::repair(&input, &input, None, &|_, _, _| true).is_err());

    let plain = dir.path().join("plain.rar");
    let output = dir.path().join("out.rar");
    fs::write(&plain, damaged(&built(None, None))).unwrap();
    assert!(arca_rar::repair(&plain, &output, None, &|_, _, _| true).is_err());
    assert!(!output.exists());

    let sound = dir.path().join("sound.rar");
    fs::write(&sound, built(Some(10), None)).unwrap();
    let report = arca_rar::repair(&sound, &output, None, &|_, _, _| true).unwrap();
    assert!(!report.changed);
    assert!(!output.exists());

    let stopped = arca_rar::repair(&input, &output, None, &|_, _, _| false);
    assert!(matches!(stopped, Err(Error::Cancelled)), "{stopped:?}");
    assert!(!output.exists());

    let bytes = built(Some(10), Some(b"c"));
    for cut in (0..bytes.len()).step_by(331) {
        let path = dir.path().join("cut.rar");
        fs::write(&path, &bytes[..cut]).unwrap();
        let _ = arca_rar::repair(&path, &output, None, &|_, _, _| true);
        if let Ok(a) = RarArchive::open(&path, None) {
            let _ = a.comment(None);
        }
    }
    let leftovers = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn damaged_headers_say_why_they_cannot_be_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = built(Some(10), None);
    let at = bytes.windows(8).position(|w| w == b"data.bin").unwrap();
    bytes[at] ^= 0x20;
    let input = dir.path().join("headers.rar");
    fs::write(&input, &bytes).unwrap();
    let output = dir.path().join("out.rar");
    let error = arca_rar::repair(&input, &output, None, &|_, _, _| true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("damaged headers cannot be repaired"),
        "{error}"
    );
    assert!(!output.exists());
}
