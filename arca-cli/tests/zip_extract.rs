use arca_core::{Codec, Level};
use arca_zip::{aes, ZipWriter};
use std::fs::{self, File};
use std::io::Write;
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
                .add_with_password(&name, &body[..], codec, Level::NORMAL, None, password)
                .unwrap();
            expected.push((name, body));
        }
        writer
            .add("empty/", &b""[..], Codec::Store, Level::STORE, None)
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
            Level::NORMAL,
            None,
        )
        .unwrap();
    writer
        .add(
            "same.txt",
            &b"second"[..],
            Codec::Deflate,
            Level::NORMAL,
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
            Level::STORE,
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
            .add(name, &b"body"[..], Codec::Store, Level::STORE, None)
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
            .add(&name, &body[..], Codec::Deflate, Level::NORMAL, None)
            .unwrap();
        expected.push((name, body));
    }
    writer
        .add("dup.txt", &b"first"[..], Codec::Store, Level::STORE, None)
        .unwrap();
    writer
        .add("dup.txt", &b"second"[..], Codec::Store, Level::STORE, None)
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
        assert_eq!(dup, b"second");
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
        .add("liar.txt", &body[..], Codec::Deflate, Level::NORMAL, None)
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

#[test]
fn a_failed_overwrite_keeps_the_files_it_would_have_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("locked.zip");
    let mut writer = ZipWriter::new(File::create(&archive).unwrap());
    let big = vec![b'x'; 2 << 20];
    for (name, body) in [("small.txt", &b"tiny"[..]), ("big.bin", &big[..])] {
        writer
            .add_with_password(name, body, Codec::Deflate, Level::FAST, None, Some("right"))
            .unwrap();
    }
    writer.finish().unwrap();
    let output = tempfile::tempdir().unwrap();
    for name in ["small.txt", "big.bin"] {
        fs::write(output.path().join(name), b"mine").unwrap();
    }
    let result = extract(
        &archive,
        output.path(),
        &["--on-conflict", "overwrite", "-p", "wrong"],
    );
    assert!(!result.status.success());
    for name in ["small.txt", "big.bin"] {
        assert_eq!(
            fs::read(output.path().join(name)).unwrap(),
            b"mine",
            "{name}"
        );
    }
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 2);
}

fn aes_entry_without_local_field(
    strength: u8,
    name: &[u8],
    plain: &[u8],
    password: &str,
) -> Vec<u8> {
    let (salt_len, _) = aes::sizes(strength).unwrap();
    let salt: Vec<u8> = (0..salt_len as u8).map(|i| i ^ 0xA5).collect();
    let keys = aes::derive_with(password, &salt, strength).unwrap();
    let mut data = salt;
    data.extend_from_slice(&aes::verifier_of(&keys));
    let mut writer = aes::AesWriter::new(Vec::new(), &keys).unwrap();
    writer.write_all(plain).unwrap();
    let (cipher, auth, _) = writer.finish();
    data.extend_from_slice(&cipher);
    data.extend_from_slice(&auth);

    let mut extra = Vec::new();
    extra.extend_from_slice(&aes::EXTRA_AE.to_le_bytes());
    extra.extend_from_slice(&7u16.to_le_bytes());
    extra.extend_from_slice(&aes::AE2.to_le_bytes());
    extra.extend_from_slice(&aes::VENDOR_AE.to_le_bytes());
    extra.push(strength);
    extra.extend_from_slice(&0u16.to_le_bytes());

    let fixed = |out: &mut Vec<u8>, extra_len: u16| {
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&aes::METHOD_AE.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x5021u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&extra_len.to_le_bytes());
    };
    let mut out = Vec::new();
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&51u16.to_le_bytes());
    fixed(&mut out, 0);
    out.extend_from_slice(name);
    out.extend_from_slice(&data);

    let cd_at = out.len();
    out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    out.extend_from_slice(&51u16.to_le_bytes());
    out.extend_from_slice(&51u16.to_le_bytes());
    fixed(&mut out, extra.len() as u16);
    out.extend_from_slice(&[0u8; 14]);
    out.extend_from_slice(name);
    out.extend_from_slice(&extra);
    let cd_size = out.len() - cd_at;

    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(cd_size as u32).to_le_bytes());
    out.extend_from_slice(&(cd_at as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn an_aes_128_or_192_entry_without_its_local_field_extracts() {
    let dir = tempfile::tempdir().unwrap();
    let plain = b"una clave corta sigue siendo una clave\n".repeat(20);
    for strength in [aes::STRENGTH_128, aes::STRENGTH_192] {
        let archive = dir.path().join(format!("aes{strength}.zip"));
        fs::write(
            &archive,
            aes_entry_without_local_field(strength, b"corta.txt", &plain, "clave"),
        )
        .unwrap();
        let output = tempfile::tempdir().unwrap();
        success(extract(&archive, output.path(), &["-p", "clave"]));
        assert_eq!(fs::read(output.path().join("corta.txt")).unwrap(), plain);

        let output = tempfile::tempdir().unwrap();
        let result = extract(&archive, output.path(), &["-p", "otra"]);
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
    }
}
