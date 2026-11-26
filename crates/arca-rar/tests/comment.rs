#![cfg(feature = "rar")]

use arca_rar::RarArchive;
use rars::builder::Builder;
use rars::ArchiveVersion;
use std::fs;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn payload() -> Vec<u8> {
    (0..64 * 1024u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8)
        .collect()
}

fn built(comment: Option<&[u8]>) -> Vec<u8> {
    let mut b = Builder::new(ArchiveVersion::Rar50)
        .store(true)
        .comment(comment.map(<[u8]>::to_vec));
    b.add_bytes(b"data.bin".to_vec(), payload(), None, None)
        .unwrap();
    b.to_bytes().unwrap()
}

#[test]
fn the_archive_comment_is_read_and_a_missing_one_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let with = dir.path().join("with.rar");
    fs::write(&with, built(Some("nota del archivo\nsegunda".as_bytes()))).unwrap();
    let a = RarArchive::open(&with, None).unwrap();
    assert_eq!(
        a.comment(None).unwrap().as_deref(),
        Some("nota del archivo\nsegunda")
    );
    let a = RarArchive::open(&fixture("plain.rar"), None).unwrap();
    assert_eq!(a.comment(None).unwrap(), None);
}

#[test]
fn a_cut_or_garbled_comment_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cut.rar");
    let bytes = built(Some(&b"un comentario algo largo ".repeat(40)));
    for cut in (0..bytes.len()).step_by(211) {
        fs::write(&path, &bytes[..cut]).unwrap();
        if let Ok(a) = RarArchive::open(&path, None) {
            let _ = a.comment(None);
        }
    }
    for at in (0..bytes.len().min(4096)).step_by(7) {
        let mut bad = bytes.clone();
        bad[at] ^= 0xA5;
        fs::write(&path, &bad).unwrap();
        if let Ok(a) = RarArchive::open(&path, None) {
            let _ = a.comment(None);
        }
    }
}
