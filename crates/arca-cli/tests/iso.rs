use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../arca-iso/tests/fixtures")
        .join(name)
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn iso_images_list_test_and_extract() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("DISC.ISO");
    std::fs::copy(fixture("rockridge.iso"), &archive).unwrap();
    let listing = run(&["list".as_ref(), archive.as_os_str()]);
    assert!(listing.status.success());
    let stdout = String::from_utf8_lossy(&listing.stdout);
    assert!(stdout.contains("docs/A long name with spaces.txt"));
    assert!(!stdout.contains("link-to-readme"));
    assert!(String::from_utf8_lossy(&listing.stderr).contains("link-to-readme"));
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
        std::fs::read(dest.join("docs/año.txt")).unwrap(),
        "ñandú\n".as_bytes()
    );
}

#[test]
fn iso_images_cannot_be_created_or_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.txt");
    std::fs::write(&source, b"input").unwrap();
    let out = dir.path().join("out.iso");
    let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ISO is read-only"));
    assert!(!out.exists());
    std::fs::copy(fixture("plain.iso"), &out).unwrap();
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

#[test]
fn a_damaged_image_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("cut.iso");
    let data = std::fs::read(fixture("joliet.iso")).unwrap();
    std::fs::write(&archive, &data[..data.len() / 2]).unwrap();
    let dest = dir.path().join("x");
    let calls: [&[&std::ffi::OsStr]; 3] = [
        &["list".as_ref(), archive.as_os_str()],
        &["test".as_ref(), archive.as_os_str()],
        &[
            "extract".as_ref(),
            archive.as_os_str(),
            "-o".as_ref(),
            dest.as_os_str(),
        ],
    ];
    for args in calls {
        let result = run(args);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("truncated"));
    }
    assert!(!dest.exists());
}
