use super::*;
use arca_core::{Codec, Error, Level};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const GO: Notify<'static> = &|_, _, _| true;

fn zip_of(path: &Path, files: &[(&str, &[u8])], password: Option<&str>) {
    let mut w = arca_zip::ZipWriter::new(File::create(path).unwrap());
    for (name, data) in files {
        w.add_with_password(
            name,
            *data,
            Codec::Deflate,
            Level::NORMAL,
            Some(1_700_000_000),
            password,
        )
        .unwrap();
    }
    w.finish().unwrap().flush().unwrap();
}

enum Put<'a> {
    File(&'a str, &'a [u8], i64),
    Folder(&'a str),
    Symlink(&'a str, &'a str),
    Hard(&'a str, &'a str),
}

fn tar_of(path: &Path, items: &[Put<'_>]) {
    let mut w = arca_tar::TarWriter::new(File::create(path).unwrap());
    for item in items {
        match item {
            Put::File(name, data, mtime) => w
                .add(name, data.len() as u64, *mtime, 0o644, *data)
                .unwrap(),
            Put::Folder(name) => w.add(name, 0, 0, 0o755, &b""[..]).unwrap(),
            Put::Symlink(name, target) => w.add_link(name, target, 0, b'2').unwrap(),
            Put::Hard(name, target) => w.add_link(name, target, 0, b'1').unwrap(),
        }
    }
    w.finish().unwrap().flush().unwrap();
}

fn side(path: &Path) -> Side {
    Side::open(path, None, GO).unwrap()
}

fn lines(a: &Path, b: &Path, options: Options) -> Vec<String> {
    compare(&side(a), &side(b), options, GO)
        .unwrap()
        .iter()
        .map(Difference::line)
        .collect()
}

const DEFAULT: Options = Options {
    quick: false,
    times: false,
};
const QUICK: Options = Options {
    quick: true,
    times: false,
};

#[test]
fn the_same_files_in_two_formats_have_no_differences() {
    let room = tempfile::tempdir().unwrap();
    let zip = room.path().join("a.zip");
    let tar = room.path().join("b.tar");
    zip_of(&zip, &[("dir/one.txt", b"one"), ("two.txt", b"two")], None);
    tar_of(
        &tar,
        &[
            Put::Folder("./dir/"),
            Put::File("./dir/one.txt", b"one", 1_700_000_000),
            Put::File("two.txt", b"two", 1_700_000_000),
        ],
    );
    assert!(lines(&zip, &tar, DEFAULT).is_empty());
    assert!(lines(&zip, &tar, QUICK).is_empty());
    assert!(lines(&zip, &zip, DEFAULT).is_empty());
}

#[test]
fn added_removed_changed_and_retyped_names_are_each_told() {
    let room = tempfile::tempdir().unwrap();
    let a = room.path().join("a.zip");
    let b = room.path().join("b.zip");
    zip_of(
        &a,
        &[
            ("same.txt", b"same"),
            ("edited.txt", b"abcd"),
            ("grown.txt", b"abc"),
            ("gone.txt", b"bye"),
            ("kind", b"was a file"),
        ],
        None,
    );
    zip_of(
        &b,
        &[
            ("same.txt", b"same"),
            ("edited.txt", b"abce"),
            ("grown.txt", b"abcdef"),
            ("new/inner.txt", b"hello"),
            ("kind/", b""),
        ],
        None,
    );
    assert_eq!(
        lines(&a, &b, DEFAULT),
        [
            "M edited.txt",
            "- gone.txt",
            "M grown.txt",
            "T kind",
            "+ new/",
            "+ new/inner.txt",
        ]
    );
    assert_eq!(
        lines(&b, &a, DEFAULT),
        [
            "M edited.txt",
            "+ gone.txt",
            "M grown.txt",
            "T kind",
            "- new/",
            "- new/inner.txt",
        ]
    );
}

#[test]
fn quick_trusts_sizes_and_the_crc_only_where_both_sides_have_one() {
    let room = tempfile::tempdir().unwrap();
    let (a, b) = (room.path().join("a.zip"), room.path().join("b.zip"));
    zip_of(&a, &[("x.txt", b"abcd")], None);
    zip_of(&b, &[("x.txt", b"abce")], None);
    assert_eq!(lines(&a, &b, QUICK), ["M x.txt"]);
    let (c, d) = (room.path().join("c.tar"), room.path().join("d.tar"));
    tar_of(&c, &[Put::File("x.txt", b"abcd", 0)]);
    tar_of(&d, &[Put::File("x.txt", b"abce", 0)]);
    assert!(lines(&c, &d, QUICK).is_empty());
    assert_eq!(lines(&c, &d, DEFAULT), ["M x.txt"]);
    assert_eq!(lines(&a, &d, QUICK), Vec::<String>::new());
}

#[test]
fn an_archive_matches_the_folder_it_was_extracted_to() {
    let room = tempfile::tempdir().unwrap();
    let zip = room.path().join("a.zip");
    zip_of(
        &zip,
        &[("deep/er/one.txt", b"one"), ("two.txt", b"two")],
        None,
    );
    let out = room.path().join("out");
    fs::create_dir_all(out.join("deep/er")).unwrap();
    fs::write(out.join("deep/er/one.txt"), b"one").unwrap();
    fs::write(out.join("two.txt"), b"two").unwrap();
    assert!(lines(&zip, &out, DEFAULT).is_empty());
    assert!(lines(&out, &zip, DEFAULT).is_empty());

    fs::write(out.join("two.txt"), b"tw0").unwrap();
    fs::create_dir(out.join("empty")).unwrap();
    fs::remove_file(out.join("deep/er/one.txt")).unwrap();
    assert_eq!(
        lines(&zip, &out, DEFAULT),
        ["- deep/er/one.txt", "+ empty/", "M two.txt"]
    );
    assert_eq!(lines(&zip, &out, QUICK), ["- deep/er/one.txt", "+ empty/"]);
}

#[test]
fn times_are_only_reported_when_asked_for_and_past_the_dos_slack() {
    let room = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        room.path().join("a.tar"),
        room.path().join("b.tar"),
        room.path().join("c.tar"),
    );
    tar_of(&a, &[Put::File("x", b"x", 1_000)]);
    tar_of(&b, &[Put::File("x", b"x", 1_002)]);
    tar_of(&c, &[Put::File("x", b"x", 5_000)]);
    let times = Options {
        quick: false,
        times: true,
    };
    assert!(lines(&a, &c, DEFAULT).is_empty());
    assert!(lines(&a, &b, times).is_empty());
    assert_eq!(lines(&a, &c, times), ["m x"]);
}

#[test]
fn links_are_their_own_type_and_a_hard_link_reads_as_its_file() {
    let room = tempfile::tempdir().unwrap();
    let (a, b) = (room.path().join("a.tar"), room.path().join("b.tar"));
    tar_of(
        &a,
        &[
            Put::File("data", b"data", 0),
            Put::Symlink("soft", "data"),
            Put::Hard("hard", "data"),
            Put::Symlink("moved", "data"),
        ],
    );
    tar_of(
        &b,
        &[
            Put::File("data", b"data", 0),
            Put::File("soft", b"data", 0),
            Put::File("hard", b"data", 0),
            Put::Symlink("moved", "elsewhere"),
        ],
    );
    assert_eq!(lines(&a, &b, DEFAULT), ["M moved", "T soft"]);
}

#[test]
fn encrypted_archives_are_read_with_their_password() {
    let room = tempfile::tempdir().unwrap();
    let (locked, open) = (room.path().join("locked.zip"), room.path().join("open.zip"));
    zip_of(&locked, &[("secret.txt", b"secret")], Some("pw"));
    zip_of(&open, &[("secret.txt", b"secret")], None);
    let with = Side::open(&locked, Some("pw"), GO).unwrap();
    assert!(compare(&with, &side(&open), DEFAULT, GO)
        .unwrap()
        .is_empty());
    assert!(compare(&with, &side(&open), QUICK, GO).unwrap().is_empty());
    let without = side(&locked);
    assert!(compare(&without, &side(&open), DEFAULT, GO).is_err());
    let wrong = Side::open(&locked, Some("nope"), GO).unwrap();
    assert!(compare(&wrong, &side(&open), DEFAULT, GO).is_err());

    let sevenz = room.path().join("hidden.7z");
    let source = room.path().join("secret.txt");
    fs::write(&source, b"secret").unwrap();
    arca_7z::create_7z(
        &sevenz,
        &[arca_7z::Source {
            path: source,
            name: "secret.txt".into(),
        }],
        &arca_7z::CreateOptions {
            password: Some("pw"),
            hide_names: true,
            ..Default::default()
        },
        &mut |_| true,
    )
    .unwrap();
    assert!(Side::open(&sevenz, None, GO).is_err());
    let hidden = Side::open(&sevenz, Some("pw"), GO).unwrap();
    assert!(compare(&hidden, &side(&open), DEFAULT, GO)
        .unwrap()
        .is_empty());
}

#[test]
fn truncated_or_damaged_archives_are_errors_and_never_panic() {
    let room = tempfile::tempdir().unwrap();
    let zip = room.path().join("a.zip");
    zip_of(&zip, &[("x.txt", &[7u8; 5000][..]), ("y.txt", b"y")], None);
    let data = fs::read(&zip).unwrap();
    for cut in [0, 1, 22, data.len() / 2, data.len() - 1] {
        let short = room.path().join(format!("short{cut}.zip"));
        fs::write(&short, &data[..cut]).unwrap();
        if let Ok(side) = Side::open(&short, None, GO) {
            let _ = compare(&side, &self::side(&zip), DEFAULT, GO);
        }
    }
    let stored = room.path().join("stored.zip");
    let mut w = arca_zip::ZipWriter::new(File::create(&stored).unwrap());
    w.add("x.txt", &b"123456789"[..], Codec::Store, Level::STORE, None)
        .unwrap();
    w.finish().unwrap().flush().unwrap();
    let mut flipped = fs::read(&stored).unwrap();
    let at = flipped.windows(9).position(|w| w == b"123456789").unwrap();
    flipped[at] ^= 0xff;
    let damaged = room.path().join("damaged.zip");
    fs::write(&damaged, &flipped).unwrap();
    let result =
        Side::open(&damaged, None, GO).and_then(|d| compare(&d, &side(&stored), DEFAULT, GO));
    assert!(result.is_err());

    let tar = room.path().join("t.tar");
    tar_of(&tar, &[Put::File("x", &[1u8; 3000], 0)]);
    let data = fs::read(&tar).unwrap();
    let short = room.path().join("short.tar");
    fs::write(&short, &data[..1000]).unwrap();
    assert!(Side::open(&short, None, GO).is_err());
    let not_an_archive = room.path().join("plain.txt");
    fs::write(&not_an_archive, b"text").unwrap();
    assert!(Side::open(&not_an_archive, None, GO).is_err());
    assert!(Side::open(&room.path().join("missing.zip"), None, GO).is_err());
}

#[test]
fn a_stop_ends_the_comparison_with_cancelled() {
    let room = tempfile::tempdir().unwrap();
    let (a, b) = (room.path().join("a.zip"), room.path().join("b.tar"));
    zip_of(&a, &[("x", b"x")], None);
    tar_of(&b, &[Put::File("x", b"x", 0)]);
    let stop: Notify<'static> = &|_, _, _| false;
    assert!(matches!(
        compare(&side(&a), &side(&b), DEFAULT, stop),
        Err(Error::Cancelled)
    ));
    assert!(matches!(Side::open(&b, None, stop), Err(Error::Cancelled)));
    let dir: PathBuf = room.path().join("dir");
    fs::create_dir(&dir).unwrap();
    assert!(matches!(
        Side::open(&dir, None, stop),
        Err(Error::Cancelled)
    ));
}

#[test]
fn a_single_stream_is_compared_by_its_contents() {
    let room = tempfile::tempdir().unwrap();
    let plain = room.path().join("notes.txt");
    fs::write(&plain, b"some notes").unwrap();
    let xz = room.path().join("notes.txt.xz");
    let mut e = arca_stream::encoder(
        File::create(&xz).unwrap(),
        arca_core::Format::Xz,
        Level::FAST,
        1,
        None,
    )
    .unwrap();
    e.write_all(b"some notes").unwrap();
    e.finish().unwrap().flush().unwrap();
    let dir = room.path().join("dir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("notes.txt"), b"some notes").unwrap();
    assert!(lines(&xz, &dir, DEFAULT).is_empty());
    fs::write(dir.join("notes.txt"), b"other text").unwrap();
    assert_eq!(lines(&xz, &dir, DEFAULT), ["M notes.txt"]);
    assert!(lines(&xz, &dir, QUICK).is_empty());
}

#[test]
fn names_are_matched_after_normalizing_slashes() {
    assert_eq!(side::key("a\\b//c/"), "a/b/c");
    assert_eq!(side::key("./a/./b"), "a/b");
    assert_eq!(side::key("/"), "");
}
