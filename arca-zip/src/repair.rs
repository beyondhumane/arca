use super::*;
use std::collections::{HashMap, HashSet};

const CHUNK: usize = 1 << 20;
const MAX_HITS: usize = 1 << 22;
const MAX_REPORTED: usize = 10_000;

pub struct Recovered {
    pub name: String,
    pub offset: u64,
    pub verified: bool,
}

pub struct Skipped {
    pub offset: u64,
    pub name: Option<String>,
    pub reason: String,
}

#[derive(Default)]
pub struct Repaired {
    pub recovered: Vec<Recovered>,
    pub skipped: Vec<Skipped>,
    pub unreported: u64,
}

struct Descriptor {
    crc: u32,
    comp: u64,
    size: u64,
    end: u64,
    wide: bool,
}

struct Budget {
    read: u64,
    read_cap: u64,
    inflated: u64,
    inflated_cap: u64,
}

pub fn repair<R: Read + Seek, W: Write + Seek>(source: R, out: W) -> Result<(W, Repaired)> {
    repair_with(source, out, &|_, _, _| true)
}

/// [`repair`], telling `notify` how many MiB of the file it has got through,
/// of how many, before each header it looks at. False stops it with
/// [`Error::Cancelled`].
pub fn repair_with<R: Read + Seek, W: Write + Seek>(
    mut source: R,
    out: W,
    notify: &(dyn Fn(usize, usize, &str) -> bool + Sync),
) -> Result<(W, Repaired)> {
    let total = source.seek(SeekFrom::End(0))?;
    let mib = |bytes: u64| usize::try_from(bytes >> 20).unwrap_or(usize::MAX);
    let mut w = ZipWriter::new(out);
    let mut report = Repaired::default();
    let mut index: Option<Index> = None;
    // A candidate that fails is passed by one byte, so a hostile file can
    // offer one header per byte, each claiming the rest of the file. What
    // failed checks may read and inflate is capped at a few times the file
    // itself, which a sound archive never comes near.
    let mut budget = Budget {
        read: 0,
        read_cap: total.saturating_mul(4).saturating_add(64 << 20),
        inflated: 0,
        inflated_cap: total
            .saturating_mul(limits::MAX_RATIO)
            .saturating_add(1 << 30),
    };
    let mut at = 0u64;
    let mut seen = 0u64;

    while let Some(pos) = next_header(&mut source, at, total)? {
        let last = report.recovered.last().map_or("", |r| r.name.as_str());
        if !notify(mib(pos), mib(total).max(1), last) {
            return Err(Error::Cancelled);
        }
        seen += 1;
        if seen > limits::MAX_ENTRIES {
            return Err(Error::Limit(format!(
                "more than {} local headers",
                limits::MAX_ENTRIES
            )));
        }
        match candidate(&mut source, pos, total, &mut index, &mut budget) {
            Ok((e, k, end, verified)) => {
                w.copy_raw(&mut source, &e, &k, None)?;
                report.recovered.push(Recovered {
                    name: e.name,
                    offset: pos,
                    verified,
                });
                at = end;
            }
            Err((name, reason)) => {
                if report.skipped.len() < MAX_REPORTED {
                    report.skipped.push(Skipped {
                        offset: pos,
                        name,
                        reason,
                    });
                } else {
                    report.unreported += 1;
                }
                at = pos + 1;
            }
        }
    }
    Ok((w.finish()?, report))
}

type Rejection = (Option<String>, String);
type Index = std::result::Result<HashMap<u64, Vec<Descriptor>>, String>;

fn candidate<R: Read + Seek>(
    src: &mut R,
    pos: u64,
    total: u64,
    index: &mut Option<Index>,
    budget: &mut Budget,
) -> std::result::Result<(Entry, Kept, u64, bool), Rejection> {
    let fail = |name: Option<&str>, reason: String| (name.map(str::to_owned), reason);
    let io_fail = |e: io::Error| (None, format!("unreadable local header: {e}"));

    if total - pos < LFH_FIXED as u64 {
        return Err(fail(None, "local header cut short".into()));
    }
    src.seek(SeekFrom::Start(pos)).map_err(io_fail)?;
    let mut lfh = [0u8; LFH_FIXED];
    src.read_exact(&mut lfh).map_err(io_fail)?;
    let word = |at: usize| u16::from_le_bytes([lfh[at], lfh[at + 1]]);
    let long = |at: usize| u32::from_le_bytes([lfh[at], lfh[at + 1], lfh[at + 2], lfh[at + 3]]);
    let (version, flags) = (word(4), word(6));
    let crc = long(14);
    let (comp32, size32) = (long(18), long(22));
    let (n_len, x_len) = (word(26) as usize, word(28) as usize);

    let data_start = pos + (LFH_FIXED + n_len + x_len) as u64;
    if data_start > total {
        return Err(fail(None, "name and extra field run past the end".into()));
    }
    let mut tail = vec![0u8; n_len + x_len];
    src.read_exact(&mut tail).map_err(io_fail)?;
    let local_extra = &tail[n_len..];

    let mut c = Vec::with_capacity(CD_FIXED + tail.len());
    let extra = strip_extra(local_extra, &[EXTRA_ZIP64]);
    let name = &tail[..n_len];
    let external: u32 = if name.ends_with(b"/") { 0x10 } else { 0 };
    c.extend_from_slice(&SIG_CD.to_le_bytes());
    c.extend_from_slice(&(version & 0xFF).to_le_bytes());
    c.extend_from_slice(&lfh[4..18]);
    c.extend_from_slice(&[0u8; 8]);
    c.extend_from_slice(&(n_len as u16).to_le_bytes());
    c.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    c.extend_from_slice(&[0u8; 6]);
    c.extend_from_slice(&external.to_le_bytes());
    c.extend_from_slice(&0u32.to_le_bytes());
    c.extend_from_slice(name);
    c.extend_from_slice(&extra);
    let (mut e, k) = read_central(&mut Cursor::new(&c)).map_err(|e| fail(None, e.to_string()))?;
    let named = Some(e.name.as_str());
    e.offset = pos;

    if flags & 8 == 0 {
        let (comp, size) = if comp32 == u32::MAX || size32 == u32::MAX {
            zip64_sizes(local_extra)
                .ok_or_else(|| fail(named, "Zip64 sizes missing from the local header".into()))?
        } else {
            (comp32 as u64, size32 as u64)
        };
        let end = data_start
            .checked_add(comp)
            .filter(|&end| end <= total)
            .ok_or_else(|| fail(named, "the data runs past the end of the file".into()))?;
        e.crc32 = Some(crc);
        e.compressed_size = comp;
        e.size = size;
        let verified = check(src, &e, budget).map_err(|r| fail(named, r))?;
        return Ok((e, k, end, verified));
    }

    let index = index
        .get_or_insert_with(|| descriptor_index(src, total).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|r| fail(named, r.clone()))?;
    let wide = has_extra(local_extra, EXTRA_ZIP64);
    let mut found: Vec<&Descriptor> = index
        .get(&data_start)
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    found.sort_by_key(|d| (d.wide != wide, d.end));
    let mut last = "no data descriptor matches where its data starts".to_owned();
    for d in found {
        e.crc32 = Some(d.crc);
        e.compressed_size = d.comp;
        e.size = d.size;
        match check(src, &e, budget) {
            Ok(verified) => return Ok((e, k, d.end, verified)),
            Err(r) => last = r,
        }
    }
    Err(fail(named, last))
}

fn zip64_sizes(extra: &[u8]) -> Option<(u64, u64)> {
    let mut x = Cursor::new(extra);
    while x.remaining() >= 4 {
        let id = x.u16le("extra field id").ok()?;
        let len = x.u16le("extra field size").ok()? as usize;
        let body = x.bytes(len, "extra field").ok()?;
        if id == EXTRA_ZIP64 {
            let mut z = Cursor::new(body);
            let size = z.u64le("uncompressed size").ok()?;
            let comp = z.u64le("compressed size").ok()?;
            return Some((comp, size));
        }
    }
    None
}

// An encrypted entry cannot be opened without its password, so all that can
// be said for it is that its header agrees with itself and its bytes are all
// there. Everything else is decompressed and its checksum compared.
fn check<R: Read + Seek>(
    src: &mut R,
    e: &Entry,
    budget: &mut Budget,
) -> std::result::Result<bool, String> {
    if e.encrypted {
        let (_, crypt) = open_data(src, e).map_err(|e| e.to_string())?;
        let least = match crypt {
            Crypt::Aes { strength } => {
                aes::overhead(strength).ok_or_else(|| format!("AES strength {strength}"))? as u64
            }
            Crypt::Zip { .. } => zipcrypto::HEADER as u64,
            Crypt::None => 0,
        };
        if e.compressed_size < least {
            return Err("shorter than its own encryption header".into());
        }
        return Ok(false);
    }
    // Accepted entries never overlap, so what they cost is bounded by the file;
    // only the work spent on candidates that fail is counted.
    if budget.read > budget.read_cap || budget.inflated > budget.inflated_cap {
        return Err("not checked: the scan has already read too much of this file".into());
    }
    extract_entry_with(src, e, io::sink(), None).map_err(|err| {
        budget.read = budget.read.saturating_add(e.compressed_size);
        budget.inflated = budget.inflated.saturating_add(e.size);
        err.to_string()
    })?;
    Ok(true)
}

// The next header is nearly always right where the last entry ended, so the
// window starts small and only grows while nothing turns up: reading a whole
// chunk per entry would read the file once for every megabyte of entries.
fn next_header<R: Read + Seek>(src: &mut R, from: u64, total: u64) -> Result<Option<u64>> {
    let mut found = None;
    scan(src, from, total, &[SIG_LFH], 64, |p, _| {
        found = Some(p);
        Ok(false)
    })?;
    Ok(found)
}

fn scan<R: Read + Seek>(
    src: &mut R,
    from: u64,
    total: u64,
    signatures: &[u32],
    first_window: usize,
    mut hit: impl FnMut(u64, u32) -> Result<bool>,
) -> Result<()> {
    let mut window = first_window.clamp(4, CHUNK + 3);
    let mut buf = vec![0u8; window];
    let mut at = from;
    while total.saturating_sub(at) >= 4 {
        if buf.len() < window {
            buf.resize(window, 0);
        }
        let want = ((total - at) as usize).min(window);
        window = (window * 2).min(CHUNK + 3);
        src.seek(SeekFrom::Start(at))?;
        src.read_exact(&mut buf[..want])?;
        for (i, w) in buf[..want].windows(4).enumerate() {
            if w[0] != b'P' || w[1] != b'K' {
                continue;
            }
            let sig = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
            if signatures.contains(&sig) && !hit(at + i as u64, sig)? {
                return Ok(());
            }
        }
        at += (want - 3) as u64;
    }
    Ok(())
}

// A streamed entry says how long it is only after its data, so every place a
// descriptor could sit is read once, up front, and filed under the offset its
// size field would put the start of the data at. One with a signature sits at
// its own signature; one without ends where the next header begins, or at the
// end of the file. Only offsets where a local header's data really begins are
// kept.
fn descriptor_index<R: Read + Seek>(
    src: &mut R,
    total: u64,
) -> Result<HashMap<u64, Vec<Descriptor>>> {
    let mut hits = Vec::new();
    scan(
        src,
        0,
        total,
        &[SIG_DESCRIPTOR, SIG_LFH, SIG_CD, SIG_EOCD],
        CHUNK + 3,
        |p, s| {
            if hits.len() >= MAX_HITS {
                return Err(Error::Limit(format!(
                    "more than {MAX_HITS} ZIP signatures to look through"
                )));
            }
            hits.push((p, s));
            Ok(true)
        },
    )?;

    let mut starts = HashSet::new();
    let mut fixed = [0u8; LFH_FIXED];
    for &(p, s) in &hits {
        if s != SIG_LFH || total - p < LFH_FIXED as u64 {
            continue;
        }
        src.seek(SeekFrom::Start(p))?;
        src.read_exact(&mut fixed)?;
        let n = u16::from_le_bytes([fixed[26], fixed[27]]) as u64;
        let x = u16::from_le_bytes([fixed[28], fixed[29]]) as u64;
        starts.insert(p + LFH_FIXED as u64 + n + x);
    }
    hits.push((total, 0));

    let mut index: HashMap<u64, Vec<Descriptor>> = HashMap::new();
    let mut d = [0u8; 20];
    for (p, s) in hits {
        let forms: [(Option<u64>, bool); 2] = if s == SIG_DESCRIPTOR {
            [(Some(p + 4), false), (Some(p + 4), true)]
        } else {
            [(p.checked_sub(12), false), (p.checked_sub(20), true)]
        };
        for (at, wide) in forms {
            let Some(at) = at else { continue };
            let len = if wide { 20 } else { 12 };
            if at + len as u64 > total {
                continue;
            }
            src.seek(SeekFrom::Start(at))?;
            src.read_exact(&mut d[..len])?;
            let crc = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
            let (comp, size) = if wide {
                (
                    u64::from_le_bytes([d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11]]),
                    u64::from_le_bytes([d[12], d[13], d[14], d[15], d[16], d[17], d[18], d[19]]),
                )
            } else {
                (
                    u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as u64,
                    u32::from_le_bytes([d[8], d[9], d[10], d[11]]) as u64,
                )
            };
            let (data_end, end) = if s == SIG_DESCRIPTOR {
                (p, at + len as u64)
            } else {
                (at, p)
            };
            let Some(start) = data_end.checked_sub(comp) else {
                continue;
            };
            if starts.contains(&start) {
                index.entry(start).or_default().push(Descriptor {
                    crc,
                    comp,
                    size,
                    end,
                    wide,
                });
            }
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor as IoCursor;

    fn made_by_arca(files: &[(&str, &[u8], Codec)]) -> Vec<u8> {
        let mut w = ZipWriter::new(IoCursor::new(Vec::new()));
        for (name, data, codec) in files {
            let level = if *codec == Codec::Store {
                Level::STORE
            } else {
                Level::NORMAL
            };
            w.add(name, *data, *codec, level, Some(1_700_000_000))
                .unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn without_directory(buf: &[u8]) -> Vec<u8> {
        let a = ZipArchive::open(IoCursor::new(buf.to_vec())).unwrap();
        let cd = a
            .entries()
            .iter()
            .map(|e| e.offset + LFH_FIXED as u64 + e.compressed_size)
            .max()
            .unwrap_or(0);
        let mut c = IoCursor::new(buf);
        let mut cut = cd as usize;
        let mut sig = [0u8; 4];
        while cut + 4 <= buf.len() {
            c.set_position(cut as u64);
            c.read_exact(&mut sig).unwrap();
            if u32::from_le_bytes(sig) == SIG_CD {
                break;
            }
            cut += 1;
        }
        buf[..cut].to_vec()
    }

    fn repaired(buf: &[u8]) -> (Vec<u8>, Repaired) {
        let (out, report) = repair(IoCursor::new(buf.to_vec()), IoCursor::new(Vec::new())).unwrap();
        (out.into_inner(), report)
    }

    fn contents(buf: &[u8], password: Option<&str>) -> Vec<(String, Vec<u8>)> {
        let mut a = ZipArchive::open(IoCursor::new(buf.to_vec())).unwrap();
        (0..a.len())
            .map(|i| {
                let mut v = Vec::new();
                a.extract_to_with(i, &mut v, password).unwrap();
                (a.entries()[i].name.clone(), v)
            })
            .collect()
    }

    fn raw_entry(buf: &[u8], e: &Entry) -> Vec<u8> {
        let start = e.offset as usize;
        let n = u16::from_le_bytes([buf[start + 26], buf[start + 27]]) as usize;
        let x = u16::from_le_bytes([buf[start + 28], buf[start + 29]]) as usize;
        buf[start..start + LFH_FIXED + n + x + e.compressed_size as usize].to_vec()
    }

    // Written the way a tool writing to a pipe does it: flag bit 3, nothing in
    // the local header's checksum and sizes, and a data descriptor after the
    // data, with or without its signature, in 32 or 64 bits.
    fn streamed(name: &str, plain: &[u8], deflate: bool, signed: bool, wide: bool) -> Vec<u8> {
        let body = if deflate {
            deflate_block(plain, Level::NORMAL).unwrap()
        } else {
            plain.to_vec()
        };
        let mut out = SIG_LFH.to_le_bytes().to_vec();
        out.extend_from_slice(&45u16.to_le_bytes());
        out.extend_from_slice(&0x0808u16.to_le_bytes());
        out.extend_from_slice(&(if deflate { 8u16 } else { 0 }).to_le_bytes());
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&(if wide { 20u16 } else { 0 }).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        if wide {
            out.extend_from_slice(&EXTRA_ZIP64.to_le_bytes());
            out.extend_from_slice(&16u16.to_le_bytes());
            out.extend_from_slice(&[0u8; 16]);
        }
        out.extend_from_slice(&body);
        if signed {
            out.extend_from_slice(&SIG_DESCRIPTOR.to_le_bytes());
        }
        out.extend_from_slice(&crc32fast::hash(plain).to_le_bytes());
        if wide {
            out.extend_from_slice(&(body.len() as u64).to_le_bytes());
            out.extend_from_slice(&(plain.len() as u64).to_le_bytes());
        } else {
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        }
        out
    }

    #[test]
    fn an_archive_without_its_central_directory_comes_back() {
        let text = b"recuperable ".repeat(300);
        let buf = made_by_arca(&[
            ("uno.txt", &text, Codec::Deflate),
            ("dir/", b"", Codec::Store),
            ("dir/dos.bin", b"\x00\x01\x02PK\x03\x04", Codec::Store),
            ("vacio.txt", b"", Codec::Deflate),
        ]);
        let broken = without_directory(&buf);
        assert!(ZipArchive::open(IoCursor::new(broken.clone())).is_err());

        let (out, report) = repaired(&broken);
        assert!(
            report.skipped.is_empty(),
            "{:?}",
            report.skipped.iter().map(|s| &s.reason).collect::<Vec<_>>()
        );
        assert_eq!(report.recovered.len(), 4);
        assert!(report.recovered.iter().all(|r| r.verified));
        assert_eq!(contents(&out, None), contents(&buf, None));
        let a = ZipArchive::open(IoCursor::new(out)).unwrap();
        assert!(a.entries()[1].is_dir);
    }

    #[test]
    fn a_damaged_entry_is_skipped_and_the_rest_are_copied_byte_for_byte() {
        let text = b"datos que importan ".repeat(200);
        let buf = made_by_arca(&[
            ("a.txt", &text, Codec::Deflate),
            ("b.txt", &text, Codec::Deflate),
            ("c.txt", &text, Codec::Store),
        ]);
        let original = ZipArchive::open(IoCursor::new(buf.clone())).unwrap();
        let entries = original.entries().to_vec();
        let mut broken = without_directory(&buf);
        let middle = entries[1].offset as usize + LFH_FIXED + 5 + EXTRA_Z64 + 20;
        broken[middle] ^= 0xFF;

        let (out, report) = repaired(&broken);
        let names: Vec<&str> = report.recovered.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "c.txt"]);
        assert_eq!(report.skipped[0].name.as_deref(), Some("b.txt"));
        assert_eq!(report.skipped[0].offset, entries[1].offset);

        let fixed = ZipArchive::open(IoCursor::new(out.clone())).unwrap();
        for (got, want) in fixed.entries().iter().zip([&entries[0], &entries[2]]) {
            assert_eq!(raw_entry(&out, got), raw_entry(&buf, want));
        }
    }

    #[test]
    fn streamed_entries_are_found_through_their_descriptors() {
        let text = b"por la tuberia ".repeat(100);
        let mut buf = Vec::new();
        buf.extend(streamed("firmado.txt", &text, true, true, false));
        buf.extend(streamed("guardado.txt", &text, false, true, false));
        buf.extend(streamed("sin-firma.txt", &text, true, false, false));
        buf.extend(streamed("ancho.txt", &text, true, true, true));
        buf.extend(streamed("ancho-guardado.txt", &text, false, false, true));
        buf.extend(streamed("ultimo.txt", b"", false, false, false));

        let (out, report) = repaired(&buf);
        assert!(
            report.skipped.is_empty(),
            "{:?}",
            report
                .skipped
                .iter()
                .map(|s| (&s.name, &s.reason))
                .collect::<Vec<_>>()
        );
        let got = contents(&out, None);
        assert_eq!(got.len(), 6);
        for (name, data) in &got[..5] {
            assert_eq!(data, &text, "{name}");
        }
        assert!(got[5].1.is_empty());
    }

    #[test]
    fn a_zip_stored_inside_an_entry_stays_inside_it() {
        let inner = made_by_arca(&[
            ("dentro-1.txt", b"one", Codec::Store),
            ("dentro-2.txt", b"two", Codec::Store),
        ]);
        let plain = without_directory(&made_by_arca(&[
            ("antes.txt", b"before", Codec::Store),
            ("interior.zip", &inner, Codec::Store),
            ("despues.txt", b"after", Codec::Store),
        ]));
        let mut piped = streamed("interior.zip", &inner, false, true, false);
        piped.extend(streamed("despues.txt", b"after", false, true, false));

        for buf in [plain, piped] {
            let (out, report) = repaired(&buf);
            let names: Vec<String> = report.recovered.iter().map(|r| r.name.clone()).collect();
            assert!(!names.iter().any(|n| n.starts_with("dentro")), "{names:?}");
            assert!(names.contains(&"interior.zip".to_string()));
            let got = contents(&out, None);
            let nested = got.iter().find(|(n, _)| n == "interior.zip").unwrap();
            assert_eq!(nested.1, inner);
        }
    }

    #[test]
    fn encrypted_entries_are_kept_without_being_opened() {
        let mut w = ZipWriter::new(IoCursor::new(Vec::new()));
        w.add_with_password(
            "secreto.txt",
            &b"clave"[..],
            Codec::Deflate,
            Level::NORMAL,
            None,
            Some("pw"),
        )
        .unwrap();
        w.add("claro.txt", &b"plain"[..], Codec::Store, Level::STORE, None)
            .unwrap();
        let buf = without_directory(&w.finish().unwrap().into_inner());

        let (out, report) = repaired(&buf);
        let verified: Vec<bool> = report.recovered.iter().map(|r| r.verified).collect();
        assert_eq!(verified, [false, true]);
        let got = contents(&out, Some("pw"));
        assert_eq!(got[0].1, b"clave");
        assert_eq!(got[1].1, b"plain");
    }

    #[test]
    fn a_truncated_archive_keeps_what_came_before_the_cut() {
        let text = b"hasta aqui ".repeat(100);
        let buf = made_by_arca(&[
            ("a.txt", &text, Codec::Deflate),
            ("b.txt", &text, Codec::Deflate),
        ]);
        let second = ZipArchive::open(IoCursor::new(buf.clone()))
            .unwrap()
            .entries()[1]
            .offset;
        let (out, report) = repaired(&buf[..second as usize + 50]);
        assert_eq!(report.recovered.len(), 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(contents(&out, None)[0].1, text);
    }

    #[test]
    fn nothing_to_recover_is_an_empty_report() {
        for buf in [Vec::new(), b"PK".to_vec(), vec![0u8; 5000]] {
            let (_, report) = repaired(&buf);
            assert!(report.recovered.is_empty());
        }
    }

    #[test]
    fn cut_or_garbled_input_does_not_panic() {
        let text = b"x".repeat(40);
        let mut buf = made_by_arca(&[("a.txt", &text, Codec::Deflate)]);
        buf.extend(streamed("b.txt", &text, true, true, false));
        buf.extend(streamed("c.txt", &text, false, false, true));
        for cut in 0..buf.len() {
            let _ = repair(
                IoCursor::new(buf[..cut].to_vec()),
                IoCursor::new(Vec::new()),
            );
        }
        for at in 0..buf.len() {
            for delta in [0x01u8, 0x80, 0xFF] {
                let mut bad = buf.clone();
                bad[at] ^= delta;
                let _ = repair(IoCursor::new(bad), IoCursor::new(Vec::new()));
            }
        }
        let mut noise: Vec<u8> = (0..20_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        for i in (0..noise.len() - 4).step_by(97) {
            let sig = [SIG_LFH, SIG_DESCRIPTOR, SIG_CD, SIG_EOCD][i % 4];
            noise[i..i + 4].copy_from_slice(&sig.to_le_bytes());
        }
        let _ = repair(IoCursor::new(noise), IoCursor::new(Vec::new()));
    }

    #[test]
    fn a_repair_can_be_stopped_and_hears_how_far_it_got() {
        let buf = without_directory(&made_by_arca(&[
            ("a.txt", b"uno", Codec::Store),
            ("b.txt", b"dos", Codec::Store),
        ]));
        let heard = std::sync::Mutex::new(Vec::new());
        let (_, report) = repair_with(
            IoCursor::new(buf.clone()),
            IoCursor::new(Vec::new()),
            &|done, total, name| {
                heard.lock().unwrap().push((done, total, name.to_owned()));
                true
            },
        )
        .unwrap();
        assert_eq!(report.recovered.len(), 2);
        let heard = heard.into_inner().unwrap();
        assert_eq!(heard.len(), 2);
        assert_eq!(heard[1], (0, 1, "a.txt".to_owned()));
        assert!(matches!(
            repair_with(IoCursor::new(buf), IoCursor::new(Vec::new()), &|_, _, _| {
                false
            }),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn only_failed_checks_spend_the_budget() {
        let buf = made_by_arca(&[("a.txt", b"abc", Codec::Store)]);
        let e = ZipArchive::open(IoCursor::new(buf.clone()))
            .unwrap()
            .entries()[0]
            .clone();
        let mut src = IoCursor::new(buf);
        let mut budget = Budget {
            read: 0,
            read_cap: 100,
            inflated: 0,
            inflated_cap: 100,
        };
        assert_eq!(check(&mut src, &e, &mut budget), Ok(true));
        assert_eq!((budget.read, budget.inflated), (0, 0));

        let mut lying = e.clone();
        lying.crc32 = Some(0);
        assert!(check(&mut src, &lying, &mut budget).is_err());
        assert_eq!((budget.read, budget.inflated), (3, 3));

        budget.read = 101;
        assert!(check(&mut src, &e, &mut budget).is_err());
    }

    struct Counted {
        inner: IoCursor<Vec<u8>>,
        read: std::rc::Rc<std::cell::Cell<u64>>,
    }

    impl Read for Counted {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.read.set(self.read.get() + n as u64);
            Ok(n)
        }
    }

    impl Seek for Counted {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    #[test]
    fn many_small_entries_are_read_about_once() {
        let names: Vec<String> = (0..3000).map(|i| format!("f{i}.txt")).collect();
        let files: Vec<(&str, &[u8], Codec)> = names
            .iter()
            .map(|n| (n.as_str(), &b"small"[..], Codec::Store))
            .collect();
        let buf = without_directory(&made_by_arca(&files));
        let read = std::rc::Rc::new(std::cell::Cell::new(0));
        let source = Counted {
            inner: IoCursor::new(buf.clone()),
            read: read.clone(),
        };
        let (_, report) = repair(source, IoCursor::new(Vec::new())).unwrap();
        assert_eq!(report.recovered.len(), 3000);
        assert!(
            read.get() < buf.len() as u64 * 6,
            "{} bytes read for a {} byte file",
            read.get(),
            buf.len()
        );
    }
}
