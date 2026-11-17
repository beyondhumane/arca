use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn fail(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!error.contains("panicked"), "{error}");
    error
}

fn corpus(root: &Path) -> Vec<u8> {
    let big: Vec<u8> = (0..200_000u32)
        .flat_map(|i| (i.wrapping_mul(2_654_435_761) >> 13).to_le_bytes())
        .collect();
    fs::create_dir_all(root.join("src/sub")).unwrap();
    fs::write(root.join("src/big.bin"), &big).unwrap();
    fs::write(root.join("src/empty"), b"").unwrap();
    fs::write(root.join("src/sub/\u{00f1}.txt"), b"hola\n".repeat(50)).unwrap();
    big
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(root) {
        for e in read {
            let path = e.unwrap().path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn same_tree(a: &Path, b: &Path) {
    for name in ["big.bin", "empty", "sub/\u{00f1}.txt"] {
        assert_eq!(
            fs::read(a.join(name)).unwrap(),
            fs::read(b.join(name)).unwrap()
        );
    }
}

// (single suffix, TAR suffixes, codec name in errors, the -c value it takes)
fn kinds() -> Vec<(&'static str, Vec<&'static str>, &'static str, &'static str)> {
    let mut kinds = vec![("bz2", vec!["tar.bz2", "tbz2", "tbz"], "bzip2", "auto")];
    if cfg!(feature = "codecs-native") {
        kinds.push(("zst", vec!["tar.zst", "tzst"], "Zstandard", "zstd"));
    }
    kinds
}

#[test]
fn compressed_tars_round_trip_under_every_name() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for (_, tars, _, _) in kinds() {
        for (i, ext) in tars.iter().enumerate() {
            let name = format!("a.{ext}");
            let threads = if i == 0 { "1" } else { "4" };
            ok(dir, &["create", &name, "src", "-l", "fast", "-j", threads]);
            let listing = ok(dir, &["list", &name]);
            assert!(listing.contains("src/sub/\u{00f1}.txt"), "{listing}");
            assert!(ok(dir, &["test", &name]).contains("3 entries verified"));
            let out = format!("out-{ext}");
            ok(dir, &["extract", &name, "-o", &out]);
            same_tree(&dir.join("src"), &dir.join(&out).join("src"));
        }
    }
}

#[test]
fn a_standalone_stream_is_one_file_named_after_the_archive() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let big = corpus(dir);
    for (ext, tars, _, _) in kinds() {
        let name = format!("big.bin.{ext}");
        ok(dir, &["create", &name, "src/big.bin", "-l", "best"]);
        let listing = ok(dir, &["list", &name]);
        assert!(listing.trim_end().ends_with("big.bin"), "{listing}");
        let size = if ext == "bz2" { 0 } else { big.len() };
        assert!(
            listing.trim_start().starts_with(&size.to_string()),
            "{listing}"
        );
        assert!(ok(dir, &["test", &name]).contains("1 entries verified"));
        let out = format!("out-{ext}");
        ok(dir, &["extract", &name, "-o", &out]);
        assert_eq!(fs::read(dir.join(&out).join("big.bin")).unwrap(), big);
        ok(
            dir,
            &["extract", &name, "-o", &out, "--on-conflict", "rename"],
        );
        assert_eq!(fs::read(dir.join(&out).join("big (1).bin")).unwrap(), big);

        let two = format!("two.{ext}");
        let error = fail(dir, &["create", &two, "src/big.bin", "src/empty"]);
        assert!(
            error.contains("exactly one file") && error.contains(tars[0]),
            "{error}"
        );
        let folder = format!("folder.{ext}");
        let error = fail(dir, &["create", &folder, "src"]);
        assert!(error.contains("not a regular file"), "{error}");
        assert!(!dir.join(&two).exists() && !dir.join(&folder).exists());
    }
}

#[test]
fn passwords_and_other_codecs_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for (ext, tars, codec, accepted) in kinds() {
        for name in [format!("a.{ext}"), format!("a.{}", tars[0])] {
            let input = if name.ends_with(&format!("a.{ext}")) {
                "src/empty"
            } else {
                "src"
            };
            let error = fail(dir, &["create", &name, input, "-p", "secret"]);
            assert!(error.contains("encryption"), "{error}");
            let error = fail(dir, &["create", &name, input, "-c", "deflate"]);
            assert!(error.contains(codec), "{error}");
            let error = fail(dir, &["create", &name, input, "-c", "lzma2"]);
            assert!(error.contains(codec), "{error}");
            ok(dir, &["create", &name, input, "-c", accepted]);
        }
    }
}

#[test]
fn the_contents_decide_the_codec_and_whether_it_is_a_tar() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    for (ext, tars, _, _) in kinds() {
        let real = format!("real.{}", tars[0]);
        ok(dir, &["create", &real, "src"]);
        for name in [format!("misnamed-{ext}.{ext}"), format!("no-suffix-{ext}")] {
            fs::copy(dir.join(&real), dir.join(&name)).unwrap();
            assert!(ok(dir, &["list", &name]).contains("src/big.bin"), "{name}");
        }
        let elsewhere = format!("really-{ext}.tar.xz");
        fs::copy(dir.join(&real), dir.join(&elsewhere)).unwrap();
        assert!(ok(dir, &["test", &elsewhere]).contains("3 entries verified"));
        let plain = format!("plain.{ext}");
        ok(dir, &["create", &plain, "src/sub/\u{00f1}.txt"]);
        let misnamed = format!("plain.{}", tars[0]);
        fs::copy(dir.join(&plain), dir.join(&misnamed)).unwrap();
        let listing = ok(dir, &["list", &misnamed]);
        assert!(listing.trim_end().ends_with("plain.tar"), "{listing}");
    }
}

#[test]
fn damaged_streams_fail_and_publish_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let big = corpus(dir);
    for (ext, tars, _, _) in kinds() {
        let tar = format!("a.{}", tars[0]);
        let single = format!("big.{ext}");
        ok(dir, &["create", &tar, "src/big.bin"]);
        ok(dir, &["create", &single, "src/big.bin"]);
        for name in [tar, single] {
            let good = fs::read(dir.join(&name)).unwrap();
            let mut flipped = good.clone();
            let middle = good.len() / 2;
            flipped[middle] ^= 0x40;
            let truncated = good[..good.len() - 16].to_vec();
            let mut trailing = good.clone();
            trailing.extend_from_slice(b"junk");
            for (case, bytes) in [("flip", flipped), ("cut", truncated), ("tail", trailing)] {
                let bad = format!("bad-{case}-{name}");
                fs::write(dir.join(&bad), bytes).unwrap();
                fail(dir, &["test", &bad]);
                let out = format!("out-{case}-{name}");
                fail(dir, &["extract", &bad, "-o", &out]);
                let published = files_under(&dir.join(&out));
                let streamed_tar = name.starts_with("a.");
                if streamed_tar && case != "cut" {
                    // A TAR is extracted as it streams, and the stream's own
                    // checksum and anything after it are only reached once
                    // each file has its full length. The run still fails, as
                    // it does for .tar.gz.
                    assert!(published.len() <= 1, "{bad}");
                    if case == "tail" && published.len() == 1 {
                        assert_eq!(fs::read(&published[0]).unwrap(), big, "{bad}");
                    }
                } else {
                    assert_eq!(published, Vec::<PathBuf>::new(), "{bad}");
                }
            }
        }
        let junk = format!("junk.{ext}");
        fs::write(dir.join(&junk), b"not compressed at all").unwrap();
        fail(dir, &["list", &junk]);
        fail(dir, &["extract", &junk, "-o", "junk"]);
    }
}

#[cfg(not(feature = "codecs-native"))]
#[test]
fn zstd_is_refused_when_it_was_left_out() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    corpus(dir);
    let error = fail(dir, &["create", "a.tar.zst", "src"]);
    assert!(error.contains("Zstandard"), "{error}");
    assert!(!dir.join("a.tar.zst").exists());
}
