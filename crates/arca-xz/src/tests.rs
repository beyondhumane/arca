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
    for level in [Level::STORE, Level::FAST, Level::NORMAL, Level::BEST] {
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
    let mut e = encoder(Vec::new(), Level::STORE, 3).unwrap();
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
        let packed = compress(b"", Level::NORMAL, threads);
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
    let a = compress(b"first ", Level::FAST, 1);
    let b = compress(b"second", Level::FAST, 1);
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
    let packed = compress(&sample(50_000), Level::NORMAL, 1);
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
    let packed = compress(&data, Level::NORMAL, 1);
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
    let packed = compress(&data, Level::FAST, 1);
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
    let packed = compress(b"hello", Level::FAST, 1);
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
    let mut packed = compress(b"hello", Level::FAST, 1);
    patch_block_header(&mut packed, |h| {
        let at = lzma2_filter(h);
        h[at + 2] = 40;
    });
    let error = decode(&packed).unwrap_err().to_string();
    assert!(error.contains("dictionary"), "{error}");
}

#[test]
fn an_unknown_filter_is_an_error() {
    let mut packed = compress(b"hello", Level::FAST, 1);
    patch_block_header(&mut packed, |h| {
        let at = lzma2_filter(h);
        h[at] = 0x30;
    });
    assert!(decode(&packed).is_err());
}

#[test]
fn an_unknown_check_type_is_an_error() {
    let mut packed = compress(b"hello", Level::FAST, 1);
    packed[7] = 0x02;
    let crc = crc32fast::hash(&packed[6..8]);
    packed[8..12].copy_from_slice(&crc.to_le_bytes());
    assert!(decode(&packed).is_err());
}

#[test]
fn index_parsing_rejects_hostile_values() {
    assert!(parse_index(&[], |_, _| ()).is_err());
    assert!(parse_index(&[1, 0, 0, 0, 0, 0, 0, 0], |_, _| ()).is_err());
    let mut lying = vec![0u8, 0xFF, 0xFF, 0xFF, 0x0F, 0, 0, 0];
    let crc = crc32fast::hash(&lying[..4]);
    lying[4..].copy_from_slice(&crc.to_le_bytes());
    assert!(parse_index(&lying, |_, _| ()).is_err());
    let mut padded = vec![0u8, 0x80, 0x00, 0x00];
    let crc = crc32fast::hash(&padded);
    padded.extend_from_slice(&crc.to_le_bytes());
    assert!(parse_index(&padded, |_, _| ()).is_err());
    for junk in [&b""[..], &[0; 8], &[0; 24], b"YZYZYZYZYZYZYZYZYZYZYZYZ"] {
        assert!(uncompressed_size(&mut Cursor::new(junk)).is_err());
    }
}

#[test]
fn workers_fit_the_memory_budget() {
    assert_eq!(workers(Level::NORMAL, 1), 1);
    assert_eq!(workers(Level::FAST, 4), 4);
    assert_eq!(workers(Level::BEST, 64), 1);
    assert!(workers(Level::NORMAL, 64) * worker_mib(6) as usize <= ENCODER_BUDGET_MIB as usize);
    assert!(workers(Level::NORMAL, 0) >= 1);
    assert_eq!(preset(Level::STORE), 0);
    assert_eq!(preset(Level::BEST), 9);
}

#[test]
fn open_reads_a_file_and_checks_it_against_its_index() {
    let dir = std::env::temp_dir().join(format!("arca-xz-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let data = sample(4_000);
    let path = dir.join("b.xz");
    std::fs::write(&path, compress(&data, Level::FAST, 1)).unwrap();
    let mut out = Vec::new();
    open(&path).unwrap().read_to_end(&mut out).unwrap();
    assert_eq!(out, data);
    std::fs::write(&path, b"not xz at all").unwrap();
    assert!(open(&path).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

fn small_blocks(data: &[u8], block: u64) -> Vec<u8> {
    let mut options = XzOptions::with_preset(1);
    options.lzma_options.dict_size = 4096;
    options.set_block_size(NonZeroU64::new(block));
    let mut w = XzWriterMt::new(Vec::new(), options, 3).unwrap();
    w.write_all(data).unwrap();
    w.finish().unwrap()
}

fn parallel_decoder(bytes: &[u8], threads: usize) -> Result<Decoder<BufReader<Cursor<Vec<u8>>>>> {
    seekable(Cursor::new(bytes.to_vec()), threads)
}

fn decode_parallel(bytes: &[u8], threads: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    parallel_decoder(bytes, threads)?.read_to_end(&mut out)?;
    Ok(out)
}

// The (unpadded, size) records of a one-stream file, and where its index starts.
fn records(packed: &[u8]) -> (Vec<(u64, u64)>, usize) {
    let len = packed.len();
    let backward = (u64::from(le32(&packed[len - 8..])) + 1) * 4;
    let start = len - 12 - backward as usize;
    let mut list = Vec::new();
    parse_index(&packed[start..len - 12], |u, s| list.push((u, s))).unwrap();
    (list, start)
}

// The same blocks behind an index rebuilt from `list`.
fn with_index(packed: &[u8], list: &[(u64, u64)]) -> Vec<u8> {
    let (_, start) = records(packed);
    let mut index = vec![0u8];
    let put = |mut v: u64, out: &mut Vec<u8>| {
        while v >= 0x80 {
            out.push(v as u8 | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    };
    put(list.len() as u64, &mut index);
    for &(u, s) in list {
        put(u, &mut index);
        put(s, &mut index);
    }
    while index.len() % 4 != 0 {
        index.push(0);
    }
    let crc = crc32fast::hash(&index);
    index.extend_from_slice(&crc.to_le_bytes());
    let mut out = packed[..start].to_vec();
    out.extend_from_slice(&index);
    let mut footer = packed[packed.len() - 12..].to_vec();
    footer[4..8].copy_from_slice(&(index.len() as u32 / 4 - 1).to_le_bytes());
    let crc = crc32fast::hash(&footer[4..10]);
    footer[..4].copy_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&footer);
    out
}

#[test]
fn several_blocks_decode_in_parallel_and_in_order() {
    let data = sample(200_000);
    let packed = small_blocks(&data, 16 << 10);
    assert!(records(&packed).0.len() > 10);
    let decoder = parallel_decoder(&packed, 4).unwrap();
    assert!(matches!(decoder.inner, Inner::Parallel(_)));
    assert_eq!(decode_parallel(&packed, 4).unwrap(), data);
    assert_eq!(decode_parallel(&packed, 1).unwrap(), data);
    let one = compress(&data, Level::FAST, 1);
    let decoder = parallel_decoder(&one, 4).unwrap();
    assert!(matches!(decoder.inner, Inner::Single(_)));
    let mut two = one.clone();
    two.extend_from_slice(&packed);
    assert!(matches!(
        parallel_decoder(&two, 4).unwrap().inner,
        Inner::Single(_)
    ));
    let mut both = data.clone();
    both.extend_from_slice(&data);
    assert_eq!(decode_parallel(&two, 4).unwrap(), both);
}

#[test]
fn parallel_threads_stay_within_the_budget() {
    let block = |size| parallel::Block {
        unpadded: size / 2,
        size,
    };
    assert_eq!(parallel::threads(&[], 16), 1);
    assert_eq!(parallel::threads(&[block(1 << 20)], 16), 1);
    assert_eq!(parallel::threads(&[block(1 << 20); 3], 16), 3);
    assert_eq!(parallel::threads(&[block(1 << 30); 8], 16), 1);
    let blocks = [block(24 << 20); 64];
    let threads = parallel::threads(&blocks, 64) as u64;
    assert!(threads > 1);
    assert!((threads + 2) * (12 + 72 + 1) <= DECODER_BUDGET_MIB);
}

#[test]
fn parallel_truncation_and_bit_flips_fail_without_panicking() {
    let data = sample(20_000);
    let packed = small_blocks(&data, 4096);
    assert!(records(&packed).0.len() >= 4);
    for cut in (0..packed.len()).step_by(11).chain([packed.len() - 1]) {
        assert!(decode_parallel(&packed[..cut], 4).is_err(), "cut at {cut}");
    }
    for at in (0..packed.len()).step_by(3) {
        let mut bad = packed.clone();
        bad[at] ^= 0x10;
        if let Ok(out) = decode_parallel(&bad, 4) {
            assert_eq!(out, data, "byte {at} changed the output silently");
        }
    }
}

#[test]
fn parallel_index_that_moves_bytes_between_blocks_is_caught() {
    let data = sample(60_000);
    let packed = small_blocks(&data, 8192);
    let (mut list, _) = records(&packed);
    assert_eq!(
        decode_parallel(&with_index(&packed, &list), 4).unwrap(),
        data
    );
    list[1].1 += 1;
    list[2].1 -= 1;
    assert!(decode_parallel(&with_index(&packed, &list), 4).is_err());
    let (mut list, _) = records(&packed);
    list[0].0 += 4;
    list[1].0 -= 4;
    assert!(decode_parallel(&with_index(&packed, &list), 4).is_err());
}

#[test]
fn a_damaged_block_stops_the_parallel_stream_after_the_blocks_before_it() {
    let data = sample(60_000);
    let packed = small_blocks(&data, 8192);
    let (list, _) = records(&packed);
    let third = 12 + ((list[0].0 + 3) & !3) + ((list[1].0 + 3) & !3);
    let mut bad = packed.clone();
    bad[third as usize + 30] ^= 0x55;
    let mut decoder = parallel_decoder(&bad, 4).unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; 1000];
    let error = loop {
        match decoder.read(&mut buf) {
            Ok(0) => panic!("the damaged block decoded"),
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) => break e,
        }
    };
    assert!(out.len() as u64 >= list[0].1 + list[1].1);
    assert_eq!(out[..], data[..out.len()]);
    let _ = error;
    assert!(decoder.read(&mut buf).is_err());
}

#[test]
fn a_file_cut_short_gives_out_every_block_before_the_cut() {
    let data = sample(200_000);
    let packed = small_blocks(&data, 16 << 10);
    let (list, start) = records(&packed);
    let blocks: Vec<parallel::Block> = list
        .iter()
        .map(|&(unpadded, size)| parallel::Block { unpadded, size })
        .collect();
    let header: [u8; 12] = packed[..12].try_into().unwrap();
    for cut in [start * 3 / 10, start / 2, start * 7 / 10, start - 1] {
        let mut whole = 0;
        let mut end = 12;
        for &(unpadded, size) in &list {
            end += (unpadded + 3) & !3;
            if end > cut as u64 {
                break;
            }
            whole += size as usize;
        }
        let source = Cursor::new(packed[12..cut].to_vec());
        let mut reader = parallel::Parallel::new(source, header, blocks.clone(), 4).unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        let failed = loop {
            match reader.read(&mut buf) {
                Ok(0) => break false,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => break true,
            }
        };
        assert!(failed, "cut at {cut}");
        assert_eq!(out.len(), whole, "cut at {cut}");
        assert!(out == data[..whole], "cut at {cut}");
    }
}
