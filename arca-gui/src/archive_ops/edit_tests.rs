use super::*;
use crate::i18n::{strings, Lang};
use crate::test_support::Room;
use arca_tar::{TarReader, TarWriter};
use std::sync::atomic::{AtomicUsize, Ordering};

const GO: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| true;

fn tar_formats() -> Vec<Format> {
    let mut formats = vec![Format::Tar, Format::TarGz, Format::TarXz, Format::TarBz2];
    if cfg!(feature = "codecs-native") {
        formats.push(Format::TarZst);
    }
    formats
}

/// A TAR with what an archiver has to keep: a folder with its own time and
/// permissions, a program, a symbolic link and a hard link.
fn fixture(room: &Room, format: Format) -> PathBuf {
    let mut w = TarWriter::new(Vec::new());
    w.add("top/", 0, 1_500_000_000, 0o750, std::io::empty())
        .unwrap();
    w.add("top/a.txt", 5, 1_500_000_001, 0o644, &b"alpha"[..])
        .unwrap();
    w.add("top/run.sh", 9, 1_500_000_002, 0o755, &b"#!/bin/sh"[..])
        .unwrap();
    let big = vec![7u8; 300_000];
    w.add(
        "top/inner/b.bin",
        big.len() as u64,
        1_500_000_003,
        0o600,
        &big[..],
    )
    .unwrap();
    w.add_link("top/link", "a.txt", 1_500_000_004, b'2')
        .unwrap();
    w.add_link("top/hard", "top/a.txt", 1_500_000_005, b'1')
        .unwrap();
    let tar = w.finish().unwrap();
    let path = room.path(&format!("fixture.{}", format.extension()));
    let bytes = match format {
        Format::Tar => tar,
        Format::TarGz => {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(&tar).unwrap();
            e.finish().unwrap()
        }
        _ => {
            let mut e = arca_stream::encoder(Vec::new(), format, Level::FAST, 1, None).unwrap();
            e.write_all(&tar).unwrap();
            e.finish().unwrap()
        }
    };
    fs::write(&path, bytes).unwrap();
    assert_eq!(detect(&path), Some(format));
    path
}

type Listed = (String, u32, i64, u8, Option<String>, Vec<u8>);

fn tar_listing(path: &Path, format: Format) -> Vec<Listed> {
    let mut r = TarReader::new(open_source(path, format).unwrap());
    let mut out = Vec::new();
    while let Some(e) = r.next_entry().unwrap() {
        let mut data = Vec::new();
        r.copy_data(&e, &mut data).unwrap();
        out.push((
            e.entry.name.clone(),
            e.mode,
            e.entry.mtime.unwrap_or(0),
            e.kind,
            e.link.clone(),
            data,
        ));
    }
    out
}

fn run(job: Job) -> std::result::Result<String, String> {
    run_job_blocking(job, strings(Lang::En), GO, &|_| Answer::Cancel)
}

fn delete(archive: &Path, names: &[&str], password: Option<&str>) -> Job {
    Job::Delete {
        archive: archive.to_path_buf(),
        names: names.iter().map(|n| n.to_string()).collect(),
        password: password.map(str::to_string),
    }
}

fn rename(archive: &Path, from: &str, to: &str, folder: bool, password: Option<&str>) -> Job {
    Job::Rename {
        archive: archive.to_path_buf(),
        from: from.into(),
        to: to.into(),
        folder,
        password: password.map(str::to_string),
    }
}

fn add(archive: &Path, inputs: Vec<PathBuf>, dir: &str, password: Option<&str>) -> Job {
    Job::Add {
        archive: archive.to_path_buf(),
        inputs,
        dir: dir.into(),
        codec: Codec::Deflate,
        level: Level::FAST,
        password: password.map(str::to_string),
    }
}

fn new_folder(archive: &Path, name: &str, password: Option<&str>) -> Job {
    Job::NewFolder {
        archive: archive.to_path_buf(),
        name: name.into(),
        password: password.map(str::to_string),
    }
}

fn moving(archive: &Path, from: &str, to: &str, password: Option<&str>) -> Job {
    Job::Move {
        archive: archive.to_path_buf(),
        moves: vec![(from.into(), to.into())],
        password: password.map(str::to_string),
    }
}

fn leftovers(room: &Room) -> Vec<String> {
    fs::read_dir(&room.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".arca-new"))
        .collect()
}

#[test]
fn every_change_keeps_a_tar_whole_in_every_compression() {
    for format in tar_formats() {
        let room = Room::new();
        let archive = fixture(&room, format);
        let original = tar_listing(&archive, format);
        let renamed = |e: &Listed, from: &str, to: &str| {
            let mut e = e.clone();
            if let Some(rest) = e.0.strip_prefix(from) {
                e.0 = format!("{to}{rest}");
            }
            if let Some(rest) =
                e.4.as_deref()
                    .filter(|_| e.3 == b'1')
                    .and_then(|l| l.strip_prefix(from))
            {
                e.4 = Some(format!("{to}{rest}"));
            }
            e
        };

        run(delete(&archive, &["top/inner/b.bin"], None)).unwrap();
        assert_eq!(detect(&archive), Some(format));
        let mut want: Vec<Listed> = original.clone();
        want.retain(|e| e.0 != "top/inner/b.bin");
        assert_eq!(tar_listing(&archive, format), want, "{format:?} delete");
        assert_eq!(
            tar_listing(&undo_path(&archive), format),
            original,
            "{format:?} undo copy"
        );

        run(rename(&archive, "top/a.txt", "top/c.txt", false, None)).unwrap();
        want = want
            .iter()
            .map(|e| renamed(e, "top/a.txt", "top/c.txt"))
            .collect();
        assert_eq!(tar_listing(&archive, format), want, "{format:?} rename");

        run(rename(&archive, "top", "moved", true, None)).unwrap();
        want = want.iter().map(|e| renamed(e, "top/", "moved/")).collect();
        assert_eq!(
            tar_listing(&archive, format),
            want,
            "{format:?} folder rename"
        );

        run(moving(&archive, "moved/run.sh", "moved/bin/run.sh", None)).unwrap();
        want = want
            .iter()
            .map(|e| renamed(e, "moved/run.sh", "moved/bin/run.sh"))
            .collect();
        assert_eq!(tar_listing(&archive, format), want, "{format:?} move");

        run(new_folder(&archive, "moved/empty/", None)).unwrap();
        let got = tar_listing(&archive, format);
        let folder = got.iter().find(|e| e.0 == "moved/empty/").unwrap();
        assert_eq!((folder.1, folder.3), (0o755, b'5'));
        assert_eq!(got[..want.len()], want[..], "{format:?} new folder");

        let extra = room.path("extra.txt");
        fs::write(&extra, b"added later").unwrap();
        run(add(&archive, vec![extra], "moved/", None)).unwrap();
        fs::create_dir_all(room.path("second")).unwrap();
        let replacement = room.path("second/extra.txt");
        fs::write(&replacement, b"replaced").unwrap();
        run(add(&archive, vec![replacement], "moved/", None)).unwrap();
        let got = tar_listing(&archive, format);
        let find = |name: &str| got.iter().filter(|e| e.0 == name).collect::<Vec<_>>();
        assert_eq!(find("moved/extra.txt").len(), 1);
        assert_eq!(find("moved/extra.txt")[0].5, b"replaced");
        assert_eq!(find("moved/c.txt")[0].5, b"alpha");
        assert_eq!(find("moved/hard")[0].4.as_deref(), Some("moved/c.txt"));
        assert_eq!(find("moved/link")[0].4.as_deref(), Some("a.txt"));
        assert_eq!(find("moved/bin/run.sh")[0].1, 0o755);
        assert!(leftovers(&room).is_empty(), "{format:?}");
    }
}

#[test]
fn deleting_everything_leaves_an_empty_tar_that_still_opens() {
    for format in tar_formats() {
        let room = Room::new();
        let archive = fixture(&room, format);
        let names: Vec<String> = tar_listing(&archive, format)
            .into_iter()
            .map(|e| e.0)
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        run(delete(&archive, &names, None)).unwrap();
        assert_eq!(detect(&archive), Some(format), "{format:?}");
        let (entries, _) = list_entries(&archive, None, GO).unwrap();
        assert!(entries.is_empty(), "{format:?}");
    }
}

#[test]
fn a_hard_link_keeps_its_target_from_being_deleted() {
    let room = Room::new();
    let archive = fixture(&room, Format::TarGz);
    let before = fs::read(&archive).unwrap();
    let error = run(delete(&archive, &["top/a.txt"], None)).unwrap_err();
    assert!(error.contains("hard link"), "{error}");
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert!(!undo_path(&archive).exists());
}

/// Every format, stopped at every few progress calls along the way.
#[test]
fn a_stop_halfway_leaves_the_original_untouched() {
    let mut formats = tar_formats();
    formats.push(Format::SevenZ);
    for format in formats {
        let room = Room::new();
        let archive = if format == Format::SevenZ {
            room.archive(None, false)
        } else {
            fixture(&room, format)
        };
        let before = fs::read(&archive).unwrap();
        let calls = AtomicUsize::new(0);
        let count: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| {
            calls.fetch_add(1, Ordering::Relaxed);
            true
        };
        let names = vec!["nothing".to_string()];
        let job = |archive: &Path| Job::Delete {
            archive: archive.to_path_buf(),
            names: names.clone(),
            password: None,
        };
        let copy = room.path(&format!("count.{}", format.extension()));
        fs::copy(&archive, &copy).unwrap();
        run_job_blocking(job(&copy), strings(Lang::En), count, &|_| Answer::Cancel).unwrap();
        let total = calls.load(Ordering::Relaxed);
        assert!(total > 2, "{format:?}");
        for stop_at in (1..total).step_by((total / 7).max(1)) {
            let seen = AtomicUsize::new(0);
            let stop: &(dyn Fn(usize, usize, &str) -> bool + Sync) =
                &|_, _, _| seen.fetch_add(1, Ordering::Relaxed) + 1 < stop_at;
            let result =
                run_job_blocking(job(&archive), strings(Lang::En), stop, &|_| Answer::Cancel);
            assert!(result.is_err(), "{format:?} stopped at {stop_at}");
            assert_eq!(
                fs::read(&archive).unwrap(),
                before,
                "{format:?} at {stop_at}"
            );
            assert!(!undo_path(&archive).exists(), "{format:?} at {stop_at}");
            assert!(leftovers(&room).is_empty(), "{format:?} at {stop_at}");
        }
    }
}

#[test]
fn damaged_archives_are_refused_and_left_alone() {
    let mut formats = tar_formats();
    formats.push(Format::SevenZ);
    for format in formats {
        let room = Room::new();
        let archive = if format == Format::SevenZ {
            room.archive(Some("secret"), false)
        } else {
            fixture(&room, format)
        };
        let whole = fs::read(&archive).unwrap();
        let password = (format == Format::SevenZ).then_some("secret");
        let mut cuts: Vec<usize> = (1..8).map(|i| whole.len() * i / 8).collect();
        if format != Format::Tar {
            cuts.push(whole.len() - 1);
        }
        for cut in cuts {
            fs::write(&archive, &whole[..cut]).unwrap();
            let result = run(delete(
                &archive,
                &["top/inner/b.bin", "source/b.txt"],
                password,
            ));
            assert!(result.is_err(), "{format:?} cut at {cut}");
            assert_eq!(fs::read(&archive).unwrap(), &whole[..cut], "{format:?}");
            assert!(leftovers(&room).is_empty(), "{format:?}");
        }
        let mut flipped = whole.clone();
        let middle = flipped.len() / 2;
        flipped[middle] ^= 0x55;
        fs::write(&archive, &flipped).unwrap();
        if run(delete(&archive, &["nothing"], password)).is_ok() {
            assert_eq!(
                format,
                Format::Tar,
                "only a plain tar has no checksum over its data"
            );
        }
    }
}

fn sevenz_listing(
    path: &Path,
    password: Option<&str>,
) -> Vec<(String, bool, Option<i64>, Vec<u8>)> {
    let (entries, _) = list_entries(path, password, GO).unwrap();
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let mut data = Vec::new();
        if !e.is_dir {
            read_entry(path, i, &mut data, password, GO).unwrap();
        }
        out.push((e.name.clone(), e.is_dir, e.mtime, data));
    }
    out.sort();
    out
}

#[test]
fn every_change_works_on_a_7z_with_and_without_a_password() {
    for (password, hidden) in [
        (None, false),
        (Some("secret"), false),
        (Some("secret"), true),
    ] {
        let room = Room::new();
        let archive = room.archive(password, hidden);
        let original = sevenz_listing(&archive, password);
        let names = |path: &Path| -> Vec<String> {
            sevenz_listing(path, password)
                .into_iter()
                .map(|e| e.0)
                .collect()
        };

        run(delete(&archive, &["source/a.txt"], password)).unwrap();
        let mut want = original.clone();
        want.retain(|e| e.0 != "source/a.txt");
        assert_eq!(sevenz_listing(&archive, password), want);
        let kept = room.path("kept.7z");
        fs::copy(undo_path(&archive), &kept).unwrap();
        assert_eq!(sevenz_listing(&kept, password), original);

        run(rename(&archive, "source", "kept", true, password)).unwrap();
        assert_eq!(names(&archive), ["kept/", "kept/b.txt", "kept/empty/"]);
        run(moving(&archive, "kept/b.txt", "kept/empty/b.txt", password)).unwrap();
        run(new_folder(&archive, "kept/new/", password)).unwrap();
        let extra = room.path("extra.txt");
        fs::write(&extra, b"added later").unwrap();
        run(add(&archive, vec![extra], "kept/", password)).unwrap();
        let got = sevenz_listing(&archive, password);
        let names: Vec<&str> = got.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(
            names,
            [
                "kept/",
                "kept/empty/",
                "kept/empty/b.txt",
                "kept/extra.txt",
                "kept/new/"
            ]
        );
        assert_eq!(got[2].3, b"second contents");
        assert_eq!(got[3].3, b"added later");
        assert!(got[4].1);
        let moved = original.iter().find(|e| e.0 == "source/b.txt").unwrap();
        assert_eq!(got[2].2, moved.2, "the time stays with the file");

        let again = arca_7z::SevenZArchive::open(File::open(&archive).unwrap(), password).unwrap();
        assert_eq!(again.has_encrypted(), password.is_some());
        assert_eq!(again.has_encrypted_header(), hidden);
        if password.is_some() {
            assert!(run(delete(&archive, &["kept/new"], None)).is_err());
            assert!(run(delete(&archive, &["kept/new"], Some("wrong"))).is_err());
        }
        assert!(leftovers(&room).is_empty());
    }
}

#[test]
fn single_streams_and_zip_containers_still_cannot_change() {
    let room = Room::new();
    let source = room.path("one.txt");
    fs::write(&source, b"one").unwrap();
    let xz = room.path("one.txt.xz");
    compress(
        &xz,
        &[source],
        Format::Xz,
        Codec::Deflate,
        Level::FAST,
        GO,
        (None, false),
    )
    .unwrap();
    let error = run(delete(&xz, &["one.txt"], None)).unwrap_err();
    assert_eq!(error, strings(Lang::En).only_archives_change);
    let jar = room.path("lib.jar");
    fs::write(&jar, b"PK").unwrap();
    assert!(run(delete(&jar, &["a"], None))
        .unwrap_err()
        .contains("read-only"));
    for format in [
        Format::Xz,
        Format::Bz2,
        Format::Zst,
        Format::Jar,
        Format::Rar,
        Format::Iso,
    ] {
        assert!(!editable(format), "{format:?}");
    }
    for format in tar_formats()
        .into_iter()
        .chain([Format::Zip, Format::SevenZ])
    {
        assert!(editable(format), "{format:?}");
    }
    let password = Job::Password {
        archive: room.path("a.7z"),
        current: None,
        new: Some("x".into()),
    };
    assert!(!can_edit(&password));
    assert!(can_edit(&delete(&room.path("a.tar.gz"), &["x"], None)));
}
