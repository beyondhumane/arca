use arca_7z::{create_7z, password_required, CreateOptions, Phase, SevenZArchive, Source};
use arca_core::{Error, Level, Method};
use sevenz_rust2::{
    encoder_options::{AesEncoderOptions, Lzma2Options},
    ArchiveEntry, ArchiveWriter, EncoderConfiguration, Password, SourceReader,
};
use std::{
    fs,
    io::{self, Cursor},
    sync::OnceLock,
};

fn fixture(solid: bool, password: Option<&str>, hidden: bool) -> Vec<u8> {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    let mut methods: Vec<EncoderConfiguration> = vec![];
    if let Some(password) = password {
        methods.push(
            AesEncoderOptions {
                password: Password::from(password),
                iv: [7; 16],
                salt: [11; 16],
                num_cycles_power: 1,
            }
            .into(),
        );
    }
    methods.push(Lzma2Options::from_level(1).into());
    writer.set_content_methods(methods);
    writer.set_encrypt_header(hidden);
    writer
        .push_archive_entry::<io::Empty>(ArchiveEntry::new_directory("dir"), None)
        .unwrap();
    if solid {
        writer
            .push_archive_entries(
                ["dir/first", "dir/second", "dir/third"]
                    .into_iter()
                    .map(ArchiveEntry::new_file)
                    .collect(),
                [
                    b"first contents".as_slice(),
                    b"second contents",
                    b"third contents",
                ]
                .iter()
                .map(|&d| SourceReader::new(d))
                .collect(),
            )
            .unwrap();
    } else {
        writer
            .push_archive_entry(
                ArchiveEntry::new_file("dir/first"),
                Some(b"first contents".as_slice()),
            )
            .unwrap();
        writer
            .push_archive_entry(
                ArchiveEntry::new_file("dir/second"),
                Some(b"second contents".as_slice()),
            )
            .unwrap();
    }
    writer
        .push_archive_entry::<io::Empty>(ArchiveEntry::new_file("empty"), None)
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn encrypted() -> &'static [u8] {
    static FIXTURE: OnceLock<Vec<u8>> = OnceLock::new();
    FIXTURE.get_or_init(|| fixture(true, Some("secret"), false))
}

#[test]
fn plain_and_solid_selection_is_block_sequential() {
    for solid in [false, true] {
        let data = fixture(solid, None, false);
        let mut archive = SevenZArchive::open(Cursor::new(&data), None).unwrap();
        assert_eq!(archive.is_solid(), solid);
        archive.test(&mut |_| true).unwrap();
        let mut found = Vec::new();
        let mut validation = 0;
        archive
            .extract(
                &[2, 1, 2],
                &mut |index, entry, reader| {
                    let mut bytes = Vec::new();
                    reader.read_to_end(&mut bytes)?;
                    found.push((index, entry.name.clone(), bytes));
                    Ok(())
                },
                &mut |p| {
                    if p.phase == Phase::Validate {
                        validation += 1;
                    }
                    true
                },
            )
            .unwrap();
        assert_eq!(validation, 0);
        assert_eq!(found.iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(found[1].2, b"second contents");
        if solid {
            assert!(archive.entries()[1].compressed_size > 0);
            assert_eq!(archive.entries()[2].compressed_size, 0);
            assert_eq!(archive.entries()[3].compressed_size, 0);
        }
        assert!(matches!(
            archive.read_entry(1, 2, &mut |_| true),
            Err(Error::Limit(_))
        ));
        assert_eq!(
            archive.read_entry(1, 1024, &mut |_| true).unwrap(),
            b"first contents"
        );
    }
}

#[test]
fn empty_and_unicode_roundtrip_all_levels_and_password_modes() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("dir")).unwrap();
    fs::write(temp.path().join("data"), b"hello compressed world").unwrap();
    fs::write(temp.path().join("empty"), b"").unwrap();
    let sources = vec![
        Source {
            path: temp.path().join("dir"),
            name: "folder".into(),
        },
        Source {
            path: temp.path().join("data"),
            name: "folder/\u{00f1}-\u{1f680}.txt".into(),
        },
        Source {
            path: temp.path().join("empty"),
            name: "empty".into(),
        },
    ];
    for (level, password, hide_names) in [
        (Level::Store, None, false),
        (Level::Fast, None, false),
        (Level::Normal, Some("p\u{00e4}ss\u{1f511}"), false),
        (Level::Best, Some("p\u{00e4}ss\u{1f511}"), true),
    ] {
        let path = temp.path().join("out.7z");
        create_7z(
            &path,
            &sources,
            &CreateOptions {
                level,
                password,
                hide_names,
            },
            &mut |_| true,
        )
        .unwrap();
        let mut archive = SevenZArchive::open(fs::File::open(path).unwrap(), password).unwrap();
        assert_eq!(archive.has_encrypted_header(), hide_names);
        assert_eq!(archive.has_encrypted(), password.is_some());
        assert_eq!(archive.entries()[1].name, sources[1].name);
        assert!(archive.entries()[0].is_dir);
        assert!(!archive.entries()[2].is_dir);
        assert_eq!(archive.read_entry(2, 0, &mut |_| true).unwrap(), b"");
        assert_eq!(
            archive.read_entry(1, 100, &mut |_| true).unwrap(),
            b"hello compressed world"
        );
        archive.test(&mut |_| true).unwrap();
    }
}

#[test]
fn every_truncation_offset_fails_without_panicking() {
    for data in [
        fixture(false, None, false),
        fixture(true, None, false),
        encrypted().to_vec(),
        fixture(true, Some("secret"), true),
    ] {
        for end in 0..data.len() {
            assert!(
                SevenZArchive::open(Cursor::new(&data[..end]), Some("secret")).is_err(),
                "accepted prefix {end}/{}",
                data.len()
            );
        }
        SevenZArchive::open(Cursor::new(data), Some("secret"))
            .unwrap()
            .test(&mut |_| true)
            .unwrap();
    }
}

#[test]
fn passwords_are_classified_and_wrong_password_touches_no_sink() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing");
    let absent = temp.path().join("absent");
    fs::write(&existing, b"keep me").unwrap();
    for password in [None, Some("incorrect")] {
        let mut archive = SevenZArchive::open(Cursor::new(encrypted()), password).unwrap();
        assert!(archive.has_encrypted());
        let indices = (0..archive.len()).collect::<Vec<_>>();
        let error = archive
            .extract(
                &indices,
                &mut |_, _, _| {
                    fs::create_dir(&absent)?;
                    fs::write(&existing, b"destroyed")?;
                    Ok(())
                },
                &mut |_| true,
            )
            .unwrap_err();
        assert!(password_required(&error), "{error}");
        assert_eq!(fs::read(&existing).unwrap(), b"keep me");
        assert!(!absent.exists());
    }
    let mut archive = SevenZArchive::open(Cursor::new(encrypted()), Some("secret")).unwrap();
    archive.validate_encrypted(&mut |_| true).unwrap();
    assert_eq!(
        archive.read_entry(2, 100, &mut |_| true).unwrap(),
        b"second contents"
    );
    let hidden = fixture(true, Some("secret"), true);
    assert!(matches!(
        SevenZArchive::open(Cursor::new(&hidden), None),
        Err(Error::PasswordRequired)
    ));
    match SevenZArchive::open(Cursor::new(&hidden), Some("incorrect")) {
        Err(error) => assert!(password_required(&error), "{error}"),
        Ok(_) => panic!("incorrect password accepted"),
    }
}

#[test]
fn cancellation_is_distinct_and_creation_preserves_existing_output() {
    let mut archive = SevenZArchive::open(Cursor::new(encrypted()), Some("secret")).unwrap();
    let mut called = false;
    assert!(matches!(
        archive.extract(
            &[0, 1],
            &mut |_, _, _| {
                called = true;
                Ok(())
            },
            &mut |_| false
        ),
        Err(Error::Cancelled)
    ));
    assert!(!called);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("out");
    let source = temp.path().join("source");
    fs::write(&output, b"old").unwrap();
    fs::write(&source, vec![1; 100_000]).unwrap();
    let sources = [Source {
        path: source,
        name: "data".into(),
    }];
    assert!(matches!(
        create_7z(&output, &sources, &CreateOptions::default(), &mut |p| p
            .bytes_done
            < 5000),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read(&output).unwrap(), b"old");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    for password in [None, Some("")] {
        assert!(create_7z(
            &output,
            &sources,
            &CreateOptions {
                password,
                hide_names: true,
                ..Default::default()
            },
            &mut |_| true
        )
        .is_err());
        assert_eq!(fs::read(&output).unwrap(), b"old");
    }
}

#[test]
fn unsafe_names_and_duplicate_normalized_names_are_rejected() {
    for name in [
        "../escape",
        "a/../../escape",
        "C:\\evil",
        "/root",
        "\\\\host\\share",
        "foo:bar",
        "CON.txt",
        "a/..",
        "a/evil.",
        "a/evil ",
        "",
        ".",
    ] {
        let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        writer
            .push_archive_entry::<io::Empty>(ArchiveEntry::new_file(name), None)
            .unwrap();
        let data = writer.finish().unwrap().into_inner();
        assert!(
            SevenZArchive::open(Cursor::new(data), None).is_err(),
            "accepted {name:?}"
        );
    }
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for name in ["a/b", "a\\b"] {
        writer
            .push_archive_entry::<io::Empty>(ArchiveEntry::new_file(name), None)
            .unwrap();
    }
    assert!(SevenZArchive::open(writer.finish().unwrap(), None).is_err());
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

#[test]
fn hostile_header_counts_and_offsets_are_errors() {
    for header in [
        vec![1, 5, 255, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![1, 4, 6, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![1, 4, 7, 11, 255, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![1, 5, 1, 17, 0, 0, 0],
    ] {
        assert!(SevenZArchive::open(Cursor::new(with_header(&header, &[])), None).is_err());
    }
    let mut data = fixture(false, None, false);
    data[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
    let crc = crc32fast::hash(&data[12..32]);
    data[8..12].copy_from_slice(&crc.to_le_bytes());
    assert!(SevenZArchive::open(Cursor::new(data), None).is_err());
}

#[test]
fn malformed_decoder_properties_are_bounded_errors() {
    for properties in [vec![41], vec![40], vec![255], vec![]] {
        // One LZMA2 block and one named streamed file with a valid outer CRC.
        let mut header = vec![
            1,
            4,
            6,
            0,
            1,
            9,
            1,
            0,
            7,
            11,
            1,
            0,
            1,
            0x21,
            0x21,
            properties.len() as u8,
        ];
        header.extend(properties);
        header.extend([12, 1, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0, 0, 0, 0, 0]);
        let data = with_header(&header, &[0]);
        let outcome =
            SevenZArchive::open(Cursor::new(data), None).and_then(|mut a| a.test(&mut |_| true));
        assert!(outcome.is_err());
    }
}

#[test]
fn corruption_and_sink_errors_are_not_success_or_misclassified() {
    let mut data = encrypted().to_vec();
    data[32] ^= 0x80;
    let mut archive = SevenZArchive::open(Cursor::new(data), Some("secret")).unwrap();
    let mut called = false;
    assert!(archive
        .extract(
            &[0],
            &mut |_, _, _| {
                called = true;
                Ok(())
            },
            &mut |_| true
        )
        .is_err());
    assert!(!called);
    let mut archive = SevenZArchive::open(Cursor::new(encrypted()), Some("secret")).unwrap();
    assert!(matches!(
        archive.extract(
            &[1],
            &mut |_, _, _| Err(Error::Io(io::Error::other("sink full"))),
            &mut |_| true
        ),
        Err(Error::Io(_))
    ));
}

#[test]
fn sevenz_methods_have_no_zip_code() {
    for method in [Method::Lzma, Method::Lzma2, Method::Bzip2, Method::Other7z] {
        assert!(method.code().is_err());
    }
    assert_eq!(Method::Store.code().unwrap(), 0);
    assert_eq!(Method::Deflate.code().unwrap(), 8);
}

#[test]
fn empty_directory_between_solid_members_is_not_skipped() {
    let mut header = vec![
        1, 4, 6, 0, 1, 9, 2, 0, 7, 11, 1, 0, 1, 1, 0, 12, 2, 0, 8, 13, 2, 9, 1, 0, 0, 5, 3, 14, 1,
        0x40, 17, 17, 0,
    ];
    for ch in "a\0dir\0b\0".encode_utf16() {
        header.extend(ch.to_le_bytes());
    }
    header.extend([0, 0]);
    let mut archive = SevenZArchive::open(Cursor::new(with_header(&header, b"ab")), None).unwrap();
    let mut found = Vec::new();
    archive
        .extract(
            &[1, 2],
            &mut |i, e, r| {
                let mut bytes = Vec::new();
                r.read_to_end(&mut bytes)?;
                found.push((i, e.is_dir, bytes));
                Ok(())
            },
            &mut |_| true,
        )
        .unwrap();
    assert_eq!(found, vec![(1, true, vec![]), (2, false, b"b".to_vec())]);
}

#[test]
fn empty_archive_and_empty_encrypted_archive() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("out");
    create_7z(&output, &[], &CreateOptions::default(), &mut |_| true).unwrap();
    assert!(SevenZArchive::open(fs::File::open(&output).unwrap(), None)
        .unwrap()
        .is_empty());
    let options = CreateOptions {
        password: Some("secret"),
        hide_names: true,
        ..Default::default()
    };
    create_7z(&output, &[], &options, &mut |_| true).unwrap();
    assert!(matches!(
        SevenZArchive::open(fs::File::open(&output).unwrap(), None),
        Err(Error::PasswordRequired)
    ));
    let mut archive =
        SevenZArchive::open(fs::File::open(&output).unwrap(), Some("secret")).unwrap();
    assert!(archive.is_empty());
    assert!(archive.has_encrypted());
    archive.test(&mut |_| true).unwrap();
    let error = create_7z(
        &output,
        &[],
        &CreateOptions {
            hide_names: false,
            ..options
        },
        &mut |_| true,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Unsupported(_)));
}

#[test]
fn an_unselected_corrupt_encrypted_block_prevents_all_writes() {
    let mut data = fixture(false, Some("secret"), false);
    let raw =
        sevenz_rust2::Archive::read(&mut Cursor::new(&data), &Password::from("secret")).unwrap();
    let second = 32 + raw.pack_pos() as usize + raw.pack_sizes()[0] as usize;
    data[second] ^= 0x80;
    let mut archive = SevenZArchive::open(Cursor::new(data), Some("secret")).unwrap();
    let mut called = false;
    assert!(matches!(
        archive.extract(
            &[0, 1],
            &mut |_, _, _| {
                called = true;
                Ok(())
            },
            &mut |_| true
        ),
        Err(Error::PasswordOrCorrupt)
    ));
    assert!(!called);
}

fn single_block(coder: &[u8], size: u8, packed: &[u8]) -> Vec<u8> {
    let mut header = vec![1, 4, 6, 0, 1, 9, packed.len() as u8, 0, 7, 11, 1, 0];
    header.extend_from_slice(coder);
    header.extend([12, size, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0, 0, 0, 0, 0]);
    with_header(&header, packed)
}

#[test]
fn decoder_truncation_and_dictionary_limits() {
    for length in 0..12 {
        let data = single_block(&[1, 1, 0], 12, &vec![b'a'; length]);
        let mut archive = SevenZArchive::open(Cursor::new(data), None).unwrap();
        assert!(
            archive.test(&mut |_| true).is_err(),
            "copy truncation {length}"
        );
    }
    for coder in [
        vec![1, 0x21, 0x21, 1, 40],
        vec![1, 0x23, 3, 1, 1, 5, 0x5d, 255, 255, 255, 255],
    ] {
        let mut archive =
            SevenZArchive::open(Cursor::new(single_block(&coder, 1, &[0])), None).unwrap();
        assert!(matches!(archive.test(&mut |_| true), Err(Error::Limit(_))));
    }
    // A tiny encoded-header descriptor cannot request a multi-gigabyte decoded header.
    let header = [
        0x17, 6, 0, 1, 9, 1, 0, 7, 11, 1, 0, 1, 1, 0, 12, 255, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0,
    ];
    assert!(SevenZArchive::open(Cursor::new(with_header(&header, &[0])), None).is_err());
}

#[test]
fn crc_metadata_distinguishes_absent_and_present_checksums() {
    let data = single_block(&[1, 1, 0], 1, b"a");
    let mut archive = SevenZArchive::open(Cursor::new(data), None).unwrap();
    assert_eq!(archive.entries()[0].crc32, None);
    archive.test(&mut |_| true).unwrap();
    for solid in [false, true] {
        let archive = SevenZArchive::open(Cursor::new(fixture(solid, None, false)), None).unwrap();
        assert_eq!(archive.entries()[0].crc32, None);
        assert_eq!(
            archive.entries()[1].crc32,
            Some(arca_core::crc32(b"first contents"))
        );
    }
}

#[test]
fn malformed_aes_properties_do_not_run_unbounded_kdf() {
    for properties in [vec![62, 0], vec![255], vec![0xc1, 0xff], vec![]] {
        let mut coder = vec![1, 0x24, 6, 0xf1, 7, 1, properties.len() as u8];
        coder.extend(properties);
        let outcome =
            SevenZArchive::open(Cursor::new(single_block(&coder, 1, &[0])), Some("secret"))
                .and_then(|mut a| a.test(&mut |_| true));
        assert!(outcome.is_err());
    }
}

#[test]
fn empty_encrypted_stream_cannot_authenticate_any_password() {
    let mut header = vec![1, 4, 6, 0, 1, 9, 16, 0, 7, 11, 1, 0];
    header.extend([1, 0x24, 6, 0xf1, 7, 1, 2, 1, 0]);
    header.extend([
        12, 0, 10, 1, 0, 0, 0, 0, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0, 0, 0, 0, 0,
    ]);
    let mut archive =
        SevenZArchive::open(Cursor::new(with_header(&header, &[0; 16])), Some("wrong")).unwrap();
    assert!(archive.has_encrypted());
    assert!(matches!(
        archive.test(&mut |_| true),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        archive.validate_encrypted(&mut |_| true),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn malformed_coder_counts_and_graphs() {
    for coder in [
        vec![0],
        vec![5],
        vec![1, 0x11, 0, 0, 0],
        vec![2, 1, 0, 1, 0, 0, 0],
        vec![2, 1, 0, 1, 0, 9, 0],
        vec![3, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1],
    ] {
        assert!(SevenZArchive::open(Cursor::new(single_block(&coder, 1, &[0])), None).is_err());
    }
}

#[test]
fn malformed_header_mutations_never_panic() {
    let base = [
        1, 4, 6, 0, 1, 9, 1, 0, 7, 11, 1, 0, 1, 1, 0, 12, 1, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0,
        0, 0, 0, 0,
    ];
    for index in 0..base.len() {
        for byte in [0, 1, 0x7f, 0xff] {
            let mut header = base;
            header[index] = byte;
            if let Ok(mut archive) =
                SevenZArchive::open(Cursor::new(with_header(&header, &[0])), None)
            {
                let _ = archive.test(&mut |_| true);
            }
        }
    }
}
