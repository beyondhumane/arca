use super::*;
use std::path::PathBuf;

struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("arca-stream-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Room(dir)
    }

    fn put(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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

fn compress(format: Format, data: &[u8], threads: usize, size: Option<u64>) -> Vec<u8> {
    let mut e = encoder(Vec::new(), format, Level::NORMAL, threads, size).unwrap();
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn decode(room: &Room, format: Format, bytes: &[u8]) -> Result<Vec<u8>> {
    let path = room.put(&format!("decode.{}", format.extension()), bytes);
    let mut out = Vec::new();
    open(&path, format)?.read_to_end(&mut out)?;
    Ok(out)
}

fn single_formats() -> Vec<Format> {
    let mut formats = vec![Format::Xz, Format::Bz2];
    if cfg!(feature = "codecs-native") {
        formats.push(Format::Zst);
    }
    formats
}

#[test]
fn every_codec_round_trips_and_reports_its_threads() {
    let room = Room::new("round");
    let data = sample(400_000);
    for format in single_formats() {
        for threads in [1, 3] {
            let e = encoder(Vec::new(), format, Level::FAST, threads, None).unwrap();
            let expected = match format {
                Format::Bz2 => 1,
                Format::Xz => arca_xz::workers(Level::FAST, threads),
                _ => threads,
            };
            assert_eq!(e.workers(), expected, "{format:?}");
            let packed = compress(format, &data, threads, Some(data.len() as u64));
            assert_eq!(identify_bytes(&room, format, &packed), Some(format));
            assert_eq!(decode(&room, format, &packed).unwrap(), data, "{format:?}");
        }
        for level in [Level::STORE, Level::BEST] {
            let mut e = encoder(Vec::new(), format, level, 1, None).unwrap();
            e.write_all(b"level").unwrap();
            let packed = e.finish().unwrap();
            assert_eq!(decode(&room, format, &packed).unwrap(), b"level");
        }
        let empty = compress(format, b"", 1, Some(0));
        assert_eq!(decode(&room, format, &empty).unwrap(), b"", "{format:?}");
    }
    assert!(encoder(Vec::new(), Format::Zip, Level::NORMAL, 1, None).is_err());
    assert!(open(&room.put("x.zip", b"PK"), Format::Zip).is_err());
}

fn identify_bytes(room: &Room, format: Format, bytes: &[u8]) -> Option<Format> {
    identify(&room.put(&format!("id.{}", format.extension()), bytes))
}

#[test]
fn output_names_drop_the_suffix() {
    for (archive, name) in [
        ("notes.txt.xz", "notes.txt"),
        ("NOTES.XZ", "NOTES"),
        ("backup.tar.xz", "backup.tar"),
        ("backup.txz", "backup.tar"),
        ("notes.txt.bz2", "notes.txt"),
        ("backup.tar.bz2", "backup.tar"),
        ("backup.TBZ2", "backup.tar"),
        ("backup.tbz", "backup.tar"),
        ("notes.txt.zst", "notes.txt"),
        ("backup.tar.zst", "backup.tar"),
        ("backup.tzst", "backup.tar"),
        ("blob", "blob.out"),
        (".xz", ".xz.out"),
        (".bz2", ".bz2.out"),
    ] {
        assert_eq!(output_name(Path::new(archive)), name);
    }
}

#[test]
fn identify_looks_inside_instead_of_trusting_the_name() {
    let room = Room::new("identify");
    let mut tar = arca_tar::TarWriter::new(Vec::new());
    tar.add("a.txt", 3, 0, 0o644, &b"abc"[..]).unwrap();
    let tar = tar.finish().unwrap();
    for single in single_formats() {
        let ext = single.extension();
        let tar_format = single.stream_tar().unwrap();
        let pack = |data: &[u8]| compress(single, data, 1, None);
        let packed_tar = pack(&tar);
        let plain = pack(&sample(4_000));
        let empty_tar = pack(&[0; 10_240]);
        let end_marker = pack(&[0; 1_024]);
        let one_block = pack(&[0; 512]);
        let ragged = pack(&[0; 1_500]);
        let past_record = pack(&[0; 10_752]);
        let zeros = pack(&vec![0; 1 << 20]);
        for (name, bytes, expected) in [
            (format!("a.tar.{ext}"), &packed_tar, Some(tar_format)),
            (format!("a.{ext}"), &packed_tar, Some(tar_format)),
            ("no-suffix".to_string(), &packed_tar, Some(tar_format)),
            (format!("b.{ext}"), &plain, Some(single)),
            (format!("b.tar.{ext}"), &plain, Some(single)),
            ("b.bin".to_string(), &plain, Some(single)),
            (format!("empty.tar.{ext}"), &empty_tar, Some(tar_format)),
            (format!("zeros.{ext}"), &empty_tar, Some(single)),
            (
                format!("arca-empty.tar.{ext}"),
                &end_marker,
                Some(tar_format),
            ),
            (format!("one-block.tar.{ext}"), &one_block, Some(single)),
            (format!("ragged.tar.{ext}"), &ragged, Some(single)),
            (format!("past-record.tar.{ext}"), &past_record, Some(single)),
            (format!("zeros.tar.{ext}"), &zeros, Some(single)),
            ("x.zip".to_string(), &packed_tar, Some(Format::Zip)),
            (
                format!("fake.{ext}"),
                &b"not compressed".to_vec(),
                Some(single),
            ),
            ("plain.bin".to_string(), &b"not compressed".to_vec(), None),
        ] {
            let path = room.put(&name, bytes);
            assert_eq!(identify(&path), expected, "{name}");
        }
        assert_eq!(
            identify(&room.0.join(format!("missing.tar.{ext}"))),
            Some(tar_format)
        );
    }
    assert_eq!(identify(&room.0.join("missing")), None);
    assert_eq!(identify(&room.0.join("missing.tbz")), Some(Format::TarBz2));

    let xz_named_bz2 = room.put("other.bz2", &compress(Format::Xz, b"hi", 1, None));
    assert_eq!(identify(&xz_named_bz2), Some(Format::Xz));
    let bz2_named_xz = room.put("other.tar.xz", &compress(Format::Bz2, &tar, 1, None));
    assert_eq!(identify(&bz2_named_xz), Some(Format::TarBz2));
}

#[test]
fn the_entry_of_a_standalone_stream() {
    let room = Room::new("entry");
    let data = sample(4_000);
    for format in single_formats() {
        let packed = compress(format, &data, 1, Some(data.len() as u64));
        let path = room.put(&format!("b.{}", format.extension()), &packed);
        let e = entry(&path, format).unwrap();
        assert_eq!(e.name, "b");
        assert_eq!(e.compressed_size, packed.len() as u64);
        let (size, method) = match format {
            Format::Xz => (4_000, Method::Lzma2),
            Format::Bz2 => (0, Method::Bzip2),
            _ => (4_000, Method::Zstd),
        };
        assert_eq!((e.size, e.method), (size, method), "{format:?}");
        let junk = room.put(
            &format!("junk.{}", format.extension()),
            b"junk junk junk junk",
        );
        assert!(entry(&junk, format).is_err(), "{format:?}");
    }
    assert!(entry(&room.put("a.zip", b"PK"), Format::Zip).is_err());
}

#[test]
fn bzip2_streams_join_and_reject_garbage() {
    let room = Room::new("bz2");
    let a = compress(Format::Bz2, b"first ", 1, None);
    let b = compress(Format::Bz2, b"second", 1, None);
    let mut joined = a.clone();
    joined.extend_from_slice(&b);
    assert_eq!(
        decode(&room, Format::Bz2, &joined).unwrap(),
        b"first second"
    );
    let mut garbage = a.clone();
    garbage.extend_from_slice(b"junk");
    assert!(decode(&room, Format::Bz2, &garbage).is_err());
    let mut zeros = a;
    zeros.extend_from_slice(&[0; 16]);
    assert!(decode(&room, Format::Bz2, &zeros).is_err());
    for junk in [
        &b""[..],
        b"BZ",
        b"BZh9",
        b"BZh0\x31\x41\x59\x26\x53\x59",
        b"BZh91AY&SY",
    ] {
        assert!(decode(&room, Format::Bz2, junk).is_err(), "{junk:?}");
    }
}

#[test]
fn bzip2_truncation_and_damage_fail_without_panicking() {
    let room = Room::new("bz2-damage");
    let data = sample(20_000);
    let packed = compress(Format::Bz2, &data, 1, None);
    let in_memory = |bytes: &[u8]| -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        bz2::decoder(bytes).read_to_end(&mut out)?;
        Ok(out)
    };
    for cut in (0..packed.len()).step_by(7).chain([packed.len() - 1]) {
        assert!(in_memory(&packed[..cut]).is_err(), "cut at {cut}");
    }
    for cut in [0, 3, 9, packed.len() / 2, packed.len() - 1] {
        assert!(
            decode(&room, Format::Bz2, &packed[..cut]).is_err(),
            "cut at {cut}"
        );
    }
    for at in (0..packed.len()).step_by(3) {
        let mut bad = packed.clone();
        bad[at] ^= 0x04;
        if let Ok(out) = in_memory(&bad) {
            assert_eq!(out, data, "byte {at} changed the output silently");
        }
    }
}

#[cfg(feature = "codecs-native")]
mod zstd_tests {
    use super::*;
    use std::io::Cursor;

    fn frame(data: &[u8], pledged: bool, checksum: bool) -> Vec<u8> {
        let mut e = zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
        e.include_checksum(checksum).unwrap();
        if pledged {
            e.set_pledged_src_size(Some(data.len() as u64)).unwrap();
        }
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn skippable(len: u32) -> Vec<u8> {
        let mut out = 0x184D_2A53u32.to_le_bytes().to_vec();
        out.extend_from_slice(&len.to_le_bytes());
        out.extend(std::iter::repeat_n(0xAB, len as usize));
        out
    }

    fn size_of(bytes: &[u8]) -> Result<Option<u64>> {
        zst::declared_size(&mut Cursor::new(bytes))
    }

    #[test]
    fn arca_writes_a_checksum_and_the_size_it_was_given() {
        let data = sample(300_000);
        for threads in [1, 4] {
            let packed = compress(Format::Zst, &data, threads, Some(data.len() as u64));
            assert_eq!(packed[4] & 0x04, 0x04, "checksum flag");
            assert_eq!(size_of(&packed).unwrap(), Some(data.len() as u64));
        }
        let streamed = compress(Format::Zst, &data, 1, None);
        assert_eq!(size_of(&streamed).unwrap(), None);
        let mut e = encoder(Vec::new(), Format::Zst, Level::FAST, 1, Some(10)).unwrap();
        let wrote = e.write_all(&data);
        assert!(
            wrote.is_err() || e.finish().is_err(),
            "a size that does not match is an error"
        );
    }

    #[test]
    fn frames_and_skippable_frames_are_one_payload() {
        let room = Room::new("zst-frames");
        let mut joined = skippable(5);
        joined.extend(frame(b"first ", true, true));
        joined.extend(skippable(0));
        joined.extend(frame(b"second", true, false));
        assert_eq!(size_of(&joined).unwrap(), Some(12));
        assert_eq!(
            decode(&room, Format::Zst, &joined).unwrap(),
            b"first second"
        );
        let mut mixed = frame(b"first ", true, true);
        mixed.extend(frame(b"second", false, true));
        assert_eq!(size_of(&mixed).unwrap(), None);
        assert_eq!(decode(&room, Format::Zst, &mixed).unwrap(), b"first second");
        let path = room.put("mixed.zst", &mixed);
        assert_eq!(entry(&path, Format::Zst).unwrap().size, 0);

        let mut garbage = frame(b"first", true, true);
        garbage.extend_from_slice(b"junk");
        assert!(size_of(&garbage).is_err());
        assert!(decode(&room, Format::Zst, &garbage).is_err());
        assert!(size_of(&skippable(3)).is_err(), "no real frame");
    }

    #[test]
    fn a_frame_that_lies_about_its_size_is_caught() {
        let room = Room::new("zst-lie");
        let mut packed = frame(b"hello", true, false);
        assert_eq!(packed[4] >> 6, 0);
        assert_eq!(packed[4] & 0x20, 0x20);
        packed[5] = 6;
        assert_eq!(size_of(&packed).unwrap(), Some(6));
        assert!(decode(&room, Format::Zst, &packed).is_err());
    }

    #[test]
    fn a_window_over_the_limit_is_refused() {
        let room = Room::new("zst-window");
        let data = sample(1_000);
        let mut packed = frame(&data, false, true);
        assert_eq!(packed[4] & 0x20, 0, "has a window descriptor");
        packed[5] = (31 - 10) << 3;
        let error = decode(&room, Format::Zst, &packed).unwrap_err().to_string();
        assert!(error.contains("window"), "{error}");
    }

    #[test]
    fn checksums_are_verified() {
        let room = Room::new("zst-check");
        let data = sample(10_000);
        let mut packed = frame(&data, true, true);
        let at = packed.len() - 1;
        packed[at] ^= 0x01;
        assert!(decode(&room, Format::Zst, &packed).is_err());
    }

    #[test]
    fn truncation_anywhere_fails_without_panicking() {
        let room = Room::new("zst-cut");
        let data = sample(20_000);
        for packed in [
            compress(Format::Zst, &data, 1, Some(data.len() as u64)),
            compress(Format::Zst, &data, 1, None),
        ] {
            for cut in [0, 3, 9, packed.len() / 2, packed.len() - 1] {
                assert!(
                    decode(&room, Format::Zst, &packed[..cut]).is_err(),
                    "cut at {cut}"
                );
            }
            for cut in (0..packed.len()).step_by(5).chain([packed.len() - 1]) {
                assert!(size_of(&packed[..cut]).is_err(), "walk cut at {cut}");
                let mut out = Vec::new();
                let streaming = zst::decoder(Cursor::new(packed[..cut].to_vec()))
                    .and_then(|mut d| Ok(d.read_to_end(&mut out)?));
                assert!(streaming.is_err(), "streaming cut at {cut}");
            }
        }
    }

    #[test]
    fn every_single_bit_flip_is_safe() {
        let data = sample(3_000);
        let packed = compress(Format::Zst, &data, 1, Some(data.len() as u64));
        for at in 0..packed.len() {
            let mut bad = packed.clone();
            bad[at] ^= 0x80;
            let _ = size_of(&bad);
            let mut out = Vec::new();
            let decoded =
                zst::decoder(Cursor::new(bad)).and_then(|mut d| Ok(d.read_to_end(&mut out)?));
            if decoded.is_ok() {
                assert_eq!(out, data, "byte {at} changed the output silently");
            }
        }
    }

    #[test]
    fn hostile_headers_are_errors() {
        let mut reserved = frame(b"x", true, false);
        reserved[4] |= 0x08;
        assert!(size_of(&reserved).is_err());
        let mut block_type = frame(b"x", true, false);
        let block = 4 + 1 + 1;
        block_type[block] |= 0x06;
        assert!(size_of(&block_type).is_err());
        let mut huge = frame(b"x", true, false);
        huge[block] |= 0xF8;
        huge[block + 1] = 0xFF;
        huge[block + 2] = 0xFF;
        assert!(size_of(&huge).is_err());
        let mut eight = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
        eight.extend_from_slice(&[0xFF; 8]);
        eight.extend_from_slice(&[0x01, 0x00, 0x00]);
        let mut twice = eight.clone();
        twice.extend_from_slice(&eight);
        assert!(size_of(&twice).is_err(), "sizes that overflow");
        for junk in [
            &b""[..],
            &[0x28, 0xB5, 0x2F],
            &[0x28, 0xB5, 0x2F, 0xFD],
            b"\x50\x2A\x4D\x18\xFF\xFF\xFF\xFF",
        ] {
            assert!(size_of(junk).is_err(), "{junk:?}");
        }
    }
}

#[cfg(not(feature = "codecs-native"))]
#[test]
fn zstd_says_it_was_left_out_of_the_build() {
    let room = Room::new("zst-missing");
    let path = room.put(
        "a.zst",
        &[0x28, 0xB5, 0x2F, 0xFD, 0x20, 0x00, 0x01, 0x00, 0x00],
    );
    assert!(open(&path, Format::Zst).is_err());
    assert!(encoder(Vec::new(), Format::Zst, Level::NORMAL, 1, None).is_err());
    assert_eq!(identify(&path), Some(Format::Zst));
}

fn cut(room: &Room, name: &str, bytes: &[u8], size: usize) -> PathBuf {
    let whole = room.0.join(name);
    for (i, chunk) in bytes.chunks(size).enumerate() {
        std::fs::write(arca_core::volumes::part_path(&whole, i as u32 + 1), chunk).unwrap();
    }
    whole
}

#[test]
fn split_streams_read_as_the_whole_file() {
    let room = Room::new("split");
    let data = sample(200_000);
    for format in single_formats() {
        let packed = compress(format, &data, 1, Some(data.len() as u64));
        let name = format!("notes.txt.{}", format.extension());
        let whole = cut(&room, &name, &packed, packed.len() / 3 + 1);
        let second = arca_core::volumes::part_path(&whole, 2);
        assert_eq!(identify(&second), Some(format));
        let mut out = Vec::new();
        open(&second, format)
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out, data);
        let e = entry(&second, format).unwrap();
        assert_eq!(e.name, "notes.txt");
        assert_eq!(e.compressed_size, packed.len() as u64);
        assert_eq!(output_name(&second), "notes.txt");
    }
}

#[test]
fn a_split_tar_stream_is_still_a_tar() {
    let room = Room::new("split-tar");
    let mut tar = arca_tar::TarWriter::new(Vec::new());
    let data = sample(50_000);
    tar.add("a.bin", data.len() as u64, 0, 0o644, &data[..])
        .unwrap();
    let tar = tar.finish().unwrap();
    let packed = compress(Format::TarXz, &tar, 1, None);
    let whole = cut(&room, "b.tar.xz", &packed, 1000);
    assert_eq!(
        identify(&arca_core::volumes::part_path(&whole, 1)),
        Some(Format::TarXz)
    );
    let unnamed = cut(&room, "b.bin", &packed, 1000);
    assert_eq!(
        identify(&arca_core::volumes::part_path(&unnamed, 1)),
        Some(Format::TarXz)
    );
    assert_eq!(identify(Path::new("x.rar.001")), None);
    assert_eq!(identify(Path::new("x.iso.001")), None);
}

#[test]
fn a_broken_split_stream_fails_without_panicking() {
    let room = Room::new("split-broken");
    let data = sample(100_000);
    let packed = compress(Format::Xz, &data, 1, Some(data.len() as u64));
    let size = packed.len() / 4 + 1;
    let whole = cut(&room, "c.xz", &packed, size);
    let first = arca_core::volumes::part_path(&whole, 1);
    let last = arca_core::volumes::part_path(&whole, 4);
    let full = std::fs::read(&last).unwrap();
    std::fs::write(&last, &full[..full.len() / 2]).unwrap();
    let mut out = Vec::new();
    assert!(open(&first, Format::Xz)
        .and_then(|mut r| r.read_to_end(&mut out).map_err(Into::into))
        .is_err());
    std::fs::write(&last, &full).unwrap();
    std::fs::remove_file(arca_core::volumes::part_path(&whole, 2)).unwrap();
    let error = open(&first, Format::Xz).err().unwrap().to_string();
    assert!(error.contains("c.xz.002"), "{error}");
    assert!(entry(&first, Format::Xz).is_err());
    assert_eq!(identify(&first), Some(Format::Xz));
}
