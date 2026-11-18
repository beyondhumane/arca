use super::*;
use crate::test_support::Room;
use arca_core::Error;
use std::collections::HashSet;

const GO: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| true;

fn replace(_: &Path) -> Answer {
    Answer::Replace
}

fn sample(room: &Room) -> PathBuf {
    let source = room.path("source");
    fs::create_dir_all(source.join("inner")).unwrap();
    fs::write(source.join("a.txt"), b"first contents").unwrap();
    fs::write(source.join("inner/b.bin"), vec![7u8; 300_000]).unwrap();
    source
}

fn make(out: &Path, inputs: &[PathBuf], format: Format) -> arca_core::Result<(u64, u64)> {
    compress(
        out,
        inputs,
        format,
        Codec::Deflate,
        Level::FAST,
        GO,
        (None, false),
    )
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".arca-new"))
        .collect()
}

fn singles() -> Vec<Format> {
    let mut formats = vec![Format::Gz, Format::Bz2];
    if cfg!(feature = "codecs-native") {
        formats.push(Format::Zst);
    }
    formats
}

#[test]
fn names_aliases_and_contents_are_detected() {
    let room = Room::new();
    assert_eq!(detect(Path::new("A/B.TBZ2")), Some(Format::TarBz2));
    assert_eq!(detect(Path::new("A/B.tbz")), Some(Format::TarBz2));
    assert_eq!(detect(Path::new("A/B.Tar.Zst")), Some(Format::TarZst));
    assert_eq!(detect(Path::new("A/B.TZST")), Some(Format::TarZst));
    assert_eq!(detect(Path::new("A/notes.TXT.BZ2")), Some(Format::Bz2));
    assert_eq!(detect(Path::new("A/notes.txt.zst")), Some(Format::Zst));
    assert_eq!(detect(Path::new("A/access.LOG.GZ")), Some(Format::Gz));
    assert_eq!(detect(Path::new("A/B.TGZ")), Some(Format::TarGz));
    let source = sample(&room);
    for single in singles() {
        let tar = single.stream_tar().unwrap();
        let archive = room.path(&format!("bundle.{}", tar.extension()));
        make(&archive, std::slice::from_ref(&source), tar).unwrap();
        let renamed = room.path(&format!("bundle.{}", single.extension()));
        fs::copy(&archive, &renamed).unwrap();
        assert_eq!(detect(&renamed), Some(tar));
        let plain = room.path(&format!("one.txt.{}", single.extension()));
        make(&plain, &[room.path("source/a.txt")], single).unwrap();
        let misnamed = room.path(&format!("one.{}", tar.extension()));
        fs::copy(&plain, &misnamed).unwrap();
        let by_name = if single == Format::Gz { tar } else { single };
        assert_eq!(detect(&misnamed), Some(by_name));
        assert_eq!(archive_stem(&archive), "bundle");
    }
}

#[test]
fn compressed_tars_list_preview_test_and_extract() {
    let room = Room::new();
    let source = sample(&room);
    for name in ["bundle.tbz2", "bundle.tzst"] {
        let archive = room.path(name);
        let format = detect(&archive).unwrap();
        if format == Format::TarZst && !cfg!(feature = "codecs-native") {
            continue;
        }
        make(&archive, std::slice::from_ref(&source), format).unwrap();
        assert!(leftovers(&room.0).is_empty());
        let (entries, _) = list_entries(&archive, None, GO).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["source/a.txt", "source/inner/b.bin"]);
        let mut preview = Vec::new();
        read_entry(&archive, 0, &mut preview, None, GO).unwrap();
        assert_eq!(preview, b"first contents");
        assert_eq!(test_archive(&archive, None, None, GO).unwrap(), (2, vec![]));
        let only: HashSet<String> = ["source/inner/b.bin".to_string()].into();
        assert_eq!(test_archive(&archive, Some(&only), None, GO).unwrap().0, 1);
        let dest = room.path(&format!("out-{name}"));
        let written = extract(&archive, &dest, &[], GO, &replace, None).unwrap();
        assert_eq!(written, 300_000 + 14);
        let picked = room.path(&format!("picked-{name}"));
        extract(&archive, &picked, &[false, true], GO, &replace, None).unwrap();
        assert!(!picked.join("source/a.txt").exists());
        assert!(picked.join("source/inner/b.bin").exists());
        let opened = extract_one(&archive, &entries[1], None, GO).unwrap();
        assert_eq!(fs::read(&opened).unwrap(), vec![7u8; 300_000]);
        let _ = fs::remove_dir_all(opened.parent().unwrap().parent().unwrap());
    }
}

#[test]
fn a_standalone_stream_is_one_entry_named_after_the_archive() {
    let room = Room::new();
    sample(&room);
    let input = room.path("source/inner/b.bin");
    for single in singles() {
        let name = format!("b.bin.{}", single.extension());
        assert_eq!(
            quick_output(std::slice::from_ref(&input), single)
                .file_name()
                .unwrap(),
            name.as_str()
        );
        let archive = room.path(&name);
        let (from, _) = make(&archive, std::slice::from_ref(&input), single).unwrap();
        assert_eq!(from, 300_000);
        let (entries, _) = list_entries(&archive, None, GO).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b.bin");
        let known = if matches!(single, Format::Bz2 | Format::Gz) {
            0
        } else {
            300_000
        };
        assert_eq!(entries[0].size, known);
        let mut preview = Vec::new();
        read_entry(&archive, 0, &mut preview, None, GO).unwrap();
        assert_eq!(preview.len(), 300_000);
        assert!(read_entry(&archive, 1, &mut preview, None, GO).is_err());
        assert_eq!(test_archive(&archive, None, None, GO).unwrap(), (1, vec![]));
        let dest = room.path(&format!("out-{name}"));
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("b.bin"), b"older").unwrap();
        let rename = |_: &Path| Answer::Rename;
        assert_eq!(
            extract(&archive, &dest, &[], GO, &rename, None).unwrap(),
            300_000
        );
        assert_eq!(fs::read(dest.join("b.bin")).unwrap(), b"older");
        assert_eq!(fs::read(dest.join("b (1).bin")).unwrap().len(), 300_000);
        let opened = extract_one(&archive, &entries[0], None, GO).unwrap();
        assert_eq!(fs::read(&opened).unwrap().len(), 300_000);
        let _ = fs::remove_dir_all(opened.parent().unwrap());
    }
}

#[test]
fn a_standalone_stream_refuses_folders_and_several_files() {
    let room = Room::new();
    let source = sample(&room);
    for single in singles() {
        let out = room.path(&format!("many.{}", single.extension()));
        let tar = format!(".{}", single.stream_tar().unwrap().extension());
        for inputs in [
            vec![source.clone()],
            vec![source.join("a.txt"), source.join("inner/b.bin")],
        ] {
            assert!(matches!(
                make(&out, &inputs, single),
                Err(Error::Unsupported(m)) if m.contains(&tar)
            ));
            assert!(!out.exists());
        }
    }
    assert!(leftovers(&room.0).is_empty());
}

#[test]
fn damaged_streams_report_errors_and_publish_nothing() {
    let room = Room::new();
    let source = sample(&room);
    for single in singles() {
        let plain = room.path(&format!("b.bin.{}", single.extension()));
        make(&plain, &[source.join("inner/b.bin")], single).unwrap();
        let mut bytes = fs::read(&plain).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x55;
        fs::write(&plain, &bytes).unwrap();
        let (good, bad) = test_archive(&plain, None, None, GO).unwrap();
        assert_eq!((good, bad.len()), (0, 1), "{single:?}");
        let dest = room.path(&format!("out-{}", single.extension()));
        assert!(extract(&plain, &dest, &[], GO, &replace, None).is_err());
        assert_eq!(fs::read_dir(&dest).map_or(0, |d| d.count()), 0);
        let mut preview = b"unchanged".to_vec();
        assert!(read_entry(&plain, 0, &mut preview, None, GO).is_err());

        let tar = single.stream_tar().unwrap();
        let archive = room.path(&format!("bundle.{}", tar.extension()));
        make(&archive, std::slice::from_ref(&source), tar).unwrap();
        let full = fs::read(&archive).unwrap();
        let cut = room.path(&format!("cut.{}", tar.extension()));
        for keep in [full.len() - 1, full.len() / 2, 40] {
            fs::write(&cut, &full[..keep]).unwrap();
            let dest = room.path(&format!("cut-{keep}-{}", single.extension()));
            assert!(extract(&cut, &dest, &[], GO, &replace, None).is_err());
            let b = dest.join("source/inner/b.bin");
            assert!(!b.exists() || fs::read(&b).unwrap() == vec![7u8; 300_000]);
            assert!(test_archive(&cut, None, None, GO).map_or(true, |(_, bad)| !bad.is_empty()));
        }
    }
}

#[test]
fn a_stop_during_creation_or_extraction_publishes_nothing() {
    let room = Room::new();
    let source = sample(&room);
    let stop: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| false;
    let small = source.join("a.txt");
    for single in singles() {
        let out = room.path(&format!("a.txt.{}", single.extension()));
        for format in [single, single.stream_tar().unwrap()] {
            let result = compress(
                &out,
                std::slice::from_ref(&small),
                format,
                Codec::Deflate,
                Level::FAST,
                stop,
                (None, false),
            );
            assert!(matches!(result, Err(Error::Cancelled)), "{format:?}");
            assert!(!out.exists());
        }
        make(&out, std::slice::from_ref(&small), single).unwrap();
        let dest = room.path(&format!("out-{}", single.extension()));
        assert!(matches!(
            extract(&out, &dest, &[], stop, &replace, None),
            Err(Error::Cancelled)
        ));
        assert!(!dest.join("a.txt").exists());
        assert!(matches!(
            test_archive(&out, None, None, stop),
            Err(Error::Cancelled)
        ));
    }
    assert!(leftovers(&room.0).is_empty());
}
