use arca_7z::{create_7z, salvage_7z, CreateOptions, SevenZArchive, Source};
use arca_core::Level;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn made(dir: &Path) -> PathBuf {
    let sources: [(&str, Option<Vec<u8>>); 5] = [
        ("src/", None),
        ("src/a.txt", Some(b"first file ".repeat(3000))),
        ("src/b.txt", Some(b"second file ".repeat(2000))),
        ("src/empty.txt", Some(Vec::new())),
        (
            "tool.exe",
            Some(b"MZ\x90\0 not really a program".repeat(50)),
        ),
    ];
    let mut list = Vec::new();
    for (name, data) in sources {
        let path = dir.join("in").join(name);
        match data {
            Some(data) => {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, data).unwrap();
            }
            None => fs::create_dir_all(&path).unwrap(),
        }
        list.push(Source {
            path,
            name: name.trim_end_matches('/').to_string(),
        });
    }
    let out = dir.join("made.7z");
    create_7z(
        &out,
        &list,
        &CreateOptions {
            level: Level::FAST,
            password: None,
            hide_names: false,
            ..Default::default()
        },
        &mut |_| true,
    )
    .unwrap();
    out
}

fn names(path: &Path) -> Vec<String> {
    let mut a = SevenZArchive::open(fs::File::open(path).unwrap(), None).unwrap();
    a.test(&mut |_| true).unwrap();
    a.entries().iter().map(|e| e.name.clone()).collect()
}

fn block_holding(path: &Path, name: &str) -> std::ops::Range<usize> {
    let archive = sevenz_rust2::Archive::open(path).unwrap();
    let file = archive.files.iter().position(|f| f.name == name).unwrap();
    let block = archive.stream_map.file_block_index[file].unwrap();
    let pack = archive.stream_map.block_first_pack_stream_index()[block];
    let start = 32 + archive.pack_pos() + archive.pack_sizes()[..pack].iter().sum::<u64>();
    let end = start + archive.pack_sizes()[pack];
    start as usize..end as usize
}

#[test]
fn a_damaged_block_costs_only_its_own_entries() {
    let dir = tempfile::tempdir().unwrap();
    let input = made(dir.path());
    let range = block_holding(&input, "src/a.txt");
    let mut bytes = fs::read(&input).unwrap();
    let middle = range.start + range.len() / 2;
    for b in &mut bytes[middle..middle + 8] {
        *b ^= 0x5A;
    }
    fs::write(&input, &bytes).unwrap();
    let output = dir.path().join("saved.7z");
    let report = salvage_7z(&input, &output, None, Level::FAST, &mut |_| true).unwrap();
    assert_eq!(fs::read(&input).unwrap(), bytes);
    let kept = names(&output);
    assert_eq!(kept.len(), report.kept);
    assert!(kept.contains(&"tool.exe".to_string()), "{kept:?}");
    assert!(kept.contains(&"src/empty.txt".to_string()), "{kept:?}");
    assert!(!report.lost.is_empty());
    for (name, _) in &report.lost {
        assert!(!kept.contains(name));
        assert!(name.starts_with("src/"), "{name}");
    }
}

#[test]
fn a_sound_archive_keeps_everything() {
    let dir = tempfile::tempdir().unwrap();
    let input = made(dir.path());
    let output = dir.path().join("saved.7z");
    let report = salvage_7z(&input, &output, None, Level::FAST, &mut |_| true).unwrap();
    assert!(report.lost.is_empty());
    assert_eq!(names(&output), names(&input));
}

#[test]
fn a_damaged_header_its_own_input_and_cut_files_are_refused_without_panic() {
    let dir = tempfile::tempdir().unwrap();
    let input = made(dir.path());
    assert!(salvage_7z(&input, &input, None, Level::FAST, &mut |_| true).is_err());
    let bytes = fs::read(&input).unwrap();
    let output = dir.path().join("saved.7z");
    let mut header = bytes.clone();
    let end = header.len();
    for b in &mut header[end - 20..end - 12] {
        *b ^= 0xFF;
    }
    let bad = dir.path().join("bad.7z");
    fs::write(&bad, &header).unwrap();
    assert!(salvage_7z(&bad, &output, None, Level::FAST, &mut |_| true).is_err());
    assert!(!output.exists());
    for cut in (0..bytes.len()).step_by(61) {
        fs::write(&bad, &bytes[..cut]).unwrap();
        let _ = salvage_7z(&bad, &output, None, Level::FAST, &mut |_| true);
        let _ = fs::remove_file(&output);
    }
    for at in (32..bytes.len()).step_by(41) {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x81;
        fs::write(&bad, &flipped).unwrap();
        if salvage_7z(&bad, &output, None, Level::FAST, &mut |_| true).is_ok() {
            names(&output);
        }
        let _ = fs::remove_file(&output);
    }
    assert!(matches!(
        salvage_7z(&input, &output, None, Level::FAST, &mut |_| false),
        Err(arca_core::Error::Cancelled)
    ));
}

#[test]
fn a_wrong_password_saves_nothing_even_from_the_plain_blocks() {
    use sevenz_rust2::{
        encoder_options::{AesEncoderOptions, Lzma2Options},
        ArchiveEntry, ArchiveWriter, EncoderMethod, Password,
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("partial.7z");
    let mut writer = ArchiveWriter::create(&input).unwrap();
    writer.set_encrypt_header(false);
    writer.set_content_methods(vec![Lzma2Options::from_level(1).into()]);
    let plain = b"plain text".repeat(100);
    writer
        .push_archive_entry(ArchiveEntry::new_file("plain.txt"), Some(&plain[..]))
        .unwrap();
    writer.set_content_methods(vec![
        AesEncoderOptions::new(Password::from("right")).into(),
        Lzma2Options::from_level(1).into(),
    ]);
    let secret = b"secret".repeat(100);
    writer
        .push_archive_entry(ArchiveEntry::new_file("secret.txt"), Some(&secret[..]))
        .unwrap();
    writer.set_content_methods(vec![EncoderMethod::COPY.into()]);
    writer.finish().unwrap();
    let out = dir.path().join("out.7z");
    let error = salvage_7z(&input, &out, Some("wrong"), Level::FAST, &mut |_| true)
        .err()
        .unwrap();
    assert!(arca_7z::password_required(&error), "{error:?}");
    assert!(!out.exists());
    let saved = salvage_7z(&input, &out, Some("right"), Level::FAST, &mut |_| true).unwrap();
    assert_eq!((saved.kept, saved.lost.len()), (2, 0));
}
