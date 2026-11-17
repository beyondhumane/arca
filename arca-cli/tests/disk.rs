use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

// The NTFS image arca-disk tests with, unpacked by arca itself.
fn ntfs(dir: &Path) -> PathBuf {
    let packed =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../arca-disk/tests/fixtures/ntfs.img.xz");
    let out = run(&[
        "extract".as_ref(),
        packed.as_os_str(),
        "-o".as_ref(),
        dir.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir.join("ntfs.img")
}

// A fixed VHD is the disk followed by a 512-byte footer.
fn fixed_vhd(disk: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 512];
    f[..8].copy_from_slice(b"conectix");
    f[8..12].copy_from_slice(&2u32.to_be_bytes());
    f[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    f[16..24].copy_from_slice(&u64::MAX.to_be_bytes());
    f[40..48].copy_from_slice(&(disk.len() as u64).to_be_bytes());
    f[48..56].copy_from_slice(&(disk.len() as u64).to_be_bytes());
    f[60..64].copy_from_slice(&2u32.to_be_bytes());
    let sum = f.iter().fold(0u32, |s, &b| s.wrapping_add(u32::from(b)));
    f[64..68].copy_from_slice(&(!sum).to_be_bytes());
    let mut out = disk.to_vec();
    out.extend(f);
    out
}

#[test]
fn an_ntfs_image_lists_tests_extracts_and_compares() {
    let dir = tempfile::tempdir().unwrap();
    let image = ntfs(dir.path());
    let listing = run(&["list".as_ref(), image.as_os_str()]);
    assert!(listing.status.success());
    let stdout = String::from_utf8_lossy(&listing.stdout);
    assert!(stdout.contains("file-with-12345"), "{stdout}");
    assert!(stdout.contains("many_subdirs/512"));
    assert!(run(&["test".as_ref(), image.as_os_str()]).status.success());
    let dest = dir.path().join("out");
    let result = run(&[
        "extract".as_ref(),
        image.as_os_str(),
        "-o".as_ref(),
        dest.as_os_str(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read(dest.join("file-with-12345")).unwrap(),
        b"12345"
    );
    let same = run(&["diff".as_ref(), image.as_os_str(), dest.as_os_str()]);
    assert_eq!(
        same.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&same.stdout)
    );
    std::fs::write(dest.join("file-with-12345"), b"54321").unwrap();
    let changed = run(&["diff".as_ref(), image.as_os_str(), dest.as_os_str()]);
    assert_eq!(changed.status.code(), Some(1));
}

#[test]
fn a_fixed_vhd_is_read_and_a_cut_one_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let disk = std::fs::read(ntfs(dir.path())).unwrap();
    let vhd = dir.path().join("server.vhd");
    std::fs::write(&vhd, fixed_vhd(&disk)).unwrap();
    let listing = run(&["list".as_ref(), vhd.as_os_str()]);
    assert!(
        listing.status.success(),
        "{}",
        String::from_utf8_lossy(&listing.stderr)
    );
    assert!(String::from_utf8_lossy(&listing.stdout).contains("1000-bytes-file"));
    let cut = dir.path().join("cut.vhd");
    std::fs::write(&cut, &disk[..disk.len() / 2]).unwrap();
    let out = run(&["list".as_ref(), cut.as_os_str()]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
}

#[test]
fn disk_images_cannot_be_created() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.txt");
    std::fs::write(&source, b"input").unwrap();
    for name in ["out.vhd", "out.vhdx", "out.vmdk", "out.dmg", "out.img"] {
        let out = dir.path().join(name);
        let result = run(&["create".as_ref(), out.as_os_str(), source.as_os_str()]);
        assert!(!result.status.success(), "{name}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("read-only"),
            "{name}"
        );
        assert!(!out.exists(), "{name}");
    }
}
