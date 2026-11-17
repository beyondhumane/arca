use super::*;
use crate::test_support::Room;
use arca_core::volumes::part_path;
use arca_core::Error;

const GO: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| true;

fn replace(_: &Path) -> Answer {
    Answer::Replace
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2468_ace1u32;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (state >> 24) as u8
        })
        .collect()
}

fn sample(room: &Room) -> (PathBuf, Vec<u8>) {
    let source = room.path("source");
    fs::create_dir_all(source.join("inner")).unwrap();
    fs::write(source.join("a.txt"), b"first contents").unwrap();
    let big = noise(150_000);
    fs::write(source.join("inner/b.bin"), &big).unwrap();
    (source, big)
}

fn parts(out: &Path) -> u32 {
    (1..).take_while(|&i| part_path(out, i).exists()).count() as u32
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn every_family_splits_and_opens_from_any_part() {
    let room = Room::new();
    let (source, big) = sample(&room);
    for format in [Format::Zip, Format::SevenZ, Format::TarGz, Format::TarXz] {
        let out = room.path(&format!("set.{}", format.extension()));
        let (from, to) = compress_split(
            &out,
            Some(32 * 1024),
            std::slice::from_ref(&source),
            format,
            Codec::Deflate,
            Level::FAST,
            GO,
            (None, false),
        )
        .unwrap();
        assert!(from >= big.len() as u64);
        let count = parts(&out);
        assert!(count >= 3, "{format:?}: {count}");
        let total: u64 = (1..=count)
            .map(|i| fs::metadata(part_path(&out, i)).unwrap().len())
            .sum();
        assert_eq!(to, total);
        assert!(!out.exists());
        for opened in [
            part_path(&out, 1),
            part_path(&out, 2),
            part_path(&out, count),
        ] {
            assert_eq!(detect(&opened), Some(format));
            let (entries, _) = list_entries(&opened, None, GO).unwrap();
            let at = entries
                .iter()
                .position(|e| e.name.ends_with("inner/b.bin"))
                .unwrap();
            let mut data = Vec::new();
            read_entry(&opened, at, &mut data, None, GO).unwrap();
            assert_eq!(data, big);
            test_archive(&opened, None, None, GO).unwrap();
        }
        let dest = room.path(&format!("out-{}", format.extension()));
        let wanted = vec![true; list_entries(&out_first(&out), None, GO).unwrap().0.len()];
        extract(&part_path(&out, 2), &dest, &wanted, GO, &replace, None).unwrap();
        assert_eq!(fs::read(dest.join("source/inner/b.bin")).unwrap(), big);
    }
    assert!(names(&room.0).iter().all(|n| !n.ends_with(".arca-new")));
}

fn out_first(out: &Path) -> PathBuf {
    part_path(out, 1)
}

#[test]
fn a_missing_part_is_an_error_and_never_a_panic() {
    let room = Room::new();
    let (source, _) = sample(&room);
    let out = room.path("set.zip");
    compress_split(
        &out,
        Some(20_000),
        &[source],
        Format::Zip,
        Codec::Store,
        Level::STORE,
        GO,
        (None, false),
    )
    .unwrap();
    fs::remove_file(part_path(&out, 3)).unwrap();
    for opened in [part_path(&out, 1), part_path(&out, 4)] {
        let error = list_entries(&opened, None, GO).unwrap_err().to_string();
        assert!(error.contains("set.zip.003"), "{error}");
        assert!(test_archive(&opened, None, None, GO).is_err());
    }
}

#[test]
fn a_stop_leaves_no_part_behind() {
    let room = Room::new();
    let (source, _) = sample(&room);
    let out = room.path("stopped.zip");
    let before = names(&room.0);
    let stop: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|done, _, _| done == 0;
    let result = compress_split(
        &out,
        Some(4096),
        &[source],
        Format::TarXz,
        Codec::Deflate,
        Level::FAST,
        stop,
        (None, false),
    );
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_eq!(names(&room.0), before);
}

#[test]
fn rar_is_written_as_rar_volumes_not_split_parts() {
    let room = Room::new();
    let (source, _) = sample(&room);
    let out = room.path("a.rar");
    let result = compress_split(
        &out,
        Some(64 << 10),
        &[source],
        Format::Rar,
        Codec::Store,
        Level::NORMAL,
        GO,
        (None, false),
    );
    if cfg!(not(feature = "rar")) {
        assert!(matches!(result, Err(Error::Unsupported(_))), "{result:?}");
        return;
    }
    result.unwrap();
    let first = room.path("a.part1.rar");
    assert!(first.is_file() && room.path("a.part2.rar").is_file());
    assert!(!out.exists() && !part_path(&out, 1).exists());
    assert_eq!(created_first(&out, Format::Rar, Some(64 << 10)), first);
}

#[test]
fn the_name_that_counts_for_a_split_set_is_its_first_part() {
    let room = Room::new();
    let out = room.path("a.zip");
    assert_eq!(creation_output(&out, Some(10), &replace).unwrap(), out);
    fs::write(&out, b"single").unwrap();
    let asked = std::cell::RefCell::new(Vec::new());
    let ask = |p: &Path| {
        asked.borrow_mut().push(p.to_path_buf());
        Answer::Rename
    };
    assert_eq!(creation_output(&out, Some(10), &ask).unwrap(), out);
    assert!(asked.borrow().is_empty());
    fs::write(part_path(&out, 1), b"part").unwrap();
    fs::write(part_path(&room.path("a (1).zip"), 1), b"part").unwrap();
    let renamed = creation_output(&out, Some(10), &ask).unwrap();
    assert_eq!(asked.borrow().as_slice(), [part_path(&out, 1)]);
    assert_eq!(renamed, room.path("a (2).zip"));
    assert_eq!(creation_output(&out, Some(10), &replace).unwrap(), out);
    assert_eq!(
        creation_output(&out, Some(10), &|_| Answer::Cancel)
            .unwrap_err()
            .to_string(),
        Error::Cancelled.to_string()
    );
}

#[test]
fn replacing_a_longer_set_leaves_only_the_new_parts() {
    let room = Room::new();
    let (source, _) = sample(&room);
    let out = room.path("set.tar");
    let make = |size| {
        compress_split(
            &out,
            Some(size),
            std::slice::from_ref(&source),
            Format::Tar,
            Codec::Store,
            Level::STORE,
            GO,
            (None, false),
        )
        .unwrap()
    };
    make(10_000);
    let many = parts(&out);
    make(100_000);
    let few = parts(&out);
    assert!(few < many, "{few} {many}");
    assert!(!part_path(&out, few + 1).exists());
    assert!(list_entries(&part_path(&out, 1), None, GO).is_ok());
}

#[test]
fn the_compress_box_turns_its_choice_into_a_volume_size() {
    use crate::model::VolumeChoice;
    let room = Room::new();
    let file = room.path("notes.txt");
    fs::write(&file, b"notes").unwrap();
    let mut app = crate::AppController::new(crate::settings::Settings::default());
    app.state.format = Format::Zip;
    app.prepare_compress(vec![file]);
    let volume_of = |app: &crate::AppController| match app.compression_job() {
        Some(Job::Compress { volume, .. }) => Some(volume),
        _ => None,
    };
    assert_eq!(volume_of(&app), Some(None));
    app.state.volume = VolumeChoice::Fat32;
    assert_eq!(volume_of(&app), Some(Some(4092 << 20)));
    assert!(VolumeChoice::Fat32.bytes().unwrap() < 4 << 30);
    app.state.volume = VolumeChoice::Custom;
    for (typed, bytes) in [
        ("250", 250 << 20),
        (" 2g ", 2 << 30),
        ("1000", 1000 << 20),
        ("64k", 64 << 10),
    ] {
        app.state.volume_text = typed.into();
        assert_eq!(volume_of(&app), Some(Some(bytes)), "{typed}");
    }
    for typed in ["", "abc", "0", "1.5g"] {
        app.state.volume_text = typed.into();
        assert!(app.volume_size().is_err(), "{typed}");
        assert_eq!(volume_of(&app), None, "{typed}");
    }
    app.state.format = Format::Rar;
    app.state.volume_text = "64k".into();
    assert_eq!(app.volume_size(), Ok(Some(64 << 10)));
    app.state.volume_text = "63k".into();
    assert!(app.volume_size().is_err());
    app.state.volume = VolumeChoice::Mb100;
    assert_eq!(app.volume_size(), Ok(Some(100 << 20)));
    for choice in VolumeChoice::ALL {
        for lang in [
            crate::i18n::Lang::En,
            crate::i18n::Lang::Es,
            crate::i18n::Lang::Uk,
        ] {
            assert!(!choice.label(crate::i18n::strings(lang)).is_empty());
        }
    }
}

#[test]
fn a_split_archive_open_in_the_window_cannot_be_changed() {
    let room = Room::new();
    let (source, _) = sample(&room);
    let out = room.path("set.zip");
    compress_split(
        &out,
        Some(50_000),
        &[source],
        Format::Zip,
        Codec::Store,
        Level::STORE,
        GO,
        (None, false),
    )
    .unwrap();
    let mut app = crate::AppController::new(crate::settings::Settings::default());
    app.state.archive = Some(part_path(&out, 1));
    app.state.format = Format::Zip;
    assert!(!app.writable());
    assert!(!app.can_change());
    let before = fs::read(part_path(&out, 1)).unwrap();
    app.run_job(Job::Delete {
        archive: part_path(&out, 1),
        names: vec!["source/a.txt".into()],
        password: None,
    });
    assert!(app.state.error);
    assert_eq!(app.state.notice, app.s().split_read_only);
    assert_eq!(fs::read(part_path(&out, 1)).unwrap(), before);
}

#[test]
fn saving_a_copy_of_a_split_archive_joins_its_parts() {
    let room = Room::new();
    let (source, big) = sample(&room);
    let out = room.path("set.zip");
    compress_split(
        &out,
        Some(40_000),
        &[source],
        Format::Zip,
        Codec::Store,
        Level::STORE,
        GO,
        (None, false),
    )
    .unwrap();
    let copy = room.path("joined.zip");
    run_job_blocking(
        Job::CopyTo {
            archive: part_path(&out, 2),
            dest: copy.clone(),
        },
        crate::i18n::strings(crate::i18n::Lang::En),
        GO,
        &replace,
    )
    .unwrap();
    let total: u64 = (1..=parts(&out))
        .map(|i| fs::metadata(part_path(&out, i)).unwrap().len())
        .sum();
    assert_eq!(fs::metadata(&copy).unwrap().len(), total);
    let (entries, _) = list_entries(&copy, None, GO).unwrap();
    let at = entries
        .iter()
        .position(|e| e.name.ends_with("inner/b.bin"))
        .unwrap();
    let mut data = Vec::new();
    read_entry(&copy, at, &mut data, None, GO).unwrap();
    assert_eq!(data, big);
}
