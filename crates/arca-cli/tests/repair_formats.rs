use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

fn text(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn made(dir: &Path, name: &str) -> PathBuf {
    let input = dir.join("input");
    std::fs::create_dir_all(input.join("sub")).unwrap();
    std::fs::write(input.join("a.txt"), b"first file ".repeat(400)).unwrap();
    std::fs::write(input.join("b.txt"), b"second file ".repeat(400)).unwrap();
    std::fs::write(input.join("sub/c.txt"), b"third file ".repeat(400)).unwrap();
    let out = dir.join(name);
    let made = run(&["create".as_ref(), out.as_os_str(), input.as_os_str()]);
    assert!(made.status.success(), "{}", text(&made).1);
    out
}

fn repair(broken: &Path, fixed: &Path) -> Output {
    run(&[
        "repair".as_ref(),
        broken.as_os_str(),
        "-o".as_ref(),
        fixed.as_os_str(),
    ])
}

#[test]
fn a_tar_with_a_damaged_header_loses_only_that_entry() {
    let dir = tempfile::tempdir().unwrap();
    let tar = made(dir.path(), "good.tar");
    let mut bytes = std::fs::read(&tar).unwrap();
    let at = (0..bytes.len())
        .step_by(512)
        .find(|&at| bytes[at..].starts_with(b"input/b.txt"))
        .unwrap();
    bytes[at + 2] ^= 0x20;
    let broken = dir.path().join("broken.tar");
    std::fs::write(&broken, &bytes).unwrap();
    let fixed = dir.path().join("fixed.tar");
    let out = repair(&broken, &fixed);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("entries recovered, 1 lost"), "{stdout}");
    assert!(stderr.contains("lost at offset"), "{stderr}");
    assert_eq!(std::fs::read(&broken).unwrap(), bytes);
    assert!(run(&["test".as_ref(), fixed.as_os_str()]).status.success());
    let listing = text(&run(&["list".as_ref(), fixed.as_os_str()])).0;
    assert!(listing.contains("input/a.txt") && listing.contains("input/sub/c.txt"));
    assert!(!listing.contains("input/b.txt"));
}

#[test]
fn a_cut_tar_gz_keeps_what_decodes_and_says_the_stream_is_damaged() {
    let dir = tempfile::tempdir().unwrap();
    let tgz = made(dir.path(), "good.tar.gz");
    let bytes = std::fs::read(&tgz).unwrap();
    let broken = dir.path().join("broken.tar.gz");
    std::fs::write(&broken, &bytes[..bytes.len() - 12]).unwrap();
    let fixed = dir.path().join("fixed.tar.gz");
    let out = repair(&broken, &fixed);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("entries recovered"), "{stdout}");
    assert!(stderr.contains("stream is damaged"), "{stderr}");
    assert!(run(&["test".as_ref(), fixed.as_os_str()]).status.success());
    assert!(!dir.path().join("fixed.tar.gz.arca-spool").exists());
    assert!(!dir.path().join("fixed.tar.gz.arca-new").exists());
}

#[test]
fn repair_keeps_the_format_and_refuses_its_input() {
    let dir = tempfile::tempdir().unwrap();
    let tar = made(dir.path(), "good.tar");
    let zip = dir.path().join("out.zip");
    let out = repair(&tar, &zip);
    assert!(!out.status.success());
    assert!(text(&out).1.contains("--out needs a .tar name"));
    assert!(!repair(&tar, &tar).status.success());
    let sound = dir.path().join("same.tar");
    assert!(repair(&tar, &sound).status.success());
}

fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn a_damaged_7z_keeps_its_folders_and_whatever_checks_out() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(input.join("sub")).unwrap();
    std::fs::write(input.join("a.txt"), noise(1, 40_000)).unwrap();
    std::fs::write(input.join("b.txt"), noise(2, 40_000)).unwrap();
    std::fs::write(input.join("sub/c.txt"), noise(3, 40_000)).unwrap();
    let sz = dir.path().join("good.7z");
    let made = run(&["create".as_ref(), sz.as_os_str(), input.as_os_str()]);
    assert!(made.status.success(), "{}", text(&made).1);
    let mut bytes = std::fs::read(&sz).unwrap();
    let packed = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    bytes[32 + packed - packed / 6] ^= 0x5A;
    let broken = dir.path().join("broken.7z");
    std::fs::write(&broken, &bytes).unwrap();
    let fixed = dir.path().join("fixed.7z");
    let out = repair(&broken, &fixed);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("entries recovered, 1 lost"), "{stdout}");
    assert!(stderr.contains("lost: input/sub/c.txt"), "{stderr}");
    assert!(run(&["test".as_ref(), fixed.as_os_str()]).status.success());
    let listing = text(&run(&["list".as_ref(), fixed.as_os_str()])).0;
    assert!(listing.contains("input/sub/"), "{listing}");
    assert!(listing.contains("input/a.txt"), "{listing}");
    assert!(!listing.contains("input/sub/c.txt"), "{listing}");
}

#[test]
fn a_7z_whose_every_file_is_lost_is_not_rewritten_with_only_its_folders() {
    let dir = tempfile::tempdir().unwrap();
    let sz = made(dir.path(), "good.7z");
    let mut bytes = std::fs::read(&sz).unwrap();
    for b in &mut bytes[40..48] {
        *b ^= 0x5A;
    }
    let broken = dir.path().join("broken.7z");
    std::fs::write(&broken, &bytes).unwrap();
    let fixed = dir.path().join("fixed.7z");
    let out = repair(&broken, &fixed);
    let (_, stderr) = text(&out);
    assert!(!out.status.success());
    assert!(stderr.contains("no file of"), "{stderr}");
    assert!(stderr.contains("lost: input/a.txt"), "{stderr}");
    assert!(!fixed.exists());
    assert!(!dir.path().join("fixed.7z.arca-new").exists());
}

#[cfg(feature = "rar")]
#[test]
fn a_rar_without_a_recovery_record_cannot_be_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let rar = made(dir.path(), "good.rar");
    let mut bytes = std::fs::read(&rar).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x5A;
    let broken = dir.path().join("broken.rar");
    std::fs::write(&broken, &bytes).unwrap();
    let fixed = dir.path().join("fixed.rar");
    let out = repair(&broken, &fixed);
    assert!(!out.status.success());
    assert!(text(&out).1.contains("recovery record"), "{}", text(&out).1);
    assert!(!fixed.exists());
}

#[test]
fn a_tar_xz_whose_first_header_is_damaged_is_still_taken_for_a_tar() {
    let dir = tempfile::tempdir().unwrap();
    let tar = made(dir.path(), "plain.tar");
    let mut bytes = std::fs::read(&tar).unwrap();
    bytes[3] ^= 0x20;
    let damaged = dir.path().join("damaged.tar");
    std::fs::write(&damaged, &bytes).unwrap();
    let stream = dir.path().join("stream.xz");
    let packed = run(&["create".as_ref(), stream.as_os_str(), damaged.as_os_str()]);
    assert!(packed.status.success(), "{}", text(&packed).1);
    let broken = dir.path().join("broken.tar.xz");
    std::fs::rename(&stream, &broken).unwrap();
    let fixed = dir.path().join("fixed.tar.xz");
    let out = repair(&broken, &fixed);
    let (stdout, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("entries recovered, 1 lost"), "{stdout}");
    assert!(run(&["test".as_ref(), fixed.as_os_str()]).status.success());
}

fn names(dir: &Path) -> Vec<std::ffi::OsString> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    names
}

fn never_replaces_a_file(made_as: &str, fresh_as: &str) {
    let dir = tempfile::tempdir().unwrap();
    let archive = made(dir.path(), made_as);
    let original = std::fs::read(&archive).unwrap();
    let taken = dir.path().join(format!("taken{}", &fresh_as[5..]));
    std::fs::write(&taken, b"somebody else's file").unwrap();
    let before = names(dir.path());

    let out = repair(&archive, &taken);
    let (stdout, stderr) = text(&out);
    assert!(!out.status.success(), "{stdout}");
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("never replaces"), "{stderr}");
    assert!(
        stderr.contains(taken.file_name().unwrap().to_str().unwrap()),
        "{stderr}"
    );
    assert_eq!(std::fs::read(&taken).unwrap(), b"somebody else's file");
    assert_eq!(names(dir.path()), before);

    let mut itself = vec![archive.clone()];
    if cfg!(windows) {
        itself.push(dir.path().join(made_as.to_uppercase()));
    }
    for same in itself {
        let out = repair(&archive, &same);
        let (_, stderr) = text(&out);
        assert!(!out.status.success());
        assert!(stderr.contains("is the damaged archive"), "{stderr}");
        assert!(stderr.contains("never replaces"), "{stderr}");
        assert_eq!(std::fs::read(&archive).unwrap(), original);
        assert_eq!(names(dir.path()), before);
    }

    let fresh = dir.path().join(fresh_as);
    let out = repair(&archive, &fresh);
    assert!(out.status.success(), "{}", text(&out).1);
    assert!(run(&["test".as_ref(), fresh.as_os_str()]).status.success());
    let mut after = before.clone();
    after.push(fresh_as.into());
    after.sort();
    assert_eq!(names(dir.path()), after);
}

#[test]
fn a_tar_repair_never_replaces_a_file() {
    never_replaces_a_file("good.tar", "fresh.tar");
}

#[test]
fn a_tar_gz_repair_never_replaces_a_file() {
    never_replaces_a_file("good.tar.gz", "fresh.tar.gz");
}

#[test]
fn a_7z_repair_never_replaces_a_file() {
    never_replaces_a_file("good.7z", "fresh.7z");
}
