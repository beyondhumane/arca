use arca_7z::{
    create_7z, rewrite_7z, Addition, Change, Compression, CreateOptions, Recoded, SevenZArchive,
    Source,
};
use arca_core::{Entry, Level};
use sevenz_rust2::{Archive, EncoderMethod as M, Password};
use std::{fs, io::Cursor, path::Path};

fn text(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut n = 0u64;
    while out.len() < len {
        n = n.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        out.extend_from_slice(format!("entry {} of the list, {}\n", n % 613, n >> 54).as_bytes());
    }
    out.truncate(len);
    out
}

/// An x86-64 program header in front of branchy code.
fn program() -> Vec<u8> {
    let mut out = vec![0u8; 4096];
    out[..2].copy_from_slice(b"MZ");
    out[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    out[0x80..0x84].copy_from_slice(b"PE\0\0");
    out[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    out[0x98..0x9A].copy_from_slice(&0x20Bu16.to_le_bytes());
    let mut target = 0x1000u32;
    while out.len() < 30_000 {
        out.push(0xE8);
        out.extend_from_slice(&target.wrapping_sub(out.len() as u32).to_le_bytes());
        out.extend_from_slice(b"\x48\x89\xc7\x90");
        target = target.wrapping_add(16);
    }
    out
}

/// Every block's coder ids, outermost first and AES left out, with the
/// names of its entries.
fn blocks(path: &Path, password: Option<&str>) -> Vec<(Vec<String>, Vec<Vec<u8>>)> {
    let bytes = fs::read(path).unwrap();
    let password = Password::from(password.unwrap_or(""));
    let archive = Archive::read(&mut Cursor::new(&bytes), &password).unwrap();
    let mut out: Vec<(Vec<String>, Vec<Vec<u8>>)> = archive
        .blocks
        .iter()
        .map(|block| {
            let ids = block
                .ordered_coder_iter()
                .map(|(_, c)| c.encoder_method_id().to_vec())
                .filter(|id| id.as_slice() != M::ID_AES256_SHA256)
                .collect();
            (Vec::new(), ids)
        })
        .collect();
    for (i, file) in archive.files.iter().enumerate() {
        if let Some(block) = archive.stream_map.file_block_index[i] {
            out[block].0.push(file.name.clone());
        }
    }
    for (names, _) in &mut out {
        names.sort();
    }
    out.sort();
    out
}

fn drop_b(e: &Entry) -> Option<String> {
    (e.name != "b.txt").then(|| e.name.clone())
}

#[test]
fn every_block_keeps_its_method_and_filter() {
    let room = tempfile::tempdir().unwrap();
    let write = |name: &str, data: &[u8]| {
        let path = room.path().join(name);
        fs::write(&path, data).unwrap();
        path
    };
    let sources = vec![
        Source {
            path: write("a", &text(80_000)),
            name: "a.txt".into(),
        },
        Source {
            path: write("b", &text(30_000)),
            name: "b.txt".into(),
        },
        Source {
            path: write("c", &program()),
            name: "tool.exe".into(),
        },
    ];
    let added = write("d", &text(20_000));
    for compression in Compression::ALL.into_iter().filter(|c| c.available()) {
        for password in [None, Some("secret")] {
            let input = room.path().join("in.7z");
            create_7z(
                &input,
                &sources,
                &CreateOptions {
                    level: Level::FAST,
                    password,
                    compression,
                    ..Default::default()
                },
                &mut |_| true,
            )
            .unwrap();
            let before = blocks(&input, password);
            let output = room.path().join("out.7z");
            let add = [Addition {
                source: Some(added.clone()),
                name: "new.txt".into(),
            }];
            let change = Change {
                name: &drop_b,
                add: &add,
                level: Level::NORMAL,
            };
            let recoded = rewrite_7z(&input, &output, password, &change, &mut |_| true).unwrap();
            assert!(recoded.is_empty(), "{recoded:?}");
            let after = blocks(&output, password);
            let chain_of = |list: &[(Vec<String>, Vec<Vec<u8>>)], name: &str| {
                list.iter()
                    .find(|(names, _)| names.iter().any(|n| n == name))
                    .map(|(_, ids)| ids.clone())
                    .unwrap()
            };
            for name in ["a.txt", "tool.exe"] {
                assert_eq!(
                    chain_of(&after, name),
                    chain_of(&before, name),
                    "{name} {compression:?}"
                );
            }
            assert_eq!(
                chain_of(&after, "new.txt"),
                chain_of(&before, "a.txt"),
                "addition {compression:?}"
            );
            let mut archive =
                SevenZArchive::open(fs::File::open(&output).unwrap(), password).unwrap();
            archive.test(&mut |_| true).unwrap();
            assert!(archive.entries().iter().all(|e| e.name != "b.txt"));
        }
    }
}

fn with_header(header: &[u8], packed: &[u8]) -> Vec<u8> {
    let mut data = b"7z\xbc\xaf\x27\x1c\x00\x04".to_vec();
    let mut fields = Vec::new();
    fields.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    fields.extend_from_slice(&(header.len() as u64).to_le_bytes());
    fields.extend_from_slice(&crc32fast::hash(header).to_le_bytes());
    data.extend_from_slice(&crc32fast::hash(&fields).to_le_bytes());
    data.extend(fields);
    data.extend_from_slice(packed);
    data.extend_from_slice(header);
    data
}

// 7-Zip may give the ARM64 filter a start offset. Arca's encoder has no
// place for one, so a block that has it comes out without the filter, and
// says so; the data, plain text the filter leaves alone, is unchanged.
#[test]
fn a_filter_the_encoder_cannot_repeat_is_reported() {
    let data = b"plain words that no branch filter would ever touch";
    let size = data.len() as u8;
    let mut header = vec![1, 4, 6, 0, 1, 9, size, 0, 7, 11, 1, 0];
    header.extend([2, 0x21, 0x0A, 4, 0, 0, 0, 0, 0x01, 0x00, 0, 1]);
    header.extend([
        12, size, size, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0, 0, 0, 0, 0,
    ]);
    let room = tempfile::tempdir().unwrap();
    let input = room.path().join("arm64.7z");
    fs::write(&input, with_header(&header, data)).unwrap();
    let output = room.path().join("out.7z");
    let change = Change {
        name: &|e: &Entry| Some(e.name.clone()),
        add: &[],
        level: Level::NORMAL,
    };
    let recoded = rewrite_7z(&input, &output, None, &change, &mut |_| true).unwrap();
    assert_eq!(
        recoded,
        [Recoded {
            entries: vec!["x".into()],
            lost: vec!["BCJ_ARM64".into()],
        }]
    );
    assert_eq!(blocks(&output, None)[0].1, [M::ID_COPY.to_vec()]);
    let mut archive = SevenZArchive::open(fs::File::open(&output).unwrap(), None).unwrap();
    assert_eq!(archive.read_entry(0, 1 << 10, &mut |_| true).unwrap(), data);
}
