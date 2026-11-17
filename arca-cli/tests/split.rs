use std::fs;
use std::path::Path;
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
    let big: Vec<u8> = (0..60_000u32)
        .flat_map(|i| (i.wrapping_mul(2_654_435_761) >> 13).to_le_bytes())
        .collect();
    fs::create_dir_all(root.join("src/sub")).unwrap();
    fs::write(root.join("src/big.bin"), &big).unwrap();
    fs::write(root.join("src/sub/\u{00f1}.txt"), b"hola\n".repeat(50)).unwrap();
    big
}

fn parts(dir: &Path, name: &str) -> Vec<u64> {
    (1..)
        .map(|i| dir.join(format!("{name}.{i:03}")))
        .take_while(|p| p.exists())
        .map(|p| fs::metadata(p).unwrap().len())
        .collect()
}

fn nothing_staged(dir: &Path) {
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(!name.ends_with(".arca-new"), "{name}");
    }
}

#[test]
fn every_creatable_family_splits_and_reads_back_from_any_part() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let big = corpus(dir);
    let mut names = vec![
        "a.zip",
        "a.7z",
        "a.tar",
        "a.tar.gz",
        "a.tar.xz",
        "a.tar.bz2",
    ];
    if cfg!(feature = "codecs-native") {
        names.push("a.tar.zst");
    }
    for name in names {
        let summary = ok(dir, &["create", name, "src", "-v", "20k", "-l", "fast"]);
        assert!(summary.contains(&format!("{name}.001")), "{summary}");
        assert!(summary.contains(" parts"), "{summary}");
        assert!(!dir.join(name).exists(), "{name}");
        let sizes = parts(dir, name);
        assert!(sizes.len() >= 2, "{name}: {sizes:?}");
        assert!(sizes[..sizes.len() - 1].iter().all(|&s| s == 20 * 1024));
        let last = format!("{name}.{:03}", sizes.len());
        for part in [format!("{name}.001"), format!("{name}.002"), last] {
            let listing = ok(dir, &["list", &part]);
            assert!(listing.contains("big.bin"), "{name}: {listing}");
            assert!(ok(dir, &["test", &part]).contains("entries verified"));
        }
        let out = format!("out-{name}");
        ok(dir, &["extract", &format!("{name}.002"), "-o", &out]);
        assert_eq!(fs::read(dir.join(&out).join("src/big.bin")).unwrap(), big);
        nothing_staged(dir);
    }
}

#[test]
fn a_standalone_stream_splits_and_keeps_its_name() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let big = corpus(dir);
    ok(
        dir,
        &[
            "create",
            "big.bin.xz",
            "src/big.bin",
            "-v",
            "8k",
            "-l",
            "store",
        ],
    );
    let listing = ok(dir, &["list", "big.bin.xz.003"]);
    assert!(listing.trim_end().ends_with("big.bin"), "{listing}");
    ok(dir, &["extract", "big.bin.xz.001", "-o", "out"]);
    assert_eq!(fs::read(dir.join("out/big.bin")).unwrap(), big);
}

#[test]
fn an_existing_set_is_not_replaced_and_a_broken_one_is_named() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    ok(dir, &["create", "a.zip", "src", "-v", "16k", "-l", "store"]);
    let before = parts(dir, "a.zip");
    let error = fail(dir, &["create", "a.zip", "src", "-v", "16k"]);
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(parts(dir, "a.zip"), before);
    nothing_staged(dir);

    fs::rename(dir.join("a.zip.002"), dir.join("moved")).unwrap();
    for part in ["a.zip.001", "a.zip.003"] {
        let error = fail(dir, &["list", part]);
        assert!(
            error.contains("a.zip.002") && error.contains("missing"),
            "{error}"
        );
        fail(dir, &["extract", part, "-o", "out"]);
    }
    fs::write(dir.join("a.zip.002"), b"short").unwrap();
    let error = fail(dir, &["test", "a.zip.001"]);
    assert!(error.contains("wrong size"), "{error}");
    fs::rename(dir.join("moved"), dir.join("a.zip.002")).unwrap();
    let last = format!("a.zip.{:03}", before.len());
    let whole = fs::read(dir.join(&last)).unwrap();
    fs::write(dir.join(&last), &whole[..whole.len() / 2]).unwrap();
    fail(dir, &["test", "a.zip.001"]);
    fs::remove_file(dir.join(&last)).unwrap();
    fail(dir, &["list", "a.zip.001"]);
}

#[test]
fn sizes_formats_and_changes_that_cannot_work_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for bad in ["0", "12x", "-3k", "k"] {
        let output = run(dir, &["create", "a.zip", "src", "-v", bad]);
        assert_eq!(output.status.code(), Some(2), "{bad}: {output:?}");
    }
    let error = fail(dir, &["create", "a.zip", "src", "-v", "1"]);
    assert!(error.contains("9999"), "{error}");
    nothing_staged(dir);
    assert!(parts(dir, "a.zip").is_empty());
    let error = fail(dir, &["create", "a.rar", "src", "-v", "1m"]);
    assert!(error.contains(".rar"), "{error}");
    ok(dir, &["create", "b.zip.001", "src", "-v", "1m"]);
    assert_eq!(parts(dir, "b.zip").len(), 1);
    let error = fail(dir, &["password", "b.zip.001", "--new", "x"]);
    assert!(error.contains("read-only"), "{error}");
    let error = fail(dir, &["list", "c.rar.001"]);
    assert!(error.contains("split RAR"), "{error}");
}
