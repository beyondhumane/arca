#![cfg(feature = "rar")]

mod corpus;

use arca_core::Level;
use arca_rar::{create_rar, CreateOptions, RarArchive, Source};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const LEVELS: [u8; 5] = [1, 3, 6, 7, 9];

struct Written {
    _dir: tempfile::TempDir,
    root: PathBuf,
    sources: Vec<Source>,
    expected: Vec<(String, Vec<u8>)>,
}

fn write_kind(files: Vec<corpus::File>) -> Written {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("in");
    std::fs::create_dir(&root).unwrap();
    let mut sources = Vec::new();
    let mut expected = Vec::new();
    for file in files {
        let name = file.path.replace('/', "_");
        let path = root.join(&name);
        std::fs::write(&path, &file.data).unwrap();
        sources.push(Source {
            path,
            name: name.clone(),
        });
        expected.push((name, file.data));
    }
    Written {
        _dir: dir,
        root,
        sources,
        expected,
    }
}

fn create(written: &Written, level: u8, label: &str) -> PathBuf {
    let archive = written
        .root
        .parent()
        .unwrap()
        .join(format!("{label}-{level}.rar"));
    let options = CreateOptions {
        level: Level::new(level),
    };
    create_rar(&archive, &written.sources, &options, &|_, _, _| true).unwrap();
    archive
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

fn tester(program: &str) -> Option<String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plain.rar");
    Command::new(program)
        .args(["t", "-idq"])
        .arg(&fixture)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .map(|_| program.to_string())
}

fn assert_tool_accepts(tool: &str, archive: &Path) {
    let status = Command::new(tool)
        .args(["t", "-idq"])
        .arg(archive)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "{tool} t {}", archive.display());
}

fn every_kind_at_every_level(tool: Option<&str>) {
    for kind in corpus::KINDS {
        let written = write_kind(corpus::kind(kind, 0.02));
        for level in LEVELS {
            let archive = create(&written, level, kind);
            assert_reads_back(&archive, &written.expected);
            if let Some(tool) = tool {
                assert_tool_accepts(tool, &archive);
            }
        }
    }
}

fn a_member_larger_than_its_dictionary(tool: Option<&str>) {
    let written = write_kind(vec![corpus::File {
        path: "server.log".into(),
        data: corpus::log(201, 5 * 1024 * 1024 + 12_345),
    }]);
    for level in [1, 3, 6] {
        let archive = create(&written, level, "long");
        assert_reads_back(&archive, &written.expected);
        if let Some(tool) = tool {
            assert_tool_accepts(tool, &archive);
        }
    }
}

#[test]
fn every_corpus_kind_roundtrips_at_every_level() {
    let unrar = tester("unrar");
    every_kind_at_every_level(unrar.as_deref());
}

#[test]
fn a_member_cut_into_runs_with_history_roundtrips() {
    let unrar = tester("unrar");
    a_member_larger_than_its_dictionary(unrar.as_deref());
}

/// WinRAR's own console tester, which CI does not have. Run with
/// `ARCA_RAR_EXE=<path to Rar.exe> cargo test -p arca-rar --features rar
/// --test corpus_roundtrip -- --ignored`; without the variable the default
/// Windows install path is tried.
#[test]
#[ignore]
fn rar_exe_accepts_every_corpus_kind_at_every_level() {
    let program = std::env::var("ARCA_RAR_EXE")
        .unwrap_or_else(|_| r"C:\Program Files\WinRAR\Rar.exe".to_string());
    let tool = tester(&program).expect("Rar.exe not found; set ARCA_RAR_EXE");
    every_kind_at_every_level(Some(&tool));
    a_member_larger_than_its_dictionary(Some(&tool));
}

#[test]
fn the_corpus_is_the_same_every_time() {
    for kind in corpus::KINDS {
        let first = corpus::kind(kind, 0.01);
        let second = corpus::kind(kind, 0.01);
        assert_eq!(first.len(), second.len(), "{kind}");
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.path, b.path);
            assert!(a.data == b.data, "{kind}: {}", a.path);
        }
    }
}
