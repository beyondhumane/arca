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

#[test]
fn lists_tests_and_extracts_external_archives() {
    for name in [
        "plain.rar",
        "stored.rar",
        "solid.rar",
        "encrypted.rar",
        "headers.rar",
    ] {
        let password = name.contains("encrypted") || name.contains("headers");
        let password = password.then_some("arca-test-only");
        let a = RarArchive::open(&fixture(name), password).unwrap();
        assert_eq!(
            a.entries().iter().filter(|e| !e.is_dir).count(),
            2,
            "{name}"
        );
        a.test(password, &|_, _, _| true).unwrap();
        let dir = tempfile::tempdir().unwrap();
        a.extract(dir.path(), &[], password, &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .unwrap();
        assert_eq!(
            fs::read(dir.path().join("first.txt")).unwrap(),
            b"Arca RAR fixture alpha\n".repeat(64),
            "{name}"
        );
        assert_eq!(
            fs::read(dir.path().join("folder/second.txt")).unwrap(),
            b"Arca RAR fixture beta\n".repeat(64),
            "{name}"
        );
        assert!(dir.path().join("empty").is_dir());
        let i = a
            .entries()
            .iter()
            .position(|e| e.name == "folder/second.txt")
            .unwrap();
        assert_eq!(
            a.read_entry(i, password).unwrap(),
            b"Arca RAR fixture beta\n".repeat(64)
        );
    }
}

#[test]
fn extracts_legacy_solid_second_member_without_publishing_predecessor() {
    let a = RarArchive::open(&fixture("legacy-solid.rar"), None).unwrap();
    assert_eq!(a.entries().len(), 2);
    let dir = tempfile::tempdir().unwrap();
    a.extract(dir.path(), &[false, true], None, &|_, _, _| true, &|_| {
        Conflict::Overwrite
    })
    .unwrap();
    assert!(!dir.path().join("one.txt").exists());
    assert_eq!(
        fs::read(dir.path().join("two.txt")).unwrap(),
        b"shared prefix shared prefix shared prefix beta\n"
    );
}

#[test]
fn encrypted_headers_require_password_before_listing() {
    assert!(matches!(
        RarArchive::open(&fixture("headers.rar"), None),
        Err(Error::PasswordRequired)
    ));
    assert!(matches!(
        RarArchive::open(&fixture("headers.rar"), Some("wrong")),
        Err(Error::BadPassword)
    ));
}

#[test]
fn wrong_password_publishes_nothing() {
    let a = RarArchive::open(&fixture("encrypted.rar"), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("not-created");
    assert!(a
        .extract(&dest, &[], Some("wrong"), &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .is_err());
    assert!(!dest.exists());
    fs::write(dir.path().join("first.txt"), b"keep me").unwrap();
    assert!(a
        .extract(dir.path(), &[], Some("wrong"), &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .is_err());
    assert_eq!(fs::read(dir.path().join("first.txt")).unwrap(), b"keep me");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn corrupt_later_member_never_publishes_earlier_good_member() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = fs::read(fixture("stored.rar")).unwrap();
    let at = data.windows(4).rposition(|p| p == b"Arca").unwrap();
    data[at] ^= 1;
    let path = dir.path().join("corrupt.rar");
    fs::write(&path, data).unwrap();
    let a = RarArchive::open(&path, None).unwrap();
    assert!(a.test(None, &|_, _, _| true).is_err());
    let dest = dir.path().join("out");
    assert!(a
        .extract(&dest, &[], None, &|_, _, _| true, &|_| Conflict::Overwrite)
        .is_err());
    assert!(!dest.exists());
}

#[test]
fn every_rar5_truncation_fails_without_panic_or_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.rar");
    let dest = dir.path().join("out");
    for name in ["plain.rar", "solid.rar"] {
        let data = fs::read(fixture(name)).unwrap();
        for length in 0..data.len() {
            fs::write(&path, &data[..length]).unwrap();
            let result = RarArchive::open(&path, None).and_then(|a| {
                a.extract(&dest, &[], None, &|_, _, _| true, &|_| Conflict::Overwrite)
            });
            assert!(result.is_err(), "{name} accepted prefix {length}");
            assert!(!dest.exists(), "{name} published prefix {length}");
        }
    }
}

#[test]
fn cancellation_stops_before_publication() {
    assert!(matches!(
        RarArchive::open_with_progress(&fixture("plain.rar"), None, &|_, _, _| false),
        Err(Error::Cancelled)
    ));
    let a = RarArchive::open(&fixture("plain.rar"), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        a.extract(
            &dir.path().join("out"),
            &[],
            None,
            &|_, _, _| false,
            &|_| Conflict::Overwrite
        ),
        Err(Error::Cancelled)
    ));
    assert!(!dir.path().join("out").exists());
}

#[test]
fn refuses_multi_volume_without_guessing_other_paths() {
    for name in ["volume.part1.rar", "volume.part4.rar"] {
        assert!(
            matches!(
                RarArchive::open(&fixture(name), None),
                Err(Error::Unsupported(_))
            ),
            "{name}"
        );
    }
}

#[test]
fn respects_skip_rename_and_cancel_conflicts() {
    let a = RarArchive::open(&fixture("plain.rar"), None).unwrap();
    for policy in [Conflict::Skip, Conflict::Rename, Conflict::Cancel] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("first.txt"), b"keep me").unwrap();
        let result = a.extract(dir.path(), &[], None, &|_, _, _| true, &|_| policy);
        assert_eq!(fs::read(dir.path().join("first.txt")).unwrap(), b"keep me");
        if matches!(policy, Conflict::Cancel) {
            assert!(matches!(result, Err(Error::Cancelled)));
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        } else {
            result.unwrap();
            assert_eq!(
                dir.path().join("first (1).txt").exists(),
                matches!(policy, Conflict::Rename)
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn refuses_destination_symlinks_and_preserves_external_files() {
    use std::os::unix::fs::symlink;
    let a = RarArchive::open(&fixture("plain.rar"), None).unwrap();
    for target in ["folder", "first.txt"] {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("second.txt"), b"untouched").unwrap();
        symlink(outside.path(), dir.path().join(target)).unwrap();
        assert!(a
            .extract(dir.path(), &[], None, &|_, _, _| true, &|_| {
                Conflict::Overwrite
            })
            .is_err());
        assert_eq!(
            fs::read(outside.path().join("second.txt")).unwrap(),
            b"untouched"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
