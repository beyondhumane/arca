use std::fs::{self, File};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

const WHEN: u64 = 1_600_000_000;

fn ok(dir: &Path, args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn modified(path: &Path) -> u64 {
    fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn extraction_restores_the_recorded_modification_time() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::create_dir_all(dir.join("src/sub")).unwrap();
    for name in ["src/a.txt", "src/sub/b.txt"] {
        fs::write(dir.join(name), b"dated contents").unwrap();
        File::options()
            .write(true)
            .open(dir.join(name))
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(WHEN))
            .unwrap();
    }
    for archive in ["dated.zip", "dated.tar", "dated.tar.gz", "dated.7z"] {
        ok(dir, &["create", archive, "src"]);
        let out = format!("out-{archive}");
        ok(dir, &["extract", archive, "-o", &out]);
        for name in ["src/a.txt", "src/sub/b.txt"] {
            assert_eq!(
                modified(&dir.join(&out).join(name)),
                WHEN,
                "{archive}: {name}"
            );
        }
    }
}

#[test]
fn an_archive_made_inside_its_own_folder_leaves_itself_out() {
    let room = tempfile::tempdir().unwrap();
    let dir = room.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.txt"), b"contents").unwrap();
    for archive in ["pack.zip", "pack.tar", "pack.tar.gz", "pack.7z"] {
        let inside = format!("src/{archive}");
        fs::write(dir.join(&inside), b"an older archive").unwrap();
        ok(dir, &["create", &inside, "src"]);
        let out = dir.join(format!("out-{archive}"));
        ok(dir, &["extract", &inside, "-o", out.to_str().unwrap()]);
        assert!(out.join("src/a.txt").is_file(), "{archive}");
        assert!(!out.join(&inside).exists(), "{archive} took itself in");
        fs::remove_file(dir.join(&inside)).unwrap();
    }
}
