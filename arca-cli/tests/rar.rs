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

#[test]
fn rar_and_cbr_cannot_be_created_or_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.txt");
    std::fs::write(&source, b"input").unwrap();
    for extension in ["rar", "cbr"] {
        let out = dir.path().join(format!("out.{extension}"));
        let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("read-only"));
        assert!(!out.exists());
        std::fs::copy(fixture("plain.rar"), &out).unwrap();
        let before = std::fs::read(&out).unwrap();
        let result = run(&[
            "password".as_ref(),
            out.as_os_str(),
            "--new".as_ref(),
            "no".as_ref(),
        ]);
        assert!(!result.status.success());
        assert_eq!(std::fs::read(&out).unwrap(), before);
    }
}

#[cfg(not(feature = "rar"))]
#[test]
fn explains_how_to_enable_the_reader() {
    let archive = fixture("plain.rar");
    for verb in ["list", "test", "extract"] {
        let result = run(&[verb.as_ref(), archive.as_os_str()]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("--features rar"));
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
