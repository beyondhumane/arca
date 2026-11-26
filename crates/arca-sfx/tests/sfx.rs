// Running a program with bytes appended is what a self-extractor is, and on
// macOS that is not known to survive the linker's signature; self-extractors
// are made for Windows, and Linux runs these the same way.
#![cfg(not(target_os = "macos"))]

use arca_core::sfx::{self, CONSOLE};
use arca_core::{Codec, Level};
use arca_zip::ZipWriter;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn stub() -> Vec<u8> {
    fs::read(env!("CARGO_BIN_EXE_arca-sfx")).unwrap()
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        w.add(name, *data, Codec::Store, Level::STORE, Some(1_700_000_000))
            .unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn program(dir: &Path, name: &str, payload: &[u8], flags: u32) -> PathBuf {
    let bytes = sfx::build(Cursor::new(Vec::new()), &stub(), flags, |mut w| {
        w.write_all(payload)?;
        Ok(w)
    })
    .unwrap()
    .into_inner();
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    write_program(&path, &bytes);
    path
}

fn write_program(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn run(exe: &Path, args: &[&str]) -> Output {
    Command::new(exe).args(args).output().unwrap()
}

fn failed(output: &Output) -> String {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let error = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!error.contains("panicked"), "{error}");
    error
}

#[test]
fn a_self_extractor_extracts_what_it_carries() {
    let temp = tempfile::tempdir().unwrap();
    let big: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let payload = zip(&[
        ("docs/readme.txt", b"hello"),
        ("data/big.bin", &big),
        ("empty", b""),
    ]);
    let exe = program(temp.path(), "setup", &payload, 0);
    let dest = temp.path().join("out");
    let output = run(&exe, &["-o", dest.to_str().unwrap(), "--console"]);
    assert!(output.status.success(), "{output:?}");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(said.contains('3'), "{said}");
    assert_eq!(fs::read(dest.join("docs/readme.txt")).unwrap(), b"hello");
    assert_eq!(fs::read(dest.join("data/big.bin")).unwrap(), big);
    assert_eq!(fs::read(dest.join("empty")).unwrap(), b"");
    let again = run(&exe, &["-o", dest.to_str().unwrap(), "--console"]);
    assert!(again.status.success(), "{again:?}");
}

#[test]
fn a_self_extractor_whose_offsets_count_from_the_program_extracts() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = sfx::build(Cursor::new(Vec::new()), &stub(), 0, |w| {
        let mut z = ZipWriter::after(w)?;
        z.add("a.txt", &b"alpha"[..], Codec::Deflate, Level::NORMAL, None)?;
        z.finish()
    })
    .unwrap()
    .into_inner();
    let names: Vec<String> = arca_zip::ZipArchive::open(Cursor::new(bytes.clone()))
        .unwrap()
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["a.txt"]);
    let exe = temp
        .path()
        .join(format!("whole{}", std::env::consts::EXE_SUFFIX));
    write_program(&exe, &bytes);
    let dest = temp.path().join("out");
    let output = run(&exe, &["-o", dest.to_str().unwrap(), "--console"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"alpha");
}

#[test]
fn the_console_flag_asks_on_standard_input() {
    let temp = tempfile::tempdir().unwrap();
    let exe = program(temp.path(), "asks", &zip(&[("a.txt", b"a")]), CONSOLE);
    let dest = temp.path().join("typed here");
    let mut child = Command::new(&exe)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "\"{}\"", dest.display()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"a");
    let asks = program(temp.path(), "default", &zip(&[("b.txt", b"b")]), CONSOLE);
    let output = Command::new(&asks)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let beside = temp.path().join("default");
    let beside = if beside == asks {
        temp.path().join("default.files")
    } else {
        beside
    };
    assert_eq!(fs::read(beside.join("b.txt")).unwrap(), b"b");
}

#[test]
fn a_hostile_name_cannot_leave_the_chosen_folder() {
    let temp = tempfile::tempdir().unwrap();
    let mut payload = zip(&[("fine.txt", b"ok"), ("xx/evil.txt", b"gotcha")]);
    let mut at = 0;
    while let Some(found) = payload[at..].windows(11).position(|w| w == b"xx/evil.txt") {
        payload[at + found..at + found + 2].copy_from_slice(b"..");
        at += found + 11;
    }
    let exe = program(temp.path(), "evil", &payload, 0);
    let dest = temp.path().join("inside");
    failed(&run(&exe, &["-o", dest.to_str().unwrap(), "--console"]));
    assert!(!temp.path().join("evil.txt").exists());
    assert!(!dest.join("evil.txt").exists());
}

#[test]
fn damage_is_reported_and_leaves_no_file_behind() {
    let temp = tempfile::tempdir().unwrap();
    let mut payload = zip(&[("a.txt", b"hello world")]);
    let at = payload
        .windows(11)
        .position(|w| w == b"hello world")
        .unwrap();
    payload[at] = b'j';
    let exe = program(temp.path(), "crc", &payload, 0);
    let dest = temp.path().join("new").join("out");
    failed(&run(&exe, &["-o", dest.to_str().unwrap(), "--console"]));
    assert!(!temp.path().join("new").exists());
    let dest = temp.path().join("out");
    fs::create_dir(&dest).unwrap();
    failed(&run(&exe, &["-o", dest.to_str().unwrap(), "--console"]));
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);

    let good = program(temp.path(), "cut", &zip(&[("a.txt", b"hello")]), 0);
    let bytes = fs::read(&good).unwrap();
    for keep in [bytes.len() - 1, bytes.len() - 40, stub().len() + 10] {
        fs::write(&good, &bytes[..keep]).unwrap();
        failed(&run(&good, &["-o", dest.to_str().unwrap(), "--console"]));
    }
    let mut odd = bytes.clone();
    let trailer = odd.len() - sfx::TRAILER_LEN as usize;
    odd[trailer] ^= 1;
    fs::write(&good, &odd).unwrap();
    failed(&run(&good, &["-o", dest.to_str().unwrap(), "--console"]));
}

#[test]
fn the_stub_alone_says_it_carries_nothing() {
    let output = run(
        Path::new(env!("CARGO_BIN_EXE_arca-sfx")),
        &["-o", "unused", "--console"],
    );
    let error = failed(&output);
    assert!(error.contains("no archive"), "{error}");
    assert_eq!(
        run(Path::new(env!("CARGO_BIN_EXE_arca-sfx")), &["--bogus"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn an_output_folder_never_waits_for_a_click() {
    let temp = tempfile::tempdir().unwrap();
    let exe = program(temp.path(), "quiet", &zip(&[("a.txt", b"a")]), 0);
    let dest = temp.path().join("out");
    let broken = program(temp.path(), "broken", b"not a zip", 0);
    for (exe, works) in [(&exe, true), (&broken, false)] {
        let mut child = Command::new(exe)
            .args(["-o", dest.to_str().unwrap()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!("{} is waiting for something", exe.display());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        assert_eq!(status.success(), works, "{}", exe.display());
    }
    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"a");
}
