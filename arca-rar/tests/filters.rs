#![cfg(feature = "rar")]

mod corpus;

use arca_core::Level;
use arca_rar::{create_rar, CreateOptions, RarArchive, Source};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MIB: usize = 1024 * 1024;

fn wav(data: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x02\0");
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&[4, 0, 16, 0]);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn table(size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size + 16);
    let mut address = 0x0001_4000_1000u64;
    let mut index = 0u64;
    while out.len() < size {
        address += 24 + (index * 7919) % 40;
        out.extend_from_slice(&address.to_le_bytes());
        out.extend_from_slice(&(index / 3).to_le_bytes());
        index += 1;
    }
    out.truncate(size);
    out
}

/// Members the filters are for, and two they must leave alone.
fn members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("tone.wav", wav(&corpus::pcm(500, 3 * MIB / 2))),
        ("tone.pcm", corpus::pcm(501, MIB)),
        ("image.bmp", corpus::bitmap(502, 3 * MIB / 2)),
        ("prog.exe", corpus::x86(503, 5 * MIB / 2)),
        ("table.bin", table(MIB)),
        ("book.txt", corpus::prose(504, MIB / 2)),
        ("packed.bin", corpus::random(505, MIB / 4)),
    ]
}

struct Input {
    dir: tempfile::TempDir,
    sources: Vec<Source>,
    expected: Vec<(String, Vec<u8>)>,
}

fn input(members: Vec<(&'static str, Vec<u8>)>) -> Input {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("in");
    std::fs::create_dir(&root).unwrap();
    let mut sources = Vec::new();
    let mut expected = Vec::new();
    for (name, data) in members {
        let path = root.join(name);
        std::fs::write(&path, &data).unwrap();
        sources.push(Source {
            path,
            name: name.into(),
        });
        expected.push((name.to_string(), data));
    }
    Input {
        dir,
        sources,
        expected,
    }
}

fn create(input: &Input, name: &str, options: &CreateOptions<'_>) -> Vec<PathBuf> {
    let out = input.dir.path().join(name);
    create_rar(&out, &input.sources, options, &|_, _, _| true).unwrap()
}

fn at_level(level: u8) -> CreateOptions<'static> {
    CreateOptions {
        level: Level::new(level),
        ..CreateOptions::default()
    }
}

fn assert_reads_back(archive: &Path, expected: &[(String, Vec<u8>)]) {
    let rar = RarArchive::open(archive, None).unwrap();
    rar.test(None, &|_, _, _| true).unwrap();
    assert_eq!(rar.entries().len(), expected.len());
    for (index, (name, data)) in expected.iter().enumerate() {
        assert_eq!(&rar.entries()[index].name, name);
        assert!(&rar.read_entry(index, None).unwrap() == data, "{name}");
    }
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).unwrap().len()
}

#[test]
fn filtered_members_round_trip_at_every_level() {
    let input = input(members());
    for level in 0..=9 {
        let published = create(&input, &format!("l{level}.rar"), &at_level(level));
        assert_reads_back(&published[0], &input.expected);
    }
}

/// WinRAR filters from `-m3` on and Arca from the level that stands in for
/// it, `-l 5`. Audio packs a third smaller filtered, far more than one level
/// of search is worth, so the step shows in the size.
#[test]
fn filters_start_where_winrar_starts_them() {
    let input = input(vec![("tone.wav", wav(&corpus::pcm(500, MIB)))]);
    let unfiltered = create(&input, "l4.rar", &at_level(4));
    let filtered = create(&input, "l5.rar", &at_level(5));
    assert!(
        size(&filtered[0]) * 10 < size(&unfiltered[0]) * 8,
        "{} against {}",
        size(&filtered[0]),
        size(&unfiltered[0])
    );
}

#[test]
fn filtered_volume_sets_and_solid_archives_round_trip() {
    let input = input(members());
    let published = create(
        &input,
        "set.rar",
        &CreateOptions {
            volume: Some(MIB as u64),
            ..CreateOptions::default()
        },
    );
    assert!(published.len() > 1);
    assert_reads_back(&published[0], &input.expected);
    let solid = create(
        &input,
        "solid.rar",
        &CreateOptions {
            solid: true,
            ..CreateOptions::default()
        },
    );
    assert_reads_back(&solid[0], &input.expected);
}

#[test]
fn truncated_and_corrupted_filtered_archives_never_panic() {
    let input = input(vec![
        ("tone.pcm", corpus::pcm(510, 200 * 1024)),
        ("prog.exe", corpus::x86(511, 200 * 1024)),
    ]);
    let published = create(&input, "small.rar", &CreateOptions::default());
    let bytes = std::fs::read(&published[0]).unwrap();
    let probe = input.dir.path().join("probe.rar");
    let read = |data: &[u8]| {
        std::fs::write(&probe, data).unwrap();
        if let Ok(archive) = RarArchive::open(&probe, None) {
            let _ = archive.test(None, &|_, _, _| true);
            for index in 0..archive.entries().len() {
                let _ = archive.read_entry(index, None);
            }
        }
    };
    for len in (0..bytes.len()).step_by(1999) {
        read(&bytes[..len]);
    }
    for at in (0..bytes.len()).step_by(431) {
        let mut corrupt = bytes.clone();
        corrupt[at] ^= 0x5a;
        read(&corrupt);
    }
}

fn winrar() -> Option<PathBuf> {
    let path = std::env::var_os("ARCA_RAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\WinRAR\Rar.exe"));
    path.is_file().then_some(path)
}

fn rar(program: &Path, dir: &Path, args: &[&str]) {
    let status = Command::new(program)
        .current_dir(dir)
        .args(["-y", "-idq"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "Rar.exe {args:?}");
}

#[test]
#[ignore = "needs WinRAR's console Rar.exe (ARCA_RAR or C:\\Program Files\\WinRAR\\Rar.exe)"]
fn winrar_tests_and_extracts_filtered_archives() {
    let program = winrar().expect("Rar.exe not found");
    let mut members = members();
    members.push(("long.pcm", corpus::pcm(512, 5 * MIB + 3)));
    let input = input(members);
    let mut archives = Vec::new();
    for level in [5, 6, 7, 9] {
        archives.push(create(&input, &format!("l{level}.rar"), &at_level(level)));
    }
    archives.push(create(
        &input,
        "set.rar",
        &CreateOptions {
            volume: Some(MIB as u64),
            ..CreateOptions::default()
        },
    ));
    for (index, published) in archives.iter().enumerate() {
        let first = published[0].to_str().unwrap();
        rar(&program, input.dir.path(), &["t", first]);
        let out = input.dir.path().join(format!("x{index}"));
        std::fs::create_dir(&out).unwrap();
        rar(&program, &out, &["x", first]);
        for (name, data) in &input.expected {
            assert!(
                &std::fs::read(out.join(name)).unwrap() == data,
                "{name} from {first}"
            );
        }
    }
}
