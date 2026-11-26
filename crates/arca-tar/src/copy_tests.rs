use super::*;
use crate::{checksum, octal, pax_record, ustar_header, write_octal, BLOCK, PAX_LIMIT};

fn sample() -> Vec<u8> {
    let mut w = TarWriter::new(Vec::new());
    w.add("dir/", 0, 1_600_000_000, 0o750, io::empty()).unwrap();
    w.add("dir/tool", 4, 1_600_000_001, 0o755, &b"run!"[..])
        .unwrap();
    let long = format!("{}/{}", "d".repeat(150), "f".repeat(120));
    w.add(&long, 3, 1_600_000_002, 0o644, &b"abc"[..]).unwrap();
    w.add_link("dir/sym", "tool", 1_600_000_003, b'2').unwrap();
    w.add_link("dir/hard", "dir/tool", 1_600_000_004, b'1')
        .unwrap();
    w.add("big.bin", 3000, 1_600_000_005, 0o600, &vec![7u8; 3000][..])
        .unwrap();
    w.finish().unwrap()
}

type Listed = (String, u32, i64, u8, Option<String>, Vec<u8>);

fn listed(tar: &[u8]) -> Vec<Listed> {
    let mut r = TarReader::new(tar);
    let mut out = Vec::new();
    while let Some(e) = r.next_entry().unwrap() {
        let mut data = Vec::new();
        r.copy_data(&e, &mut data).unwrap();
        out.push((
            e.entry.name.clone(),
            e.mode,
            e.entry.mtime.unwrap(),
            e.kind,
            e.link.clone(),
            data,
        ));
    }
    out
}

fn copy(tar: &[u8], name: &dyn Fn(&TarEntry) -> Option<String>) -> Result<Vec<u8>> {
    let mut r = TarReader::new(tar);
    let mut w = TarWriter::new(Vec::new());
    copy_entries(&mut r, &mut w, name, &mut |_, _| true)?;
    w.finish()
}

fn same(e: &TarEntry) -> Option<String> {
    Some(e.entry.name.clone())
}

#[test]
fn an_unchanged_copy_is_the_same_bytes() {
    let tar = sample();
    assert_eq!(copy(&tar, &same).unwrap(), tar);
}

#[test]
fn leaving_entries_out_keeps_the_others_whole() {
    let tar = sample();
    let out = copy(&tar, &|e| {
        (!e.entry.name.starts_with("big")).then(|| e.entry.name.clone())
    })
    .unwrap();
    let mut before = listed(&tar);
    before.retain(|e| e.0 != "big.bin");
    assert_eq!(listed(&out), before);
}

fn moved(name: &str) -> String {
    match name.strip_prefix("dir/") {
        Some(rest) => format!("other/{rest}"),
        None => name.to_string(),
    }
}

#[test]
fn a_rename_keeps_everything_but_the_name_and_hard_links_follow_it() {
    let tar = sample();
    let out = copy(&tar, &|e| Some(moved(&e.entry.name))).unwrap();
    let got = listed(&out);
    let want: Vec<Listed> = listed(&tar)
        .into_iter()
        .map(|mut e| {
            e.0 = moved(&e.0);
            if e.3 == b'1' {
                e.4 = Some("other/tool".into());
            }
            e
        })
        .collect();
    assert_eq!(got, want);
    assert_eq!(got[0].0, "other/");
    assert_eq!(got[4].4.as_deref(), Some("other/tool"));
    assert_eq!(got[3].4.as_deref(), Some("tool"));
}

fn owned_header(name: &str, magic: &[u8; 8]) -> [u8; BLOCK] {
    let mut h = [0u8; BLOCK];
    h[..name.len()].copy_from_slice(name.as_bytes());
    write_octal(&mut h[100..108], 0o4711);
    write_octal(&mut h[108..116], 1000);
    write_octal(&mut h[116..124], 100);
    write_octal(&mut h[124..136], 2);
    write_octal(&mut h[136..148], 1_234_567_890);
    h[156] = b'0';
    h[257..265].copy_from_slice(magic);
    h[265..270].copy_from_slice(b"alice");
    h[297..302].copy_from_slice(b"staff");
    if magic.starts_with(b"ustar  ") {
        h[345..357].copy_from_slice(b"01234567012\0");
    }
    seal(&mut h);
    h
}

fn one_entry(header: [u8; BLOCK], extra: &[u8]) -> Vec<u8> {
    let mut tar = extra.to_vec();
    tar.extend_from_slice(&header);
    tar.extend_from_slice(b"hi");
    tar.resize(tar.len().next_multiple_of(BLOCK), 0);
    tar.extend_from_slice(&[0u8; 2 * BLOCK]);
    tar
}

fn main_header(tar: &[u8]) -> [u8; BLOCK] {
    let mut r = TarReader::new(tar);
    r.keep_headers();
    let e = r.next_entry().unwrap().unwrap();
    let raw = e.headers.unwrap();
    raw[raw.len() - BLOCK..].try_into().unwrap()
}

#[test]
fn owners_and_gnu_fields_survive_a_rename_to_a_long_name() {
    for magic in [b"ustar\x0000", b"ustar  \0"] {
        let tar = one_entry(owned_header("a.txt", magic), &[]);
        let long = format!("{}.txt", "n".repeat(300));
        let out = copy(&tar, &|_| Some(long.clone())).unwrap();
        let (was, now) = (main_header(&tar), main_header(&out));
        assert_eq!(was[100..148], now[100..148]);
        assert_eq!(was[156..345], now[156..345]);
        if magic.starts_with(b"ustar  ") {
            assert_eq!(was[345..], now[345..]);
        }
        let got = listed(&out);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, long);
        assert_eq!((got[0].1, got[0].2), (0o4711, 1_234_567_890));
        assert_eq!(got[0].5, b"hi");
        assert_eq!(
            u64::from(checksum(&now)),
            octal(&now[148..156], "checksum").unwrap()
        );
    }
}

#[test]
fn other_pax_records_are_kept_and_the_old_name_is_not() {
    let mut records = pax_record("path", "old/long-name.txt");
    records.extend(pax_record("SCHILY.xattr.user.tag", "blue"));
    records.extend(pax_record("mtime", "1234567890.5"));
    let mut extra =
        ustar_header("PaxHeaders/x", "", "", records.len() as u64, 0, 0o644, b'x').to_vec();
    extra.extend_from_slice(&records);
    extra.resize(extra.len().next_multiple_of(BLOCK), 0);
    let tar = one_entry(owned_header("cut", b"ustar\x0000"), &extra);
    assert_eq!(listed(&tar)[0].0, "old/long-name.txt");
    let out = copy(&tar, &|_| Some("new.txt".into())).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("SCHILY.xattr.user.tag=blue"));
    assert!(text.contains("mtime=1234567890.5"));
    assert!(!text.contains("old/long-name"));
    let got = listed(&out);
    assert_eq!((got[0].0.as_str(), got[0].2), ("new.txt", 1_234_567_890));
}

#[test]
fn a_hard_link_to_an_entry_left_out_is_refused() {
    let tar = sample();
    let error = copy(&tar, &|e| {
        (e.entry.name != "dir/tool").then(|| e.entry.name.clone())
    })
    .unwrap_err();
    assert!(error.to_string().contains("hard link"), "{error}");
}

#[test]
fn a_stop_from_progress_cancels() {
    let tar = sample();
    let mut r = TarReader::new(&tar[..]);
    let mut w = TarWriter::new(Vec::new());
    let mut calls = 0;
    let result = copy_entries(&mut r, &mut w, &same, &mut |_, _| {
        calls += 1;
        calls < 3
    });
    assert!(matches!(result, Err(Error::Cancelled)));
}

#[test]
fn headers_past_the_limit_are_copied_from_their_fields() {
    let mut extra = Vec::new();
    let records = vec![b'#'; PAX_LIMIT as usize];
    for _ in 0..5 {
        extra.extend_from_slice(&ustar_header(
            "PaxHeaders/g",
            "",
            "",
            records.len() as u64,
            0,
            0o644,
            b'g',
        ));
        extra.extend_from_slice(&records);
    }
    let tar = one_entry(owned_header("a.txt", b"ustar\x0000"), &extra);
    let out = copy(&tar, &same).unwrap();
    assert!(out.len() < tar.len());
    let got = listed(&out);
    assert_eq!(
        (got[0].0.as_str(), got[0].1, got[0].5.as_slice()),
        ("a.txt", 0o4711, &b"hi"[..])
    );
}

#[test]
fn truncated_and_broken_archives_are_errors_not_panics() {
    let tar = sample();
    for cut in (0..tar.len()).step_by(97) {
        let _ = copy(&tar[..cut], &same);
        let _ = copy(&tar[..cut], &|e| Some(format!("x/{}", e.entry.name)));
    }
    for cut in [BLOCK + 1, 2 * BLOCK + 100, tar.len() - 3 * BLOCK] {
        assert!(copy(&tar[..cut], &same).is_err(), "{cut}");
    }
    for seed in 0u32..300 {
        let mut broken = tar.clone();
        let at = (seed as usize * 7919) % broken.len();
        broken[at] ^= 0x5a;
        let _ = copy(&broken, &same);
        let _ = copy(&broken, &|e| Some(moved(&e.entry.name)));
    }
}

#[test]
fn kept_headers_that_do_not_parse_are_errors() {
    assert!(segments(&[0u8; 100]).is_err());
    assert!(segments(&[]).is_err());
    let mut lone = ustar_header("PaxHeaders/x", "", "", 10, 0, 0o644, b'x').to_vec();
    assert!(segments(&lone).is_err());
    lone.extend_from_slice(&[0u8; BLOCK]);
    assert!(segments(&lone).is_err());
    let main = owned_header("a", b"ustar\x0000");
    let mut twice = main.to_vec();
    twice.extend_from_slice(&main);
    assert!(segments(&twice).is_err());
    assert!(renamed(&twice, "b", None).is_err());
    assert!(renamed(&main, "b", Some("")).is_err());
    assert!(renamed(&main, &"n".repeat(limits::MAX_NAME + 1), None).is_err());
}

fn raw_entry(name: &str, kind: u8, data: &[u8], link: &str) -> Vec<u8> {
    let size = data.len() as u64;
    let mut out = ustar_header(name, "", link, size, 1_600_000_000, 0o644, kind).to_vec();
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(BLOCK), 0);
    out
}

fn global() -> Vec<u8> {
    raw_entry(
        "pax_global_header",
        b'g',
        &pax_record("comment", "a commit"),
        "",
    )
}

fn ended(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut tar = parts.concat();
    tar.extend_from_slice(&[0; 2 * BLOCK]);
    tar
}

fn without(gone: &'static [&'static str]) -> impl Fn(&TarEntry) -> Option<String> {
    move |e| (!gone.contains(&e.entry.name.as_str())).then(|| e.entry.name.clone())
}

#[test]
fn the_root_entry_and_a_global_header_are_copied_as_they_were() {
    let root = raw_entry("./", b'5', b"", "");
    let a = raw_entry("./a", b'0', b"aaa", "");
    let b = raw_entry("./b", b'0', b"b", "");
    let tar = ended(&[global(), root.clone(), a.clone(), b.clone()]);
    assert_eq!(copy(&tar, &same).unwrap(), tar);
    let out = copy(&tar, &without(&["a"])).unwrap();
    assert_eq!(out, ended(&[global(), root.clone(), b]));
    let out = copy(&tar, &without(&["a", "b"])).unwrap();
    assert_eq!(out, ended(&[global(), root]));
}

#[test]
fn a_global_header_moves_to_the_next_entry_when_its_own_is_left_out() {
    let a = raw_entry("a", b'0', b"aaa", "");
    let b = raw_entry("b", b'0', b"b", "");
    let tar = ended(&[global(), a, b.clone()]);
    assert_eq!(copy(&tar, &without(&["a"])).unwrap(), ended(&[global(), b]));
    assert_eq!(
        copy(&tar, &without(&["a", "b"])).unwrap(),
        ended(&[global()])
    );
    let only = ended(&[global()]);
    let mut r = TarReader::new(&only[..]);
    assert!(r.next_entry().unwrap().is_none());
}

#[test]
fn hard_link_targets_are_matched_once_resolved() {
    for target in ["dir//tool", "./dir/x/../tool", "dir/./tool/"] {
        let tar = ended(&[
            raw_entry("dir/tool", b'0', b"run", ""),
            raw_entry("hard", b'1', b"", target),
        ]);
        let left_out = copy(&tar, &without(&["dir/tool"]));
        assert!(
            matches!(left_out, Err(Error::Unsupported(_))),
            "{target}: {:?}",
            left_out.err()
        );
        let out = copy(&tar, &|e| Some(e.entry.name.replace("dir/", "other/"))).unwrap();
        let hard = listed(&out).into_iter().find(|e| e.0 == "hard").unwrap();
        assert_eq!(hard.4.as_deref(), Some("other/tool"), "{target}");
    }
}

#[test]
fn damaged_copies_with_a_root_entry_never_panic() {
    let tar = ended(&[
        global(),
        raw_entry("./", b'5', b"", ""),
        raw_entry("./a", b'0', b"aaa", ""),
        raw_entry("b", b'1', b"", "./a"),
    ]);
    for cut in (0..tar.len()).step_by(97) {
        let _ = copy(&tar[..cut], &same);
        let _ = copy(&tar[..cut], &without(&["a"]));
    }
    for at in (0..tar.len()).step_by(13) {
        let mut bad = tar.clone();
        bad[at] ^= 0x5a;
        let _ = copy(&bad, &same);
        let _ = copy(&bad, &without(&["a"]));
    }
}
