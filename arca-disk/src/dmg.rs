use arca_core::{Error, Result};
use std::io::Read;
use std::path::Path;

use crate::device::{be32, be64, past_end, Device, Opened, Raw};
use crate::part::Partition;

const MAX_PLIST: u64 = 64 * 1024 * 1024;
const MAX_CHUNK: u64 = 64 * 1024 * 1024;
const MAX_CHUNKS: usize = 4 * 1024 * 1024;
const CHECK_PIECE: u64 = 1024 * 1024;
const CACHE: u64 = 128 * 1024 * 1024;
const REDECODE: u64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Codec {
    Zero,
    Raw,
    Adc,
    Zlib,
    Bzip2,
    Lzfse,
    Lzma,
}

#[derive(Clone, Copy, Debug)]
struct Chunk {
    start: u64,
    sectors: u64,
    codec: Codec,
    at: u64,
    len: u64,
}

struct Block {
    start: u64,
    sectors: u64,
    crc: Option<u32>,
}

pub(crate) fn open(path: &Path) -> Result<Opened> {
    let (dmg, partitions, notices) = parse(path)?;
    Ok(Opened {
        device: Box::new(dmg),
        partitions: Some(partitions),
        notices,
    })
}

fn parse(path: &Path) -> Result<(Dmg, Vec<Partition>, Vec<String>)> {
    let mut file = Raw::open(path)?;
    let len = file.len();
    if len >= 8 && file.bytes(0, 8)? == b"encrcdsa" {
        return Err(encrypted());
    }
    if len < 512 {
        return Err(Error::Format("not a DMG: the file is too short".into()));
    }
    let koly = file.bytes(len - 512, 512)?;
    if &koly[..4] != b"koly" {
        if &koly[504..] == b"cdsaencr" {
            return Err(encrypted());
        }
        return Err(Error::Format("not a DMG: its trailer is missing".into()));
    }
    if be32(&koly, 4) != 4 || be32(&koly, 8) != 512 {
        return Err(Error::Unsupported(format!(
            "DMG trailer version {}",
            be32(&koly, 4)
        )));
    }
    if be32(&koly, 60) > 1 {
        return Err(Error::Unsupported(
            "this DMG is split into segments; Arca opens single-file images".into(),
        ));
    }
    let fork = be64(&koly, 24);
    let (xml_at, xml_len) = (be64(&koly, 216), be64(&koly, 224));
    if xml_len == 0 {
        return Err(Error::Unsupported(
            "this DMG keeps its block map in a resource fork, an old layout Arca does not read"
                .into(),
        ));
    }
    if xml_len > MAX_PLIST {
        return Err(Error::Limit(format!(
            "a DMG property list of {xml_len} bytes"
        )));
    }
    let xml = file.bytes(xml_at, xml_len)?;
    let text = String::from_utf8_lossy(&xml);
    let mut chunks = Vec::new();
    let mut blocks = Vec::new();
    let mut partitions = Vec::new();
    let mut notices = Vec::new();
    let mut size = 0u64;
    let mut lzfse = false;
    let mut packed = 0u64;
    for (name, data) in blkx(&text)? {
        let block = mish(&data, fork, len, &mut chunks)?;
        if chunks.len() > MAX_CHUNKS {
            return Err(Error::Limit("a DMG of too many chunks".into()));
        }
        let end = (block.start + block.sectors).saturating_mul(512);
        size = size.max(end);
        if block.sectors > 0 && !name.contains("Apple_Free") {
            partitions.push(Partition {
                start: block.start * 512,
                len: block.sectors * 512,
            });
        }
        blocks.push(block);
    }
    for chunk in &chunks {
        lzfse |= chunk.codec == Codec::Lzfse;
        if !matches!(chunk.codec, Codec::Zero | Codec::Raw) {
            packed += chunk.sectors * 512;
        }
    }
    if lzfse {
        notices.push(
            "this DMG compresses with LZFSE, which Arca does not decode; the partitions it holds cannot be read"
                .into(),
        );
    }
    chunks.sort_by_key(|c| c.start);
    let dmg = Dmg {
        file,
        size,
        chunks,
        blocks,
        cached: Vec::new(),
        cache: CACHE,
        decoded: 0,
        allowed: 2 * MAX_CHUNK + REDECODE * packed,
    };
    Ok((dmg, partitions, notices))
}

fn encrypted() -> Error {
    Error::Unsupported("this DMG is encrypted; Arca does not open encrypted disk images".into())
}

fn mish(data: &[u8], fork: u64, file_len: u64, chunks: &mut Vec<Chunk>) -> Result<Block> {
    if data.len() < 204 || &data[..4] != b"mish" {
        return Err(Error::Format("a DMG block table is damaged".into()));
    }
    let start = be64(data, 8);
    let sectors = be64(data, 16);
    let offset = be64(data, 24);
    let count = be32(data, 200) as usize;
    if start.checked_add(sectors).is_none_or(|end| end > 1 << 50) {
        return Err(Error::Format("a DMG block lies past any disk".into()));
    }
    if data.len() < 204 + 40 * count {
        return Err(Error::Format("a DMG block table is cut short".into()));
    }
    let crc = (be32(data, 64) == 2 && be32(data, 68) == 32).then(|| be32(data, 72));
    for c in data[204..204 + 40 * count].as_chunks::<40>().0 {
        let codec = match be32(c, 0) {
            0 | 2 => Codec::Zero,
            1 => Codec::Raw,
            0x8000_0004 => Codec::Adc,
            0x8000_0005 => Codec::Zlib,
            0x8000_0006 => Codec::Bzip2,
            0x8000_0007 => Codec::Lzfse,
            0x8000_0008 => Codec::Lzma,
            0x7fff_fffe | 0xffff_ffff => continue,
            other => {
                return Err(Error::Unsupported(format!(
                    "DMG chunks of type {other:#010x}"
                )))
            }
        };
        let (first, n) = (be64(c, 8), be64(c, 16));
        let at = fork
            .checked_add(offset)
            .and_then(|a| a.checked_add(be64(c, 24)))
            .ok_or_else(|| Error::Format("a DMG chunk lies past the file".into()))?;
        let len = be64(c, 32);
        if first.checked_add(n).is_none_or(|end| end > sectors) {
            return Err(Error::Format("a DMG chunk lies outside its block".into()));
        }
        if codec != Codec::Zero && at.checked_add(len).is_none_or(|end| end > file_len) {
            return Err(Error::Format(
                "a DMG chunk lies past the end of the file".into(),
            ));
        }
        if codec == Codec::Raw && len < n * 512 {
            return Err(Error::Format(
                "a DMG chunk is shorter than its sectors".into(),
            ));
        }
        if !matches!(codec, Codec::Zero | Codec::Raw) && (n * 512 > MAX_CHUNK || len > MAX_CHUNK) {
            return Err(Error::Limit("a DMG chunk over 64 MiB".into()));
        }
        if n == 0 {
            continue;
        }
        chunks.push(Chunk {
            start: start + first,
            sectors: n,
            codec,
            at,
            len,
        });
    }
    Ok(Block {
        start,
        sectors,
        crc,
    })
}

/// Every partition in the `blkx` array of the property list: its name and
/// its decoded `mish` table.
fn blkx(text: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let missing = || Error::Format("the DMG property list has no block map".into());
    let key = text.find("<key>blkx</key>").ok_or_else(missing)?;
    let rest = &text[key..];
    let open = rest.find("<array>").ok_or_else(missing)?;
    let close = rest[open..].find("</array>").ok_or_else(missing)? + open;
    let mut out = Vec::new();
    for dict in rest[open..close].split("<dict>").skip(1) {
        let Some(data) = after(dict, "<key>Data</key>", "<data>", "</data>") else {
            continue;
        };
        let name = after(dict, "<key>Name</key>", "<string>", "</string>")
            .or_else(|| after(dict, "<key>CFName</key>", "<string>", "</string>"))
            .unwrap_or("");
        out.push((unescape(name), base64(data)?));
    }
    Ok(out)
}

fn after<'a>(text: &'a str, key: &str, open: &str, close: &str) -> Option<&'a str> {
    let at = text.find(key)? + key.len();
    let rest = text[at..].trim_start();
    let rest = rest.strip_prefix(open)?;
    Some(&rest[..rest.find(close)?])
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn base64(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => {
                return Err(Error::Format(
                    "the DMG property list holds bad base64".into(),
                ))
            }
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Apple Data Compression: literal runs and two copy forms reaching back
/// at most 64 KiB into what was already written.
pub(crate) fn adc(input: &[u8], want: usize) -> Result<Vec<u8>> {
    let bad = || Error::Format("damaged ADC data in a DMG".into());
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while i < input.len() && out.len() < want {
        let b = input[i];
        let (len, back) = if b & 0x80 != 0 {
            let n = usize::from(b & 0x7f) + 1;
            let lit = input.get(i + 1..i + 1 + n).ok_or_else(bad)?;
            out.extend_from_slice(lit);
            i += 1 + n;
            continue;
        } else if b & 0x40 != 0 {
            let hi = *input.get(i + 1).ok_or_else(bad)?;
            let lo = *input.get(i + 2).ok_or_else(bad)?;
            i += 3;
            (
                usize::from(b & 0x3f) + 4,
                usize::from(u16::from_be_bytes([hi, lo])),
            )
        } else {
            let lo = *input.get(i + 1).ok_or_else(bad)?;
            i += 2;
            (
                usize::from((b & 0x3c) >> 2) + 3,
                usize::from(b & 0x03) << 8 | usize::from(lo),
            )
        };
        if back >= out.len() {
            return Err(bad());
        }
        let from = out.len() - back - 1;
        for k in 0..len {
            let byte = out[from + k];
            out.push(byte);
        }
    }
    out.truncate(want);
    Ok(out)
}

struct Dmg {
    file: Raw,
    size: u64,
    chunks: Vec<Chunk>,
    blocks: Vec<Block>,
    /// Decoded chunks, the one used last at the end, within `cache` bytes.
    cached: Vec<(usize, Vec<u8>)>,
    cache: u64,
    decoded: u64,
    allowed: u64,
}

impl Dmg {
    fn unpack(&mut self, index: usize) -> Result<&[u8]> {
        match self.cached.iter().position(|(i, _)| *i == index) {
            Some(k) => {
                let hit = self.cached.remove(k);
                self.cached.push(hit);
            }
            None => {
                let out = self.decode(index)?;
                let mut held = out.len() as u64;
                held += self.cached.iter().map(|(_, b)| b.len() as u64).sum::<u64>();
                while held > self.cache && !self.cached.is_empty() {
                    held -= self.cached.remove(0).1.len() as u64;
                }
                self.cached.push((index, out));
            }
        }
        Ok(self.cached.last().map_or(&[][..], |(_, b)| b))
    }

    fn decode(&mut self, index: usize) -> Result<Vec<u8>> {
        let c = self.chunks[index];
        let want = (c.sectors * 512) as usize;
        self.decoded = self.decoded.saturating_add(want as u64);
        if self.decoded > self.allowed {
            return Err(Error::Limit(
                "reading this DMG decodes its chunks over and over, more than four times the disk it holds"
                    .into(),
            ));
        }
        let packed = self.file.bytes(c.at, c.len)?;
        let damaged = |e: std::io::Error| Error::Format(format!("a damaged DMG chunk: {e}"));
        let mut out = Vec::with_capacity(want);
        match c.codec {
            Codec::Adc => out = adc(&packed, want)?,
            Codec::Zlib => {
                flate2::read::ZlibDecoder::new(&packed[..])
                    .take(want as u64)
                    .read_to_end(&mut out)
                    .map_err(damaged)?;
            }
            Codec::Bzip2 => {
                bzip2::read::BzDecoder::new(&packed[..])
                    .take(want as u64)
                    .read_to_end(&mut out)
                    .map_err(damaged)?;
            }
            Codec::Lzma if packed.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) => {
                arca_xz::Decoder::new(&packed[..])
                    .take(want as u64)
                    .read_to_end(&mut out)
                    .map_err(damaged)?;
            }
            Codec::Lzma => {
                return Err(Error::Unsupported(
                    "DMG chunks compressed with raw LZMA".into(),
                ))
            }
            Codec::Lzfse => {
                return Err(Error::Unsupported(
                    "DMG chunks compressed with LZFSE; Arca has no LZFSE decoder it trusts with untrusted data"
                        .into(),
                ))
            }
            Codec::Zero | Codec::Raw => {}
        }
        if out.len() != want {
            return Err(Error::Format(
                "a DMG chunk decodes to fewer bytes than it holds".into(),
            ));
        }
        Ok(out)
    }
}

impl Device for Dmg {
    fn len(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        let end = pos.checked_add(buf.len() as u64).ok_or_else(past_end)?;
        if end > self.size {
            return Err(past_end());
        }
        let mut done = 0usize;
        while done < buf.len() {
            let at = pos + done as u64;
            let sector = at / 512;
            let i = self.chunks.partition_point(|c| c.start <= sector);
            let inside = i
                .checked_sub(1)
                .filter(|&i| sector < self.chunks[i].start + self.chunks[i].sectors);
            let Some(i) = inside else {
                let next = self
                    .chunks
                    .get(i)
                    .map_or(end, |c| c.start.saturating_mul(512).min(end));
                let n = (next - at) as usize;
                buf[done..done + n].fill(0);
                done += n;
                continue;
            };
            let c = self.chunks[i];
            let within = at - c.start * 512;
            let n = ((c.sectors * 512 - within) as usize).min(buf.len() - done);
            let part = &mut buf[done..done + n];
            match c.codec {
                Codec::Zero => part.fill(0),
                Codec::Raw => self.file.read_at(c.at + within, part)?,
                _ => {
                    let data = self.unpack(i)?;
                    let from = within as usize;
                    part.copy_from_slice(&data[from..from + n]);
                }
            }
            done += n;
        }
        Ok(())
    }

    /// Every block's CRC-32 over the data it holds once decoded, as 7-Zip
    /// checks it.
    fn check(&mut self, carry_on: &dyn Fn() -> bool) -> Result<()> {
        let mut buf = vec![0u8; CHECK_PIECE as usize];
        for b in 0..self.blocks.len() {
            let Some(expected) = self.blocks[b].crc else {
                continue;
            };
            let (start, len) = (self.blocks[b].start * 512, self.blocks[b].sectors * 512);
            let mut hasher = crc32fast::Hasher::new();
            let mut at = 0;
            while at < len {
                if !carry_on() {
                    return Err(Error::Cancelled);
                }
                let n = (len - at).min(CHECK_PIECE) as usize;
                self.read_at(start + at, &mut buf[..n])?;
                hasher.update(&buf[..n]);
                at += n as u64;
            }
            let found = hasher.finalize();
            if found != expected {
                return Err(Error::Integrity {
                    name: format!("DMG block {b}"),
                    expected,
                    found,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: u64 = 64 * 512;

    fn disk() -> Vec<u8> {
        (0..32 * CHUNK as u32)
            .map(|i| (i / 512 % 251) as u8 ^ (i % 7) as u8 | 1)
            .collect()
    }

    fn opened(disk: &[u8]) -> (tempfile::TempDir, Dmg) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disk.dmg");
        std::fs::write(&path, crate::tests::containers::dmg(&[("disk", disk)])).unwrap();
        let (dmg, _, _) = parse(&path).unwrap();
        (dir, dmg)
    }

    fn read(dmg: &mut Dmg, at: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        dmg.read_at(at, &mut buf)?;
        Ok(buf)
    }

    #[test]
    fn reads_that_alternate_between_chunks_decode_each_once() {
        let disk = disk();
        let (_dir, mut dmg) = opened(&disk);
        for turn in 0..200u64 {
            let at = (turn % 2) * CHUNK + turn % 50 * 512;
            let got = read(&mut dmg, at, 512).unwrap();
            assert_eq!(got, disk[at as usize..at as usize + 512]);
        }
        assert_eq!(dmg.decoded, 2 * CHUNK);
        let mut all = vec![0u8; disk.len()];
        dmg.read_at(0, &mut all).unwrap();
        assert_eq!(all, disk);
        assert_eq!(dmg.decoded, 24 * CHUNK);
    }

    #[test]
    fn a_small_cache_still_reads_right_and_evicts_the_oldest() {
        let disk = disk();
        let (_dir, mut dmg) = opened(&disk);
        dmg.cache = 2 * CHUNK;
        for turn in 0..30u64 {
            let at = (turn % 3) * CHUNK + 100;
            assert_eq!(
                read(&mut dmg, at, 700).unwrap(),
                disk[at as usize..at as usize + 700]
            );
            assert!(dmg.cached.len() <= 2);
        }
        assert_eq!(dmg.decoded, 30 * CHUNK);
    }

    #[test]
    fn decoding_the_same_chunks_over_and_over_stops() {
        let disk = disk();
        let (_dir, mut dmg) = opened(&disk);
        assert_eq!(dmg.allowed, 2 * MAX_CHUNK + REDECODE * 24 * CHUNK);
        dmg.cache = 2 * CHUNK;
        dmg.allowed = 10 * CHUNK;
        let stopped = (0..100u64).find_map(|turn| read(&mut dmg, (turn % 3) * CHUNK, 512).err());
        assert!(matches!(stopped, Some(Error::Limit(_))), "{stopped:?}");
        assert!(dmg.decoded <= 11 * CHUNK);
    }
}
