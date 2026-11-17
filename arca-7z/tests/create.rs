use arca_7z::{
    create_7z, ppmd_parameters, Compression, CreateOptions, SevenZArchive, Source, PPMD_MEMORY_MAX,
};
use arca_core::{Error, Level};
use sevenz_rust2::{Archive, EncoderMethod as M, Password};
use std::io::Cursor;

/// Bytes that look like branchy code: calls whose targets drift.
fn code(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut target = 0x1000u32;
    while out.len() < len {
        out.push(0xE8);
        out.extend_from_slice(&target.wrapping_sub(out.len() as u32).to_le_bytes());
        out.extend_from_slice(b"\x48\x89\xc7\x90\x94\x00\x00\x00");
        target = target.wrapping_add(16);
    }
    out.truncate(len);
    out
}

fn text(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut n = 0u64;
    while out.len() < len {
        n = n.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        out.extend_from_slice(format!("line {} of the text, {}\n", n % 977, n >> 52).as_bytes());
    }
    out.truncate(len);
    out
}

/// Code with a header of zeros but for `fields`, each at its offset.
fn with_head(fields: &[(usize, &[u8])]) -> Vec<u8> {
    let mut out = code(20_000);
    out[..4096].fill(0);
    for (at, bytes) in fields {
        out[*at..*at + bytes.len()].copy_from_slice(bytes);
    }
    out
}

fn pe(machine: u16) -> Vec<u8> {
    with_head(&[
        (0, b"MZ"),
        (0x3C, &0x80u32.to_le_bytes()),
        (0x80, b"PE\0\0"),
        (0x84, &machine.to_le_bytes()),
        (0x94, &0xF0u16.to_le_bytes()),
        (0x98, &0x20Bu16.to_le_bytes()),
    ])
}

fn elf(machine: u16, be: bool) -> Vec<u8> {
    let machine = if be {
        machine.to_be_bytes()
    } else {
        machine.to_le_bytes()
    };
    with_head(&[
        (0, b"\x7fELF"),
        (4, &[2, if be { 2 } else { 1 }, 1]),
        (0x12, &machine),
    ])
}

fn macho(cpu: u32, be: bool) -> Vec<u8> {
    let (magic, cpu, count) = if be {
        (
            [0xFE, 0xED, 0xFA, 0xCF],
            cpu.to_be_bytes(),
            4u32.to_be_bytes(),
        )
    } else {
        (
            [0xCF, 0xFA, 0xED, 0xFE],
            cpu.to_le_bytes(),
            4u32.to_le_bytes(),
        )
    };
    with_head(&[(0, &magic), (4, &cpu), (16, &count)])
}

fn wav(channels: u16, bits: u16) -> Vec<u8> {
    let frame = channels * bits / 8;
    let mut out = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100 * u32::from(frame)).to_le_bytes());
    out.extend_from_slice(&frame.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&40_000u32.to_le_bytes());
    for i in 0..40_000 / u32::from(frame) {
        let sample = ((f64::from(i) / 9.0).sin() * 12_000.0) as i16;
        for _ in 0..channels {
            out.extend_from_slice(&sample.to_le_bytes()[..usize::from(bits / 8)]);
        }
    }
    out
}

struct Set {
    room: tempfile::TempDir,
    sources: Vec<Source>,
    data: Vec<Vec<u8>>,
}

fn set(files: Vec<(&str, Vec<u8>)>) -> Set {
    let room = tempfile::tempdir().unwrap();
    let mut sources = Vec::new();
    let mut data = Vec::new();
    for (i, (name, bytes)) in files.into_iter().enumerate() {
        let path = room.path().join(format!("f{i}"));
        std::fs::write(&path, &bytes).unwrap();
        sources.push(Source {
            path,
            name: name.into(),
        });
        data.push(bytes);
    }
    Set {
        room,
        sources,
        data,
    }
}

fn create(set: &Set, options: &CreateOptions<'_>) -> Vec<u8> {
    let out = set.room.path().join("out.7z");
    create_7z(&out, &set.sources, options, &mut |_| true).unwrap();
    std::fs::read(&out).unwrap()
}

/// The coder ids of the block each entry is in, outermost first and AES
/// left out, with the name of the entry.
fn chains(bytes: &[u8], password: Option<&str>) -> Vec<(String, Vec<Vec<u8>>)> {
    let password = Password::from(password.unwrap_or(""));
    let archive = Archive::read(&mut Cursor::new(bytes), &password).unwrap();
    archive
        .files
        .iter()
        .enumerate()
        .filter_map(|(i, file)| {
            let block = archive.stream_map.file_block_index[i]?;
            let ids = archive.blocks[block]
                .ordered_coder_iter()
                .map(|(_, c)| c.encoder_method_id().to_vec())
                .filter(|id| id.as_slice() != M::ID_AES256_SHA256)
                .collect();
            Some((file.name.clone(), ids))
        })
        .collect()
}

fn reads_back(bytes: &[u8], set: &Set, password: Option<&str>) {
    let mut archive = SevenZArchive::open(Cursor::new(bytes), password).unwrap();
    archive.test(&mut |_| true).unwrap();
    for (source, data) in set.sources.iter().zip(&set.data) {
        let index = archive
            .entries()
            .iter()
            .position(|e| e.name == source.name)
            .unwrap();
        let read = archive.read_entry(index, 64 << 20, &mut |_| true).unwrap();
        assert!(&read == data, "{}", source.name);
    }
}

#[test]
fn every_method_round_trips_and_is_the_one_written() {
    let set = set(vec![
        ("notes.txt", text(300_000)),
        ("more/notes.md", text(50_000)),
        ("program.exe", pe(0x8664)),
    ]);
    for compression in Compression::ALL {
        if !compression.available() {
            let error = create_7z(
                &set.room.path().join("no.7z"),
                &set.sources,
                &CreateOptions {
                    compression,
                    ..Default::default()
                },
                &mut |_| true,
            );
            assert!(matches!(error, Err(Error::Unsupported(_))), "{error:?}");
            continue;
        }
        let main: &[u8] = match compression {
            Compression::Lzma2 => M::ID_LZMA2,
            Compression::Lzma => M::ID_LZMA,
            Compression::Ppmd => M::ID_PPMD,
            Compression::Bzip2 => M::ID_BZIP2,
            Compression::Deflate => M::ID_DEFLATE,
            Compression::Store => M::ID_COPY,
        };
        for (level, password) in [
            (Level::FAST, None),
            (Level::NORMAL, Some("secret")),
            (Level::BEST, None),
        ] {
            let bytes = create(
                &set,
                &CreateOptions {
                    level,
                    password,
                    compression,
                    ..Default::default()
                },
            );
            reads_back(&bytes, &set, password);
            for (name, chain) in chains(&bytes, password) {
                assert_eq!(chain[0], main, "{name} with {compression:?}");
                let filtered = name == "program.exe" && compression != Compression::Store;
                assert_eq!(chain.len(), 1 + usize::from(filtered), "{name}");
            }
            if compression == Compression::Store {
                assert!(bytes.len() > 370_000);
            } else {
                assert!(bytes.len() < 300_000, "{compression:?} {level}");
            }
        }
    }
    let stored = create(
        &set,
        &CreateOptions {
            level: Level::STORE,
            compression: Compression::Ppmd,
            ..Default::default()
        },
    );
    assert!(chains(&stored, None)
        .iter()
        .all(|(_, chain)| chain == &[M::ID_COPY.to_vec()]));
    assert_eq!(Compression::default(), Compression::Lzma2);
    assert_eq!(Compression::Ppmd.name(), "PPMd");
    assert_eq!(Compression::Ppmd.threads(Level::BEST), 1);
    assert_eq!(Compression::Lzma2.threads(Level::STORE), 1);
}

#[test]
fn each_program_gets_the_filter_of_its_machine() {
    let set = set(vec![
        ("win/x64.dll", pe(0x8664)),
        ("win/arm64.dll", pe(0xAA64)),
        ("win/armnt.dll", pe(0x01C4)),
        ("win/wince.dll", pe(0x01C2)),
        ("win/ia64.dll", pe(0x0200)),
        ("win/riscv.dll", pe(0x5064)),
        ("linux/arm", elf(40, false)),
        ("linux/aarch64", elf(183, false)),
        ("linux/ppc", elf(20, true)),
        ("linux/ppc64le", elf(21, false)),
        ("linux/sparc", elf(43, true)),
        ("linux/x86_64", elf(62, false)),
        ("linux/riscv", elf(243, false)),
        ("mac/arm64", macho(0x0100_000C, false)),
        ("mac/x86_64", macho(0x0100_0007, false)),
        ("mac/ppc", macho(18, true)),
        ("audio/stereo.wav", wav(2, 16)),
        ("audio/mono.wav", wav(1, 8)),
        ("plain/fake.exe", code(20_000)),
        ("plain/Class.class", {
            let mut java = code(20_000);
            java[..4].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
            java
        }),
        ("plain/notes.txt", text(20_000)),
        ("plain/tiny.exe", b"MZ".to_vec()),
    ]);
    let bytes = create(&set, &CreateOptions::default());
    reads_back(&bytes, &set, None);
    let expected = |name: &str| -> Option<&[u8]> {
        Some(match name {
            "win/x64.dll" | "linux/x86_64" | "mac/x86_64" | "plain/fake.exe" | "plain/tiny.exe" => {
                M::ID_BCJ_X86
            }
            "win/arm64.dll" | "linux/aarch64" | "mac/arm64" => M::ID_BCJ_ARM64,
            "win/armnt.dll" => M::ID_BCJ_ARM_THUMB,
            "win/wince.dll" | "linux/arm" => M::ID_BCJ_ARM,
            "win/ia64.dll" => M::ID_BCJ_IA64,
            "linux/ppc" | "mac/ppc" => M::ID_BCJ_PPC,
            "linux/sparc" => M::ID_BCJ_SPARC,
            "audio/stereo.wav" | "audio/mono.wav" => M::ID_DELTA,
            _ => return None,
        })
    };
    let chains = chains(&bytes, None);
    assert_eq!(chains.len(), set.sources.len());
    for (name, chain) in &chains {
        assert_eq!(chain[0], M::ID_LZMA2, "{name}");
        assert_eq!(chain.get(1).map(Vec::as_slice), expected(name), "{name}");
    }
    let archive = Archive::read(&mut Cursor::new(&bytes), &Password::empty()).unwrap();
    let mut distances: Vec<Vec<u8>> = archive
        .blocks
        .iter()
        .flat_map(|b| b.coders.iter())
        .filter(|c| c.encoder_method_id() == M::ID_DELTA)
        .map(|c| c.properties().to_vec())
        .collect();
    distances.sort();
    assert_eq!(distances, [vec![0], vec![3]]);
}

#[test]
fn ppmd_memory_stays_within_what_the_reader_accepts() {
    for level in 1..=9 {
        let level = Level::new(level);
        let mut last = 0;
        for size in [0, 1, 1 << 10, 1 << 16, 1 << 20, 12 << 20, u64::MAX] {
            let (order, memory) = ppmd_parameters(level, size);
            assert!((2..=64).contains(&order));
            assert!((2048..=PPMD_MEMORY_MAX).contains(&memory), "{level} {size}");
            assert!(memory >= last);
            last = memory;
        }
    }
    const { assert!(PPMD_MEMORY_MAX <= 256 << 20) };
    assert_eq!(ppmd_parameters(Level::BEST, u64::MAX), (32, 192 << 20));
    assert_eq!(ppmd_parameters(Level::NORMAL, u64::MAX), (8, 32 << 20));
    assert_eq!(ppmd_parameters(Level::BEST, 1000), (32, 1 << 16));
    let set = set(vec![("big.txt", text(13 << 20))]);
    let bytes = create(
        &set,
        &CreateOptions {
            level: Level::BEST,
            compression: Compression::Ppmd,
            ..Default::default()
        },
    );
    let archive = Archive::read(&mut Cursor::new(&bytes), &Password::empty()).unwrap();
    let properties = archive.blocks[0].coders[0].properties().to_vec();
    assert_eq!(properties, [32, 0, 0, 0, 12]);
    reads_back(&bytes, &set, None);
}
