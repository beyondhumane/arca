use super::*;
use crate::test_support::Room;
use arca_core::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn sevenz_detection_and_stems_are_case_insensitive() {
    assert_eq!(detect(Path::new("Folder/Backup.7Z")), Some(Format::SevenZ));
    assert_eq!(archive_stem(Path::new("Folder/Backup.7Z")), "Backup");
    assert_eq!(Format::SevenZ.extension(), "7z");
}

#[test]
fn sevenz_open_rejects_entries_changed_since_listing_without_writing() {
    let room = Room::new();
    let archive = room.archive(None, false);
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap();
    let mut entry = entries
        .iter()
        .find(|e| e.name == "source/a.txt")
        .unwrap()
        .clone();
    let temp = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join("Arca")
        .join(archive_stem(&archive));
    assert!(!temp.exists());
    entry.name = "stale.txt".into();
    let result = extract_one(&archive, &entry, None, &|_, _, _| true);
    let untouched = !temp.exists();
    if !untouched {
        fs::remove_dir_all(&temp).unwrap();
    }
    assert!(matches!(result, Err(Error::Format(_))));
    assert!(untouched);
    entry.offset = u64::MAX;
    assert!(matches!(
        extract_one(&archive, &entry, None, &|_, _, _| true),
        Err(Error::Format(_))
    ));
    assert!(!temp.exists());
}

#[test]
fn wrong_password_never_creates_or_truncates_destinations_or_open_file_output() {
    for hidden in [false, true] {
        let room = Room::new();
        let archive = room.archive(Some("secret"), hidden);
        let entries = list_entries(&archive, Some("secret"), &|_, _, _| true).unwrap();
        let index = entries
            .iter()
            .position(|e| e.name == "source/a.txt")
            .unwrap();
        let dest = room.path("missing");
        let notify = |_, _, _: &str| true;
        let ask = |_: &Path| panic!("wrong passwords must not ask about conflicts");
        assert!(arca_7z::password_required(
            &extract(&archive, &dest, &[], &notify, &ask, Some("wrong")).unwrap_err()
        ));
        assert!(!dest.exists());
        fs::create_dir_all(dest.join("source")).unwrap();
        fs::write(dest.join("source/a.txt"), b"keep me").unwrap();
        assert!(extract(&archive, &dest, &[], &notify, &ask, Some("wrong")).is_err());
        assert_eq!(fs::read(dest.join("source/a.txt")).unwrap(), b"keep me");
        assert!(!dest.join("source/empty").exists());
        let temp = std::env::temp_dir()
            .join("Arca")
            .join(archive_stem(&archive));
        assert!(!temp.exists());
        assert!(extract_one(&archive, &entries[index], Some("wrong"), &notify).is_err());
        assert!(!temp.exists());
        fs::create_dir_all(temp.join("source")).unwrap();
        fs::write(temp.join("source/a.txt"), b"keep temp").unwrap();
        assert!(extract_one(&archive, &entries[index], Some("wrong"), &notify).is_err());
        assert_eq!(fs::read(temp.join("source/a.txt")).unwrap(), b"keep temp");
        let mut preview = b"unchanged".to_vec();
        assert!(read_entry(&archive, index, &mut preview, Some("wrong"), &notify).is_err());
        assert_eq!(preview, b"unchanged");
        fs::remove_dir_all(temp).unwrap();
    }
}

#[test]
fn encrypted_selection_preview_testing_and_conflict_answers_work() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let entries = list_entries(&archive, Some("secret"), &|_, _, _| true).unwrap();
    let index = entries
        .iter()
        .position(|e| e.name == "source/b.txt")
        .unwrap();
    let wanted: Vec<_> = (0..entries.len()).map(|i| i == index).collect();
    let dest = room.path("out");
    let notify = |_, _, _: &str| true;
    let bytes = extract(
        &archive,
        &dest,
        &wanted,
        &notify,
        &|_| Answer::Replace,
        Some("secret"),
    )
    .unwrap();
    assert_eq!(bytes, 15);
    assert!(!dest.join("source/a.txt").exists());
    assert_eq!(
        fs::read(dest.join("source/b.txt")).unwrap(),
        b"second contents"
    );
    fs::write(dest.join("source/b.txt"), b"existing").unwrap();
    assert_eq!(
        extract(
            &archive,
            &dest,
            &wanted,
            &notify,
            &|_| Answer::Skip,
            Some("secret")
        )
        .unwrap(),
        0
    );
    assert_eq!(fs::read(dest.join("source/b.txt")).unwrap(), b"existing");
    extract(
        &archive,
        &dest,
        &wanted,
        &notify,
        &|_| Answer::Rename,
        Some("secret"),
    )
    .unwrap();
    assert_eq!(
        fs::read(dest.join("source/b (1).txt")).unwrap(),
        b"second contents"
    );
    let mut preview = Vec::new();
    read_entry(&archive, index, &mut preview, Some("secret"), &notify).unwrap();
    assert_eq!(preview, b"second contents");
    let (good, bad) = test_archive(&archive, None, Some("secret"), &notify).unwrap();
    assert_eq!(good, 2);
    assert!(bad.is_empty());
    let only = HashSet::from(["source/b.txt".to_owned()]);
    assert_eq!(
        test_archive(&archive, Some(&only), Some("secret"), &notify)
            .unwrap()
            .0,
        1
    );
    assert!(test_archive(&archive, None, Some("wrong"), &notify).is_err());
}

#[test]
fn sevenz_cancellation_does_not_prepare_output() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let dest = room.path("out");
    assert!(matches!(
        extract(
            &archive,
            &dest,
            &[],
            &|_, _, _| false,
            &|_| Answer::Replace,
            Some("secret")
        ),
        Err(Error::Cancelled)
    ));
    assert!(!dest.exists());
    let out = room.path("cancel.7z");
    fs::write(&out, b"original").unwrap();
    assert!(matches!(
        compress(
            &out,
            &[room.path("source")],
            Format::SevenZ,
            Codec::Deflate,
            Level::Normal,
            &|_, _, _| false,
            (None, false)
        ),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read(out).unwrap(), b"original");
}

#[test]
fn corrupt_unselected_encrypted_content_prevents_even_directory_creation() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap();
    let wanted: Vec<_> = entries.iter().map(|e| e.is_dir).collect();
    let mut bytes = fs::read(&archive).unwrap();
    bytes[32] ^= 0x80;
    fs::write(&archive, bytes).unwrap();
    let dest = room.path("out");
    assert!(matches!(
        extract(
            &archive,
            &dest,
            &wanted,
            &|_, _, _| true,
            &|_| Answer::Replace,
            Some("secret")
        ),
        Err(Error::PasswordOrCorrupt)
    ));
    assert!(!dest.exists());
}

#[test]
fn sevenz_sink_errors_are_not_reported_as_password_failures() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let dest = room.path("file-not-directory");
    fs::write(&dest, b"keep").unwrap();
    assert!(matches!(
        extract(
            &archive,
            &dest,
            &[],
            &|_, _, _| true,
            &|_| Answer::Replace,
            Some("secret")
        ),
        Err(Error::Io(_))
    ));
    assert_eq!(fs::read(dest).unwrap(), b"keep");
}

#[test]
fn sevenz_creation_levels_and_options_are_forwarded_without_zip_codes() {
    let room = Room::new();
    let input = room.path("a.txt");
    fs::write(&input, b"example contents").unwrap();
    for level in [Level::Store, Level::Fast, Level::Normal, Level::Best] {
        let out = room.path("created.7z");
        compress(
            &out,
            std::slice::from_ref(&input),
            Format::SevenZ,
            Codec::Zstd,
            level,
            &|_, _, _| true,
            (None, false),
        )
        .unwrap();
        let entries = list_entries(&out, None, &|_, _, _| true).unwrap();
        assert_eq!(
            entries[0].method,
            if level == Level::Store {
                arca_core::Method::Store
            } else {
                arca_core::Method::Lzma2
            }
        );
    }
    let out = room.path("invalid.7z");
    assert!(compress(
        &out,
        &[input],
        Format::SevenZ,
        Codec::Deflate,
        Level::Normal,
        &|_, _, _| true,
        (None, true)
    )
    .is_err());
    assert!(!out.exists());
}

#[cfg(unix)]
#[test]
fn sevenz_extraction_rejects_existing_destination_symlinks() {
    let room = Room::new();
    let archive = room.archive(None, false);
    let dest = room.path("out");
    let outside = room.path("outside");
    fs::create_dir(&dest).unwrap();
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, dest.join("source")).unwrap();
    assert!(extract(
        &archive,
        &dest,
        &[],
        &|_, _, _| true,
        &|_| Answer::Replace,
        None
    )
    .is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn sevenz_creation_rejects_symlinks_instead_of_silently_omitting_sources() {
    let room = Room::new();
    let input = room.path("link");
    fs::write(room.path("target"), b"contents").unwrap();
    std::os::unix::fs::symlink(room.path("target"), &input).unwrap();
    let out = room.path("out.7z");
    assert!(compress(
        &out,
        &[input],
        Format::SevenZ,
        Codec::Deflate,
        Level::Normal,
        &|_, _, _| true,
        (None, false)
    )
    .is_err());
    assert!(!out.exists());
}

#[test]
fn sevenz_replacement_does_not_truncate_existing_hardlinks() {
    for password in [None, Some("secret")] {
        let room = Room::new();
        let archive = room.archive(password, password.is_some());
        let dest = room.path("out");
        let outside = room.path("outside");
        fs::create_dir_all(dest.join("source")).unwrap();
        fs::write(&outside, b"preserve the other link").unwrap();
        fs::hard_link(&outside, dest.join("source/a.txt")).unwrap();
        extract(
            &archive,
            &dest,
            &[],
            &|_, _, _| true,
            &|_| Answer::Replace,
            password,
        )
        .unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"preserve the other link");
        assert_eq!(
            fs::read(dest.join("source/a.txt")).unwrap(),
            b"first contents"
        );
        assert_eq!(fs::read_dir(dest.join("source")).unwrap().count(), 3);
    }
}

#[test]
fn sevenz_cancelled_file_preserves_target_and_removes_staging_file() {
    let room = Room::new();
    let input = room.path("large.bin");
    fs::write(&input, vec![0x61; 256 * 1024]).unwrap();
    let archive = room.path("plain.7z");
    compress(
        &archive,
        &[input],
        Format::SevenZ,
        Codec::Store,
        Level::Store,
        &|_, _, _| true,
        (None, false),
    )
    .unwrap();
    let dest = room.path("out");
    fs::create_dir(&dest).unwrap();
    for exists in [false, true] {
        if exists {
            fs::write(dest.join("large.bin"), b"original").unwrap();
        }
        let calls = AtomicUsize::new(0);
        let notify = |_, _, _: &str| calls.fetch_add(1, Ordering::Relaxed) < 2;
        assert!(matches!(
            extract(&archive, &dest, &[], &notify, &|_| Answer::Replace, None),
            Err(Error::Cancelled)
        ));
        assert_eq!(fs::read_dir(&dest).unwrap().count(), usize::from(exists));
        if exists {
            assert_eq!(fs::read(dest.join("large.bin")).unwrap(), b"original");
        }
    }
}

#[test]
fn exhausted_extraction_rename_does_not_fall_back_to_overwrite() {
    let room = Room::new();
    let original = room.path("a.txt");
    fs::write(&original, b"original").unwrap();
    let mut claimed = (1..10_000)
        .map(|n| room.path(&format!("a ({n}).txt")))
        .collect();
    assert!(matches!(
        dest_path(&room.0, "a.txt", false, &|_| Answer::Rename, &mut claimed),
        Err(Error::Limit(_))
    ));
    assert_eq!(fs::read(original).unwrap(), b"original");
}

#[test]
#[ignore = "requires the external 7z command"]
fn external_solid_selection_streams_multiple_members_in_archive_order() {
    let room = Room::new();
    for name in ["a", "b", "c"] {
        fs::write(room.path(name), name.repeat(100_000)).unwrap();
    }
    let archive = room.path("solid.7z");
    let output = std::process::Command::new("7z")
        .current_dir(&room.0)
        .args([
            "a",
            "-t7z",
            "-ms=on",
            "-m0=lzma2",
            "solid.7z",
            "a",
            "b",
            "c",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let a = arca_7z::SevenZArchive::open(fs::File::open(&archive).unwrap(), None).unwrap();
    assert!(a.is_solid());
    let entries = a.entries();
    let wanted: Vec<_> = entries.iter().map(|e| e.name != "b").collect();
    let dest = room.path("out");
    let seen = std::sync::Mutex::new(Vec::new());
    extract(
        &archive,
        &dest,
        &wanted,
        &|done, total, _| {
            seen.lock().unwrap().push((done, total));
            true
        },
        &|_| Answer::Replace,
        None,
    )
    .unwrap();
    assert_eq!(fs::read(dest.join("a")).unwrap(), vec![b'a'; 100_000]);
    assert!(!dest.join("b").exists());
    assert_eq!(fs::read(dest.join("c")).unwrap(), vec![b'c'; 100_000]);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.last(), Some(&(2, 2)));
    assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));
}

#[test]
#[ignore = "requires the external 7z command"]
fn external_encrypted_solid_selection_supports_both_header_modes() {
    for hide in ["-mhe=off", "-mhe=on"] {
        let room = Room::new();
        for name in ["a", "b", "c"] {
            fs::write(room.path(name), name.repeat(100_000)).unwrap();
        }
        let archive = room.path("solid.7z");
        let output = std::process::Command::new("7z")
            .current_dir(&room.0)
            .args([
                "a",
                "-t7z",
                "-ms=on",
                "-m0=lzma2",
                "-psecret",
                hide,
                "solid.7z",
                "a",
                "b",
                "c",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let a =
            arca_7z::SevenZArchive::open(File::open(&archive).unwrap(), Some("secret")).unwrap();
        assert!(a.is_solid());
        let wanted: Vec<_> = a.entries().iter().map(|e| e.name != "b").collect();
        let dest = room.path("out");
        assert!(extract(
            &archive,
            &dest,
            &wanted,
            &|_, _, _| true,
            &|_| Answer::Replace,
            Some("wrong")
        )
        .is_err());
        assert!(!dest.exists());
        extract(
            &archive,
            &dest,
            &wanted,
            &|_, _, _| true,
            &|_| Answer::Replace,
            Some("secret"),
        )
        .unwrap();
        assert_eq!(fs::read(dest.join("a")).unwrap(), vec![b'a'; 100_000]);
        assert!(!dest.join("b").exists());
        assert_eq!(fs::read(dest.join("c")).unwrap(), vec![b'c'; 100_000]);
    }
}
