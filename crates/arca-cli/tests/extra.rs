use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../arca-extra/tests/fixtures")
        .join(name)
}

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

fn newc(entries: &[(&str, u32, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, (name, mode, data)) in entries
        .iter()
        .chain([&("TRAILER!!!", 0, &b""[..])])
        .enumerate()
    {
        out.extend_from_slice(
            format!(
                "070701{:08x}{mode:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
                i + 1,
                0,
                0,
                1,
                1_700_000_000u32,
                data.len(),
                0,
                0,
                0,
                0,
                name.len() + 1,
                0
            )
            .as_bytes(),
        );
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    out
}

#[test]
fn a_cabinet_lists_tests_extracts_and_compares() {
    let dir = tempfile::tempdir().unwrap();
    let archive = fixture("lzx.cab");
    let listing = run(&["list".as_ref(), archive.as_os_str()]);
    assert!(listing.status.success());
    let stdout = String::from_utf8_lossy(&listing.stdout);
    assert!(stdout.contains("lzx  ") && stdout.contains("docs/big.txt"));
    assert!(run(&["test".as_ref(), archive.as_os_str()])
        .status
        .success());
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
        std::fs::read(dest.join("a.txt")).unwrap(),
        b"hello from a cabinet\n"
    );
    let same = run(&["diff".as_ref(), archive.as_os_str(), dest.as_os_str()]);
    assert_eq!(
        same.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&same.stdout)
    );
    std::fs::write(dest.join("a.txt"), b"changed").unwrap();
    let changed = run(&["diff".as_ref(), archive.as_os_str(), dest.as_os_str()]);
    assert_eq!(changed.status.code(), Some(1));
}

#[test]
fn a_cpio_without_a_suffix_is_known_by_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("initramfs");
    std::fs::write(
        &archive,
        newc(&[
            ("./init", 0o100755, b"#!/bin/sh\n"),
            ("./dev/console", 0o020600, b""),
        ]),
    )
    .unwrap();
    let listing = run(&["list".as_ref(), archive.as_os_str()]);
    assert!(listing.status.success());
    assert!(String::from_utf8_lossy(&listing.stdout).contains("init"));
    assert!(String::from_utf8_lossy(&listing.stderr).contains("device files"));
}

#[test]
fn the_extra_formats_cannot_be_created_or_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.txt");
    std::fs::write(&source, b"input").unwrap();
    for name in ["out.cab", "out.deb", "out.cpio"] {
        let out = dir.path().join(name);
        let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
        assert!(!result.status.success(), "{name}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("read-only"),
            "{name}"
        );
        assert!(!out.exists(), "{name}");
    }
    let cab = dir.path().join("copy.cab");
    std::fs::copy(fixture("mszip.cab"), &cab).unwrap();
    let before = std::fs::read(&cab).unwrap();
    let result = run(&[
        "password".as_ref(),
        cab.as_os_str(),
        "--new".as_ref(),
        "no".as_ref(),
    ]);
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&cab).unwrap(), before);
}

#[test]
fn a_damaged_cabinet_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("cut.cab");
    let data = std::fs::read(fixture("mszip.cab")).unwrap();
    std::fs::write(&archive, &data[..data.len() / 2]).unwrap();
    let dest = dir.path().join("x");
    for args in [
        vec!["list".as_ref(), archive.as_os_str()],
        vec!["test".as_ref(), archive.as_os_str()],
        vec![
            "extract".as_ref(),
            archive.as_os_str(),
            "-o".as_ref(),
            dest.as_os_str(),
        ],
    ] {
        let out = run(&args);
        assert!(!out.status.success(), "{args:?}");
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    }
}
