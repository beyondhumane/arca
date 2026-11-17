use super::*;
use std::io::Cursor as IoCursor;

fn bz(data: &[u8]) -> Vec<u8> {
    let mut e = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(9));
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn archive(compressed: &[u8], crc: u32, size: u64) -> Vec<u8> {
    let mut w = ZipWriter::new(IoCursor::new(Vec::new()));
    w.add_compressed("data.bin", compressed, crc, size, Method::Bzip2, None)
        .unwrap();
    w.finish().unwrap().into_inner()
}

fn sample() -> Vec<u8> {
    (0..200_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8 & 0x3F)
        .collect()
}

struct Counting(u64);

impl Write for Counting {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_bzip2_entry_reads_back() {
    let data = sample();
    let buf = archive(&bz(&data), crc32fast::hash(&data), data.len() as u64);
    let mut a = ZipArchive::open(IoCursor::new(buf)).unwrap();
    assert_eq!(a.entries()[0].method, Method::Bzip2);
    let mut out = Vec::new();
    a.extract_to(0, &mut out).unwrap();
    assert_eq!(out, data);
}

#[test]
fn an_entry_that_lies_about_its_size_stops_one_byte_past_it() {
    let bomb = vec![0u8; 8 << 20];
    let buf = archive(&bz(&bomb), crc32fast::hash(&bomb[..10]), 10);
    let mut a = ZipArchive::open(IoCursor::new(buf)).unwrap();
    let mut sink = Counting(0);
    assert!(a.extract_to(0, &mut sink).is_err());
    assert!(sink.0 <= 11, "{} bytes came out", sink.0);
}

#[test]
fn truncated_and_damaged_streams_fail_without_panicking() {
    let data = sample();
    let packed = bz(&data);
    let crc = crc32fast::hash(&data);
    for cut in (0..packed.len()).step_by(97).chain([packed.len() - 1]) {
        let buf = archive(&packed[..cut], crc, data.len() as u64);
        let mut a = ZipArchive::open(IoCursor::new(buf)).unwrap();
        assert!(a.extract_to(0, io::sink()).is_err(), "cut at {cut}");
    }
    for at in (0..packed.len()).step_by(13) {
        let mut bad = packed.clone();
        bad[at] ^= 0x10;
        let buf = archive(&bad, crc, data.len() as u64);
        let mut a = ZipArchive::open(IoCursor::new(buf)).unwrap();
        let mut out = Vec::new();
        if a.extract_to(0, &mut out).is_ok() {
            assert_eq!(out, data, "byte {at} changed the output silently");
        }
    }
    let buf = archive(b"BZh9 not really bzip2", crc, data.len() as u64);
    let mut a = ZipArchive::open(IoCursor::new(buf)).unwrap();
    assert!(a.extract_to(0, io::sink()).is_err());
}

#[test]
fn a_bzip2_entry_survives_a_rewrite_unchanged() {
    let data = sample();
    let packed = bz(&data);
    let mut w = ZipWriter::new(IoCursor::new(Vec::new()));
    w.add_compressed(
        "data.bin",
        &packed,
        crc32fast::hash(&data),
        data.len() as u64,
        Method::Bzip2,
        None,
    )
    .unwrap();
    w.add("gone.txt", &b"bye"[..], Codec::Store, Level::STORE, None)
        .unwrap();
    let buf = w.finish().unwrap().into_inner();
    let room = std::env::temp_dir().join(format!("arca-zip-bzip2-{}", std::process::id()));
    std::fs::create_dir_all(&room).unwrap();
    let (from, to) = (room.join("a.zip"), room.join("b.zip"));
    std::fs::write(&from, &buf).unwrap();
    remove_entries(&from, &to, None, &|e| e.name != "gone.txt", &|_, _, _| true).unwrap();
    let mut a = ZipArchive::open(std::fs::File::open(&to).unwrap()).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a.entries()[0].method, Method::Bzip2);
    assert_eq!(a.entries()[0].compressed_size, packed.len() as u64);
    let mut out = Vec::new();
    a.extract_to(0, &mut out).unwrap();
    assert_eq!(out, data);
    drop(a);
    std::fs::remove_dir_all(&room).unwrap();
}

#[test]
fn bzip2_is_still_refused_for_new_entries() {
    let mut hasher = crc32fast::Hasher::new();
    let result = compress_stream(
        &mut &b"x"[..],
        Vec::new(),
        Method::Bzip2,
        Level::NORMAL,
        &mut hasher,
        1,
    );
    assert!(matches!(result, Err(Error::Unsupported(_))));
}
