use super::*;
use arca_core::Error;
use std::fs;
use std::io::Cursor;

const S: usize = 2048;

fn record(extent: u32, size: u32, flags: u8, ident: &[u8], su: &[u8]) -> Vec<u8> {
    let pad = usize::from(ident.len().is_multiple_of(2));
    let len = 33 + ident.len() + pad + su.len();
    let mut r = vec![0u8; len];
    r[0] = len as u8;
    r[2..6].copy_from_slice(&extent.to_le_bytes());
    r[6..10].copy_from_slice(&extent.to_be_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[14..18].copy_from_slice(&size.to_be_bytes());
    r[18..25].copy_from_slice(&[124, 1, 2, 3, 4, 5, 0]);
    r[25] = flags;
    r[28] = 1;
    r[31] = 1;
    r[32] = ident.len() as u8;
    r[33..33 + ident.len()].copy_from_slice(ident);
    r[33 + ident.len() + pad..].copy_from_slice(su);
    r
}

fn file(extent: u32, size: u32, ident: &[u8]) -> Vec<u8> {
    record(extent, size, 0, ident, &[])
}

fn dir(extent: u32, ident: &[u8]) -> Vec<u8> {
    record(extent, S as u32, 2, ident, &[])
}

fn sp() -> Vec<u8> {
    vec![b'S', b'P', 7, 1, 0xBE, 0xEF, 0]
}

fn nm(name: &[u8]) -> Vec<u8> {
    let mut e = vec![b'N', b'M', (5 + name.len()) as u8, 1, 0];
    e.extend_from_slice(name);
    e
}

fn px(mode: u32) -> Vec<u8> {
    let mut e = vec![b'P', b'X', 36, 1];
    e.extend_from_slice(&mode.to_le_bytes());
    e.extend_from_slice(&mode.to_be_bytes());
    e.resize(36, 0);
    e
}

fn ce(block: u32, offset: u32, len: u32) -> Vec<u8> {
    let mut e = vec![b'C', b'E', 28, 1];
    for v in [block, offset, len] {
        e.extend_from_slice(&v.to_le_bytes());
        e.extend_from_slice(&v.to_be_bytes());
    }
    e
}

fn directory(own: u32, parent: u32, su: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut d = record(own, S as u32, 2, &[0], su);
    d.extend(record(parent, S as u32, 2, &[1], &[]));
    for c in children {
        d.extend_from_slice(c);
    }
    d
}

struct Image(Vec<u8>);

impl Image {
    fn new(rock_ridge: bool, root: &[Vec<u8>]) -> Self {
        let mut image = Image(Vec::new());
        image.put(17, &{
            let mut t = vec![255u8];
            t.extend_from_slice(b"CD001\x01");
            t
        });
        let su = if rock_ridge { sp() } else { Vec::new() };
        image.put(18, &directory(18, 18, &su, root));
        image
    }

    fn put(&mut self, sector: usize, bytes: &[u8]) -> &mut Self {
        let end = (sector * S + bytes.len()).div_ceil(S) * S;
        if self.0.len() < end {
            self.0.resize(end.max(19 * S), 0);
        }
        self.0[sector * S..sector * S + bytes.len()].copy_from_slice(bytes);
        self
    }

    fn bytes(&mut self) -> Vec<u8> {
        let total = (self.0.len() / S) as u32;
        let mut pvd = vec![0u8; S];
        pvd[0] = 1;
        pvd[1..7].copy_from_slice(b"CD001\x01");
        pvd[80..84].copy_from_slice(&total.to_le_bytes());
        pvd[84..88].copy_from_slice(&total.to_be_bytes());
        pvd[128..130].copy_from_slice(&2048u16.to_le_bytes());
        pvd[130..132].copy_from_slice(&2048u16.to_be_bytes());
        pvd[156..190].copy_from_slice(&record(18, S as u32, 2, &[0], &[]));
        self.put(16, &pvd);
        self.0.clone()
    }
}

fn parse(data: &[u8]) -> arca_core::Result<parse::Parsed> {
    parse::parse(&mut Cursor::new(data), &|| true)
}

fn names(data: &[u8]) -> Vec<String> {
    parse(data)
        .unwrap()
        .entries
        .into_iter()
        .map(|e| e.name)
        .collect()
}

fn open(data: &[u8]) -> (tempfile::TempDir, IsoArchive) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.iso");
    fs::write(&path, data).unwrap();
    let archive = IsoArchive::open(&path).unwrap();
    (dir, archive)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn error(data: &[u8]) -> String {
    match parse(data) {
        Ok(_) => panic!("image was accepted"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn plain_names_lose_their_version_and_trailing_dot() {
    let data = Image::new(
        false,
        &[file(19, 5, b"README.TXT;1"), file(19, 0, b"NOEXT.;1")],
    )
    .put(19, b"hello")
    .bytes();
    assert_eq!(names(&data), ["README.TXT", "NOEXT"]);
    let (_dir, archive) = open(&data);
    assert_eq!(archive.read_entry(0).unwrap(), b"hello");
    let e = &archive.entries()[0];
    assert_eq!((e.size, e.compressed_size, e.crc32), (5, 5, None));
    assert_eq!(e.mtime, Some(1_704_164_645));
}

#[test]
fn rock_ridge_names_win_over_plain_ones() {
    let rr_file = record(19, 3, 0, b"LONGNA~1.TXT;1", &nm(b"A long name.txt"));
    let data = Image::new(true, &[rr_file]).put(19, b"abc").bytes();
    assert_eq!(names(&data), ["A long name.txt"]);
}

#[test]
fn files_split_into_several_extents_are_one_entry() {
    let data = Image::new(
        false,
        &[
            record(19, S as u32, 0x80, b"BIG.BIN;1", &[]),
            record(20, S as u32, 0x80, b"BIG.BIN;1", &[]),
            file(21, 7, b"BIG.BIN;1"),
        ],
    )
    .put(19, &[1; S])
    .put(20, &[2; S])
    .put(21, b"the end")
    .bytes();
    let (dir, archive) = open(&data);
    assert_eq!(archive.entries().len(), 1);
    assert_eq!(archive.entries()[0].size, 2 * S as u64 + 7);
    let mut expected = vec![1; S];
    expected.extend([2; S]);
    expected.extend(b"the end");
    assert_eq!(archive.read_entry(0).unwrap(), expected);
    let out = dir.path().join("out");
    archive
        .extract(&out, &[], &|_, _, _| true, &|_| Conflict::Cancel)
        .unwrap();
    assert_eq!(fs::read(out.join("BIG.BIN")).unwrap(), expected);
    archive.test(&[], &|_, _, _| true).unwrap();
}

#[test]
fn broken_multi_extent_chains_are_rejected() {
    let interrupted = Image::new(
        false,
        &[record(19, S as u32, 0x80, b"A;1", &[]), file(20, 1, b"B;1")],
    )
    .put(20, b"x")
    .bytes();
    assert!(error(&interrupted).contains("interrupted"));
    let unfinished = Image::new(false, &[record(19, S as u32, 0x80, b"A;1", &[])])
        .put(19, &[0; S])
        .bytes();
    assert!(error(&unfinished).contains("no final extent"));
}

#[test]
fn file_data_past_the_end_is_rejected() {
    let data = Image::new(false, &[file(500, 10, b"GHOST;1")]).bytes();
    assert!(error(&data).contains("past the end"));
}

#[test]
fn a_directory_pointing_back_at_an_ancestor_is_a_loop() {
    let data = Image::new(false, &[dir(19, b"A")])
        .put(19, &directory(19, 18, &[], &[dir(18, b"B")]))
        .bytes();
    assert!(error(&data).contains("loop"));
}

#[test]
fn directory_records_start_after_the_extended_attributes() {
    let mut a = dir(19, b"A");
    a[1] = 1;
    let data = Image::new(false, &[a])
        .put(20, &directory(20, 18, &[], &[file(21, 3, b"F;1")]))
        .put(21, b"abc")
        .bytes();
    assert_eq!(names(&data), ["A", "A/F"]);
}

#[test]
fn only_the_selected_files_are_tested() {
    let a = IsoArchive::open(&fixture("joliet.iso")).unwrap();
    let wanted: Vec<bool> = a.entries().iter().map(|e| e.name == "readme.txt").collect();
    assert_eq!(a.test(&wanted, &|_, _, _| true).unwrap(), 1);
    let files = a.entries().iter().filter(|e| !e.is_dir).count();
    assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), files);
}

#[test]
fn nesting_is_bounded() {
    let mut image = Image::new(false, &[dir(19, b"D")]);
    for level in 0..80u32 {
        image.put(
            19 + level as usize,
            &directory(19 + level, 18 + level, &[], &[dir(20 + level, b"D")]),
        );
    }
    image.put(99, &directory(99, 98, &[], &[]));
    assert!(error(&image.bytes()).contains("nested deeper"));
}

#[test]
fn names_that_could_escape_the_destination_are_rejected() {
    for bad in [
        &b".."[..],
        b"a/b",
        b"a\\b",
        b"x\0y",
        b"C:x",
        b"what?",
        b"tab\t",
        b"CON",
        b"nul.txt",
        b"COM1",
        b"trailing.",
        b"trailing ",
    ] {
        let data = Image::new(true, &[record(19, 0, 0, b"SAFE;1", &nm(bad))]).bytes();
        assert!(error(&data).contains("unsafe"), "{bad:?}");
    }
    let plain = Image::new(false, &[file(19, 0, b"A/B;1")]).bytes();
    assert!(error(&plain).contains("unsafe"));
}

#[test]
fn the_same_path_twice_is_rejected() {
    let data = Image::new(false, &[file(19, 0, b"A;1"), file(19, 0, b"A;2")]).bytes();
    assert!(error(&data).contains("duplicate"));
    let collision = Image::new(false, &[file(19, 0, b"A;1"), dir(20, b"A")])
        .put(20, &directory(20, 18, &[], &[]))
        .bytes();
    assert!(error(&collision).contains("duplicate"));
    let mut upper = nm(b"Readme");
    upper.extend(px(0o100_644));
    let case = Image::new(
        true,
        &[
            record(19, 0, 0, b"A;1", &nm(b"readme")),
            record(19, 0, 0, b"B;1", &upper),
        ],
    )
    .bytes();
    assert!(error(&case).contains("duplicate"));
}

#[cfg(unix)]
#[test]
fn a_destination_reached_through_a_link_is_trusted() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let base = tempfile::tempdir().unwrap();
    fs::create_dir(base.path().join("real")).unwrap();
    std::os::unix::fs::symlink(base.path().join("real"), base.path().join("alias")).unwrap();
    let dest = base.path().join("alias/new/out");
    a.extract(&dest, &[], &|_, _, _| true, &|_| Conflict::Overwrite)
        .unwrap();
    assert!(base.path().join("real/new/out/docs").is_dir());
}

#[cfg(unix)]
#[test]
fn links_already_in_the_destination_are_not_followed() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), out.path().join("docs")).unwrap();
    let result = a.extract(out.path(), &[], &|_, _, _| true, &|_| Conflict::Overwrite);
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("link") || error.contains("directory"),
        "{error}"
    );
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}

#[test]
fn links_and_devices_are_skipped_and_reported() {
    let mut link = nm(b"link");
    link.extend([b'S', b'L', 5, 1, 0]);
    let mut device = nm(b"null");
    device.extend(px(0o020_666));
    let mut fifo = nm(b"pipe");
    fifo.extend(px(0o010_644));
    let mut regular = nm(b"real.txt");
    regular.extend(px(0o100_644));
    let data = Image::new(
        true,
        &[
            record(19, 0, 0, b"LINK;1", &link),
            record(19, 0, 0, b"NULL;1", &device),
            record(19, 0, 0, b"PIPE;1", &fifo),
            record(19, 2, 0, b"REAL.TXT;1", &regular),
        ],
    )
    .put(19, b"ok")
    .bytes();
    let parsed = parse(&data).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].name, "real.txt");
    assert_eq!(parsed.notices.len(), 1);
    assert!(
        parsed.notices[0].contains("skipped 3"),
        "{}",
        parsed.notices[0]
    );
}

#[test]
fn rock_ridge_continuations_are_followed_and_bounded() {
    let data = Image::new(true, &[record(19, 0, 0, b"X;1", &ce(20, 100, 10))])
        .put(20, &[vec![0; 100], nm(b"named")].concat())
        .bytes();
    assert_eq!(names(&data), ["named"]);
    let looping = Image::new(true, &[record(19, 0, 0, b"X;1", &ce(20, 0, 28))])
        .put(20, &ce(20, 0, 28))
        .bytes();
    assert!(error(&looping).contains("continuation"));
    let outside = Image::new(true, &[record(19, 0, 0, b"X;1", &ce(9_000, 0, 28))]).bytes();
    assert!(error(&outside).contains("truncated"));
}

#[test]
fn records_crossing_a_sector_and_interleaving_are_refused() {
    let files: Vec<_> = (0..50)
        .map(|n| file(19, 0, format!("F{n:03};1").as_bytes()))
        .collect();
    let crossing = Image::new(false, &[record(19, 2 * S as u32, 2, b"A", &[])])
        .put(19, &directory(19, 18, &[], &files))
        .bytes();
    assert!(error(&crossing).contains("sector boundary"));
    let mut interleaved = file(19, 0, b"I;1");
    interleaved[26] = 1;
    interleaved[27] = 1;
    let data = Image::new(false, &[interleaved]).bytes();
    assert!(error(&data).contains("interleaved"));
}

#[test]
fn not_iso_and_udf_only_images_are_told_apart() {
    assert!(error(&vec![0; 40 * S]).contains("not an ISO 9660"));
    let mut udf = vec![0u8; 40 * S];
    udf[16 * S + 1..16 * S + 6].copy_from_slice(b"BEA01");
    assert!(matches!(parse(&udf), Err(Error::Unsupported(_))));
    let mut no_terminator = Image::new(false, &[]).bytes();
    no_terminator[17 * S..18 * S].copy_from_slice(&[0; S]);
    assert!(error(&no_terminator).contains("terminator"));
}

#[test]
fn every_truncation_of_a_real_image_is_an_error_not_a_panic() {
    for name in ["plain.iso", "joliet.iso", "rockridge.iso"] {
        let data = fs::read(fixture(name)).unwrap();
        for cut in 0..data.len() {
            assert!(parse(&data[..cut]).is_err(), "{name} cut at {cut}");
        }
    }
}

#[test]
fn random_damage_never_panics() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for name in ["plain.iso", "joliet.iso", "rockridge.iso"] {
        let clean = fs::read(fixture(name)).unwrap();
        for _ in 0..1500 {
            let mut data = clean.clone();
            for _ in 0..1 + next() % 16 {
                let at = 16 * S + (next() as usize) % (data.len() - 16 * S).min(8 * S);
                data[at] = next() as u8;
            }
            if let Ok(parsed) = parse(&data) {
                for e in &parsed.entries {
                    arca_core::safe_name(&e.name).unwrap();
                }
            }
        }
    }
}

#[test]
fn real_images_list_the_same_tree_whatever_the_naming() {
    let expected = [
        "docs",
        "docs/A long name with spaces.txt",
        "docs/año.txt",
        "docs/deep",
        "docs/deep/er",
        "docs/deep/er/leaf.bin",
        "empty",
        "readme.txt",
        "zero",
    ];
    for name in ["joliet.iso", "rockridge.iso"] {
        let a = IsoArchive::open(&fixture(name)).unwrap();
        let mut got: Vec<_> = a.entries().iter().map(|e| e.name.as_str()).collect();
        got.sort();
        assert_eq!(got, expected, "{name}");
    }
    let rr = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    assert_eq!(rr.notices().len(), 1);
    assert!(rr.notices()[0].contains("link-to-readme"));
    let plain = IsoArchive::open(&fixture("plain.iso")).unwrap();
    assert!(plain.entries().iter().any(|e| e.name == "README.TXT"));
    assert!(plain.notices().is_empty());
}

#[test]
fn extraction_writes_the_files_and_settles_conflicts() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let all = |c| a.extract(out.path(), &[], &|_, _, _| true, &|_| c);
    all(Conflict::Cancel).unwrap();
    let readme = out.path().join("readme.txt");
    assert_eq!(fs::read(&readme).unwrap(), b"hello from arca\n");
    assert_eq!(
        fs::read(out.path().join("docs/deep/er/leaf.bin")).unwrap(),
        b"nested\n"
    );
    assert!(out.path().join("empty").is_dir());
    assert!(!out.path().join("link-to-readme").exists());

    fs::write(&readme, b"mine").unwrap();
    assert!(matches!(all(Conflict::Cancel), Err(Error::Cancelled)));
    all(Conflict::Skip).unwrap();
    assert_eq!(fs::read(&readme).unwrap(), b"mine");
    all(Conflict::Rename).unwrap();
    assert_eq!(
        fs::read(out.path().join("readme (1).txt")).unwrap(),
        b"hello from arca\n"
    );
    all(Conflict::Overwrite).unwrap();
    assert_eq!(fs::read(&readme).unwrap(), b"hello from arca\n");
    let leftovers: Vec<_> = fs::read_dir(out.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_selection_extracts_only_what_was_picked() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let index = a
        .entries()
        .iter()
        .position(|e| e.name == "docs/deep/er/leaf.bin")
        .unwrap();
    let mut wanted = vec![false; a.entries().len()];
    wanted[index] = true;
    let out = tempfile::tempdir().unwrap();
    let written = a
        .extract(out.path(), &wanted, &|_, _, _| true, &|_| Conflict::Cancel)
        .unwrap();
    assert_eq!(written, 7);
    assert!(!out.path().join("readme.txt").exists());
    assert!(out.path().join("docs/deep/er/leaf.bin").exists());
}

#[test]
fn stopping_cancels() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let out = tempfile::tempdir().unwrap();
    assert!(matches!(
        a.extract(out.path(), &[], &|_, _, _| false, &|_| Conflict::Cancel),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        a.test(&[], &|_, _, _| false),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        IsoArchive::open_with_progress(&fixture("rockridge.iso"), &|_, _, _| false),
        Err(Error::Cancelled)
    ));
}

#[test]
fn directories_cannot_be_previewed() {
    let a = IsoArchive::open(&fixture("rockridge.iso")).unwrap();
    let docs = a.entries().iter().position(|e| e.name == "docs").unwrap();
    assert!(a.read_entry(docs).is_err());
    assert!(a.read_entry(10_000).is_err());
}
