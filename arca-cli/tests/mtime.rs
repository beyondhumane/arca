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
