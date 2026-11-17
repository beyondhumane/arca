#![cfg(feature = "rar")]

use arca_core::{Error, Level};
use arca_rar::{create_limits, create_rar, Conflict, CreateOptions, RarArchive, Source};
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PASSWORD: &str = "contraseña segura";

fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

struct Tree {
    root: tempfile::TempDir,
    sources: Vec<Source>,
    expected: Vec<(String, Option<Vec<u8>>)>,
}

/// Files that compress, files that do not, an empty file, an empty
/// directory and names outside ASCII, so a set of a few hundred KiB splits
/// members across volumes.
fn tree() -> Tree {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    fs::create_dir_all(input.join("vacío")).unwrap();
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("a.bin", noise(1, 180_000)),
        (
            "b.txt",
            b"Arca RAR feature fixture, line after line\n".repeat(3000),
        ),
        ("empty.txt", Vec::new()),
        ("ñandú/c.bin", noise(2, 220_000)),
        (
            "ñandú/日本語.txt",
            "año, camión, señal\n".repeat(400).into_bytes(),
        ),
    ];
    let mut sources = vec![Source {
        path: input.join("vacío"),
        name: "vacío".into(),
    }];
    let mut expected = vec![("vacío".to_string(), None)];
    for (name, data) in files {
        let path = input.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &data).unwrap();
        sources.push(Source {
            path,
            name: name.into(),
        });
        expected.push((name.to_string(), Some(data)));
    }
    Tree {
        root,
        sources,
        expected,
    }
}

fn names(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn create(out: &Path, tree: &Tree, options: &CreateOptions<'_>) -> Vec<PathBuf> {
    create_rar(out, &tree.sources, options, &|_, _, _| true).unwrap()
}

fn assert_contents(first: &Path, password: Option<&str>, expected: &[(String, Option<Vec<u8>>)]) {
    let archive = RarArchive::open(first, password).unwrap();
    archive.test(password, &|_, _, _| true).unwrap();
    let listed: Vec<_> = archive.entries().iter().map(|e| e.name.clone()).collect();
    let wanted: Vec<_> = expected.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(listed, wanted);
    let out = tempfile::tempdir().unwrap();
    archive
        .extract(out.path(), &[], password, &|_, _, _| true, &|_| {
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

fn main_header(path: &Path, password: Option<&str>) -> rars::rar50::MainHeader {
    let data = fs::read(path).unwrap();
    let archive = rars::ArchiveReader::read_with_options(
        &data,
        rars::ArchiveReadOptions::with_optional_password(password.map(str::as_bytes)),
    )
    .unwrap();
    match archive {
        rars::Archive::Rar50Plus(archive) => archive.main,
        _ => panic!("not RAR 5"),
    }
}

#[test]
fn solid_archives_roundtrip_and_say_they_are_solid() {
    let tree = tree();
    let out = tree.root.path().join("solid.rar");
    let options = CreateOptions {
        solid: true,
        ..CreateOptions::default()
    };
    assert_eq!(create(&out, &tree, &options), vec![out.clone()]);
    assert!(main_header(&out, None).is_solid());
    assert_contents(&out, None, &tree.expected);
}

#[test]
fn encrypted_archives_need_the_password_for_data_but_not_for_names() {
    let tree = tree();
    let out = tree.root.path().join("encrypted.rar");
    let options = CreateOptions {
        password: Some(PASSWORD),
        level: Level::FAST,
        ..CreateOptions::default()
    };
    create(&out, &tree, &options);
    assert_contents(&out, Some(PASSWORD), &tree.expected);
    let archive = RarArchive::open(&out, None).unwrap();
    assert_eq!(archive.entries().len(), tree.expected.len());
    assert!(matches!(
        archive.test(None, &|_, _, _| true),
        Err(Error::PasswordRequired)
    ));
    assert!(matches!(
        archive.test(Some("wrong"), &|_, _, _| true),
        Err(Error::BadPassword)
    ));
    let raw = fs::read(&out).unwrap();
    assert!(!raw
        .windows(20)
        .any(|w| w == &tree.expected[2].1.as_ref().unwrap()[..20]));
}

#[test]
fn hidden_names_cannot_be_listed_without_the_password() {
    let tree = tree();
    let out = tree.root.path().join("hidden.rar");
    let options = CreateOptions {
        password: Some(PASSWORD),
        hide_names: true,
        ..CreateOptions::default()
    };
    create(&out, &tree, &options);
    assert!(main_header(&out, Some(PASSWORD)).encrypted_headers);
    assert!(matches!(
        RarArchive::open(&out, None),
        Err(Error::PasswordRequired)
    ));
    assert!(matches!(
        RarArchive::open(&out, Some("wrong")),
        Err(Error::BadPassword)
    ));
    let raw = fs::read(&out).unwrap();
    assert!(!raw.windows(5).any(|w| w == b"b.txt"));
    assert_contents(&out, Some(PASSWORD), &tree.expected);
}

#[test]
fn a_recovery_record_repairs_damaged_data() {
    let tree = tree();
    let out = tree.root.path().join("recovery.rar");
    let options = CreateOptions {
        recovery: Some(5),
        password: Some(PASSWORD),
        hide_names: true,
        level: Level::STORE,
        ..CreateOptions::default()
    };
    create(&out, &tree, &options);
    assert!(main_header(&out, Some(PASSWORD)).has_recovery_record());
    let original = fs::read(&out).unwrap();
    let mut bad = original.clone();
    for byte in &mut bad[100_000..100_064] {
        *byte ^= 0x5a;
    }
    let damaged = tree.root.path().join("damaged.rar");
    fs::write(&damaged, &bad).unwrap();
    let broken = RarArchive::open(&damaged, Some(PASSWORD)).unwrap();
    assert!(broken.test(Some(PASSWORD), &|_, _, _| true).is_err());
    let fixed = tree.root.path().join("fixed.rar");
    let report = arca_rar::repair(&damaged, &fixed, Some(PASSWORD), &|_, _, _| true).unwrap();
    assert!(report.changed && report.data_repaired);
    assert_eq!(fs::read(&fixed).unwrap(), original);
}

#[test]
fn volume_sets_are_named_like_winrar_and_no_volume_outgrows_its_size() {
    let tree = tree();
    let size = 100 * 1024;
    for (label, options) in [
        ("plain", CreateOptions::default()),
        (
            "everything",
            CreateOptions {
                solid: true,
                password: Some(PASSWORD),
                hide_names: true,
                recovery: Some(5),
                ..CreateOptions::default()
            },
        ),
    ] {
        let dir = tree.root.path().join(label);
        fs::create_dir(&dir).unwrap();
        let out = dir.join("set.rar");
        let published = create(
            &out,
            &tree,
            &CreateOptions {
                volume: Some(size),
                level: Level::STORE,
                ..options
            },
        );
        assert!(published.len() > 1, "{label}");
        let expected: BTreeSet<_> = (1..=published.len())
            .map(|n| format!("set.part{n}.rar"))
            .collect();
        assert_eq!(names(&dir), expected, "{label}");
        for (index, part) in published.iter().enumerate() {
            let len = fs::metadata(part).unwrap().len();
            assert!(len <= size, "{label}: {} is {len} bytes", part.display());
            if index + 1 < published.len() {
                assert!(
                    len > size - 1024,
                    "{label}: {} is only {len}",
                    part.display()
                );
            }
            let main = main_header(part, options.password);
            assert!(main.is_volume());
            assert_eq!(main.is_solid(), options.solid);
            assert_eq!(main.has_recovery_record(), options.recovery.is_some());
        }
        assert_contents(&published[0], options.password, &tree.expected);
        assert_contents(&published[1], options.password, &tree.expected);
    }
}

#[test]
fn many_small_members_with_every_option_still_fit_each_volume() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("in");
    fs::create_dir(&input).unwrap();
    let mut sources = Vec::new();
    for i in 0..500 {
        let path = input.join(format!("file-{i:03}.txt"));
        fs::write(&path, noise(i, 1 + (i as usize % 7) * 40)).unwrap();
        sources.push(Source {
            path,
            name: format!("dir/file-{i:03}.txt"),
        });
    }
    let out = root.path().join("small.rar");
    let size = create_limits::MIN_VOLUME_BYTES;
    let published = create_rar(
        &out,
        &sources,
        &CreateOptions {
            volume: Some(size),
            recovery: Some(5),
            password: Some(PASSWORD),
            hide_names: true,
            ..CreateOptions::default()
        },
        &|_, _, _| true,
    )
    .unwrap();
    assert!(published.len() > 1);
    for part in &published {
        assert!(fs::metadata(part).unwrap().len() <= size);
    }
    let archive = RarArchive::open(&published[0], Some(PASSWORD)).unwrap();
    assert_eq!(archive.entries().len(), 500);
    archive.test(Some(PASSWORD), &|_, _, _| true).unwrap();
}

#[test]
fn a_set_of_one_volume_keeps_the_name_and_ten_volumes_pad_the_number() {
    let tree = tree();
    let one = tree.root.path().join("one.rar");
    let published = create(
        &one,
        &tree,
        &CreateOptions {
            volume: Some(1 << 30),
            ..CreateOptions::default()
        },
    );
    assert_eq!(published, vec![one.clone()]);
    assert!(main_header(&one, None).is_volume());
    assert_contents(&one, None, &tree.expected);

    let mut tree = tree;
    let extra = tree.root.path().join("input/extra.bin");
    fs::write(&extra, noise(3, 200_000)).unwrap();
    tree.sources.push(Source {
        path: extra,
        name: "extra.bin".into(),
    });
    tree.expected
        .push(("extra.bin".into(), Some(noise(3, 200_000))));
    let dir = tree.root.path().join("ten");
    fs::create_dir(&dir).unwrap();
    let published = create(
        &dir.join("ten.rar"),
        &tree,
        &CreateOptions {
            volume: Some(create_limits::MIN_VOLUME_BYTES),
            level: Level::STORE,
            ..CreateOptions::default()
        },
    );
    assert!(published.len() >= 10, "{}", published.len());
    assert_eq!(published[0].file_name().unwrap(), "ten.part01.rar");
    assert_eq!(published.len(), names(&dir).len());
    assert_contents(&published[0], None, &tree.expected);
}

#[test]
fn an_existing_volume_of_the_same_name_is_never_replaced() {
    let tree = tree();
    let dir = tree.root.path().join("taken");
    fs::create_dir(&dir).unwrap();
    let old = dir.join("set.PART3.rar");
    fs::write(&old, b"an older set").unwrap();
    let error = create_rar(
        &dir.join("set.rar"),
        &tree.sources,
        &CreateOptions {
            volume: Some(100 * 1024),
            ..CreateOptions::default()
        },
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::AlreadyExists),
        "{error}"
    );
    assert_eq!(names(&dir), BTreeSet::from(["set.PART3.rar".to_string()]));
    assert_eq!(fs::read(&old).unwrap(), b"an older set");
}

#[test]
fn a_volume_that_appears_before_publication_unpublishes_the_others() {
    let tree = tree();
    let dir = tree.root.path().join("race");
    fs::create_dir(&dir).unwrap();
    let intruder = dir.join("set.part3.rar");
    let error = create_rar(
        &dir.join("set.rar"),
        &tree.sources,
        &CreateOptions {
            volume: Some(100 * 1024),
            level: Level::STORE,
            ..CreateOptions::default()
        },
        &|done, total, name| {
            if done == total && name.is_empty() && !intruder.exists() {
                fs::write(&intruder, b"somebody else's").unwrap();
            }
            true
        },
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Io(e) if e.kind() == ErrorKind::AlreadyExists),
        "{error}"
    );
    assert_eq!(names(&dir), BTreeSet::from(["set.part3.rar".to_string()]));
    assert_eq!(fs::read(&intruder).unwrap(), b"somebody else's");
}

#[test]
fn options_no_archive_can_have_are_refused_before_any_output() {
    let tree = tree();
    let dir = tree.root.path().join("refused");
    fs::create_dir(&dir).unwrap();
    let out = dir.join("x.rar");
    for options in [
        CreateOptions {
            password: Some(""),
            ..CreateOptions::default()
        },
        CreateOptions {
            hide_names: true,
            ..CreateOptions::default()
        },
        CreateOptions {
            hide_names: true,
            password: Some(""),
            ..CreateOptions::default()
        },
        CreateOptions {
            recovery: Some(0),
            ..CreateOptions::default()
        },
        CreateOptions {
            recovery: Some(101),
            ..CreateOptions::default()
        },
        CreateOptions {
            volume: Some(create_limits::MIN_VOLUME_BYTES - 1),
            ..CreateOptions::default()
        },
    ] {
        assert!(options.check().is_err(), "{options:?}");
        assert!(
            create_rar(&out, &tree.sources, &options, &|_, _, _| true).is_err(),
            "{options:?}"
        );
        assert!(names(&dir).is_empty(), "{options:?}");
    }
    let empty = CreateOptions {
        volume: Some(1 << 20),
        ..CreateOptions::default()
    };
    assert!(create_rar(&out, &[], &empty, &|_, _, _| true).is_err());
    assert!(names(&dir).is_empty());
}

#[test]
fn a_set_past_the_volume_limit_is_refused_and_leaves_nothing() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("big.bin");
    let size = create_limits::MIN_VOLUME_BYTES;
    fs::write(
        &input,
        noise(7, (create_limits::MAX_VOLUMES + 1) * size as usize),
    )
    .unwrap();
    let dir = root.path().join("out");
    fs::create_dir(&dir).unwrap();
    let error = create_rar(
        &dir.join("big.rar"),
        &[Source {
            path: input,
            name: "big.bin".into(),
        }],
        &CreateOptions {
            volume: Some(size),
            level: Level::STORE,
            ..CreateOptions::default()
        },
        &|_, _, _| true,
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Limit(m) if m.contains("volumes")),
        "{error}"
    );
    assert!(names(&dir).is_empty());
}

#[test]
fn damaged_and_truncated_volumes_fail_without_panicking() {
    let tree = tree();
    let dir = tree.root.path().join("broken");
    fs::create_dir(&dir).unwrap();
    let published = create(
        &dir.join("set.rar"),
        &tree,
        &CreateOptions {
            volume: Some(100 * 1024),
            password: Some(PASSWORD),
            hide_names: true,
            level: Level::STORE,
            ..CreateOptions::default()
        },
    );
    let second = fs::read(&published[1]).unwrap();
    for cut in [0, 7, 8, 40, 100, second.len() / 2, second.len() - 1] {
        fs::write(&published[1], &second[..cut]).unwrap();
        let result = RarArchive::open(&published[0], Some(PASSWORD))
            .and_then(|archive| archive.test(Some(PASSWORD), &|_, _, _| true));
        assert!(result.is_err(), "cut at {cut}");
    }
    let mut flipped = second.clone();
    flipped[second.len() / 2] ^= 0xff;
    fs::write(&published[1], &flipped).unwrap();
    let result = RarArchive::open(&published[0], Some(PASSWORD))
        .and_then(|archive| archive.test(Some(PASSWORD), &|_, _, _| true));
    assert!(result.is_err());
}

/// The console RAR, from `ARCA_RAR` or WinRAR's default place.
fn winrar() -> Option<PathBuf> {
    let path = std::env::var_os("ARCA_RAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\WinRAR\Rar.exe"));
    path.is_file().then_some(path)
}

fn rar(program: &Path, dir: &Path, args: &[&str]) -> bool {
    Command::new(program)
        .current_dir(dir)
        .args(["-y", "-idq"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn assert_extracted(dir: &Path, expected: &[(String, Option<Vec<u8>>)]) {
    for (name, data) in expected {
        let path = dir.join(name);
        match data {
            Some(data) => assert_eq!(&fs::read(&path).unwrap(), data, "{name}"),
            None => assert!(path.is_dir(), "{name}"),
        }
    }
}

#[test]
#[ignore = "needs WinRAR's console Rar.exe (ARCA_RAR or C:\\Program Files\\WinRAR\\Rar.exe)"]
fn winrar_tests_extracts_and_repairs_what_arca_writes() {
    let program = winrar().expect("Rar.exe not found");
    let tree = tree();
    let password = format!("-p{PASSWORD}");
    let combinations: [(&str, CreateOptions<'_>); 6] = [
        (
            "solid",
            CreateOptions {
                solid: true,
                ..CreateOptions::default()
            },
        ),
        (
            "encrypted",
            CreateOptions {
                password: Some(PASSWORD),
                ..CreateOptions::default()
            },
        ),
        (
            "hidden",
            CreateOptions {
                password: Some(PASSWORD),
                hide_names: true,
                ..CreateOptions::default()
            },
        ),
        (
            "recovery",
            CreateOptions {
                recovery: Some(3),
                ..CreateOptions::default()
            },
        ),
        (
            "everything",
            CreateOptions {
                solid: true,
                password: Some(PASSWORD),
                hide_names: true,
                recovery: Some(5),
                ..CreateOptions::default()
            },
        ),
        (
            "best",
            CreateOptions {
                level: Level::BEST,
                password: Some(PASSWORD),
                recovery: Some(1),
                ..CreateOptions::default()
            },
        ),
    ];
    for (label, options) in combinations {
        for volume in [None, Some(100 * 1024)] {
            let dir = tree
                .root
                .path()
                .join(format!("{label}-{}", volume.is_some()));
            fs::create_dir(&dir).unwrap();
            let published = create(
                &dir.join("arca.rar"),
                &tree,
                &CreateOptions { volume, ..options },
            );
            let first = published[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            assert!(
                rar(&program, &dir, &["t", &password, &first]),
                "{label} {volume:?}: rar t"
            );
            if options.password.is_some() {
                assert!(
                    !rar(&program, &dir, &["t", "-pwrong", &first]),
                    "{label}: wrong password"
                );
            }
            fs::create_dir(dir.join("x")).unwrap();
            assert!(
                rar(&program, &dir, &["x", &password, &first, "x\\"]),
                "{label}: rar x"
            );
            assert_extracted(&dir.join("x"), &tree.expected);
        }
    }

    let dir = tree.root.path().join("repair");
    fs::create_dir(&dir).unwrap();
    for (name, options) in [
        (
            "plain.rar",
            CreateOptions {
                recovery: Some(5),
                level: Level::STORE,
                ..CreateOptions::default()
            },
        ),
        (
            "hidden.rar",
            CreateOptions {
                recovery: Some(5),
                level: Level::STORE,
                password: Some(PASSWORD),
                hide_names: true,
                ..CreateOptions::default()
            },
        ),
    ] {
        let out = dir.join(name);
        create(&out, &tree, &options);
        let original = fs::read(&out).unwrap();
        let mut bad = original.clone();
        for byte in &mut bad[200_000..200_100] {
            *byte ^= 0xa5;
        }
        fs::write(&out, &bad).unwrap();
        assert!(
            !rar(&program, &dir, &["t", &password, name]),
            "{name}: damage unseen"
        );
        assert!(
            rar(&program, &dir, &["r", &password, name]),
            "{name}: rar r"
        );
        let fixed = dir.join(format!("fixed.{name}"));
        let repaired = fs::read(&fixed).unwrap();
        if repaired != original {
            let at = repaired.iter().zip(&original).position(|(a, b)| a != b);
            eprintln!(
                "{name}: rar r wrote {} bytes for {}, first difference at {at:?}",
                repaired.len(),
                original.len()
            );
        }
        assert!(rar(
            &program,
            &dir,
            &["t", &password, &format!("fixed.{name}")]
        ));
        assert_contents(&fixed, options.password, &tree.expected);
    }

    for (label, password_option) in [("plain-set", None), ("hidden-set", Some(PASSWORD))] {
        let dir = tree.root.path().join(label);
        fs::create_dir(&dir).unwrap();
        let published = create(
            &dir.join("set.rar"),
            &tree,
            &CreateOptions {
                recovery: Some(5),
                level: Level::STORE,
                volume: Some(100 * 1024),
                password: password_option,
                hide_names: password_option.is_some(),
                ..CreateOptions::default()
            },
        );
        let second = published[1]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let mut bad = fs::read(&published[1]).unwrap();
        let middle = bad.len() / 2;
        for byte in &mut bad[middle..middle + 100] {
            *byte ^= 0xa5;
        }
        fs::write(&published[1], &bad).unwrap();
        assert!(
            !rar(&program, &dir, &["t", &password, "set.part1.rar"]),
            "{label}: damage unseen"
        );
        assert!(
            rar(&program, &dir, &["r", &password, &second]),
            "{label}: rar r"
        );
        fs::rename(dir.join(format!("fixed.{second}")), &published[1]).unwrap();
        assert!(
            rar(&program, &dir, &["t", &password, "set.part1.rar"]),
            "{label}: repaired set"
        );
        fs::create_dir(dir.join("x")).unwrap();
        assert!(rar(
            &program,
            &dir,
            &["x", &password, "set.part1.rar", "x\\"]
        ));
        assert_extracted(&dir.join("x"), &tree.expected);
        if password_option.is_some() {
            assert_eq!(fs::read(&published[1]).unwrap().len(), bad.len());
            assert_contents(&published[0], password_option, &tree.expected);
        }
    }
}

#[test]
#[ignore = "needs WinRAR's console Rar.exe (ARCA_RAR or C:\\Program Files\\WinRAR\\Rar.exe)"]
fn arca_reads_what_winrar_writes_with_the_same_options() {
    let program = winrar().expect("Rar.exe not found");
    let tree = tree();
    let password = format!("-p{PASSWORD}");
    let hidden = format!("-hp{PASSWORD}");
    let cases: [(&str, &[&str], Option<&str>); 5] = [
        ("solid", &["-s"], None),
        ("encrypted", &[password.as_str()], Some(PASSWORD)),
        ("hidden", &[hidden.as_str()], Some(PASSWORD)),
        ("recovery", &["-rr3%"], None),
        (
            "everything",
            &["-s", hidden.as_str(), "-rr5%", "-v100k"],
            Some(PASSWORD),
        ),
    ];
    let expected: Vec<_> = tree
        .expected
        .iter()
        .map(|(name, data)| (format!("input/{name}"), data.clone()))
        .collect();
    for (label, switches, pass) in cases {
        let archive = tree.root.path().join(format!("{label}.rar"));
        let mut args = vec!["a", "-ma5", "-r"];
        args.extend_from_slice(switches);
        let archive_arg = archive.to_str().unwrap().to_owned();
        args.push(&archive_arg);
        args.push("input");
        assert!(rar(&program, tree.root.path(), &args), "{label}: rar a");
        let first = if switches.contains(&"-v100k") {
            tree.root.path().join(format!("{label}.part1.rar"))
        } else {
            archive
        };
        let opened = RarArchive::open(&first, pass).unwrap();
        opened.test(pass, &|_, _, _| true).unwrap();
        let out = tempfile::tempdir().unwrap();
        opened
            .extract(out.path(), &[], pass, &|_, _, _| true, &|_| {
                Conflict::Cancel
            })
            .unwrap();
        for (name, data) in &expected {
            let path = out.path().join(name);
            match data {
                Some(data) => assert_eq!(&fs::read(&path).unwrap(), data, "{label}: {name}"),
                None => assert!(path.is_dir(), "{label}: {name}"),
            }
        }
    }
}
