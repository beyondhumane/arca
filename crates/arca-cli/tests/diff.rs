use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn arca(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn create(dir: &Path, out: &str, inputs: &[&str], extra: &[&str]) {
    let mut args = vec!["create", out];
    args.extend_from_slice(inputs);
    args.extend_from_slice(extra);
    let made = arca(dir, &args);
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
}

fn tree(dir: &Path) {
    fs::create_dir_all(dir.join("proj/src")).unwrap();
    fs::write(dir.join("proj/src/main.rs"), b"fn main() {}\n").unwrap();
    fs::write(dir.join("proj/README"), b"read me\n").unwrap();
    fs::write(dir.join("proj/gone.txt"), b"soon gone\n").unwrap();
}

#[test]
fn identical_sides_exit_zero_and_print_nothing() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "a.zip", &["proj"], &[]);
    create(dir, "b.tar.gz", &["proj"], &[]);
    create(dir, "c.7z", &["proj"], &[]);
    for (a, b) in [
        ("a.zip", "b.tar.gz"),
        ("b.tar.gz", "c.7z"),
        ("a.zip", "a.zip"),
    ] {
        let out = arca(dir, &["diff", a, b]);
        assert_eq!(out.status.code(), Some(0), "{a} {b}");
        assert_eq!(stdout(&out), "");
    }
    let out = arca(dir, &["diff", "--quick", "a.zip", "c.7z"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn every_kind_of_difference_has_its_marker_and_exit_one() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "old.zip", &["proj"], &[]);
    fs::write(dir.join("proj/src/main.rs"), b"fn main() { }\n").unwrap();
    fs::remove_file(dir.join("proj/gone.txt")).unwrap();
    fs::write(dir.join("proj/new.txt"), b"new\n").unwrap();
    fs::remove_file(dir.join("proj/README")).unwrap();
    fs::create_dir(dir.join("proj/README")).unwrap();
    fs::write(dir.join("proj/README/inner"), b"inner\n").unwrap();
    create(dir, "new.zip", &["proj"], &[]);

    let out = arca(dir, &["diff", "old.zip", "new.zip"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "T proj/README\n+ proj/README/inner\n- proj/gone.txt\n+ proj/new.txt\nM proj/src/main.rs\n"
    );
}

#[test]
fn an_archive_is_compared_with_a_folder_by_what_the_folder_holds() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "a.zip", &["proj"], &[]);
    let out = arca(dir, &["diff", "a.zip", "."]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("+ a.zip\n"), "{}", stdout(&out));
    assert!(!stdout(&out).contains("proj/src/main.rs"));

    fs::create_dir(dir.join("copy")).unwrap();
    let made = arca(dir, &["extract", "a.zip", "-o", "copy"]);
    assert!(made.status.success());
    let out = arca(dir, &["diff", "a.zip", "copy"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    fs::write(dir.join("copy/proj/README"), b"read me!\n").unwrap();
    let out = arca(dir, &["diff", "copy", "a.zip"]);
    assert_eq!(stdout(&out), "M proj/README\n");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn passwords_are_given_for_each_side() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "one.zip", &["proj"], &["-p", "first"]);
    create(dir, "two.7z", &["proj"], &["-p", "second", "--hide-names"]);
    create(dir, "plain.zip", &["proj"], &[]);

    let out = arca(dir, &["diff", "-p", "first", "one.zip", "plain.zip"]);
    assert_eq!(out.status.code(), Some(0));
    let both = [
        "diff",
        "-p",
        "first",
        "--password-b",
        "second",
        "one.zip",
        "two.7z",
    ];
    assert_eq!(arca(dir, &both).status.code(), Some(0));
    let out = arca(dir, &["diff", "-p", "first", "one.zip", "two.7z"]);
    assert_eq!(out.status.code(), Some(2));
    let out = arca(dir, &["diff", "one.zip", "plain.zip"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&out.stderr).is_empty());
}

#[test]
fn a_damaged_or_missing_side_exits_two_without_a_panic() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "a.zip", &["proj"], &[]);
    let data = fs::read(dir.join("a.zip")).unwrap();
    fs::write(dir.join("short.zip"), &data[..data.len() / 2]).unwrap();
    fs::write(dir.join("notes.txt"), b"not an archive").unwrap();
    for args in [
        ["diff", "short.zip", "a.zip"],
        ["diff", "a.zip", "missing.zip"],
        ["diff", "a.zip", "notes.txt"],
    ] {
        let out = arca(dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.starts_with("arca: "), "{err}");
        assert!(!err.contains("panicked"), "{err}");
    }
}

#[test]
fn times_are_reported_only_when_asked_for() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    tree(dir);
    create(dir, "a.tar", &["proj"], &[]);
    let file = fs::File::options()
        .write(true)
        .open(dir.join("proj/README"))
        .unwrap();
    file.set_modified(
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000),
    )
    .unwrap();
    drop(file);
    let out = arca(dir, &["diff", "a.tar", "."]);
    assert!(!stdout(&out).contains("README"));
    let out = arca(dir, &["diff", "--times", "a.tar", "."]);
    assert!(stdout(&out).contains("m proj/README\n"), "{}", stdout(&out));
}
