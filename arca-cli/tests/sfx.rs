use arca_core::sfx::{Trailer, Window, CONSOLE};
use arca_zip::ZipArchive;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(dir: &Path, stub: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_arca"));
    command.current_dir(dir).args(args);
    match stub {
        Some(stub) => command.env("ARCA_SFX_STUB", stub),
        None => command.env_remove("ARCA_SFX_STUB"),
    };
    command.output().unwrap()
}

fn corpus(dir: &Path) {
    fs::create_dir_all(dir.join("src/sub")).unwrap();
    fs::write(dir.join("src/a.txt"), b"alpha").unwrap();
    fs::write(dir.join("src/sub/b.bin"), vec![9u8; 100_000]).unwrap();
}

#[test]
fn the_program_is_the_stub_followed_by_a_zip_and_its_trailer() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let stub = dir.join("stub.bin");
    fs::write(&stub, b"not really a program").unwrap();
    let output = run(dir, Some(&stub), &["create", "setup.exe", "src", "--sfx"]);
    assert!(output.status.success(), "{output:?}");
    let made = fs::read(dir.join("setup.exe")).unwrap();
    assert!(made.starts_with(b"not really a program"));
    let mut file = fs::File::open(dir.join("setup.exe")).unwrap();
    let trailer = Trailer::read(&mut file).unwrap();
    assert_eq!(trailer.offset, 20);
    assert!(!trailer.console());
    let mut zip = ZipArchive::open(Window::payload(file).unwrap()).unwrap();
    let names: Vec<_> = zip.entries().iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["src/a.txt", "src/sub/b.bin"]);
    let mut data = Vec::new();
    zip.extract_to(0, &mut data).unwrap();
    assert_eq!(data, b"alpha");
    assert!(fs::read_dir(dir).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".arca-new")));
}

// Offsets count from the start of the program, as `zip -A` leaves them, so
// the file is a ZIP from its first byte for any tool, Arca included.
#[test]
fn the_program_reads_as_a_zip_from_its_first_byte() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let stub = dir.join("stub.bin");
    fs::write(&stub, b"not really a program").unwrap();
    let output = run(dir, Some(&stub), &["create", "setup.exe", "src", "--sfx"]);
    assert!(output.status.success(), "{output:?}");
    let mut zip = ZipArchive::open(fs::File::open(dir.join("setup.exe")).unwrap()).unwrap();
    assert_eq!(zip.entries()[0].offset, 20);
    let mut data = Vec::new();
    zip.extract_to(1, &mut data).unwrap();
    assert_eq!(data, vec![9u8; 100_000]);
    for (args, says) in [
        (&["list", "setup.exe"], "src/sub/b.bin"),
        (&["test", "setup.exe"], "2 entries verified"),
    ] {
        let output = run(dir, None, args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(says));
    }
    let output = run(dir, None, &["extract", "setup.exe", "-o", "out"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(dir.join("out/src/a.txt")).unwrap(), b"alpha");
}

#[test]
fn a_program_is_not_named_like_an_archive() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let stub = dir.join("stub.bin");
    fs::write(&stub, b"stub").unwrap();
    for name in ["out.7z", "out.zip", "out.tar.gz"] {
        let output = run(dir, Some(&stub), &["create", name, "src", "--sfx"]);
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(".exe"));
        assert!(!dir.join(name).exists());
    }
}

#[test]
fn the_console_flag_reaches_the_trailer_and_the_program() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let stub = Path::new(env!("CARGO_BIN_EXE_arca"));
    let output = run(
        dir,
        Some(stub),
        &[
            "create",
            "c.exe",
            "src",
            "--sfx",
            "--console",
            "-l",
            "store",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let mut file = fs::File::open(dir.join("c.exe")).unwrap();
    let trailer = Trailer::read(&mut file).unwrap();
    assert_eq!(trailer.flags, CONSOLE);
    if cfg!(windows) {
        let made = fs::read(dir.join("c.exe")).unwrap();
        let pe = u32::from_le_bytes(made[0x3c..0x40].try_into().unwrap()) as usize;
        assert_eq!(&made[pe + 24 + 68..pe + 24 + 70], &[3, 0]);
    }
    let again = run(
        dir,
        Some(&dir.join("c.exe")),
        &["create", "d.exe", "src", "--sfx"],
    );
    assert_eq!(again.status.code(), Some(1), "{again:?}");
    assert!(String::from_utf8_lossy(&again.stderr).contains("already carries"));
}

#[test]
fn what_a_self_extractor_cannot_hold_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let stub = dir.join("stub.bin");
    fs::write(&stub, b"stub").unwrap();
    for args in [
        &["create", "x.exe", "src", "--sfx", "-v", "1m"][..],
        &["create", "x.exe", "src", "--sfx", "-p", "secret"],
        &["create", "x.zip", "src", "--console"],
    ] {
        let output = run(dir, Some(&stub), args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    }
    let output = run(
        dir,
        Some(&stub),
        &["create", "x.exe", "src", "--sfx", "-c", "lzma2"],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("deflate"));
    let missing = dir.join("nowhere").join("arca-sfx.exe");
    let output = run(dir, Some(&missing), &["create", "x.exe", "src", "--sfx"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("arca-sfx") && error.contains("nowhere"),
        "{error}"
    );
    assert!(!dir.join("x.exe").exists());
}
