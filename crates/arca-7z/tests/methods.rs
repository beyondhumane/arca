use arca_7z::SevenZArchive;
use arca_core::{Error, Method};
use sevenz_rust2::{encoder_options::PpmdOptions, ArchiveEntry, ArchiveWriter, Password};
use std::io::Cursor;

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

/// One block of one coder holding one file named `x` of `size` bytes, with
/// no checksum anywhere: only the decoder stands between the bytes and the
/// caller. Both lengths have to fit the one-byte form of a 7z number.
fn single_block(coder: &[u8], size: usize, packed: &[u8]) -> Vec<u8> {
    assert!(size < 0x80 && packed.len() < 0x80);
    let mut header = vec![1, 4, 6, 0, 1, 9, packed.len() as u8, 0, 7, 11, 1, 0];
    header.extend_from_slice(coder);
    header.extend([
        12, size as u8, 0, 8, 0, 0, 5, 1, 17, 5, 0, b'x', 0, 0, 0, 0, 0,
    ]);
    with_header(&header, packed)
}

fn ppmd_coder(order: u8, memory: u32) -> Vec<u8> {
    let mut coder = vec![1, 0x23, 3, 4, 1, 5, order];
    coder.extend_from_slice(&memory.to_le_bytes());
    coder
}

const TEXT: &[u8] = b"the quick brown fox jumps over the lazy dog, then the fox naps; \
the dog wakes and the fox runs off.";

/// What the upstream encoder makes of `data` at order 6 with 16 MiB.
fn ppmd_packed(data: &[u8]) -> Vec<u8> {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![PpmdOptions::from_order_memory_size(6, 16 << 20).into()]);
    writer
        .push_archive_entry(ArchiveEntry::new_file("x"), Some(data))
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let archive =
        sevenz_rust2::Archive::read(&mut Cursor::new(&bytes), &Password::empty()).unwrap();
    let size = archive.pack_sizes()[0] as usize;
    bytes[32..32 + size].to_vec()
}

fn decode(data: Vec<u8>) -> arca_core::Result<Vec<u8>> {
    let mut archive = SevenZArchive::open(Cursor::new(data), None)?;
    let mut out = Vec::new();
    archive.extract(
        &[0],
        &mut |_, _, reader| {
            reader.read_to_end(&mut out)?;
            Ok(())
        },
        &mut |_| true,
    )?;
    Ok(out)
}

#[test]
fn ppmd_blocks_read_back_and_are_listed_as_ppmd() {
    let packed = ppmd_packed(TEXT);
    let data = single_block(&ppmd_coder(6, 16 << 20), TEXT.len(), &packed);
    let archive = SevenZArchive::open(Cursor::new(data.clone()), None).unwrap();
    assert_eq!(archive.entries()[0].method, Method::Ppmd);
    assert_eq!(Method::Ppmd.name(), "ppmd");
    assert!(Method::Ppmd.code().is_err());
    assert_eq!(decode(data).unwrap(), TEXT);
}

#[test]
fn malformed_ppmd_properties_are_errors() {
    let packed = ppmd_packed(TEXT);
    let short = vec![1, 0x23, 3, 4, 1, 4, 6, 0, 0, 0];
    for coder in [
        short,
        vec![1, 0x03, 3, 4, 1],
        ppmd_coder(1, 16 << 20),
        ppmd_coder(65, 16 << 20),
        ppmd_coder(6, 1024),
        ppmd_coder(6, u32::MAX),
    ] {
        let outcome = decode(single_block(&coder, TEXT.len(), &packed));
        assert!(outcome.is_err(), "{coder:?}");
    }
    let huge = decode(single_block(&ppmd_coder(6, 512 << 20), TEXT.len(), &packed));
    assert!(matches!(huge, Err(Error::Limit(_))), "{huge:?}");
}

// A PPMd stream has no end marker, so the size is all that stops it: a cut
// stream either fails or, when only the range coder's flush is missing,
// still gives exactly the original bytes. Nothing comes out longer.
#[test]
fn truncated_and_corrupt_ppmd_streams_never_panic_or_overrun() {
    let packed = ppmd_packed(TEXT);
    let coder = ppmd_coder(6, 16 << 20);
    for end in 0..packed.len() {
        if let Ok(out) = decode(single_block(&coder, TEXT.len(), &packed[..end])) {
            assert_eq!(out, TEXT, "prefix {end}");
        }
    }
    assert!(decode(single_block(
        &coder,
        TEXT.len(),
        &packed[..packed.len() / 2]
    ))
    .is_err());
    for index in 0..packed.len() {
        for flip in [0x01, 0x80, 0xff] {
            let mut bad = packed.clone();
            bad[index] ^= flip;
            if let Ok(out) = decode(single_block(&coder, TEXT.len(), &bad)) {
                assert_eq!(out.len(), TEXT.len());
            }
        }
    }
    // Declaring fewer bytes than were packed stops the decoder there.
    let shorter = decode(single_block(&coder, 10, &packed)).unwrap();
    assert_eq!(shorter, &TEXT[..10]);
}

// Read as one stream to its end, the way a rewrite drains a block, a PPMd
// block gives its declared size and not a byte more.
#[test]
fn a_ppmd_block_read_to_its_end_stops_at_its_size() {
    let packed = ppmd_packed(TEXT);
    let data = single_block(&ppmd_coder(6, 16 << 20), 10, &packed);
    let mut source = Cursor::new(data);
    let archive = sevenz_rust2::Archive::read(&mut source, &Password::empty()).unwrap();
    let password = Password::empty();
    let mut reader = sevenz_rust2::BlockDecoder::new(1, 0, &archive, &password, &mut source)
        .into_reader()
        .unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut out).unwrap();
    assert_eq!(out, &TEXT[..10]);
}
