use arca_tar::TarWriter;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &Path, args: &[&str]) -> (String, String) {
    let output = run(dir, args);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{args:?}: {stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    (String::from_utf8(output.stdout).unwrap(), stderr)
}

fn hostile_tar(path: &Path) {
    let mut w = TarWriter::new(File::create(path).unwrap());
    w.add("lib/real.so", 4, 1_700_000_000, 0o755, &b"code"[..])
        .unwrap();
    w.add_link("lib/link.so", "real.so", 1_700_000_000, b'2')
        .unwrap();
    w.add_link("lib/hard.so", "./lib/real.so", 1_700_000_000, b'1')
        .unwrap();
    w.add_link("evil", "/etc/passwd", 0, b'2').unwrap();
    w.add_link("up", "../outside", 0, b'2').unwrap();
    w.add_link("orphan", "not-in-the-archive", 0, b'1').unwrap();
    w.add_link("a", "lib", 0, b'2').unwrap();
    w.add("a/x", 7, 0, 0o644, &b"payload"[..]).unwrap();
    w.finish().unwrap();
}

// Links that stay inside come out as links where the system can make them;
// the rest are reported on stderr and leave nothing behind, inside the
// destination or out of it.
#[test]
fn tar_links_are_recreated_only_inside_the_destination() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    hostile_tar(&dir.join("links.tar"));

    let (listing, _) = ok(dir, &["list", "links.tar"]);
    assert!(listing.contains("lib/link.so -> real.so"), "{listing}");
    assert!(
        listing.contains("lib/hard.so link to ./lib/real.so"),
        "{listing}"
    );

    let (summary, stderr) = ok(dir, &["extract", "links.tar", "-o", "out"]);
    let out = dir.join("out");
    assert_eq!(fs::read(out.join("lib/hard.so")).unwrap(), b"code");
    assert_eq!(fs::read(out.join("a/x")).unwrap(), b"payload");
    assert!(!out.join("lib/x").exists());
    for refused in ["'evil'", "'up'", "'orphan'"] {
        assert!(stderr.contains(refused), "{refused}: {stderr}");
    }
    assert!(!out.join("evil").exists());
    assert!(!out.join("up").exists());
    assert!(!out.join("orphan").exists());
    assert!(!dir.join("outside").exists());
    let link = fs::symlink_metadata(out.join("lib/link.so"));
    if cfg!(unix) {
        assert!(link.unwrap().file_type().is_symlink());
        assert_eq!(fs::read(out.join("lib/link.so")).unwrap(), b"code");
        assert!(
            stderr.contains("'a' -> 'lib' was not created: a folder"),
            "{stderr}"
        );
        assert!(summary.starts_with("4 files"), "{summary}");
    } else {
        assert!(link.is_err());
        assert!(stderr.contains("'lib/link.so'"), "{stderr}");
        assert!(
            stderr.contains("cannot be created on this system"),
            "{stderr}"
        );
        assert!(summary.starts_with("3 files"), "{summary}");
    }
}

// A tree with links goes into a TAR with its links as links, whatever they
// point at, and comes back out with only the safe ones made. A ZIP still
// leaves links out.
#[cfg(unix)]
#[test]
fn creating_a_tar_stores_symbolic_links_without_following_them() {
    use std::os::unix::fs::symlink;
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    let outside = dir.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret"), b"secret").unwrap();
    let src = dir.join("src");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("sub/real.txt"), b"real").unwrap();
    symlink("sub/real.txt", src.join("near")).unwrap();
    symlink(&outside, src.join("far")).unwrap();
    let long = format!("{}sub/real.txt", "./".repeat(60));
    symlink(&long, src.join("long")).unwrap();
    symlink("sub/missing/../real.txt", src.join("through")).unwrap();

    ok(dir, &["create", "tree.tar", "src"]);
    let (listing, _) = ok(dir, &["list", "tree.tar"]);
    assert!(listing.contains("src/near -> sub/real.txt"), "{listing}");
    assert!(
        listing.contains(&format!("src/long -> {long}")),
        "{listing}"
    );
    assert!(!listing.contains("secret"), "{listing}");

    let (_, stderr) = ok(dir, &["extract", "tree.tar", "-o", "out"]);
    let out = dir.join("out/src");
    assert_eq!(
        fs::read_link(out.join("near")).unwrap(),
        Path::new("sub/real.txt")
    );
    assert_eq!(fs::read(out.join("near")).unwrap(), b"real");
    assert_eq!(fs::read_link(out.join("long")).unwrap(), Path::new(&long));
    assert_eq!(fs::read(out.join("long")).unwrap(), b"real");
    assert!(fs::symlink_metadata(out.join("far")).is_err());
    assert!(fs::symlink_metadata(out.join("through")).is_err());
    assert!(stderr.contains("'src/far'"), "{stderr}");
    assert!(stderr.contains("'src/through'"), "{stderr}");

    ok(dir, &["create", "tree.zip", "src"]);
    let (listing, _) = ok(dir, &["list", "tree.zip"]);
    assert!(!listing.contains("near"), "{listing}");
    assert!(!listing.contains("secret"), "{listing}");
}
