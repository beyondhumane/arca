use super::*;
use lzma_rust2::CheckType;
use std::io::Cursor;

fn compress(data: &[u8], level: Level, threads: usize) -> Vec<u8> {
    let mut e = encoder(Vec::new(), level, threads).unwrap();
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn with_check(data: &[u8], check: CheckType) -> Vec<u8> {
    let mut options = XzOptions::with_preset(1);
    options.set_check_sum_type(check);
    let mut w = XzWriter::new(Vec::new(), options).unwrap();
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(bytes);
    let size = uncompressed_size(&mut cursor)?;
    let mut decoder = Decoder::new(Cursor::new(bytes));
    decoder.expected = Some(size);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

fn sample(len: usize) -> Vec<u8> {
    let mut state = 0x1234_5678u32;
    (0..len)
        .map(|i| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            if i % 3 == 0 {
                (state >> 24) as u8
            } else {
                b"arca "[i % 5]
            }
        })
        .collect()
}

// The block header starts right after the 12-byte stream header; its size
// byte says how long it is, and its last four bytes are a CRC32 of the rest.
fn patch_block_header(bytes: &mut [u8], edit: impl FnOnce(&mut [u8])) {
    let size = (usize::from(bytes[12]) + 1) * 4;
    let header = &mut bytes[12..12 + size];
    edit(header);
    let crc = crc32fast::hash(&header[..size - 4]);
    header[size - 4..].copy_from_slice(&crc.to_le_bytes());
}

fn lzma2_filter(header: &mut [u8]) -> usize {
    header
        .windows(2)
        .position(|w| w == [0x21, 0x01])
        .expect("LZMA2 filter in block header")
}

#[test]
fn round_trips_every_level_single_and_parallel() {
    let data = sample(300_000);
    for level in [Level::Store, Level::Fast, Level::Normal, Level::Best] {
        for threads in [1, 4] {
            let packed = compress(&data, level, threads);
            assert_eq!(&packed[..6], MAGIC);
            assert_eq!(decode(&packed).unwrap(), data, "{level:?} -j{threads}");
        }
    }
}

#[test]
fn parallel_output_has_several_blocks_and_stays_valid() {
    let data = sample(5 << 20);
    let mut e = encoder(Vec::new(), Level::Store, 3).unwrap();
    assert_eq!(e.workers(), 3);
    for chunk in data.chunks(64 * 1024) {
        e.write_all(chunk).unwrap();
    }
    let packed = e.finish().unwrap();
    assert_eq!(
        uncompressed_size(&mut Cursor::new(&packed)).unwrap(),
        data.len() as u64
    );
    assert_eq!(decode(&packed).unwrap(), data);
}

#[test]
fn empty_input_is_a_valid_stream() {
    for threads in [1, 2] {
        let packed = compress(b"", Level::Normal, threads);
        assert_eq!(decode(&packed).unwrap(), b"");
    }
}

#[test]
fn every_check_type_is_verified() {
    let data = sample(10_000);
    for check in [
        CheckType::None,
        CheckType::Crc32,
        CheckType::Crc64,
        CheckType::Sha256,
    ] {
        let packed = with_check(&data, check);
        assert_eq!(decode(&packed).unwrap(), data, "{check:?}");
    }
    for check in [CheckType::Crc32, CheckType::Crc64, CheckType::Sha256] {
        let mut packed = with_check(&data, check);
        let size = match check {
            CheckType::Crc32 => 4,
            CheckType::Crc64 => 8,
            _ => 32,
        };
        let mut cursor = Cursor::new(&packed);
        uncompressed_size(&mut cursor).unwrap();
        let backward = (u64::from(le32(&packed[packed.len() - 8..])) + 1) * 4;
        let check_at = packed.len() - 12 - backward as usize - size;
        packed[check_at] ^= 0x55;
        let error = decode(&packed).unwrap_err().to_string();
        assert!(error.contains("checksum"), "{check:?}: {error}");
    }
}

#[test]
fn concatenated_streams_and_padding_are_one_payload() {
    let a = compress(b"first ", Level::Fast, 1);
    let b = compress(b"second", Level::Fast, 1);
    let mut joined = a.clone();
    joined.extend_from_slice(&[0; 8]);
    joined.extend_from_slice(&b);
    joined.extend_from_slice(&[0; 4]);
    assert_eq!(uncompressed_size(&mut Cursor::new(&joined)).unwrap(), 12);
    assert_eq!(decode(&joined).unwrap(), b"first second");

    let mut odd = a.clone();
    odd.extend_from_slice(&[0; 2]);
    odd.extend_from_slice(&b);
    assert!(decode(&odd).is_err());

    let mut garbage = a;
    garbage.extend_from_slice(b"junk");
    assert!(decode(&garbage).is_err());
}

#[test]
fn truncation_anywhere_fails_without_panicking() {
    let packed = compress(&sample(50_000), Level::Normal, 1);
    for cut in (0..packed.len()).step_by(7).chain([packed.len() - 1]) {
        assert!(decode(&packed[..cut]).is_err(), "cut at {cut}");
        let mut out = Vec::new();
        let streaming = Decoder::new(&packed[..cut]).read_to_end(&mut out);
        assert!(streaming.is_err(), "streaming cut at {cut}");
    }
}

#[test]
fn damaged_header_block_index_and_footer_are_rejected() {
    let data = sample(20_000);
    let packed = compress(&data, Level::Normal, 1);
    let len = packed.len();
    let backward = (u64::from(le32(&packed[len - 8..])) + 1) * 4;
    let index = len - 12 - backward as usize;
    for (what, at) in [
        ("magic", 1),
        ("stream flags", 7),
        ("block header", 14),
        ("block data", 40),
        ("block data end", index - 20),
        ("index", index + 2),
        ("footer crc", len - 12),
        ("footer flags", len - 3),
        ("footer magic", len - 1),
    ] {
        let mut bad = packed.clone();
        bad[at] ^= 0x01;
        let result = decode(&bad);
        assert!(result.is_err() || result.unwrap() != data, "{what}");
        let mut out = Vec::new();
        let _ = Decoder::new(&bad[..]).read_to_end(&mut out);
    }
}

#[test]
fn every_single_bit_flip_is_safe() {
    let data = sample(2_000);
    let packed = compress(&data, Level::Fast, 1);
    for at in 0..packed.len() {
        let mut bad = packed.clone();
        bad[at] ^= 0x80;
        if let Ok(out) = decode(&bad) {
            assert_eq!(out, data, "byte {at} changed the output silently");
        }
    }
}

#[test]
fn an_index_that_disagrees_with_the_data_is_caught() {
    let packed = compress(b"hello", Level::Fast, 1);
    let len = packed.len();
    let backward = (u64::from(le32(&packed[len - 8..])) + 1) * 4;
    let index = len - 12 - backward as usize;
    let mut bad = packed.clone();
    let body = &mut bad[index..index + backward as usize];
    assert_eq!(body[3], 5);
    body[3] = 6;
    let crc = crc32fast::hash(&body[..body.len() - 4]);
    let n = body.len();
    body[n - 4..].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(uncompressed_size(&mut Cursor::new(&bad)).unwrap(), 6);
    let error = decode(&bad).unwrap_err().to_string();
    assert!(error.contains("index declares 6"), "{error}");
}

#[test]
fn an_oversized_dictionary_is_refused_before_allocating() {
    let mut packed = compress(b"hello", Level::Fast, 1);
    patch_block_header(&mut packed, |h| {
        let at = lzma2_filter(h);
        h[at + 2] = 40;
    });
    let error = decode(&packed).unwrap_err().to_string();
    assert!(error.contains("dictionary"), "{error}");
}

#[test]
fn an_unknown_filter_is_an_error() {
    let mut packed = compress(b"hello", Level::Fast, 1);
    patch_block_header(&mut packed, |h| {
        let at = lzma2_filter(h);
        h[at] = 0x30;
    });
    assert!(decode(&packed).is_err());
}

#[test]
fn an_unknown_check_type_is_an_error() {
    let mut packed = compress(b"hello", Level::Fast, 1);
    packed[7] = 0x02;
    let crc = crc32fast::hash(&packed[6..8]);
    packed[8..12].copy_from_slice(&crc.to_le_bytes());
    assert!(decode(&packed).is_err());
}

#[test]
fn index_parsing_rejects_hostile_values() {
    assert!(parse_index(&[]).is_err());
    assert!(parse_index(&[1, 0, 0, 0, 0, 0, 0, 0]).is_err());
    let mut lying = vec![0u8, 0xFF, 0xFF, 0xFF, 0x0F, 0, 0, 0];
    let crc = crc32fast::hash(&lying[..4]);
    lying[4..].copy_from_slice(&crc.to_le_bytes());
    assert!(parse_index(&lying).is_err());
    let mut padded = vec![0u8, 0x80, 0x00, 0x00];
    let crc = crc32fast::hash(&padded);
    padded.extend_from_slice(&crc.to_le_bytes());
    assert!(parse_index(&padded).is_err());
    for junk in [&b""[..], &[0; 8], &[0; 24], b"YZYZYZYZYZYZYZYZYZYZYZYZ"] {
        assert!(uncompressed_size(&mut Cursor::new(junk)).is_err());
    }
}

#[test]
fn workers_fit_the_memory_budget() {
    assert_eq!(workers(Level::Normal, 1), 1);
    assert_eq!(workers(Level::Fast, 4), 4);
    assert_eq!(workers(Level::Best, 64), 1);
    assert!(workers(Level::Normal, 64) * worker_mib(6) as usize <= ENCODER_BUDGET_MIB as usize);
    assert!(workers(Level::Normal, 0) >= 1);
    assert_eq!(preset(Level::Store), 0);
    assert_eq!(preset(Level::Best), 9);
}

#[test]
fn output_names_drop_the_suffix() {
    for (archive, name) in [
        ("notes.txt.xz", "notes.txt"),
        ("NOTES.XZ", "NOTES"),
        ("backup.tar.xz", "backup.tar"),
        ("backup.txz", "backup.tar"),
        ("blob", "blob.out"),
        (".xz", ".xz.out"),
    ] {
        assert_eq!(output_name(Path::new(archive)), name);
    }
}

#[test]
fn identify_looks_inside_instead_of_trusting_the_name() {
    let dir = std::env::temp_dir().join(format!("arca-xz-identify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut tar = arca_tar::TarWriter::new(Vec::new());
    tar.add("a.txt", 3, 0, 0o644, &b"abc"[..]).unwrap();
    let tar = compress(&tar.finish().unwrap(), Level::Fast, 1);
    let plain = compress(&sample(4_000), Level::Fast, 1);
    let empty_tar = compress(&[0; 10_240], Level::Fast, 1);
    let end_marker = compress(&[0; 1_024], Level::Fast, 1);
    let one_block = compress(&[0; 512], Level::Fast, 1);
    let ragged = compress(&[0; 1_500], Level::Fast, 1);
    let past_record = compress(&[0; 10_752], Level::Fast, 1);
    let zeros = compress(&vec![0; 1 << 20], Level::Fast, 1);
    for (name, bytes, expected) in [
        ("a.tar.xz", &tar, Some(Format::TarXz)),
        ("a.xz", &tar, Some(Format::TarXz)),
        ("no-suffix", &tar, Some(Format::TarXz)),
        ("b.xz", &plain, Some(Format::Xz)),
        ("b.tar.xz", &plain, Some(Format::Xz)),
        ("b.bin", &plain, Some(Format::Xz)),
        ("empty.txz", &empty_tar, Some(Format::TarXz)),
        ("zeros.xz", &empty_tar, Some(Format::Xz)),
        ("arca-empty.tar.xz", &end_marker, Some(Format::TarXz)),
        ("one-block.tar.xz", &one_block, Some(Format::Xz)),
        ("ragged.tar.xz", &ragged, Some(Format::Xz)),
        ("past-record.tar.xz", &past_record, Some(Format::Xz)),
        ("zeros.tar.xz", &zeros, Some(Format::Xz)),
        ("x.zip", &tar, Some(Format::Zip)),
        ("fake.xz", &b"not xz at all".to_vec(), Some(Format::Xz)),
        ("plain.bin", &b"not xz at all".to_vec(), None),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(identify(&path), expected, "{name}");
    }
    assert_eq!(identify(&dir.join("missing.txz")), Some(Format::TarXz));
    assert_eq!(identify(&dir.join("missing")), None);

    let path = dir.join("b.xz");
    let e = entry(&path).unwrap();
    assert_eq!((e.name.as_str(), e.size), ("b", 4_000));
    assert_eq!(e.compressed_size, plain.len() as u64);
    let mut out = Vec::new();
    open(&path).unwrap().read_to_end(&mut out).unwrap();
    assert_eq!(out, sample(4_000));
    let _ = std::fs::remove_dir_all(dir);
}
