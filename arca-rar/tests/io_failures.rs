#![cfg(feature = "rar")]

use arca_core::Error;
use arca_rar::{Conflict, RarArchive};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn publication_io_failure_keeps_earlier_verified_replacement_not_directory_rollback() {
    let archive = RarArchive::open(&fixture("stored.rar"), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("first.txt"), b"old content").unwrap();
    let calls = AtomicUsize::new(0);
    let error = archive
        .extract(
            dir.path(),
            &[],
            None,
            &|_, _, name| {
                if name == "folder/second.txt" && calls.fetch_add(1, Ordering::SeqCst) == 1 {
                    // This is publication, after staging and the earlier replacement completed.
                    assert_eq!(
                        fs::read(dir.path().join("first.txt")).unwrap(),
                        b"Arca RAR fixture alpha\n".repeat(64)
                    );
                    fs::write(dir.path().join("folder"), b"publication obstacle").unwrap();
                }
                true
            },
            &|_| Conflict::Overwrite,
        )
        .unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error}");
    assert_eq!(
        fs::read(dir.path().join("first.txt")).unwrap(),
        b"Arca RAR fixture alpha\n".repeat(64)
    );
    assert_eq!(
        fs::read(dir.path().join("folder")).unwrap(),
        b"publication obstacle"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn cancellation_during_publication_does_not_roll_back_verified_files() {
    let archive = RarArchive::open(&fixture("stored.rar"), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let calls = AtomicUsize::new(0);
    let error = archive
        .extract(
            dir.path(),
            &[],
            None,
            &|_, _, name| name != "folder/second.txt" || calls.fetch_add(1, Ordering::SeqCst) == 0,
            &|_| Conflict::Overwrite,
        )
        .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert_eq!(
        fs::read(dir.path().join("first.txt")).unwrap(),
        b"Arca RAR fixture alpha\n".repeat(64)
    );
    assert!(!dir.path().join("folder/second.txt").exists());
}

#[test]
#[ignore = "run tests/disk-full.sh: requires a private 64 KiB tmpfs, never fill a host filesystem"]
fn disk_full_staging_publishes_nothing() {
    let normal = std::env::var_os("ARCA_RAR_NORMAL").expect("use disk-full.sh");
    let full = std::env::var_os("ARCA_RAR_ENOSPC").expect("use disk-full.sh");
    assert_eq!(std::env::temp_dir(), Path::new(&full));
    let archive = RarArchive::open(&fixture("matrix/rar5-dict64/set.rar"), None).unwrap();
    let dir = tempfile::tempdir_in(normal).unwrap();
    fs::write(dir.path().join("dictionary.bin"), b"old").unwrap();
    let error = archive
        .extract(dir.path(), &[], None, &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error}");
    assert!(
        error.to_string().contains("No space left on device"),
        "{error}"
    );
    assert_eq!(fs::read(dir.path().join("dictionary.bin")).unwrap(), b"old");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(full).unwrap().count(), 0, "staging cleanup");
}

#[test]
#[ignore = "run tests/disk-full.sh: requires a private 64 KiB tmpfs, never fill a host filesystem"]
fn disk_full_publication_preserves_existing_target_and_cleans_temporary_file() {
    let full = std::env::var_os("ARCA_RAR_ENOSPC").expect("use disk-full.sh");
    assert_ne!(std::env::temp_dir(), Path::new(&full));
    let archive = RarArchive::open(&fixture("matrix/rar5-dict64/set.rar"), None).unwrap();
    let dir = tempfile::tempdir_in(full).unwrap();
    fs::write(dir.path().join("dictionary.bin"), b"old").unwrap();
    let error = archive
        .extract(dir.path(), &[], None, &|_, _, _| true, &|_| {
            Conflict::Overwrite
        })
        .unwrap_err();
    let Error::Io(io) = error else {
        panic!("{error}")
    };
    assert_eq!(
        io.kind(),
        std::io::ErrorKind::StorageFull,
        "actual Linux ENOSPC: {io:?}"
    );
    assert_eq!(fs::read(dir.path().join("dictionary.bin")).unwrap(), b"old");
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "temporary-file cleanup"
    );
}
