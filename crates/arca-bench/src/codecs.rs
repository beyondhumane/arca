use crate::{Codec, LEVEL};
use arca_core::{Error, Format, Result};
use rayon::prelude::*;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

/// What one run produced, in the shape Arca would write it: a ZIP for the
/// codecs that live in one, a stream per chunk for XZ and bzip2, one RAR
/// archive for RAR.
pub(crate) enum Packed {
    Zip(Vec<u8>),
    Streams(Vec<Vec<u8>>),
    Rar(Vec<u8>),
}

impl Packed {
    pub(crate) fn len(&self) -> u64 {
        match self {
            Packed::Zip(bytes) | Packed::Rar(bytes) => bytes.len() as u64,
            Packed::Streams(streams) => streams.iter().map(|s| s.len() as u64).sum(),
        }
    }
}

fn zip_codec(codec: Codec) -> Option<arca_core::Codec> {
    match codec {
        Codec::Store => Some(arca_core::Codec::Store),
        Codec::Deflate => Some(arca_core::Codec::Deflate),
        Codec::Zstd => Some(arca_core::Codec::Zstd),
        _ => None,
    }
}

fn stream(codec: Codec, data: &[u8]) -> Result<Vec<u8>> {
    let format = if codec == Codec::Lzma2 {
        Format::Xz
    } else {
        Format::Bz2
    };
    let out = Vec::with_capacity(data.len() / 2);
    let mut encoder = arca_stream::encoder(out, format, LEVEL, 1, Some(data.len() as u64))?;
    encoder.write_all(data)?;
    encoder.finish()
}

fn unstream(codec: Codec, packed: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(size);
    if codec == Codec::Lzma2 {
        arca_xz::Decoder::new(packed).read_to_end(&mut out)?;
    } else {
        bzip2::read::MultiBzDecoder::new(packed).read_to_end(&mut out)?;
    }
    Ok(out)
}

/// Compresses `chunks` with `codec`, each chunk on its own as the threads of
/// the current rayon pool pick them up.
pub(crate) fn pack(codec: Codec, chunks: &[Arc<[u8]>]) -> Result<Packed> {
    if let Some(zip) = zip_codec(codec) {
        let blocks = chunks
            .par_iter()
            .map(|c| arca_zip::compress_block(c, zip, LEVEL))
            .collect::<Result<Vec<_>>>()?;
        let total: usize = blocks.iter().map(|b| b.0.len() + 128).sum();
        let mut writer = arca_zip::ZipWriter::new(Cursor::new(Vec::with_capacity(total)));
        for (i, ((block, method, crc), chunk)) in blocks.iter().zip(chunks).enumerate() {
            writer.add_compressed(
                &i.to_string(),
                block,
                *crc,
                chunk.len() as u64,
                *method,
                Some(0),
            )?;
        }
        return Ok(Packed::Zip(writer.finish()?.into_inner()));
    }
    match codec {
        Codec::Rar => Ok(Packed::Rar(arca_rar::pack_in_memory(chunks, LEVEL)?)),
        _ => Ok(Packed::Streams(
            chunks
                .par_iter()
                .map(|c| stream(codec, c))
                .collect::<Result<_>>()?,
        )),
    }
}

/// Decompresses what [`pack`] produced through the reader Arca opens that
/// kind of archive with, which checks every CRC on the way.
pub(crate) fn unpack(codec: Codec, packed: &Packed, sizes: &[usize]) -> Result<Vec<Vec<u8>>> {
    match packed {
        Packed::Zip(bytes) => {
            let archive = arca_zip::ZipArchive::open(Cursor::new(&bytes[..]))?;
            archive
                .entries()
                .par_iter()
                .map(|entry| {
                    let mut out = Vec::with_capacity(entry.size as usize);
                    arca_zip::extract_entry(&mut Cursor::new(&bytes[..]), entry, &mut out)?;
                    Ok(out)
                })
                .collect()
        }
        Packed::Streams(streams) => streams
            .par_iter()
            .zip(sizes)
            .map(|(s, &size)| unstream(codec, s, size))
            .collect(),
        Packed::Rar(bytes) => {
            let archive = arca_rar::MemoryArchive::open(bytes)?;
            (0..sizes.len())
                .into_par_iter()
                .map(|i| archive.member(i))
                .collect()
        }
    }
}

/// Whether this build has `codec` at all. Answered by asking for it, so the
/// answer is the one the writers themselves give.
pub(crate) fn missing(codec: Codec) -> Option<String> {
    let probe = match codec {
        Codec::Zstd => arca_zip::compress_block(b"", arca_core::Codec::Zstd, LEVEL).map(|_| ()),
        Codec::Rar => arca_rar::pack_in_memory(&[], LEVEL).map(|_| ()),
        _ => Ok(()),
    };
    match probe {
        Err(Error::Unsupported(why)) => Some(why),
        _ => None,
    }
}
