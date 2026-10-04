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
        Level::Fast,
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

#[test]
fn xz_names_aliases_and_contents_are_detected() {
    let room = Room::new();
    assert_eq!(detect(Path::new("A/B.TXZ")), Some(Format::TarXz));
    assert_eq!(detect(Path::new("A/B.Tar.Xz")), Some(Format::TarXz));
    assert_eq!(detect(Path::new("A/notes.TXT.XZ")), Some(Format::Xz));
    let source = sample(&room);
    let archive = room.path("bundle.tar.xz");
    make(&archive, &[source], Format::TarXz).unwrap();
    let renamed = room.path("bundle.xz");
    fs::copy(&archive, &renamed).unwrap();
    assert_eq!(detect(&renamed), Some(Format::TarXz));
    let plain = room.path("one.txt.xz");
    make(&plain, &[room.path("source/a.txt")], Format::Xz).unwrap();
    let misnamed = room.path("one.tar.xz");
    fs::copy(&plain, &misnamed).unwrap();
    assert_eq!(detect(&misnamed), Some(Format::Xz));
    assert_eq!(archive_stem(&archive), "bundle");
}

#[test]
fn tar_xz_lists_previews_tests_and_extracts() {
    let room = Room::new();
    let source = sample(&room);
    let archive = room.path("bundle.txz");
    make(&archive, &[source], Format::TarXz).unwrap();
    assert!(leftovers(&room.0).is_empty());
    let entries = list_entries(&archive, None, GO).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["source/a.txt", "source/inner/b.bin"]);
    let mut preview = Vec::new();
    read_entry(&archive, 0, &mut preview, None, GO).unwrap();
    assert_eq!(preview, b"first contents");
    assert_eq!(test_archive(&archive, None, None, GO).unwrap(), (2, vec![]));
    let only: HashSet<String> = ["source/inner/b.bin".to_string()].into();
    assert_eq!(test_archive(&archive, Some(&only), None, GO).unwrap().0, 1);
    let dest = room.path("out");
    let written = extract(&archive, &dest, &[], GO, &replace, None).unwrap();
    assert_eq!(written, 300_000 + 14);
    assert_eq!(
        fs::read(dest.join("source/inner/b.bin")).unwrap(),
        vec![7u8; 300_000]
    );
    let picked = room.path("picked");
    extract(&archive, &picked, &[false, true], GO, &replace, None).unwrap();
    assert!(!picked.join("source/a.txt").exists());
    assert!(picked.join("source/inner/b.bin").exists());
    let opened = extract_one(&archive, &entries[1], None, GO).unwrap();
    assert_eq!(fs::read(&opened).unwrap(), vec![7u8; 300_000]);
    let _ = fs::remove_dir_all(opened.parent().unwrap().parent().unwrap());
}

#[test]
fn standalone_xz_is_one_entry_named_after_the_archive() {
    let room = Room::new();
    sample(&room);
    let input = room.path("source/inner/b.bin");
    assert_eq!(
        quick_output(std::slice::from_ref(&input), Format::Xz)
            .file_name()
            .unwrap(),
        "b.bin.xz"
    );
    let archive = room.path("b.bin.xz");
    let (from, _) = make(&archive, std::slice::from_ref(&input), Format::Xz).unwrap();
    assert_eq!(from, 300_000);
    let entries = list_entries(&archive, None, GO).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "b.bin");
    assert_eq!(entries[0].size, 300_000);
    let mut preview = Vec::new();
    read_entry(&archive, 0, &mut preview, None, GO).unwrap();
    assert_eq!(preview.len(), 300_000);
    assert!(read_entry(&archive, 1, &mut preview, None, GO).is_err());
    assert_eq!(test_archive(&archive, None, None, GO).unwrap(), (1, vec![]));
    let dest = room.path("out");
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

#[test]
fn standalone_xz_refuses_folders_and_several_files() {
    let room = Room::new();
    let source = sample(&room);
    let out = room.path("many.xz");
    for inputs in [
        vec![source.clone()],
        vec![source.join("a.txt"), source.join("inner/b.bin")],
    ] {
        assert!(matches!(
            make(&out, &inputs, Format::Xz),
            Err(Error::Unsupported(m)) if m.contains(".tar.xz")
        ));
        assert!(!out.exists());
    }
    assert!(leftovers(&room.0).is_empty());
}

#[test]
fn stopped_or_failed_creation_leaves_no_archive() {
    let room = Room::new();
    let source = sample(&room);
    let big = room.path("big.bin");
    let data: Vec<u8> = (0..9u32 << 20)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    fs::write(&big, &data).unwrap();
    let stop: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| false;
    for (out, inputs, format) in [
        (room.path("big.bin.xz"), vec![big.clone()], Format::Xz),
        (room.path("all.tar.xz"), vec![source.clone()], Format::TarXz),
    ] {
        let result = compress(
            &out,
            &inputs,
            format,
            Codec::Deflate,
            Level::Store,
            stop,
            (None, false),
        );
        assert!(matches!(result, Err(Error::Cancelled)), "{format:?}");
        assert!(!out.exists());
    }
    let busy = room.path("taken.tar.xz");
    fs::create_dir(&busy).unwrap();
    fs::create_dir(room.path("taken.tar.xz.arca-new")).unwrap();
    assert!(make(&busy, &[source], Format::TarXz).is_err());
    assert!(busy.is_dir());
    fs::remove_dir(room.path("taken.tar.xz.arca-new")).unwrap();
    assert!(leftovers(&room.0).is_empty());
}

#[test]
fn damaged_xz_reports_errors_and_publishes_nothing() {
    let room = Room::new();
    let source = sample(&room);
    let plain = room.path("b.bin.xz");
    make(&plain, &[source.join("inner/b.bin")], Format::Xz).unwrap();
    let mut bytes = fs::read(&plain).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x55;
    fs::write(&plain, &bytes).unwrap();
    let (good, bad) = test_archive(&plain, None, None, GO).unwrap();
    assert_eq!(good, 0);
    assert_eq!(bad.len(), 1);
    let dest = room.path("out");
    assert!(extract(&plain, &dest, &[], GO, &replace, None).is_err());
    assert_eq!(fs::read_dir(&dest).map_or(0, |d| d.count()), 0);
    let mut preview = b"unchanged".to_vec();
    assert!(read_entry(&plain, 0, &mut preview, None, GO).is_err());

    let archive = room.path("bundle.tar.xz");
    make(&archive, &[source], Format::TarXz).unwrap();
    let full = fs::read(&archive).unwrap();
    let cut = room.path("cut.tar.xz");
    for keep in [full.len() - 1, full.len() / 2, 40] {
        fs::write(&cut, &full[..keep]).unwrap();
        let dest = room.path(&format!("cut-{keep}"));
        assert!(extract(&cut, &dest, &[], GO, &replace, None).is_err());
        assert!(!dest.join("source/inner/b.bin").exists());
        assert!(test_archive(&cut, None, None, GO).map_or(true, |(_, bad)| !bad.is_empty()));
    }
}

#[test]
fn a_stop_during_a_small_xz_job_publishes_nothing() {
    let room = Room::new();
    let source = sample(&room);
    let stop: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| false;
    let small = source.join("a.txt");
    let out = room.path("a.txt.xz");
    let result = compress(
        &out,
        std::slice::from_ref(&small),
        Format::Xz,
        Codec::Deflate,
        Level::Fast,
        stop,
        (None, false),
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!out.exists());
    let tarxz = room.path("tiny.tar.xz");
    let result = compress(
        &tarxz,
        std::slice::from_ref(&small),
        Format::TarXz,
        Codec::Deflate,
        Level::Fast,
        &|i, _, _| i == 0,
        (None, false),
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!tarxz.exists());
    make(&out, &[small], Format::Xz).unwrap();
    let dest = room.path("out");
    assert!(matches!(
        extract(&out, &dest, &[], stop, &replace, None),
        Err(Error::Cancelled)
    ));
    assert!(!dest.join("a.txt").exists());
    assert!(matches!(
        test_archive(&out, None, None, stop),
        Err(Error::Cancelled)
    ));
    assert!(leftovers(&room.0).is_empty());
}
