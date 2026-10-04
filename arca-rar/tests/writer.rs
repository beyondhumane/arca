#![cfg(feature = "rar")]

use arca_core::{Error, Level};
use arca_rar::{create_limits, create_rar, Conflict, CreateOptions, RarArchive, Source};
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

fn source(path: impl Into<PathBuf>, name: &str) -> Source {
    Source {
        path: path.into(),
        name: name.into(),
    }
}

fn options(level: Level) -> CreateOptions {
    CreateOptions { level }
}

fn listing(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// Runs `work` on its own thread and fails the test if it is still running
/// after `limit`, so a hung writer surfaces as a failure rather than a stall.
fn bounded<T: Send + 'static>(limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    match rx.recv_timeout(limit) {
        Ok(value) => value,
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("writer did not finish within {limit:?}"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("writer panicked"),
    }
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn bytes(&mut self, len: usize, compressible: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let word = self.next();
            if compressible {
                out.extend_from_slice(&word.to_le_bytes()[..1].repeat(8));
            } else {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        out.truncate(len);
        out
    }
}

struct Tree {
    root: tempfile::TempDir,
    sources: Vec<Source>,
    expected: Vec<(String, Option<Vec<u8>>)>,
}

fn fixture_tree() -> Tree {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    fs::create_dir_all(input.join("folder/nested")).unwrap();
    fs::create_dir_all(input.join("vacío")).unwrap();
    let mut rng = Xorshift(0x5eed_0001);
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("empty.bin", Vec::new()),
        ("notes.txt", b"Arca RAR writer fixture\n".repeat(400)),
        ("folder/random.bin", rng.bytes(300 * 1024, false)),
        ("folder/nested/text.bin", rng.bytes(1_200_000, true)),
        (
            "ñandú/日本語 ファイル.txt",
            "contenido en UTF-8: año, camión\n".repeat(50).into_bytes(),
        ),
    ];
    let mut sources = Vec::new();
    let mut expected = Vec::new();
    sources.push(source(input.join("folder"), "folder"));
    expected.push(("folder".to_string(), None));
    for (name, data) in &files {
        let path = input.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data).unwrap();
        sources.push(source(path, name));
        expected.push((name.to_string(), Some(data.clone())));
    }
    sources.push(source(input.join("vacío"), "vacío"));
    expected.push(("vacío".to_string(), None));
    Tree {
        root,
        sources,
        expected,
    }
}

fn assert_roundtrip(archive: &Path, expected: &[(String, Option<Vec<u8>>)]) {
    let a = RarArchive::open(archive, None).unwrap();
    a.test(None, &|_, _, _| true).unwrap();
    let names: Vec<_> = a.entries().iter().map(|e| e.name.clone()).collect();
    let wanted: Vec<_> = expected.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(names, wanted);
    for (i, (name, data)) in expected.iter().enumerate() {
        let entry = &a.entries()[i];
        assert_eq!(entry.is_dir, data.is_none(), "{name}");
        if let Some(data) = data {
            assert_eq!(entry.size, data.len() as u64, "{name}");
            assert_eq!(&a.read_entry(i, None).unwrap(), data, "{name}");
        }
    }
    let out = tempfile::tempdir().unwrap();
    a.extract(out.path(), &[], None, &|_, _, _| true, &|_| {
        Conflict::Cancel
    })
    .unwrap();
    for (name, data) in expected {
        let path = out.path().join(name);
        match data {
            Some(data) => assert_eq!(&fs::read(&path).unwrap(), data, "{name}"),
            None => assert!(path.is_dir(), "{name}"),
        }
    }
}

#[test]
fn every_level_roundtrips_through_the_reader() {
    for (level, dir) in [
        (Level::Store, "store"),
        (Level::Fast, "fast"),
        (Level::Normal, "normal"),
        (Level::Best, "best"),
    ] {
        let tree = fixture_tree();
        let out_dir = tree.root.path().join(dir);
        fs::create_dir(&out_dir).unwrap();
        let archive = out_dir.join("new.rar");
        let calls = AtomicUsize::new(0);
        let last = std::sync::Mutex::new((0usize, 0usize));
        create_rar(
            &archive,
            &tree.sources,
            &options(level),
            &|done, total, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                let mut last = last.lock().unwrap();
                assert!(done <= total, "{dir}: {done} > {total}");
                *last = (done, total);
                true
            },
        )
        .unwrap();
        assert_eq!(listing(&out_dir), BTreeSet::from(["new.rar".to_string()]));
        assert!(calls.load(Ordering::Relaxed) > tree.sources.len());
        assert_eq!(
            *last.lock().unwrap(),
            (tree.sources.len(), tree.sources.len())
        );
        assert_roundtrip(&archive, &tree.expected);
        let a = RarArchive::open(&archive, None).unwrap();
        let random = a
            .entries()
            .iter()
            .find(|e| e.name == "folder/random.bin")
            .unwrap();
        let text = a
            .entries()
            .iter()
            .find(|e| e.name == "folder/nested/text.bin")
            .unwrap();
        if level == Level::Store {
            assert_eq!(random.method, arca_core::Method::Store, "{dir}");
            assert_eq!(text.compressed_size, text.size, "{dir}");
        } else {
            assert!(text.compressed_size < text.size / 4, "{dir}");
        }
        assert!(random.compressed_size >= random.size - 1024, "{dir}");
        let mtime = a
            .entries()
            .iter()
            .find(|e| e.name == "notes.txt")
            .unwrap()
            .mtime;
        let modified = fs::metadata(tree.root.path().join("input/notes.txt"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(mtime, Some(modified), "{dir}");
    }
}

#[test]
fn empty_archive_and_names_with_backslashes_are_normalized() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("empty.rar");
    create_rar(&archive, &[], &CreateOptions::default(), &|_, _, _| true).unwrap();
    let a = RarArchive::open(&archive, None).unwrap();
    assert!(a.entries().is_empty());
    a.test(None, &|_, _, _| true).unwrap();

    let file = dir.path().join("file.txt");
    fs::write(&file, b"x").unwrap();
    let archive = dir.path().join("slashes.rar");
    create_rar(
        &archive,
        &[source(&file, "dir\\\\sub//file.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap();
    let a = RarArchive::open(&archive, None).unwrap();
    assert_eq!(a.entries()[0].name, "dir/sub/file.txt");
}

#[test]
fn malformed_names_are_rejected_before_any_output_exists() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file.txt");
    fs::write(&file, b"x").unwrap();
    let archive = dir.path().join("new.rar");
    let long = "a".repeat(create_limits::MAX_NAME_BYTES + 1);
    let malformed = [
        "",
        "/",
        ".",
        "../escape.txt",
        "dir/../../escape.txt",
        "/absolute.txt",
        "\\absolute.txt",
        "C:/drive.txt",
        "nul\0byte",
        "tab\there",
        "colon:here",
        "star*",
        "CON",
        "dir/LPT1.txt",
        "trailing.",
        "trailing ",
        "marker\u{fffe}",
    ];
    let attempt = |name: &str| {
        create_rar(
            &archive,
            &[source(&file, name)],
            &CreateOptions::default(),
            &|_, _, _| true,
        )
        .unwrap_err()
    };
    for name in malformed {
        let error = attempt(name);
        assert!(matches!(error, Error::Format(_)), "{name:?}: {error}");
    }
    let error = attempt(&long);
    assert!(matches!(error, Error::Limit(_)), "{error}");
    assert_eq!(
        listing(dir.path()),
        BTreeSet::from(["file.txt".to_string()])
    );
}

#[test]
fn duplicates_collisions_links_and_special_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"a").unwrap();
    fs::write(&b, b"b").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    let archive = dir.path().join("new.rar");
    let run = |sources: &[Source]| {
        create_rar(&archive, sources, &CreateOptions::default(), &|_, _, _| {
            true
        })
        .unwrap_err()
    };

    let error = run(&[source(&a, "same.txt"), source(&b, "same.txt")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("duplicate")),
        "{error}"
    );
    let error = run(&[source(&a, "Same.txt"), source(&b, "same.TXT")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("duplicate")),
        "{error}"
    );
    let error = run(&[source(&a, "dir\\x"), source(&b, "dir/x")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("duplicate")),
        "{error}"
    );

    let error = run(&[source(&a, "a"), source(&b, "a/b.txt")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("collision")),
        "{error}"
    );
    let error = run(&[source(&a, "deep/A/x.txt"), source(&b, "deep/a")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("collision")),
        "{error}"
    );
    let error = run(&[source(dir.path().join("sub"), "d"), source(&b, "d")]);
    assert!(
        matches!(&error, Error::Format(m) if m.contains("duplicate")),
        "{error}"
    );
    create_rar(
        &archive,
        &[source(dir.path().join("sub"), "d"), source(&b, "d/b.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap();
    fs::remove_file(&archive).unwrap();

    let error = run(&[source(dir.path().join("missing.txt"), "missing.txt")]);
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::NotFound),
        "{error}"
    );

    #[cfg(unix)]
    {
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&a, &link).unwrap();
        let error = run(&[source(&link, "link.txt")]);
        assert!(
            matches!(&error, Error::Unsupported(m) if m.contains("symbolic link")),
            "{error}"
        );
        let dangling = dir.path().join("dangling");
        std::os::unix::fs::symlink("nowhere", &dangling).unwrap();
        let error = run(&[source(&dangling, "dangling")]);
        assert!(
            matches!(&error, Error::Unsupported(m) if m.contains("symbolic link")),
            "{error}"
        );

        let fifo = dir.path().join("fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if made {
            let error = run(&[source(&fifo, "fifo")]);
            assert!(
                matches!(&error, Error::Unsupported(m) if m.contains("regular file")),
                "{error}"
            );
        } else {
            eprintln!("skipped: mkfifo unavailable");
        }
        let error = run(&[source(dir.path(), "here")]);
        assert!(
            matches!(&error, Error::Format(m) if m.contains("output directory")),
            "{error}"
        );
    }
    assert!(!archive.exists());
    assert!(listing(dir.path()).iter().all(|n| !n.contains(".part")));
}

#[test]
fn existing_outputs_are_never_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file.txt");
    fs::write(&file, b"payload").unwrap();
    let archive = dir.path().join("taken.rar");
    fs::write(&archive, b"not an archive, and precious").unwrap();
    let error = create_rar(
        &archive,
        &[source(&file, "file.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::AlreadyExists),
        "{error}"
    );
    assert_eq!(fs::read(&archive).unwrap(), b"not an archive, and precious");

    #[cfg(unix)]
    {
        let dangling = dir.path().join("dangling.rar");
        std::os::unix::fs::symlink("nowhere", &dangling).unwrap();
        let error = create_rar(
            &dangling,
            &[source(&file, "file.txt")],
            &CreateOptions::default(),
            &|_, _, _| true,
        )
        .unwrap_err();
        assert!(
            matches!(&error, Error::Io(e) if e.kind() == ErrorKind::AlreadyExists),
            "{error}"
        );
        assert!(fs::symlink_metadata(&dangling)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    let error = create_rar(
        &dir.path().join("comic.cbr"),
        &[source(&file, "file.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Unsupported(m) if m.contains("CBR")),
        "{error}"
    );

    let error = create_rar(
        &dir.path().join("missing/new.rar"),
        &[source(&file, "file.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::NotFound),
        "{error}"
    );
    assert!(!dir.path().join("missing").exists());

    let error = create_rar(
        &file.join("new.rar"),
        &[source(&file, "file.txt")],
        &CreateOptions::default(),
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error}");
    assert_eq!(fs::read(&file).unwrap(), b"payload");
}

#[test]
fn cancelling_before_and_during_the_write_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Xorshift(0x5eed_0002);
    let big = dir.path().join("big.bin");
    fs::write(&big, rng.bytes(6 * 1024 * 1024, true)).unwrap();
    let small = dir.path().join("small.txt");
    fs::write(&small, b"small").unwrap();
    let before = listing(dir.path());
    let sources = [source(&small, "small.txt"), source(&big, "big.bin")];
    let archive = dir.path().join("new.rar");

    let error = create_rar(&archive, &sources, &options(Level::Normal), &|_, _, _| {
        false
    })
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled), "{error}");
    assert_eq!(listing(dir.path()), before);

    let error = create_rar(
        &archive,
        &sources,
        &options(Level::Normal),
        &|done, _, _| done < 1,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled), "{error}");
    assert_eq!(listing(dir.path()), before);

    let seen = AtomicUsize::new(0);
    let error = create_rar(
        &archive,
        &sources,
        &options(Level::Normal),
        &|_, _, name| {
            if name == "big.bin" {
                seen.fetch_add(1, Ordering::Relaxed) < 2
            } else {
                true
            }
        },
    )
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled), "{error}");
    assert!(seen.load(Ordering::Relaxed) >= 2);
    assert_eq!(listing(dir.path()), before);

    let error = create_rar(
        &archive,
        &sources,
        &options(Level::Store),
        &|done, total, _| done < total,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled), "{error}");
    assert_eq!(listing(dir.path()), before);
}

#[test]
fn sources_that_change_during_the_write_fail_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Xorshift(0x5eed_0003);
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    fs::write(&a, rng.bytes(2 * 1024 * 1024, true)).unwrap();
    fs::write(&b, rng.bytes(64 * 1024, true)).unwrap();
    let before = listing(dir.path());
    let archive = dir.path().join("new.rar");

    // The scan snapshots sources in order, so touching `a` while `b` is being
    // scanned changes a file whose snapshot is already taken.
    let grown = std::sync::Once::new();
    let error = create_rar(
        &archive,
        &[source(&a, "a.bin"), source(&b, "b.bin")],
        &options(Level::Normal),
        &|_, _, name| {
            if name == "b.bin" {
                grown.call_once(|| {
                    let mut file = fs::OpenOptions::new().append(true).open(&a).unwrap();
                    std::io::Write::write_all(&mut file, b"late bytes").unwrap();
                });
            }
            true
        },
    )
    .unwrap_err();
    assert!(grown.is_completed());
    assert!(matches!(&error, Error::Io(_)), "{error}");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(listing(dir.path()), before);

    let removed = std::sync::Once::new();
    let error = create_rar(
        &archive,
        &[source(&a, "a.bin"), source(&b, "b.bin")],
        &options(Level::Store),
        &|_, _, name| {
            if name == "b.bin" {
                removed.call_once(|| fs::remove_file(&a).unwrap());
            }
            true
        },
    )
    .unwrap_err();
    assert!(removed.is_completed());
    assert!(matches!(&error, Error::Io(_)), "{error}");
    assert_eq!(
        listing(dir.path()),
        before.iter().filter(|n| *n != "a.bin").cloned().collect()
    );

    // The first callback is the scan of `b`; the second comes once its
    // snapshot is taken and the write is about to start.
    let calls = AtomicUsize::new(0);
    let error = create_rar(
        &archive,
        &[source(&b, "b.bin")],
        &options(Level::Fast),
        &|_, _, _| {
            if calls.fetch_add(1, Ordering::Relaxed) == 1 {
                let swap = dir.path().join("swap.bin");
                fs::write(&swap, b"same length? no").unwrap();
                fs::rename(&swap, &b).unwrap();
            }
            true
        },
    )
    .unwrap_err();
    assert!(matches!(&error, Error::Io(_)), "{error}");
    assert_eq!(fs::read(&b).unwrap(), b"same length? no");
    assert!(!archive.exists());
}

#[test]
fn size_and_count_limits_are_checked_before_reading_anything() {
    let dir = tempfile::tempdir().unwrap();
    let before = listing(dir.path());
    let archive = dir.path().join("new.rar");
    let small = dir.path().join("small.txt");
    fs::write(&small, b"x").unwrap();

    let many: Vec<Source> = (0..=create_limits::MAX_ENTRIES)
        .map(|i| source(&small, &format!("n{i}")))
        .collect();
    let error = create_rar(&archive, &many, &CreateOptions::default(), &|_, _, _| {
        panic!("limits must be checked before progress starts")
    })
    .unwrap_err();
    assert!(
        matches!(&error, Error::Limit(m) if m.contains("members")),
        "{error}"
    );

    let huge = dir.path().join("huge.bin");
    let sparse = fs::File::create(&huge).unwrap();
    if sparse.set_len(create_limits::MAX_MEMBER_BYTES + 1).is_ok() {
        let error = create_rar(
            &archive,
            &[source(&huge, "huge.bin")],
            &options(Level::Store),
            &|_, _, _| true,
        )
        .unwrap_err();
        assert!(
            matches!(&error, Error::Limit(m) if m.contains("4 GiB")),
            "{error}"
        );

        sparse.set_len(create_limits::MAX_MEMBER_BYTES).unwrap();
        let five: Vec<Source> = (0..5).map(|i| source(&huge, &format!("h{i}"))).collect();
        let error =
            create_rar(&archive, &five, &options(Level::Store), &|_, _, _| true).unwrap_err();
        assert!(
            matches!(&error, Error::Limit(m) if m.contains("16 GiB")),
            "{error}"
        );
    } else {
        eprintln!("skipped: sparse files unavailable");
    }
    drop(sparse);
    fs::remove_file(&huge).unwrap();
    fs::remove_file(&small).unwrap();
    assert_eq!(listing(dir.path()), before);
}

#[test]
fn header_budget_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let small = dir.path().join("small.txt");
    fs::write(&small, b"x").unwrap();
    let name_len = create_limits::MAX_NAME_BYTES - 8;
    let needed = (create_limits::MAX_HEADER_BYTES / (name_len as u64 + 128)) as usize + 1;
    assert!(needed <= create_limits::MAX_ENTRIES);
    let sources: Vec<Source> = (0..needed)
        .map(|i| {
            source(
                &small,
                &format!("{:0>width$}/{i:07}", "d", width = name_len - 8),
            )
        })
        .collect();
    let error = create_rar(
        &dir.path().join("new.rar"),
        &sources,
        &options(Level::Store),
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Limit(m) if m.contains("header")),
        "{error}"
    );
}

#[test]
fn empty_members_compress_under_the_managed_memory_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::create_dir_all(input.join("empty-dir")).unwrap();
    fs::write(input.join("empty.bin"), b"").unwrap();
    fs::write(input.join("one.bin"), b"x").unwrap();
    let mut rng = Xorshift(0x5eed_0007);
    let big = rng.bytes(5 * 1024 * 1024, true);
    fs::write(input.join("big.bin"), &big).unwrap();
    let sources = vec![
        source(input.join("empty-dir"), "empty-dir"),
        source(input.join("empty.bin"), "empty.bin"),
        source(input.join("one.bin"), "one.bin"),
        source(input.join("big.bin"), "big.bin"),
        source(input.join("empty-dir"), "nested/also-empty"),
    ];
    let expected = vec![
        ("empty-dir".to_string(), None),
        ("empty.bin".to_string(), Some(Vec::new())),
        ("one.bin".to_string(), Some(b"x".to_vec())),
        ("big.bin".to_string(), Some(big)),
        ("nested/also-empty".to_string(), None),
    ];
    for (level, name) in [
        (Level::Store, "store.rar"),
        (Level::Fast, "fast.rar"),
        (Level::Normal, "normal.rar"),
        (Level::Best, "best.rar"),
    ] {
        let archive = dir.path().join(name);
        create_rar(&archive, &sources, &options(level), &|_, _, _| true).unwrap();
        assert_roundtrip(&archive, &expected);
        let a = RarArchive::open(&archive, None).unwrap();
        let big = a.entries().iter().find(|e| e.name == "big.bin").unwrap();
        if level == Level::Store {
            assert_eq!(big.compressed_size, big.size, "{name}");
        } else {
            assert!(big.compressed_size < big.size / 4, "{name}");
        }
    }
    assert!(listing(dir.path()).iter().all(|n| !n.ends_with(".part")));
}

#[cfg(unix)]
#[test]
fn sources_swapped_for_pipes_or_links_at_reopen_are_refused() {
    let probe = tempfile::tempdir().unwrap();
    let made = Command::new("mkfifo")
        .arg(probe.path().join("probe"))
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !made {
        eprintln!("skipped: mkfifo unavailable");
        return;
    }
    // A FIFO with no writer blocks a plain open forever, so the whole case
    // runs under a deadline.
    bounded(Duration::from_secs(60), || {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut rng = Xorshift(0x5eed_0008);
        let data = rng.bytes(1024 * 1024, true);
        fs::write(&b, b"second").unwrap();
        let archive = dir.path().join("new.rar");
        let swapped = |f: &(dyn Fn(&Path) + Sync)| {
            let _ = fs::remove_file(&a);
            fs::write(&a, &data).unwrap();
            let once = std::sync::Once::new();
            let error = create_rar(
                &archive,
                &[source(&a, "a.bin"), source(&b, "b.bin")],
                &options(Level::Normal),
                &|_, _, name| {
                    if name == "b.bin" {
                        once.call_once(|| f(&a));
                    }
                    true
                },
            )
            .unwrap_err();
            assert!(once.is_completed());
            assert!(matches!(&error, Error::Io(_)), "{error}");
            assert!(!archive.exists());
            assert!(listing(dir.path()).iter().all(|n| !n.ends_with(".part")));
            error.to_string()
        };

        let error = swapped(&|path| {
            fs::remove_file(path).unwrap();
            assert!(Command::new("mkfifo").arg(path).status().unwrap().success());
        });
        assert!(
            error.contains("changed") || error.contains("regular") || error.contains("symbolic"),
            "{error}"
        );

        let error = swapped(&|path| {
            let target = path.with_file_name("elsewhere.bin");
            fs::copy(path, &target).unwrap();
            fs::remove_file(path).unwrap();
            std::os::unix::fs::symlink(&target, path).unwrap();
        });
        assert!(
            error.contains("changed") || error.contains("regular") || error.contains("symbolic"),
            "{error}"
        );

        let error = swapped(&|path| {
            fs::remove_file(path).unwrap();
            fs::create_dir(path).unwrap();
        });
        assert!(
            error.contains("changed") || error.contains("regular") || error.contains("symbolic"),
            "{error}"
        );
    });
}

#[test]
fn an_output_that_appears_during_the_write_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let mut rng = Xorshift(0x5eed_0009);
    fs::write(&a, rng.bytes(3 * 1024 * 1024, true)).unwrap();
    let archive = dir.path().join("new.rar");
    let calls = AtomicUsize::new(0);
    let planted = std::sync::Once::new();
    let error = create_rar(
        &archive,
        &[source(&a, "a.bin")],
        &options(Level::Fast),
        &|_, _, _| {
            // The first callback scans `a`; later ones come from the write.
            if calls.fetch_add(1, Ordering::Relaxed) >= 2 {
                planted.call_once(|| fs::write(&archive, b"keep me").unwrap());
            }
            true
        },
    )
    .unwrap_err();
    assert!(planted.is_completed());
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::AlreadyExists),
        "{error}"
    );
    assert_eq!(fs::read(&archive).unwrap(), b"keep me");
    assert_eq!(
        listing(dir.path()),
        BTreeSet::from(["a.bin".to_string(), "new.rar".to_string()])
    );
}

#[test]
fn seeded_stress_trees_roundtrip_at_every_level() {
    bounded(Duration::from_secs(240), || {
        for (seed, level) in [
            (0x5eed_1001u64, Level::Store),
            (0x5eed_1002, Level::Fast),
            (0x5eed_1003, Level::Normal),
            (0x5eed_1004, Level::Best),
        ] {
            let mut rng = Xorshift(seed);
            let dir = tempfile::tempdir().unwrap();
            let input = dir.path().join("in");
            fs::create_dir(&input).unwrap();
            let alphabet: Vec<char> = "abcxyz019 _-.ñé日".chars().collect();
            let mut sources = Vec::new();
            let mut expected = Vec::new();
            let mut names = BTreeSet::new();
            let mut total = 0usize;
            while sources.len() < 160 && total < 24 * 1024 * 1024 {
                let depth = rng.below(4) as usize;
                let mut parts = Vec::new();
                for _ in 0..=depth {
                    let len = 1 + rng.below(12) as usize;
                    let mut part: String = (0..len)
                        .map(|_| alphabet[rng.below(alphabet.len() as u64) as usize])
                        .collect();
                    part = part.trim_end_matches([' ', '.']).to_string();
                    if part.is_empty() || part == ".." {
                        part = "x".into();
                    }
                    parts.push(part);
                }
                let name = parts.join("/");
                let folded = name.to_lowercase();
                if names.iter().any(|n: &String| {
                    n == &folded
                        || n.starts_with(&format!("{folded}/"))
                        || folded.starts_with(&format!("{n}/"))
                }) {
                    continue;
                }
                names.insert(folded);
                let path = input.join(&name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                let kind = rng.below(10);
                if kind == 0 {
                    fs::create_dir(&path).unwrap();
                    sources.push(source(&path, &name));
                    expected.push((name, None));
                    continue;
                }
                let len = match kind {
                    1 => 0,
                    2..=6 => rng.below(4096) as usize,
                    7..=8 => rng.below(256 * 1024) as usize,
                    _ => rng.below(2 * 1024 * 1024) as usize,
                };
                let compressible = rng.below(2) == 0;
                let data = rng.bytes(len, compressible);
                fs::write(&path, &data).unwrap();
                total += len;
                sources.push(source(&path, &name));
                expected.push((name, Some(data)));
            }
            let archive = dir.path().join("stress.rar");
            create_rar(&archive, &sources, &options(level), &|_, _, _| true).unwrap();
            assert_roundtrip(&archive, &expected);
            assert_eq!(
                listing(dir.path()),
                BTreeSet::from(["in".to_string(), "stress.rar".to_string()])
            );
        }
    });
}

/// An external RAR tool that is proven to decode compressed RAR5 members by
/// testing a WinRAR-made fixture first; old p7zip builds list RAR5 but only
/// decode stored entries.
struct ExternalTool {
    program: &'static str,
    test: &'static [&'static str],
    extract: fn(&Path, &Path) -> Command,
}

fn external_tool() -> Option<ExternalTool> {
    fn sevenzip(archive: &Path, out: &Path) -> Command {
        let mut c = Command::new("7z");
        c.arg("x").arg(format!("-o{}", out.display())).arg(archive);
        c
    }
    fn sevenzz(archive: &Path, out: &Path) -> Command {
        let mut c = Command::new("7zz");
        c.arg("x").arg(format!("-o{}", out.display())).arg(archive);
        c
    }
    fn unrar(archive: &Path, out: &Path) -> Command {
        let mut c = Command::new("unrar");
        c.arg("x")
            .arg("-idq")
            .arg(archive)
            .arg(format!("{}/", out.display()));
        c
    }
    let candidates = [
        ExternalTool {
            program: "unrar",
            test: &["t", "-idq"],
            extract: unrar,
        },
        ExternalTool {
            program: "7zz",
            test: &["t"],
            extract: sevenzz,
        },
        ExternalTool {
            program: "7z",
            test: &["t"],
            extract: sevenzip,
        },
    ];
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plain.rar");
    candidates.into_iter().find(|tool| {
        Command::new(tool.program)
            .args(tool.test)
            .arg(&fixture)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

#[test]
fn external_tools_can_read_what_arca_writes() {
    let Some(tool) = external_tool() else {
        eprintln!("skipped: no external tool decodes compressed RAR5 here");
        return;
    };
    for (level, label) in [
        (Level::Store, "store"),
        (Level::Fast, "fast"),
        (Level::Normal, "normal"),
        (Level::Best, "best"),
    ] {
        let tree = fixture_tree();
        let archive = tree.root.path().join(format!("{label}.rar"));
        create_rar(&archive, &tree.sources, &options(level), &|_, _, _| true).unwrap();
        let status = Command::new(tool.program)
            .args(tool.test)
            .arg(&archive)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "{}: {label} test failed", tool.program);
        let out = tree.root.path().join("external-out");
        fs::create_dir(&out).unwrap();
        let status = (tool.extract)(&archive, &out)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "{}: {label} extract failed", tool.program);
        for (name, data) in &tree.expected {
            match data {
                Some(data) => {
                    assert_eq!(&fs::read(out.join(name)).unwrap(), data, "{label}: {name}")
                }
                None => assert!(out.join(name).is_dir(), "{label}: {name}"),
            }
        }
    }
}
