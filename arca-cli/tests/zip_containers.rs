use std::path::Path;
use std::process::{Command, Output};

const CONTAINERS: [&str; 11] = [
    "apk", "aar", "jar", "war", "ear", "epub", "cbz", "xpi", "whl", "nupkg", "ipa",
];

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
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

// A stored ZIP built by hand, so a test can hold names that `arca create`
// would never write.
fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let header = |signature: u32, central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(&signature.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            h.extend_from_slice(&20u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&0u32.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            if central {
                h.extend_from_slice(&[0; 6]);
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend_from_slice(&header(0x0403_4b50, false));
        out.extend_from_slice(data);
        central.extend_from_slice(&header(0x0201_4b50, true));
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn sample(dir: &Path) -> Vec<u8> {
    let input = dir.join("input");
    std::fs::create_dir_all(input.join("META-INF")).unwrap();
    std::fs::write(input.join("mimetype"), b"application/epub+zip").unwrap();
    std::fs::write(
        input.join("META-INF/MANIFEST.MF"),
        b"Manifest-Version: 1.0\n",
    )
    .unwrap();
    std::fs::write(input.join("page.txt"), "página ".repeat(500)).unwrap();
    let zip = dir.join("sample.zip");
    let result = run(&["create".as_ref(), zip.as_os_str(), input.as_os_str()]);
    assert!(result.status.success(), "{}", stderr(&result));
    std::fs::read(zip).unwrap()
}

fn extension_variants() -> Vec<String> {
    CONTAINERS
        .iter()
        .flat_map(|e| [e.to_string(), e.to_ascii_uppercase()])
        .collect()
}

#[test]
fn zip_containers_list_test_and_extract_like_zip() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample(dir.path());
    let zip = dir.path().join("sample.zip");
    let zip_listing = run(&["list".as_ref(), zip.as_os_str()]);
    assert!(zip_listing.status.success());
    for extension in extension_variants() {
        let archive = dir.path().join(format!("sample.{extension}"));
        std::fs::write(&archive, &bytes).unwrap();

        let listing = run(&["list".as_ref(), archive.as_os_str()]);
        assert!(
            listing.status.success(),
            "{extension}: {}",
            stderr(&listing)
        );
        assert_eq!(listing.stdout, zip_listing.stdout, "{extension}");

        let test = run(&["test".as_ref(), archive.as_os_str()]);
        assert!(test.status.success(), "{extension}: {}", stderr(&test));

        let out = dir.path().join(format!("out-{extension}"));
        let extract = run(&[
            "extract".as_ref(),
            archive.as_os_str(),
            "-o".as_ref(),
            out.as_os_str(),
        ]);
        assert!(
            extract.status.success(),
            "{extension}: {}",
            stderr(&extract)
        );
        assert_eq!(
            std::fs::read(out.join("input/mimetype")).unwrap(),
            b"application/epub+zip"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("input/page.txt")).unwrap(),
            "página ".repeat(500)
        );
    }
}

#[test]
fn zip_containers_cannot_be_created_or_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample(dir.path());
    let source = dir.path().join("input/page.txt");
    for extension in extension_variants() {
        let out = dir.path().join(format!("new.{extension}"));
        let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
        assert!(!result.status.success(), "{extension}");
        assert!(stderr(&result).contains("read-only"), "{}", stderr(&result));
        assert!(!out.exists(), "{extension}");

        std::fs::write(&out, &bytes).unwrap();
        let result = run(&[
            "password".as_ref(),
            out.as_os_str(),
            "--new".as_ref(),
            "secret".as_ref(),
        ]);
        assert!(!result.status.success(), "{extension}");
        assert!(stderr(&result).contains("read-only"), "{}", stderr(&result));
        assert_eq!(std::fs::read(&out).unwrap(), bytes, "{extension}");

        let zip = dir.path().join("sample.zip");
        let exported = dir.path().join(format!("exported.{extension}"));
        let result = run(&[
            "password".as_ref(),
            zip.as_os_str(),
            "--new".as_ref(),
            "secret".as_ref(),
            "--out".as_ref(),
            exported.as_os_str(),
        ]);
        assert!(!result.status.success(), "{extension}");
        assert!(stderr(&result).contains("read-only"), "{}", stderr(&result));
        assert!(!exported.exists(), "{extension}");
    }
}

#[test]
fn zip_containers_detect_bad_crc() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = stored_zip(&[("chapter.xhtml", b"<p>hello</p>")]);
    let at = bytes.windows(5).position(|w| w == b"hello").unwrap();
    bytes[at] = b'j';
    for extension in CONTAINERS {
        let archive = dir.path().join(format!("bad.{extension}"));
        std::fs::write(&archive, &bytes).unwrap();
        let result = run(&["test".as_ref(), archive.as_os_str()]);
        assert!(!result.status.success(), "{extension}");
        assert_eq!(result.status.code(), Some(1), "{extension}");
    }
}

#[test]
fn zip_containers_reject_paths_that_escape_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = stored_zip(&[("../evil.txt", b"evil"), ("ok.txt", b"ok")]);
    for extension in CONTAINERS {
        let archive = dir.path().join(format!("slip.{extension}"));
        std::fs::write(&archive, &bytes).unwrap();
        let out = dir.path().join(format!("out-{extension}"));
        let result = run(&[
            "extract".as_ref(),
            archive.as_os_str(),
            "-o".as_ref(),
            out.as_os_str(),
        ]);
        assert!(!result.status.success(), "{extension}");
        assert!(stderr(&result).contains("escape"), "{}", stderr(&result));
        assert!(!dir.path().join("evil.txt").exists());
    }
}

#[test]
fn truncated_zip_containers_fail_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = sample(dir.path());
    for extension in CONTAINERS {
        for keep in [0, 3, 30, bytes.len() / 2, bytes.len() - 10] {
            let archive = dir.path().join(format!("cut-{keep}.{extension}"));
            std::fs::write(&archive, &bytes[..keep]).unwrap();
            let out = dir.path().join(format!("cut-{keep}-{extension}"));
            for args in [
                vec!["list".as_ref(), archive.as_os_str()],
                vec!["test".as_ref(), archive.as_os_str()],
                vec![
                    "extract".as_ref(),
                    archive.as_os_str(),
                    "-o".as_ref(),
                    out.as_os_str(),
                ],
            ] {
                let result = run(&args);
                assert_eq!(
                    result.status.code(),
                    Some(1),
                    "{extension} cut at {keep}: {}",
                    stderr(&result)
                );
            }
        }
    }
}

#[test]
fn unknown_extensions_list_every_recognized_one() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.unknown");
    std::fs::write(&archive, b"").unwrap();
    let result = run(&["list".as_ref(), archive.as_os_str()]);
    assert!(!result.status.success());
    let message = stderr(&result);
    for extension in CONTAINERS.iter().chain(&["zip", "tar.gz", "cbr"]) {
        assert!(message.contains(&format!(".{extension}")), "{message}");
    }
}
