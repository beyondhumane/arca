use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../arca-rar/tests/fixtures")
        .join(name)
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

#[cfg(feature = "rar")]
fn run_in(dir: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A small tree with nested files, an empty file and an empty directory.
fn sample_tree(root: &Path) -> Vec<(&'static str, Option<&'static [u8]>)> {
    std::fs::create_dir_all(root.join("in/sub/empty")).unwrap();
    std::fs::write(root.join("in/a.txt"), b"alpha\n").unwrap();
    std::fs::write(root.join("in/sub/b.txt"), b"beta beta\n").unwrap();
    std::fs::write(root.join("in/zero.bin"), b"").unwrap();
    std::fs::write(root.join("top.txt"), b"top\n").unwrap();
    vec![
        ("in", None),
        ("in/a.txt", Some(b"alpha\n".as_slice())),
        ("in/sub", None),
        ("in/sub/b.txt", Some(b"beta beta\n".as_slice())),
        ("in/sub/empty", None),
        ("in/zero.bin", Some(b"".as_slice())),
        ("top.txt", Some(b"top\n".as_slice())),
    ]
}

#[test]
fn cbr_cannot_be_created_and_rar_or_cbr_cannot_be_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.txt");
    std::fs::write(&source, b"input").unwrap();
    let out = dir.path().join("out.cbr");
    let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("read-only"));
    assert!(!out.exists());
    for extension in ["rar", "cbr"] {
        let out = dir.path().join(format!("out.{extension}"));
        std::fs::copy(fixture("plain.rar"), &out).unwrap();
        let before = std::fs::read(&out).unwrap();
        let result = run(&[
            "password".as_ref(),
            out.as_os_str(),
            "--new".as_ref(),
            "no".as_ref(),
        ]);
        assert!(!result.status.success());
        assert!(stderr(&result).contains("disabled"), "{}", stderr(&result));
        assert_eq!(std::fs::read(&out).unwrap(), before);
    }
    assert_eq!(listing(dir.path()), ["input.txt", "out.cbr", "out.rar"]);
}

#[test]
fn help_mentions_rar_creation() {
    let result = run(&["create".as_ref(), "--help".as_ref()]);
    assert!(result.status.success());
    let help = stdout(&result);
    assert!(help.contains(".rar"), "{help}");
    assert!(
        help.contains(".7z and .rar creation is sequential"),
        "{help}"
    );
}

#[cfg(not(feature = "rar"))]
#[test]
fn explains_how_to_enable_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    sample_tree(dir.path());
    let out = dir.path().join("out.rar");
    let result = run(&[
        "create".as_ref(),
        out.as_os_str(),
        dir.path().join("in").as_os_str(),
    ]);
    assert!(!result.status.success());
    let message = stderr(&result);
    assert!(
        message.contains("RAR creation is disabled in this build"),
        "{message}"
    );
    assert!(message.contains("--features rar"), "{message}");
    assert_eq!(listing(dir.path()), ["in", "top.txt"]);
}

#[cfg(feature = "rar")]
#[test]
fn creates_lists_tests_and_extracts_a_rar() {
    let dir = tempfile::tempdir().unwrap();
    let expected = sample_tree(dir.path());
    let out = dir.path().join("out.rar");
    let result = run_in(
        dir.path(),
        &[
            "create".as_ref(),
            "out.rar".as_ref(),
            "in".as_ref(),
            "top.txt".as_ref(),
        ],
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        stdout(&result).contains("out.rar: 4 files"),
        "{}",
        stdout(&result)
    );
    assert_eq!(listing(dir.path()), ["in", "out.rar", "top.txt"]);

    let result = run(&["list".as_ref(), out.as_os_str()]);
    assert!(result.status.success(), "{}", stderr(&result));
    let listed = stdout(&result);
    for (name, _) in &expected {
        assert!(
            listed.lines().any(|l| l.ends_with(&format!("  {name}"))),
            "{name}\n{listed}"
        );
    }
    assert_eq!(listed.lines().count(), expected.len());

    let result = run(&["test".as_ref(), out.as_os_str()]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        stdout(&result).contains("4 entries verified"),
        "{}",
        stdout(&result)
    );

    let dest = dir.path().join("dest");
    let result = run(&[
        "extract".as_ref(),
        out.as_os_str(),
        "-o".as_ref(),
        dest.as_os_str(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    for (name, data) in &expected {
        match data {
            Some(data) => assert_eq!(std::fs::read(dest.join(name)).unwrap(), *data, "{name}"),
            None => assert!(dest.join(name).is_dir(), "{name}"),
        }
    }
}

#[cfg(feature = "rar")]
#[test]
fn levels_and_the_store_codec_map_to_the_rar_compressor() {
    let dir = tempfile::tempdir().unwrap();
    let data = b"The quick brown fox jumps over the lazy dog. ".repeat(20_000);
    let input = dir.path().join("text.txt");
    std::fs::write(&input, &data).unwrap();
    let mut sizes = Vec::new();
    for (args, name) in [
        (vec!["--level", "store"], "store.rar"),
        (vec!["--level", "fast"], "fast.rar"),
        (vec!["--level", "normal"], "normal.rar"),
        (vec!["--level", "best"], "best.rar"),
        (vec![], "default.rar"),
        (
            vec!["--codec", "store", "--level", "best"],
            "codec-store.rar",
        ),
        (vec!["-c", "auto", "-l", "fast", "-j", "1"], "auto.rar"),
    ] {
        let out = dir.path().join(name);
        let mut full: Vec<&std::ffi::OsStr> =
            vec!["create".as_ref(), out.as_os_str(), input.as_os_str()];
        full.extend(args.iter().map(|a| -> &std::ffi::OsStr { a.as_ref() }));
        let result = run(&full);
        assert!(result.status.success(), "{name}: {}", stderr(&result));
        let result = run(&["test".as_ref(), out.as_os_str()]);
        assert!(result.status.success(), "{name}: {}", stderr(&result));
        sizes.push((name, std::fs::metadata(&out).unwrap().len()));
    }
    let size = |name: &str| sizes.iter().find(|(n, _)| *n == name).unwrap().1;
    let stored = size("store.rar");
    assert!(stored > data.len() as u64, "store: {stored}");
    assert_eq!(size("codec-store.rar"), stored);
    for name in [
        "fast.rar",
        "normal.rar",
        "best.rar",
        "default.rar",
        "auto.rar",
    ] {
        assert!(size(name) < data.len() as u64 / 4, "{name}: {}", size(name));
    }
    assert_eq!(size("default.rar"), size("normal.rar"));
    assert!(size("best.rar") <= size("fast.rar"));

    let result = run(&["list".as_ref(), dir.path().join("store.rar").as_os_str()]);
    assert!(stdout(&result).contains("  store  "), "{}", stdout(&result));
    let result = run(&["list".as_ref(), dir.path().join("best.rar").as_os_str()]);
    assert!(stdout(&result).contains("  rar  "), "{}", stdout(&result));
}

#[cfg(feature = "rar")]
#[test]
fn unsupported_options_are_refused_before_any_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("text.txt");
    std::fs::write(&input, b"text").unwrap();
    let out = dir.path().join("out.rar");
    for (args, needle) in [
        (vec!["-p", "secret"], "encryption"),
        (vec!["--password", ""], "encryption"),
        (vec!["--hide-names"], "--hide-names"),
        (vec!["--hide-names", "-p", "secret"], "--hide-names"),
        (vec!["-c", "deflate"], "--codec"),
        (vec!["--codec", "zstd"], "--codec"),
        (vec!["--codec", "lzma2"], "--codec"),
        (vec!["-j", "2"], "--threads"),
        (vec!["--threads", "8", "-l", "best"], "--threads"),
    ] {
        let mut full: Vec<&std::ffi::OsStr> =
            vec!["create".as_ref(), out.as_os_str(), input.as_os_str()];
        full.extend(args.iter().map(|a| -> &std::ffi::OsStr { a.as_ref() }));
        let result = run(&full);
        assert!(!result.status.success(), "{args:?}");
        let message = stderr(&result);
        assert!(
            message.contains(".rar") && message.contains(needle),
            "{args:?}: {message}"
        );
        assert_eq!(listing(dir.path()), ["text.txt"], "{args:?}");
    }
}

#[cfg(feature = "rar")]
#[test]
fn existing_output_and_missing_parent_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("text.txt");
    std::fs::write(&input, b"text").unwrap();
    let out = dir.path().join("out.rar");
    std::fs::write(&out, b"not an archive").unwrap();
    let result = run(&["create".as_ref(), out.as_os_str(), input.as_os_str()]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("exists"), "{}", stderr(&result));
    assert_eq!(std::fs::read(&out).unwrap(), b"not an archive");

    let nested = dir.path().join("missing/out.rar");
    let result = run(&["create".as_ref(), nested.as_os_str(), input.as_os_str()]);
    assert!(!result.status.success());
    assert!(!dir.path().join("missing").exists());
    assert_eq!(listing(dir.path()), ["out.rar", "text.txt"]);
}

#[cfg(feature = "rar")]
#[test]
fn duplicate_names_and_the_output_directory_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("one")).unwrap();
    std::fs::create_dir_all(dir.path().join("two")).unwrap();
    std::fs::write(dir.path().join("one/Same.txt"), b"1").unwrap();
    std::fs::write(dir.path().join("two/same.txt"), b"2").unwrap();
    let out = dir.path().join("out.rar");
    let result = run(&[
        "create".as_ref(),
        out.as_os_str(),
        dir.path().join("one/Same.txt").as_os_str(),
        dir.path().join("two/same.txt").as_os_str(),
    ]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("duplicate"), "{}", stderr(&result));

    let result = run(&[
        "create".as_ref(),
        out.as_os_str(),
        dir.path().join("one").as_os_str(),
        dir.path().join("one").as_os_str(),
    ]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("duplicate"), "{}", stderr(&result));

    let result = run(&["create".as_ref(), out.as_os_str(), dir.path().as_os_str()]);
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("output directory"),
        "{}",
        stderr(&result)
    );
    assert_eq!(listing(dir.path()), ["one", "two"]);
}

#[cfg(feature = "rar")]
#[test]
fn a_dot_input_names_members_relative_to_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let expected = sample_tree(&work);
    let result = run_in(
        &work,
        &["create".as_ref(), "../dot.rar".as_ref(), ".".as_ref()],
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(listing(dir.path()), ["dot.rar", "work"]);
    assert_eq!(listing(&work), ["in", "top.txt"]);

    let out = dir.path().join("dot.rar");
    let result = run(&["list".as_ref(), out.as_os_str()]);
    assert!(result.status.success(), "{}", stderr(&result));
    let listed = stdout(&result);
    for (name, _) in &expected {
        assert!(
            listed.lines().any(|l| l.ends_with(&format!("  {name}"))),
            "{name}\n{listed}"
        );
    }
    assert!(!listed.contains("./"), "{listed}");
    assert_eq!(listed.lines().count(), expected.len());

    let dest = dir.path().join("dest");
    let result = run(&[
        "extract".as_ref(),
        out.as_os_str(),
        "-o".as_ref(),
        dest.as_os_str(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    for (name, data) in &expected {
        match data {
            Some(data) => assert_eq!(std::fs::read(dest.join(name)).unwrap(), *data, "{name}"),
            None => assert!(dest.join(name).is_dir(), "{name}"),
        }
    }
}

#[cfg(feature = "rar")]
#[test]
fn an_output_inside_the_tree_is_refused_not_stored() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(work.join("nested")).unwrap();
    sample_tree(&work);
    let before = listing(&work);

    // An existing output inside the tree is met during the walk and refused
    // as the archive being created; nothing is written.
    std::fs::write(work.join("nested/taken.rar"), b"taken").unwrap();
    let result = run_in(
        &work,
        &["create".as_ref(), "nested/taken.rar".as_ref(), ".".as_ref()],
    );
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("is the archive being created"),
        "{}",
        stderr(&result)
    );
    assert_eq!(
        std::fs::read(work.join("nested/taken.rar")).unwrap(),
        b"taken"
    );
    std::fs::remove_file(work.join("nested/taken.rar")).unwrap();

    // A fresh output whose directory is part of the tree is refused by the
    // writer as well, so an archive can never contain itself or its staging
    // file; the tree is left untouched.
    let result = run_in(
        &work,
        &["create".as_ref(), "nested/out.rar".as_ref(), ".".as_ref()],
    );
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("output directory"),
        "{}",
        stderr(&result)
    );
    assert_eq!(listing(&work), before);
    assert_eq!(listing(&work.join("nested")), Vec::<String>::new());
}

#[cfg(all(feature = "rar", unix))]
#[test]
fn creation_works_with_ignored_signals_and_background_launches() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.txt");
    std::fs::write(&input, b"background creation").unwrap();
    for (name, script) in [
        ("ignored", "trap '' INT TERM HUP; exec \"$@\""),
        ("background", "\"$@\" & wait \"$!\""),
        ("nohup", "exec nohup \"$@\""),
    ] {
        let out = dir.path().join(format!("{name}.rar"));
        let result = Command::new("sh")
            .args(["-c", script, "arca-signal-test"])
            .arg(env!("CARGO_BIN_EXE_arca"))
            .arg("create")
            .arg(&out)
            .arg(&input)
            .output()
            .unwrap();
        assert!(result.status.success(), "{name}: {}", stderr(&result));
        let result = run(&["test".as_ref(), out.as_os_str()]);
        assert!(result.status.success(), "{name}: {}", stderr(&result));
    }
}

#[cfg(all(feature = "rar", unix))]
#[test]
fn an_interrupt_cancels_the_creation_and_leaves_nothing_behind() {
    use std::io::Write;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut big = std::fs::File::create(dir.path().join("big.bin")).unwrap();
    let mut chunk = vec![0u8; 1 << 16];
    for _ in 0..(96 << 20) / chunk.len() {
        for b in chunk.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *b = (state >> 24) as u8;
        }
        big.write_all(&chunk).unwrap();
    }
    drop(big);
    let out = dir.path().join("out.rar");
    for (signal, ignore_hup) in [
        ("-INT", false),
        ("-TERM", false),
        ("-INT", true),
        ("-TERM", true),
    ] {
        let mut command = if ignore_hup {
            let mut command = Command::new("sh");
            command.args([
                "-c",
                "trap '' INT TERM HUP; exec \"$@\"",
                "arca-signal-test",
            ]);
            command.arg(env!("CARGO_BIN_EXE_arca"));
            command
        } else {
            Command::new(env!("CARGO_BIN_EXE_arca"))
        };
        let mut child = command
            .args([
                "create".as_ref(),
                out.as_os_str(),
                dir.path().join("big.bin").as_os_str(),
            ])
            .args(["-l", "best"])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            child.try_wait().unwrap().is_none(),
            "{signal}: creation finished before the signal; the fixture is too small"
        );
        if ignore_hup {
            assert!(Command::new("kill")
                .args(["-HUP", &child.id().to_string()])
                .status()
                .unwrap()
                .success());
            std::thread::sleep(Duration::from_millis(400));
            assert!(
                child.try_wait().unwrap().is_none(),
                "ignored HUP cancelled creation"
            );
        }
        let kill = Command::new("kill")
            .args([signal, &child.id().to_string()])
            .status()
            .unwrap();
        assert!(kill.success());
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "{signal}: did not stop within 30 s"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut err = String::new();
        std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut err).unwrap();
        assert_eq!(status.code(), Some(1), "{signal}: {status:?} {err}");
        assert!(err.contains("cancelled"), "{signal}: {err}");
        assert_eq!(listing(dir.path()), ["big.bin"], "{signal}");
    }
}

#[cfg(all(feature = "rar", unix))]
#[test]
fn symlinks_and_special_files_are_refused_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("real.txt"), b"real").unwrap();
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, b"outside").unwrap();
    symlink(&secret, input.join("link.txt")).unwrap();
    let out = dir.path().join("out.rar");
    for source in [input.clone(), input.join("link.txt")] {
        let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
        assert!(!result.status.success(), "{}", source.display());
        assert!(
            stderr(&result).contains("symbolic link"),
            "{}",
            stderr(&result)
        );
    }
    let dangling = dir.path().join("dangling");
    symlink("nowhere", &dangling).unwrap();
    let result = run(&["create".as_ref(), out.as_os_str(), dangling.as_os_str()]);
    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("symbolic link"),
        "{}",
        stderr(&result)
    );

    let fifo = dir.path().join("fifo");
    let made = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if made {
        let result = run(&["create".as_ref(), out.as_os_str(), fifo.as_os_str()]);
        assert!(!result.status.success());
        assert!(
            stderr(&result).contains("not a regular file"),
            "{}",
            stderr(&result)
        );
    } else {
        eprintln!("skipped: mkfifo unavailable");
    }
    assert!(!out.exists());
    assert!(listing(dir.path()).iter().all(|n| !n.ends_with(".part")));
}

#[cfg(not(feature = "rar"))]
#[test]
fn explains_how_to_enable_the_reader() {
    let archive = fixture("plain.rar");
    for verb in ["list", "test", "extract"] {
        let result = run(&[verb.as_ref(), archive.as_os_str()]);
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("RAR reading is disabled in this build"));
        assert!(stderr.contains("--features rar"));
        assert!(!stderr.contains("experimental"));
    }
}

#[cfg(feature = "rar")]
#[test]
fn lists_tests_extracts_and_accepts_cbr() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("comic.CBR");
    std::fs::copy(fixture("solid.rar"), &archive).unwrap();
    for verb in ["list", "test"] {
        let result = run(&[verb.as_ref(), archive.as_os_str()]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let dest = dir.path().join("out");
    let result = run(&[
        "extract".as_ref(),
        archive.as_os_str(),
        "-o".as_ref(),
        dest.as_os_str(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read(dest.join("first.txt")).unwrap(),
        b"Arca RAR fixture alpha\n".repeat(64)
    );
}

#[cfg(feature = "rar")]
#[test]
fn passwords_work_for_listing_testing_and_extraction() {
    let dir = tempfile::tempdir().unwrap();
    let archive = fixture("headers.rar");
    for verb in ["list", "test"] {
        assert!(!run(&[verb.as_ref(), archive.as_os_str()]).status.success());
        assert!(!run(&[
            verb.as_ref(),
            archive.as_os_str(),
            "-p".as_ref(),
            "wrong".as_ref()
        ])
        .status
        .success());
        assert!(run(&[
            verb.as_ref(),
            archive.as_os_str(),
            "-p".as_ref(),
            "arca-test-only".as_ref()
        ])
        .status
        .success());
    }
    let dest = dir.path().join("out");
    assert!(!run(&[
        "extract".as_ref(),
        fixture("encrypted.rar").as_os_str(),
        "-o".as_ref(),
        dest.as_os_str(),
        "-p".as_ref(),
        "wrong".as_ref()
    ])
    .status
    .success());
    assert!(!dest.exists());
}

#[cfg(feature = "rar")]
#[test]
fn later_modern_and_legacy_volumes_use_the_complete_set() {
    let dir = tempfile::tempdir().unwrap();
    for (i, name) in ["volume.rar", "volume.r00", "volume.r01", "volume.r02"]
        .iter()
        .enumerate()
    {
        std::fs::copy(
            fixture(&format!("volume.part{}.rar", i + 1)),
            dir.path().join(name),
        )
        .unwrap();
    }
    for archive in [fixture("volume.part4.rar"), dir.path().join("volume.r02")] {
        for verb in ["list", "test"] {
            let result = run(&[verb.as_ref(), archive.as_os_str()]);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let out = tempfile::tempdir().unwrap();
        let result = run(&[
            "extract".as_ref(),
            archive.as_os_str(),
            "-o".as_ref(),
            out.path().as_os_str(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(out.path().join("first.txt")).unwrap(),
            b"Arca RAR fixture alpha\n".repeat(64)
        );
        assert_eq!(
            std::fs::read(out.path().join("folder/second.txt")).unwrap(),
            b"Arca RAR fixture beta\n".repeat(64)
        );
    }
}
