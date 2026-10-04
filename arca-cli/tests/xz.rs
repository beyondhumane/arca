use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn fail(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!error.contains("panicked"), "{error}");
    error
}

fn corpus(root: &Path) -> Vec<u8> {
    let big: Vec<u8> = (0..400_000u32)
        .flat_map(|i| (i.wrapping_mul(2_654_435_761) >> 13).to_le_bytes())
        .collect();
    fs::create_dir_all(root.join("src/sub")).unwrap();
    fs::write(root.join("src/big.bin"), &big).unwrap();
    fs::write(root.join("src/empty"), b"").unwrap();
    fs::write(root.join("src/sub/\u{00f1}.txt"), b"hola\n".repeat(50)).unwrap();
    big
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(root) {
        for e in read {
            let path = e.unwrap().path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn same_tree(a: &Path, b: &Path) {
    for name in ["big.bin", "empty", "sub/\u{00f1}.txt"] {
        assert_eq!(
            fs::read(a.join(name)).unwrap(),
            fs::read(b.join(name)).unwrap()
        );
    }
}

#[test]
fn tar_xz_and_txz_round_trip_at_every_level_and_thread_count() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for level in ["store", "fast", "normal", "best"] {
        for (threads, name) in [("1", "a.tar.xz"), ("4", "a.txz")] {
            let summary = ok(dir, &["create", name, "src", "-l", level, "-j", threads]);
            assert!(summary.contains("3 files"), "{summary}");
            let listing = ok(dir, &["list", name]);
            assert!(listing.contains("src/sub/\u{00f1}.txt"), "{listing}");
            assert!(ok(dir, &["test", name]).contains("3 entries verified"));
            let out = dir.join("out");
            let _ = fs::remove_dir_all(&out);
            ok(dir, &["extract", name, "-o", "out"]);
            same_tree(&dir.join("src"), &out.join("src"));
            assert!(!dir.join(format!("{name}.arca-new")).exists());
        }
    }
}

#[test]
fn standalone_xz_holds_one_file_named_after_the_archive() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let big = corpus(dir);
    for level in ["store", "best"] {
        ok(
            dir,
            &[
                "create",
                "big.bin.xz",
                "src/big.bin",
                "-l",
                level,
                "-j",
                "2",
            ],
        );
        let listing = ok(dir, &["list", "big.bin.xz"]);
        assert!(listing.contains(&format!("{}", big.len())), "{listing}");
        assert!(listing.contains("lzma2") && listing.trim_end().ends_with("big.bin"));
        assert!(ok(dir, &["test", "big.bin.xz"]).contains("1 entries verified"));
        let _ = fs::remove_dir_all(dir.join("out"));
        ok(dir, &["extract", "big.bin.xz", "-o", "out"]);
        assert_eq!(fs::read(dir.join("out/big.bin")).unwrap(), big);
    }

    ok(
        dir,
        &[
            "extract",
            "big.bin.xz",
            "-o",
            "out",
            "--on-conflict",
            "rename",
        ],
    );
    assert_eq!(fs::read(dir.join("out/big (1).bin")).unwrap(), big);
    ok(
        dir,
        &[
            "extract",
            "big.bin.xz",
            "-o",
            "out",
            "--on-conflict",
            "skip",
        ],
    );
    assert!(!dir.join("out/big (2).bin").exists());

    let error = fail(dir, &["create", "two.xz", "src/big.bin", "src/empty"]);
    assert!(
        error.contains("exactly one file") && error.contains(".tar.xz"),
        "{error}"
    );
    let error = fail(dir, &["create", "folder.xz", "src"]);
    assert!(error.contains("not a regular file"), "{error}");
    assert!(!dir.join("two.xz").exists() && !dir.join("folder.xz").exists());
}

#[test]
fn passwords_hidden_names_and_other_codecs_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for name in ["a.xz", "a.tar.xz"] {
        let input = if name == "a.xz" { "src/empty" } else { "src" };
        let error = fail(dir, &["create", name, input, "-p", "secret"]);
        assert!(error.contains("encryption"), "{error}");
        let error = fail(dir, &["create", name, input, "-p", "x", "--hide-names"]);
        assert!(error.contains("--hide-names"), "{error}");
        let error = fail(dir, &["create", name, input, "-c", "zstd"]);
        assert!(error.contains("LZMA2"), "{error}");
        ok(dir, &["create", name, input, "-c", "lzma2"]);
    }
}

#[test]
fn the_contents_decide_between_tar_xz_and_plain_xz() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    ok(dir, &["create", "real.tar.xz", "src"]);
    fs::copy(dir.join("real.tar.xz"), dir.join("misnamed.xz")).unwrap();
    fs::copy(dir.join("real.tar.xz"), dir.join("no-suffix")).unwrap();
    for name in ["misnamed.xz", "no-suffix"] {
        assert!(ok(dir, &["list", name]).contains("src/big.bin"), "{name}");
    }
    ok(dir, &["create", "plain.xz", "src/sub/\u{00f1}.txt"]);
    fs::copy(dir.join("plain.xz"), dir.join("plain.tar.xz")).unwrap();
    let listing = ok(dir, &["list", "plain.tar.xz"]);
    assert!(listing.trim_end().ends_with("plain.tar"), "{listing}");
}

#[test]
fn damaged_archives_fail_and_publish_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    ok(dir, &["create", "a.tar.xz", "src/big.bin"]);
    ok(dir, &["create", "big.xz", "src/big.bin"]);
    for name in ["a.tar.xz", "big.xz"] {
        let good = fs::read(dir.join(name)).unwrap();
        let mut flipped = good.clone();
        let middle = good.len() / 2;
        flipped[middle] ^= 0x40;
        let truncated = good[..good.len() - 16].to_vec();
        let mut padded = good.clone();
        padded.extend_from_slice(&[0, 0, 0]);
        for (case, bytes) in [("flip", flipped), ("cut", truncated), ("odd", padded)] {
            let bad = format!("bad-{case}-{name}");
            fs::write(dir.join(&bad), bytes).unwrap();
            fail(dir, &["test", &bad]);
            let out = format!("out-{case}-{name}");
            fail(dir, &["extract", &bad, "-o", &out]);
            assert_eq!(files_under(&dir.join(&out)), Vec::<PathBuf>::new(), "{bad}");
        }
    }
    fs::write(dir.join("junk.xz"), b"\xFD7zXZ\0 not really").unwrap();
    fail(dir, &["list", "junk.xz"]);
    fail(dir, &["extract", "junk.xz", "-o", "junk"]);
}

#[test]
fn a_failed_create_leaves_no_archive() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    fs::create_dir(dir.join("taken.xz")).unwrap();
    fail(dir, &["create", "taken.xz", "src/big.bin"]);
    assert!(dir.join("taken.xz").is_dir());
    assert!(!dir.join("taken.xz.arca-new").exists());
    fail(dir, &["create", "missing/a.tar.xz", "src"]);
    assert!(!dir.join("missing").exists());
}
