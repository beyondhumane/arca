use arca_7z::{
    create_7z, rewrite_7z, Addition, Change, CreateOptions, Phase, SevenZArchive, Source,
};
use arca_core::{Entry, Error, Level, Method};
use sevenz_rust2::{
    encoder_options::Lzma2Options, Archive, ArchiveEntry, ArchiveWriter, NtTime, Password,
    SourceReader,
};
use std::{
    fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
};

struct Room(tempfile::TempDir);

impl Room {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn write(&self, name: &str, data: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data).unwrap();
        path
    }
}

/// A plain archive Arca would make: a folder, an empty folder, an empty
/// file, two solid members and an executable in a block of its own.
fn made(room: &Room, password: Option<&str>, hide_names: bool) -> PathBuf {
    let sources = [
        ("src/", None),
        ("src/a.txt", Some(b"first file ".repeat(300))),
        ("src/b.txt", Some(b"second file ".repeat(200))),
        ("src/empty.txt", Some(Vec::new())),
        ("src/hollow/", None),
        ("tool.exe", Some(b"MZ\x90\0 not really a program".to_vec())),
    ];
    let mut list = Vec::new();
    for (name, data) in sources {
        let path = match data {
            Some(data) => room.write(&format!("in/{name}"), &data),
            None => {
                let path = room.path(&format!("in/{name}"));
                fs::create_dir_all(&path).unwrap();
                path
            }
        };
        list.push(Source {
            path,
            name: name.trim_end_matches('/').to_string(),
        });
    }
    let out = room.path("made.7z");
    create_7z(
        &out,
        &list,
        &CreateOptions {
            level: Level::FAST,
            password,
            hide_names,
            ..Default::default()
        },
        &mut |_| true,
    )
    .unwrap();
    out
}

type Listed = (String, bool, Option<i64>, Vec<u8>);

fn listed(path: &Path, password: Option<&str>) -> Vec<Listed> {
    let mut archive = SevenZArchive::open(fs::File::open(path).unwrap(), password).unwrap();
    let entries = archive.entries().to_vec();
    let mut data = vec![Vec::new(); entries.len()];
    let all: Vec<usize> = (0..entries.len()).collect();
    archive
        .extract(
            &all,
            &mut |i, _, r| {
                io::Read::read_to_end(r, &mut data[i])?;
                Ok(())
            },
            &mut |_| true,
        )
        .unwrap();
    let mut out: Vec<Listed> = entries
        .into_iter()
        .zip(data)
        .map(|(e, d)| (e.name, e.is_dir, e.mtime, d))
        .collect();
    out.sort();
    out
}

fn rewrite(
    input: &Path,
    output: &Path,
    password: Option<&str>,
    name: &dyn Fn(&Entry) -> Option<String>,
    add: &[Addition],
) -> arca_core::Result<()> {
    let change = Change {
        name,
        add,
        level: Level::FAST,
    };
    rewrite_7z(input, output, password, &change, &mut |_| true)
}

fn keep(e: &Entry) -> Option<String> {
    Some(e.name.clone())
}

#[test]
fn deleting_keeps_every_other_entry_with_its_contents_and_times() {
    let room = Room::new();
    let input = made(&room, None, false);
    let out = room.path("out.7z");
    rewrite(
        &input,
        &out,
        None,
        &|e| (e.name != "src/a.txt").then(|| e.name.clone()),
        &[],
    )
    .unwrap();
    let mut want = listed(&input, None);
    want.retain(|e| e.0 != "src/a.txt");
    assert_eq!(listed(&out, None), want);
}

#[test]
fn renaming_a_folder_moves_everything_under_it() {
    let room = Room::new();
    let input = made(&room, None, false);
    let out = room.path("out.7z");
    let moved = |name: &str| match name.strip_prefix("src") {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("dst{rest}"),
        _ => name.to_string(),
    };
    rewrite(
        &input,
        &out,
        None,
        &|e| Some(format!("{}/", moved(&e.name))),
        &[],
    )
    .unwrap();
    let want: Vec<Listed> = {
        let mut v: Vec<Listed> = listed(&input, None)
            .into_iter()
            .map(|mut e| {
                e.0 = moved(&e.0);
                e
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(listed(&out, None), want);
}

#[test]
fn adding_files_and_a_folder_replaces_what_had_the_name() {
    let room = Room::new();
    let input = made(&room, None, false);
    let out = room.path("out.7z");
    let new = room.write("new/c.txt", b"third file");
    let replacement = room.write("new/b.txt", b"replaced");
    let empty = room.write("new/zero", b"");
    let add = [
        Addition {
            source: Some(new),
            name: "src/c.txt".into(),
        },
        Addition {
            source: Some(replacement),
            name: "src/b.txt".into(),
        },
        Addition {
            source: Some(empty),
            name: "zero".into(),
        },
        Addition {
            source: None,
            name: "fresh/".into(),
        },
    ];
    rewrite(&input, &out, None, &keep, &add).unwrap();
    let got = listed(&out, None);
    let find = |name: &str| got.iter().find(|e| e.0 == name).unwrap();
    assert_eq!(find("src/c.txt").3, b"third file");
    assert_eq!(find("src/b.txt").3, b"replaced");
    assert_eq!(find("src/a.txt").3, b"first file ".repeat(300));
    assert!(find("fresh/").1);
    assert!(!find("zero").1 && find("zero").3.is_empty());
    assert!(find("src/hollow").1);
    assert_eq!(got.iter().filter(|e| e.0 == "src/b.txt").count(), 1);
    assert_eq!(got.len(), 9);
}

#[test]
fn an_encrypted_archive_keeps_its_password_and_hidden_names() {
    for hidden in [false, true] {
        let room = Room::new();
        let input = made(&room, Some("secret"), hidden);
        let out = room.path("out.7z");
        assert!(matches!(
            rewrite(&input, &out, None, &keep, &[]),
            Err(Error::PasswordRequired)
        ));
        rewrite(
            &input,
            &out,
            Some("secret"),
            &|e| (e.name != "src/b.txt").then(|| e.name.clone()),
            &[],
        )
        .unwrap();
        let again = SevenZArchive::open(fs::File::open(&out).unwrap(), Some("secret")).unwrap();
        assert!(again.has_encrypted());
        assert_eq!(again.has_encrypted_header(), hidden);
        assert!(again
            .entries()
            .iter()
            .filter(|e| e.size > 0)
            .all(|e| e.encrypted));
        let mut want = listed(&input, Some("secret"));
        want.retain(|e| e.0 != "src/b.txt");
        assert_eq!(listed(&out, Some("secret")), want);
        if hidden {
            assert!(matches!(
                SevenZArchive::open(fs::File::open(&out).unwrap(), None),
                Err(Error::PasswordRequired)
            ));
        }
    }
}

#[test]
fn a_wrong_password_fails_and_writes_nothing_usable() {
    let room = Room::new();
    let input = made(&room, Some("secret"), false);
    let out = room.path("out.7z");
    let error = rewrite(&input, &out, Some("wrong"), &keep, &[]).unwrap_err();
    assert!(arca_7z::password_required(&error), "{error:?}");
}

#[test]
fn a_stop_halfway_is_a_cancellation() {
    let room = Room::new();
    let input = made(&room, None, false);
    for stop_at in [1, 3, 6, 12] {
        let out = room.path("out.7z");
        let mut calls = 0;
        let change = Change {
            name: &keep,
            add: &[],
            level: Level::FAST,
        };
        let result = rewrite_7z(&input, &out, None, &change, &mut |_| {
            calls += 1;
            calls < stop_at
        });
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "{stop_at}: {result:?}"
        );
    }
}

#[test]
fn stored_blocks_stay_stored_and_deleting_everything_leaves_an_empty_archive() {
    let room = Room::new();
    let a = room.write("in/a", &b"stored ".repeat(100));
    let input = room.path("stored.7z");
    create_7z(
        &input,
        &[Source {
            path: a,
            name: "a".into(),
        }],
        &CreateOptions {
            level: Level::STORE,
            ..Default::default()
        },
        &mut |_| true,
    )
    .unwrap();
    let out = room.path("out.7z");
    rewrite(&input, &out, None, &|e| Some(format!("b{}", e.name)), &[]).unwrap();
    let again = SevenZArchive::open(fs::File::open(&out).unwrap(), None).unwrap();
    assert_eq!(again.entries()[0].method, Method::Store);
    assert_eq!(again.entries()[0].name, "ba");
    rewrite(&input, &out, None, &|_| None, &[]).unwrap();
    assert!(SevenZArchive::open(fs::File::open(&out).unwrap(), None)
        .unwrap()
        .is_empty());
}

#[test]
fn attributes_and_all_three_times_survive() {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![Lzma2Options::from_level(1).into()]);
    let stamp = |seconds: u64| NtTime::from((seconds + 11_644_473_600) * 10_000_000);
    let stamped = |mut e: ArchiveEntry, attributes: u32| {
        e.has_windows_attributes = true;
        e.windows_attributes = attributes;
        e.has_creation_date = true;
        e.creation_date = stamp(1_000_000_000);
        e.has_last_modified_date = true;
        e.last_modified_date = stamp(1_100_000_000);
        e.has_access_date = true;
        e.access_date = stamp(1_200_000_000);
        e
    };
    writer
        .push_archive_entry::<io::Empty>(stamped(ArchiveEntry::new_directory("d"), 0x10), None)
        .unwrap();
    writer
        .push_archive_entries(
            vec![
                stamped(ArchiveEntry::new_file("d/x"), 0x21),
                stamped(ArchiveEntry::new_file("d/y"), 0x8000_0020 | (0o755 << 16)),
            ],
            vec![SourceReader::new(&b"xx"[..]), SourceReader::new(&b"yy"[..])],
        )
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let room = Room::new();
    let input = room.write("attrs.7z", &bytes);
    let out = room.path("out.7z");
    rewrite(
        &input,
        &out,
        None,
        &|e| (e.name != "d/x").then(|| e.name.replace("d", "e")),
        &[],
    )
    .unwrap();
    let read = |path: &Path| {
        Archive::read(&mut fs::File::open(path).unwrap(), &Password::empty())
            .unwrap()
            .files
    };
    let (before, after) = (read(&input), read(&out));
    assert_eq!(after.len(), 2);
    for (old, new) in [(&before[0], &after[0]), (&before[2], &after[1])] {
        assert_eq!(new.name, old.name.replace("d", "e"));
        assert_eq!(new.windows_attributes, old.windows_attributes);
        assert_eq!(new.is_directory, old.is_directory);
        assert_eq!(
            (new.creation_date, new.last_modified_date, new.access_date),
            (old.creation_date, old.last_modified_date, old.access_date)
        );
    }
}

#[test]
fn names_that_would_clash_are_refused() {
    let room = Room::new();
    let input = made(&room, None, false);
    let out = room.path("out.7z");
    let clash = rewrite(
        &input,
        &out,
        None,
        &|e| Some(e.name.replace("a.txt", "b.txt")),
        &[],
    );
    assert!(matches!(clash, Err(Error::Format(_))), "{clash:?}");
    let twice = [
        Addition {
            source: None,
            name: "x/".into(),
        },
        Addition {
            source: None,
            name: "x".into(),
        },
    ];
    assert!(rewrite(&input, &out, None, &keep, &twice).is_err());
    let unsafe_name = rewrite(&input, &out, None, &|e| Some(format!("../{}", e.name)), &[]);
    assert!(unsafe_name.is_err());
}

#[test]
fn truncated_and_damaged_archives_fail_without_panicking() {
    for password in [None, Some("secret")] {
        let room = Room::new();
        let input = made(&room, password, false);
        let original = listed(&input, password);
        let bytes = fs::read(&input).unwrap();
        let out = room.path("out.7z");
        let broken = room.path("broken.7z");
        for cut in (0..bytes.len()).step_by(41) {
            fs::write(&broken, &bytes[..cut]).unwrap();
            assert!(
                rewrite(&broken, &out, password, &keep, &[]).is_err(),
                "{cut}"
            );
        }
        // Whatever a damaged archive still lets through has to be what it held.
        for at in (32..bytes.len()).step_by(7) {
            let mut damaged = bytes.clone();
            damaged[at] ^= 0x41;
            fs::write(&broken, &damaged).unwrap();
            if rewrite(&broken, &out, password, &keep, &[]).is_ok() {
                assert_eq!(listed(&out, password), original, "{at}");
            }
        }
    }
}

#[test]
fn progress_covers_the_copy_and_the_read_back() {
    let room = Room::new();
    let input = made(&room, None, false);
    let out = room.path("out.7z");
    let mut phases = Vec::new();
    let change = Change {
        name: &keep,
        add: &[],
        level: Level::FAST,
    };
    rewrite_7z(&input, &out, None, &change, &mut |p| {
        phases.push(p.phase);
        true
    })
    .unwrap();
    assert!(phases.contains(&Phase::Create));
    assert!(phases.contains(&Phase::Test));
}
