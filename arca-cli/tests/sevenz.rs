use arca_7z::SevenZArchive;
use arca_core::Method;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "arca-cli-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("temporary directory: {e}"),
            }
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_arca"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn fail(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("panicked"), "{error}");
        error
    }

    fn fixture(&self, name: &str) {
        let bytes: &[u8] = match name {
            "solid" => include_bytes!("fixtures/solid.7z"),
            "encrypted" => include_bytes!("fixtures/encrypted.7z"),
            "hidden" => include_bytes!("fixtures/hidden.7z"),
            "mixed" => include_bytes!("fixtures/mixed.7z"),
            "unsafe" => include_bytes!("fixtures/unsafe.7z"),
            _ => panic!("unknown fixture"),
        };
        fs::write(self.0.join("archive.7z"), bytes).unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

type Snapshot = Vec<(PathBuf, Option<Vec<u8>>, SystemTime)>;

fn snapshot(root: &Path) -> Snapshot {
    fn visit(root: &Path, path: &Path, result: &mut Snapshot) {
        let meta = fs::metadata(path).unwrap();
        let data = meta.is_file().then(|| fs::read(path).unwrap());
        result.push((
            path.strip_prefix(root).unwrap().to_owned(),
            data,
            meta.modified().unwrap(),
        ));
        if meta.is_dir() {
            let mut children = fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                visit(root, &child, result);
            }
        }
    }
    let mut result = Vec::new();
    visit(root, root, &mut result);
    result
}

fn corpus(root: &Path) {
    fs::create_dir_all(root.join("src/empty-dir")).unwrap();
    fs::write(root.join("src/empty"), b"").unwrap();
    fs::write(root.join("src/first.txt"), b"hello archive\n".repeat(100)).unwrap();
    fs::write(
        root.join("src/\u{00f1}-\u{1f680}.txt"),
        [0, 255, 128, 13, 10],
    )
    .unwrap();
}

fn assert_corpus(root: &Path) {
    for name in ["empty", "first.txt", "\u{00f1}-\u{1f680}.txt"] {
        assert_eq!(
            fs::read(root.join("src").join(name)).unwrap(),
            fs::read(root.join("out/src").join(name)).unwrap()
        );
    }
    assert!(root.join("out/src/empty-dir").is_dir());
}

#[test]
fn create_list_test_extract_all_levels_and_password_modes() {
    let temp = Temp::new();
    corpus(&temp.0);
    let password = "p\u{00e4}ss\u{1f511}";
    for level in ["store", "fast", "normal", "best"] {
        for mode in ["plain", "visible", "hidden"] {
            let mut args = vec!["create", "archive.7z", "src", "-l", level, "-j", "4"];
            if mode != "plain" {
                args.extend(["-p", password]);
            }
            if mode == "hidden" {
                args.push("--hide-names");
            }
            let summary = temp.ok(&args);
            assert!(summary.contains("sequential"));
            let archive = SevenZArchive::open(
                File::open(temp.0.join("archive.7z")).unwrap(),
                Some(password),
            )
            .unwrap();
            assert_eq!(archive.has_encrypted_header(), mode == "hidden");
            assert!(!archive.is_solid());
            let entry = archive
                .entries()
                .iter()
                .find(|e| e.name == "src/first.txt")
                .unwrap();
            assert_eq!(
                entry.method,
                if level == "store" {
                    Method::Store
                } else {
                    Method::Lzma2
                }
            );
            if mode == "hidden" {
                assert!(temp.fail(&["list", "archive.7z"]).contains("password"));
                assert!(temp
                    .fail(&["list", "archive.7z", "-p", "wrong"])
                    .contains("password"));
            } else {
                assert!(temp.ok(&["list", "archive.7z"]).contains("src/first.txt"));
            }
            assert!(temp
                .ok(&["list", "archive.7z", "--password", password, "--time"])
                .contains("src/empty-dir"));
            temp.ok(&["test", "archive.7z", "-p", password]);
            temp.ok(&["extract", "archive.7z", "-o", "out", "-p", password]);
            assert_corpus(&temp.0);
            fs::remove_dir_all(temp.0.join("out")).unwrap();
        }
    }
}

#[test]
fn explicit_store_and_lzma2_and_current_directory_inputs() {
    let temp = Temp::new();
    fs::write(temp.0.join("data"), b"data").unwrap();
    for (codec, method) in [("store", Method::Store), ("lzma2", Method::Lzma2)] {
        temp.ok(&["create", "archive.7z", "data", "-c", codec]);
        let archive =
            SevenZArchive::open(File::open(temp.0.join("archive.7z")).unwrap(), None).unwrap();
        assert_eq!(archive.entries()[0].method, method);
    }
    fs::create_dir(temp.0.join("here")).unwrap();
    fs::write(temp.0.join("here/file"), b"here").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(temp.0.join("here"))
        .args(["create", "../here.7z", "."])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    temp.ok(&["test", "here.7z"]);
}

#[test]
fn empty_only_encryption_requires_hidden_names() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("empty-dir")).unwrap();
    fs::write(temp.0.join("empty"), b"").unwrap();
    assert!(temp
        .fail(&["create", "archive.7z", "empty", "-p", "secret"])
        .contains("hide_names"));
    assert!(!temp.0.join("archive.7z").exists());
    temp.ok(&[
        "create",
        "archive.7z",
        "empty-dir",
        "empty",
        "-p",
        "secret",
        "--hide-names",
    ]);
    temp.fail(&["extract", "archive.7z", "-o", "wrong", "-p", "wrong"]);
    assert!(!temp.0.join("wrong").exists());
    temp.ok(&["extract", "archive.7z", "-o", "out", "-p", "secret"]);
    assert!(temp.0.join("out/empty-dir").is_dir());
    assert_eq!(fs::metadata(temp.0.join("out/empty")).unwrap().len(), 0);
}

#[test]
fn external_solid_and_mixed_archives_roundtrip() {
    let temp = Temp::new();
    for fixture in ["solid", "encrypted", "hidden", "mixed"] {
        temp.fixture(fixture);
        let archive = SevenZArchive::open(
            File::open(temp.0.join("archive.7z")).unwrap(),
            Some("secret"),
        )
        .unwrap();
        assert_eq!(archive.is_solid(), fixture != "mixed");
        if fixture == "mixed" {
            assert!(archive.entries().iter().any(|e| e.encrypted));
            assert!(archive.entries().iter().any(|e| !e.encrypted && e.size > 0));
        }
        temp.ok(&["list", "archive.7z", "-p", "secret"]);
        temp.ok(&["test", "archive.7z", "-p", "secret"]);
        temp.ok(&["extract", "archive.7z", "-o", "out", "-p", "secret"]);
        assert_eq!(
            fs::read(temp.0.join("out/first.txt")).unwrap(),
            b"first contents\n".repeat(8)
        );
        assert_eq!(
            fs::read(temp.0.join("out/second.txt")).unwrap(),
            b"second contents\n".repeat(8)
        );
        assert!(temp.0.join("out/empty-dir").is_dir());
        assert_eq!(fs::metadata(temp.0.join("out/empty")).unwrap().len(), 0);
        fs::remove_dir_all(temp.0.join("out")).unwrap();
    }
}

#[test]
fn wrong_or_missing_password_never_touches_any_destination() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("out")).unwrap();
    fs::write(temp.0.join("out/first.txt"), b"precious first file").unwrap();
    fs::write(temp.0.join("out/empty"), b"not empty").unwrap();
    for fixture in ["encrypted", "hidden", "mixed"] {
        temp.fixture(fixture);
        for password in [None, Some("wrong")] {
            for policy in ["overwrite", "skip", "rename"] {
                for dest in ["out", "absent/nested"] {
                    let before = snapshot(&temp.0);
                    let mut args =
                        vec!["extract", "archive.7z", "-o", dest, "--on-conflict", policy];
                    if let Some(pw) = password {
                        args.extend(["-p", pw]);
                    }
                    let error = temp.fail(&args);
                    assert!(error.contains("password"), "{error}");
                    assert_eq!(snapshot(&temp.0), before, "{fixture}: {args:?}");
                }
            }
        }
        let error = temp.fail(&["test", "archive.7z", "-p", "wrong"]);
        assert!(error.contains("wrong password or corrupt"), "{error}");
    }
}

#[test]
fn solid_conflicts_overwrite_skip_and_rename_in_block_order() {
    let temp = Temp::new();
    temp.fixture("encrypted");
    for policy in ["overwrite", "skip", "rename"] {
        fs::create_dir(temp.0.join("out")).unwrap();
        fs::write(temp.0.join("out/first.txt"), b"keep").unwrap();
        fs::write(temp.0.join("out/first (1).txt"), b"also keep").unwrap();
        fs::write(temp.0.join("out/empty"), b"not empty").unwrap();
        temp.ok(&[
            "extract",
            "archive.7z",
            "-o",
            "out",
            "-p",
            "secret",
            "--on-conflict",
            policy,
        ]);
        let first = fs::read(temp.0.join("out/first.txt")).unwrap();
        assert_eq!(
            first,
            if policy == "overwrite" {
                b"first contents\n".repeat(8)
            } else {
                b"keep".to_vec()
            }
        );
        assert_eq!(
            fs::read(temp.0.join("out/first (1).txt")).unwrap(),
            b"also keep"
        );
        if policy == "rename" {
            assert_eq!(
                fs::read(temp.0.join("out/first (2).txt")).unwrap(),
                b"first contents\n".repeat(8)
            );
            assert_eq!(fs::read(temp.0.join("out/empty (1)")).unwrap(), b"");
        }
        assert_eq!(
            fs::read(temp.0.join("out/second.txt")).unwrap(),
            b"second contents\n".repeat(8)
        );
        assert_eq!(
            fs::read(temp.0.join("out/empty")).unwrap(),
            if policy == "overwrite" {
                b"".as_slice()
            } else {
                b"not empty".as_slice()
            }
        );
        fs::remove_dir_all(temp.0.join("out")).unwrap();
    }
}

#[test]
fn invalid_options_preserve_existing_output() {
    let temp = Temp::new();
    fs::write(temp.0.join("data"), b"source").unwrap();
    for (extension, options) in [
        ("7z", vec!["--hide-names"]),
        ("7z", vec!["--hide-names", "-p", ""]),
        ("7z", vec!["-p", ""]),
        ("7z", vec!["-c", "deflate"]),
        ("7z", vec!["-c", "zstd", "-l", "store"]),
        ("zip", vec!["--hide-names", "-p", "secret"]),
        ("tar", vec!["--hide-names", "-p", "secret"]),
        ("tar.gz", vec!["--hide-names"]),
        ("zip", vec!["-c", "lzma2"]),
        ("tar", vec!["-c", "lzma2"]),
        ("tar.gz", vec!["-p", "secret"]),
    ] {
        let name = format!("out.{extension}");
        fs::write(temp.0.join(&name), b"precious archive").unwrap();
        let before = snapshot(&temp.0);
        let mut args = vec!["create", name.as_str(), "data"];
        args.extend(options);
        temp.fail(&args);
        assert_eq!(snapshot(&temp.0), before);
    }
    temp.fixture("encrypted");
    let before = snapshot(&temp.0);
    assert!(temp
        .fail(&[
            "password",
            "archive.7z",
            "-p",
            "secret",
            "--new",
            "new",
            "-o",
            "new.7z"
        ])
        .contains("not supported"));
    assert_eq!(snapshot(&temp.0), before);
    assert_eq!(
        temp.run(&["create", "out.7z", "data", "--unknown"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn unsafe_names_and_all_truncation_offsets_fail_without_panics() {
    let temp = Temp::new();
    temp.fixture("unsafe");
    for command in ["list", "test"] {
        temp.fail(&[command, "archive.7z"]);
    }
    temp.fail(&["extract", "archive.7z", "-o", "absent"]);
    assert!(!temp.0.join("absent").exists());
    assert!(!temp.0.join("escape").exists());
    for fixture in ["solid", "encrypted", "hidden"] {
        temp.fixture(fixture);
        let bytes = fs::read(temp.0.join("archive.7z")).unwrap();
        for offset in 0..bytes.len() {
            fs::write(temp.0.join("truncated.7z"), &bytes[..offset]).unwrap();
            temp.fail(&["test", "truncated.7z", "-p", "secret"]);
        }
    }
}

#[test]
fn corrupt_encrypted_block_prevents_even_plain_or_skipped_files_being_written() {
    let temp = Temp::new();
    temp.fixture("mixed");
    let archive = SevenZArchive::open(
        File::open(temp.0.join("archive.7z")).unwrap(),
        Some("secret"),
    )
    .unwrap();
    let plain_size = archive
        .entries()
        .iter()
        .find(|e| !e.encrypted && e.size > 0)
        .unwrap()
        .compressed_size;
    let mut bytes = fs::read(temp.0.join("archive.7z")).unwrap();
    bytes[32 + plain_size as usize] ^= 0x40;
    fs::write(temp.0.join("archive.7z"), bytes).unwrap();
    fs::create_dir(temp.0.join("out")).unwrap();
    fs::write(temp.0.join("out/second.txt"), b"skip this encrypted file").unwrap();
    let before = snapshot(&temp.0);
    let error = temp.fail(&[
        "extract",
        "archive.7z",
        "-p",
        "secret",
        "-o",
        "out",
        "--on-conflict",
        "skip",
    ]);
    assert!(error.contains("wrong password or corrupt"));
    assert_eq!(snapshot(&temp.0), before);
}

#[cfg(unix)]
#[test]
fn existing_symlinks_are_rejected_and_hardlinks_are_not_truncated() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    temp.fixture("solid");
    fs::create_dir(temp.0.join("outside")).unwrap();
    fs::write(temp.0.join("outside/first.txt"), b"external").unwrap();
    for policy in ["overwrite", "skip", "rename"] {
        symlink("outside", temp.0.join("out")).unwrap();
        temp.fail(&[
            "extract",
            "archive.7z",
            "-o",
            "out",
            "--on-conflict",
            policy,
        ]);
        assert!(fs::symlink_metadata(temp.0.join("out"))
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_file(temp.0.join("out")).unwrap();
        fs::create_dir(temp.0.join("out")).unwrap();
        for target in ["../outside/first.txt", "../outside/missing"] {
            symlink(target, temp.0.join("out/first.txt")).unwrap();
            temp.fail(&[
                "extract",
                "archive.7z",
                "-o",
                "out",
                "--on-conflict",
                policy,
            ]);
            assert_eq!(
                fs::read_link(temp.0.join("out/first.txt")).unwrap(),
                Path::new(target)
            );
            fs::remove_file(temp.0.join("out/first.txt")).unwrap();
        }
        fs::remove_dir_all(temp.0.join("out")).unwrap();
    }
    assert_eq!(
        fs::read(temp.0.join("outside/first.txt")).unwrap(),
        b"external"
    );
    assert!(!temp.0.join("outside/missing").exists());
    fs::create_dir(temp.0.join("out")).unwrap();
    fs::hard_link(
        temp.0.join("outside/first.txt"),
        temp.0.join("out/first.txt"),
    )
    .unwrap();
    temp.ok(&["extract", "archive.7z", "-o", "out"]);
    assert_eq!(
        fs::read(temp.0.join("outside/first.txt")).unwrap(),
        b"external"
    );
    assert_eq!(
        fs::read(temp.0.join("out/first.txt")).unwrap(),
        b"first contents\n".repeat(8)
    );
}

#[test]
fn zip_and_tar_behaviors_are_preserved() {
    let temp = Temp::new();
    fs::write(temp.0.join("data"), b"legacy contents").unwrap();
    for name in ["archive.zip", "archive.tar", "archive.tar.gz"] {
        temp.ok(&["create", name, "data"]);
        assert!(temp.ok(&["list", name]).contains("data"));
        temp.ok(&["test", name]);
        temp.ok(&["extract", name, "-o", "out"]);
        assert_eq!(
            fs::read(temp.0.join("out/data")).unwrap(),
            b"legacy contents"
        );
        fs::remove_dir_all(temp.0.join("out")).unwrap();
    }
    temp.ok(&["create", "encrypted.zip", "data", "-p", "secret"]);
    temp.ok(&["password", "encrypted.zip", "-p", "secret", "--new", "new"]);
    temp.ok(&["test", "encrypted.zip", "-p", "new"]);
    temp.ok(&["extract", "encrypted.zip", "-p", "new", "-o", "out"]);
    assert_eq!(
        fs::read(temp.0.join("out/data")).unwrap(),
        b"legacy contents"
    );
}
