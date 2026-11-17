use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

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

#[test]
fn known_vectors_print_in_the_sha256sum_format() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::write(dir.join("abc.txt"), b"abc").unwrap();
    fs::write(dir.join("digits.txt"), b"123456789").unwrap();

    let out = arca(dir, &["hash", "abc.txt"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), format!("{ABC_SHA256}  abc.txt\n"));

    let out = arca(dir, &["hash", "-a", "crc32", "digits.txt"]);
    assert_eq!(stdout(&out), "cbf43926  digits.txt\n");

    let out = arca(dir, &["hash", "--algorithm", "sha1", "abc.txt"]);
    assert_eq!(
        stdout(&out),
        "a9993e364706816aba3e25717850c26c9cd0d89d  abc.txt\n"
    );
}

#[test]
fn folders_are_walked_in_name_order() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::create_dir_all(dir.join("tree/b")).unwrap();
    fs::write(dir.join("tree/b/z.txt"), b"abc").unwrap();
    fs::write(dir.join("tree/a.txt"), b"abc").unwrap();
    fs::write(dir.join("tree/c.txt"), b"abc").unwrap();
    let out = arca(dir, &["hash", "tree"]);
    assert!(out.status.success());
    let names: Vec<String> = stdout(&out)
        .lines()
        .map(|line| line.split_once("  ").unwrap().1.to_string())
        .collect();
    assert_eq!(names, ["tree/a.txt", "tree/b/z.txt", "tree/c.txt"]);
}

#[test]
fn a_sum_file_is_checked_and_a_changed_file_fails() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::create_dir(dir.join("data")).unwrap();
    fs::write(dir.join("data/one.txt"), b"abc").unwrap();
    fs::write(dir.join("data/two.txt"), b"123456789").unwrap();
    let out = arca(dir, &["hash", "data"]);
    assert!(out.status.success());
    fs::write(dir.join("SUMS"), &out.stdout).unwrap();

    let out = arca(dir, &["hash", "--check", "SUMS"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), "data/one.txt: OK\ndata/two.txt: OK\n");

    fs::write(dir.join("data/two.txt"), b"12345678").unwrap();
    fs::remove_file(dir.join("data/one.txt")).unwrap();
    let out = arca(dir, &["hash", "-c", "SUMS"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "data/one.txt: FAILED open or read\ndata/two.txt: FAILED\n"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("1 computed checksums did NOT match"), "{err}");
    assert!(err.contains("1 listed files could not be read"), "{err}");
}

#[test]
fn a_check_reads_crc32_lines_crlf_and_binary_markers() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::write(dir.join("digits.txt"), b"123456789").unwrap();
    fs::write(dir.join("abc.txt"), b"abc").unwrap();
    fs::write(
        dir.join("SUMS"),
        format!("\u{feff}CBF43926 *digits.txt\r\n{ABC_SHA256}  abc.txt\r\n\r\n"),
    )
    .unwrap();
    let out = arca(dir, &["hash", "--check", "SUMS"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout(&out), "digits.txt: OK\nabc.txt: OK\n");
}

#[test]
fn malformed_sum_files_are_reported_without_panicking() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::write(dir.join("abc.txt"), b"abc").unwrap();
    fs::write(
        dir.join("SUMS"),
        format!("not a sum\n{}\n{ABC_SHA256}  abc.txt\n", &ABC_SHA256[..40]),
    )
    .unwrap();
    let out = arca(dir, &["hash", "--check", "SUMS"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), "abc.txt: OK\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("2 lines are improperly formatted"));

    for garbage in [&b""[..], b"\xff\xfe\x00junk", &ABC_SHA256.as_bytes()[..10]] {
        fs::write(dir.join("BAD"), garbage).unwrap();
        let out = arca(dir, &["hash", "--check", "BAD"]);
        assert_eq!(out.status.code(), Some(1));
        assert!(stdout(&out).is_empty());
    }
}

#[test]
fn a_missing_path_is_reported_and_the_rest_still_printed() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::write(dir.join("abc.txt"), b"abc").unwrap();
    let out = arca(dir, &["hash", "missing.txt", "abc.txt"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), format!("{ABC_SHA256}  abc.txt\n"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("missing.txt"));
}

#[test]
fn paths_and_check_cannot_be_combined() {
    let room = tempfile::tempdir().unwrap();
    let out = arca(room.path(), &["hash", "--check", "SUMS", "x"]);
    assert_eq!(out.status.code(), Some(2));
    let out = arca(room.path(), &["hash"]);
    assert_eq!(out.status.code(), Some(2));
}
