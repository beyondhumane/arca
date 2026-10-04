use arca_core::{Codec, Level};
use arca_zip::ZipWriter;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Output};

fn extract(archive: &Path, dest: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arca"))
        .arg("extract")
        .arg(archive)
        .arg("-o")
        .arg(dest)
        .args(extra)
        .output()
        .unwrap()
}

fn success(result: Output) {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn many_entries_round_trip_with_each_codec_and_password() {
    let dir = tempfile::tempdir().unwrap();
    for password in [None, Some("secret")] {
        let archive = dir.path().join("many.zip");
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        let mut expected = Vec::new();
        for i in 0..150 {
            let name = format!("dir{}/sub/file{i}.txt", i % 5);
            let body = format!("payload {i}\n").repeat(i * 13 + 1).into_bytes();
            let codec = [Codec::Store, Codec::Deflate, Codec::Zstd][i % 3];
            writer
                .add_with_password(&name, &body[..], codec, Level::Normal, None, password)
                .unwrap();
            expected.push((name, body));
        }
        writer
            .add("empty/", &b""[..], Codec::Store, Level::Store, None)
            .unwrap();
        writer.finish().unwrap();
        for threads in ["1", "4"] {
            let output = tempfile::tempdir().unwrap();
            let mut args = vec!["-j", threads];
            if let Some(password) = password {
                args.extend(["-p", password]);
            }
            success(extract(&archive, output.path(), &args));
            for (name, body) in &expected {
                assert_eq!(fs::read(output.path().join(name)).unwrap(), *body);
            }
            assert!(output.path().join("empty").is_dir());
        }
        if password.is_some() {
            for args in [&[][..], &["-p", "wrong"][..]] {
                let output = tempfile::tempdir().unwrap();
                assert!(!extract(&archive, output.path(), args).status.success());
            }
        }
    }
}

#[test]
fn conflicts_preserve_skip_rename_and_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("duplicates.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    writer
        .add(
            "same.txt",
            &b"first"[..],
            Codec::Deflate,
            Level::Normal,
            None,
        )
        .unwrap();
    writer
        .add(
            "same.txt",
            &b"second"[..],
            Codec::Deflate,
            Level::Normal,
            None,
        )
        .unwrap();
    writer.finish().unwrap();
    for policy in ["skip", "rename"] {
        let output = tempfile::tempdir().unwrap();
        fs::write(output.path().join("same.txt"), b"existing").unwrap();
        success(extract(
            &archive,
            output.path(),
            &["--on-conflict", policy, "-j", "4"],
        ));
        assert_eq!(
            fs::read(output.path().join("same.txt")).unwrap(),
            b"existing"
        );
        if policy == "rename" {
            assert_eq!(
                fs::read(output.path().join("same (1).txt")).unwrap(),
                b"first"
            );
            assert_eq!(
                fs::read(output.path().join("same (2).txt")).unwrap(),
                b"second"
            );
        } else {
            assert_eq!(fs::read_dir(output.path()).unwrap().count(), 1);
        }
    }
    // Duplicate overwrite order is not guaranteed by the parallel extractor.
    let single = dir.path().join("single.zip");
    let mut writer = ZipWriter::new(File::create(&single).unwrap());
    writer
        .add(
            "same.txt",
            &b"replaced"[..],
            Codec::Store,
            Level::Store,
            None,
        )
        .unwrap();
    writer.finish().unwrap();
    let output = tempfile::tempdir().unwrap();
    fs::write(
        output.path().join("same.txt"),
        b"existing contents longer than replacement",
    )
    .unwrap();
    success(extract(
        &single,
        output.path(),
        &["--on-conflict", "overwrite", "-j", "4"],
    ));
    assert_eq!(
        fs::read(output.path().join("same.txt")).unwrap(),
        b"replaced"
    );
}

#[test]
fn rejects_unsafe_names_corrupt_data_and_parent_file_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../escape", "C:/absolute", "safe/file"] {
        let archive = dir.path().join("input.zip");
        let mut writer = ZipWriter::new(File::create(&archive).unwrap());
        writer
            .add(name, &b"body"[..], Codec::Store, Level::Store, None)
            .unwrap();
        writer.finish().unwrap();
        let output = tempfile::tempdir().unwrap();
        if name == "safe/file" {
            fs::write(output.path().join("safe"), b"not a directory").unwrap();
        }
        assert!(!extract(&archive, output.path(), &["-j", "4"])
            .status
            .success());
        if name == "safe/file" {
            fs::remove_file(output.path().join("safe")).unwrap();
            let mut bytes = fs::read(&archive).unwrap();
            let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
            bytes[30 + name.len() + extra_len] ^= 1;
            fs::write(&archive, bytes).unwrap();
            assert!(!extract(&archive, output.path(), &["-j", "4"])
                .status
                .success());
        }
    }
}

#[test]
fn mixed_sizes_and_duplicate_names_share_one_reader_per_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("mixed.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    let large: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
    let mut expected = Vec::new();
    for i in 0..40 {
        let name = format!("f{i:03}.bin");
        let body = if i % 7 == 3 {
            large.clone()
        } else {
            format!("small {i}").repeat(i + 1).into_bytes()
        };
        writer
            .add(&name, &body[..], Codec::Deflate, Level::Normal, None)
            .unwrap();
        expected.push((name, body));
    }
    writer
        .add("dup.txt", &b"first"[..], Codec::Store, Level::Store, None)
        .unwrap();
    writer
        .add("dup.txt", &b"second"[..], Codec::Store, Level::Store, None)
        .unwrap();
    writer.finish().unwrap();
    for threads in ["1", "3"] {
        let output = tempfile::tempdir().unwrap();
        success(extract(
            &archive,
            output.path(),
            &["-j", threads, "--on-conflict", "overwrite"],
        ));
        for (name, body) in &expected {
            assert_eq!(fs::read(output.path().join(name)).unwrap(), *body);
        }
        let dup = fs::read(output.path().join("dup.txt")).unwrap();
        assert!(dup == b"first" || dup == b"second");
        let output = tempfile::tempdir().unwrap();
        success(extract(
            &archive,
            output.path(),
            &["-j", threads, "--on-conflict", "rename"],
        ));
        assert_eq!(fs::read(output.path().join("dup.txt")).unwrap(), b"first");
        assert_eq!(
            fs::read(output.path().join("dup (1).txt")).unwrap(),
            b"second"
        );
    }
}

#[test]
fn an_entry_that_understates_its_size_fails_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("liar.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    let body = vec![b'x'; 100_000];
    writer
        .add("liar.txt", &body[..], Codec::Deflate, Level::Normal, None)
        .unwrap();
    writer.finish().unwrap();
    let mut bytes = fs::read(&archive).unwrap();
    let cd = bytes
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    bytes[cd + 24..cd + 28].copy_from_slice(&16u32.to_le_bytes());
    fs::write(&archive, bytes).unwrap();
    let output = tempfile::tempdir().unwrap();
    let result = extract(&archive, output.path(), &["-j", "1"]);
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
}
