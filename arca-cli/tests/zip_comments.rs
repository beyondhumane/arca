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
