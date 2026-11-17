use std::ffi::OsStr;
use std::process::{Command, Output};

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .args(args)
        .output()
        .unwrap()
}

fn central_directory_at(buf: &[u8]) -> usize {
    buf.windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap()
}

#[test]
fn repair_rebuilds_an_archive_whose_directory_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(input.join("a.txt"), b"first file ".repeat(100)).unwrap();
    std::fs::write(input.join("b.txt"), b"second file ".repeat(100)).unwrap();
    let good = dir.path().join("good.zip");
    assert!(
        run(&["create".as_ref(), good.as_os_str(), input.as_os_str()])
            .status
            .success()
    );

    let whole = std::fs::read(&good).unwrap();
    let broken = dir.path().join("broken.zip");
    std::fs::write(&broken, &whole[..central_directory_at(&whole)]).unwrap();
    let before = std::fs::read(&broken).unwrap();
    assert!(!run(&["list".as_ref(), broken.as_os_str()]).status.success());

    let fixed = dir.path().join("fixed.zip");
    let out = run(&[
        "repair".as_ref(),
        broken.as_os_str(),
        "-o".as_ref(),
        fixed.as_os_str(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("2 entries recovered, 0 skipped"));
    assert_eq!(
        std::fs::read(&broken).unwrap(),
        before,
        "the input was modified"
    );

    let test = run(&["test".as_ref(), fixed.as_os_str()]);
    assert!(test.status.success());
    let listing =
        String::from_utf8_lossy(&run(&["list".as_ref(), fixed.as_os_str()]).stdout).into_owned();
    assert!(listing.contains("input/a.txt") && listing.contains("input/b.txt"));
}

#[test]
fn repair_refuses_to_write_over_its_input_or_to_a_non_zip() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.zip");
    std::fs::write(&broken, b"PK\x03\x04 not much of an archive").unwrap();
    let before = std::fs::read(&broken).unwrap();

    let same = run(&[
        "repair".as_ref(),
        broken.as_os_str(),
        "-o".as_ref(),
        broken.as_os_str(),
    ]);
    assert!(!same.status.success());
    assert_eq!(std::fs::read(&broken).unwrap(), before);

    let tar = dir.path().join("out.tar");
    assert!(!run(&[
        "repair".as_ref(),
        broken.as_os_str(),
        "-o".as_ref(),
        tar.as_os_str()
    ])
    .status
    .success());

    let empty = dir.path().join("empty.zip");
    let none = run(&[
        "repair".as_ref(),
        broken.as_os_str(),
        "-o".as_ref(),
        empty.as_os_str(),
    ]);
    assert!(!none.status.success());
    assert!(String::from_utf8_lossy(&none.stderr).contains("skipped at offset 0"));
    assert!(
        !empty.exists(),
        "nothing recovered must leave nothing behind"
    );
    assert!(std::fs::read_dir(dir.path()).unwrap().count() == 1);
}
