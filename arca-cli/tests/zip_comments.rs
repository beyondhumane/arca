use std::process::{Command, Output};

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

fn commented_zip(name: &str, data: &[u8], entry_comment: &[u8], comment: &[u8]) -> Vec<u8> {
    let crc = crc32(data);
    let size = data.len() as u32;
    let fixed = |h: &mut Vec<u8>| {
        h.extend_from_slice(&20u16.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&(name.len() as u16).to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
    };
    let mut out = 0x0403_4b50u32.to_le_bytes().to_vec();
    fixed(&mut out);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(data);
    let start = out.len() as u32;
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    fixed(&mut out);
    out.extend_from_slice(&(entry_comment.len() as u16).to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(entry_comment);
    let size_cd = out.len() as u32 - start;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&size_cd.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
    out.extend_from_slice(comment);
    out
}

#[test]
fn list_shows_comments_on_stderr_without_terminal_escapes() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("c.zip");
    std::fs::write(
        &zip,
        commented_zip(
            "a.txt",
            b"hola",
            b"per entry",
            b"first line\r\nsecond \x1b[31mline",
        ),
    )
    .unwrap();

    let out = run(&["list".as_ref(), zip.as_os_str()]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stdout.lines().count(), 1, "stdout must stay a clean list");
    assert!(stdout.contains("a.txt"));
    assert!(stderr.contains("comment on a.txt: per entry"), "{stderr}");
    assert!(stderr.contains("archive comment:"), "{stderr}");
    assert!(stderr.contains("  first line\n"), "{stderr}");
    assert!(stderr.contains("second \u{FFFD}[31mline"), "{stderr}");
    assert!(!stderr.contains('\x1b'), "an escape reached the terminal");
}

fn text(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn comment_shows_sets_and_clears_archive_and_entry_comments() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("c.zip");
    std::fs::write(&zip, commented_zip("a.txt", b"hola", b"old note", b"old")).unwrap();
    let arca = |args: &[&str]| {
        let mut all: Vec<&std::ffi::OsStr> = vec!["comment".as_ref(), zip.as_os_str()];
        all.extend(args.iter().map(std::ffi::OsStr::new));
        run(&all)
    };

    let shown = arca(&[]);
    assert!(shown.status.success());
    assert_eq!(text(&shown).0.trim_end(), "old");
    assert_eq!(text(&arca(&["--entry", "a.txt"])).0.trim_end(), "old note");

    let set = arca(&["--set", "l\u{ed}nea uno\nl\u{ed}nea dos"]);
    assert!(set.status.success(), "{}", text(&set).1);
    assert_eq!(
        text(&arca(&[])).0.trim_end(),
        "l\u{ed}nea uno\nl\u{ed}nea dos"
    );
    assert_eq!(text(&arca(&["--entry", "a.txt"])).0.trim_end(), "old note");

    let note = dir.path().join("note.txt");
    std::fs::write(&note, "de un fichero").unwrap();
    let from_file = arca(&["--entry", "a.txt", "--file", note.to_str().unwrap()]);
    assert!(from_file.status.success(), "{}", text(&from_file).1);
    assert_eq!(
        text(&arca(&["--entry", "a.txt"])).0.trim_end(),
        "de un fichero"
    );

    assert!(arca(&["--clear"]).status.success());
    assert_eq!(text(&arca(&[])).0, "");
    let test = run(&["test".as_ref(), zip.as_os_str()]);
    assert!(test.status.success());

    let missing = arca(&["--entry", "nope.txt", "--set", "x"]);
    assert!(!missing.status.success());
    assert!(text(&missing).1.contains("no entry named"));
    assert!(!arca(&["--set", "a", "--clear"]).status.success());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("arca-new"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn comment_writes_elsewhere_with_out_and_refuses_what_it_cannot_change() {
    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("c.zip");
    let original = commented_zip("a.txt", b"hola", b"", b"");
    std::fs::write(&zip, &original).unwrap();
    let copy = dir.path().join("copy.zip");
    let out = run(&[
        "comment".as_ref(),
        zip.as_os_str(),
        "--set".as_ref(),
        "nuevo".as_ref(),
        "-o".as_ref(),
        copy.as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out).1);
    assert_eq!(std::fs::read(&zip).unwrap(), original);
    assert_eq!(
        text(&run(&["comment".as_ref(), copy.as_os_str()]))
            .0
            .trim_end(),
        "nuevo"
    );

    let tar = dir.path().join("t.tar");
    let input = dir.path().join("in.txt");
    std::fs::write(&input, b"x").unwrap();
    assert!(
        run(&["create".as_ref(), tar.as_os_str(), input.as_os_str()])
            .status
            .success()
    );
    let refused = run(&[
        "comment".as_ref(),
        tar.as_os_str(),
        "--set".as_ref(),
        "x".as_ref(),
    ]);
    assert!(!refused.status.success());
    assert!(text(&refused)
        .1
        .contains("only ZIP comments can be changed"));
    assert!(run(&["comment".as_ref(), tar.as_os_str()]).status.success());
}
