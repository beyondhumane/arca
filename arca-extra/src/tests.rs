use super::*;
use std::fs;

const MTIME: u32 = 1_700_000_000;

fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn open(format: Format, bytes: &[u8]) -> (tempfile::TempDir, Result<ExtraArchive>) {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "archive", bytes);
    let archive = ExtraArchive::open(&path, format);
    (dir, archive)
}

fn names(a: &ExtraArchive) -> Vec<&str> {
    a.entries().iter().map(|e| e.name.as_str()).collect()
}

fn read(a: &ExtraArchive, name: &str) -> Vec<u8> {
    let i = a.entries().iter().position(|e| e.name == name).unwrap();
    a.read_entry(i).unwrap()
}

fn extract_all(a: &ExtraArchive) -> (tempfile::TempDir, Extracted) {
    let dest = tempfile::tempdir().unwrap();
    let out = a
        .extract(dest.path(), &[], &|_, _, _| true, &|_| Conflict::Overwrite)
        .unwrap();
    (dest, out)
}

// Every prefix and a flipped byte at every position: none may panic, and
// whatever opens must survive a full test and a read of every entry.
fn survives_damage(format: Format, bytes: &[u8], step: usize) {
    survives_damage_after(format, bytes, bytes.len(), step);
}

// Every position up to `dense`, then one in `step`: the headers are where
// damage finds the most paths, and decoding all of a long file for every
// position past them only makes the run slow.
fn survives_damage_after(format: Format, bytes: &[u8], dense: usize, step: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("damaged");
    let exercise = |data: &[u8]| {
        fs::write(&path, data).unwrap();
        if let Ok(a) = ExtraArchive::open(&path, format) {
            let _ = a.test(&[], &|_, _, _| true);
            for i in 0..a.entries().len().min(8) {
                let _ = a.read_entry(i);
            }
        }
    };
    let positions: Vec<usize> = (0..bytes.len())
        .filter(|&at| at < dense || at % step == 0)
        .collect();
    for &cut in &positions {
        exercise(&bytes[..cut]);
    }
    for &at in &positions {
        for mask in [0x01u8, 0x80, 0xff] {
            let mut damaged = bytes.to_vec();
            damaged[at] ^= mask;
            exercise(&damaged);
        }
    }
}

// --- CPIO ----------------------------------------------------------------

struct C<'a> {
    name: &'a str,
    mode: u32,
    data: &'a [u8],
    ino: u32,
    nlink: u32,
}

fn file<'a>(name: &'a str, data: &'a [u8]) -> C<'a> {
    C {
        name,
        mode: 0o100644,
        data,
        ino: 0,
        nlink: 1,
    }
}

const TRAILER_ENTRY: C<'static> = C {
    name: "TRAILER!!!",
    mode: 0,
    data: b"",
    ino: 0,
    nlink: 1,
};

fn pad(out: &mut Vec<u8>, to: usize) {
    while !out.len().is_multiple_of(to) {
        out.push(0);
    }
}

fn newc(entries: &[C], crc: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, e) in entries.iter().chain([&TRAILER_ENTRY]).enumerate() {
        let sum: u32 = if crc {
            e.data.iter().map(|&b| u32::from(b)).sum()
        } else {
            0
        };
        let ino = if e.ino == 0 { i as u32 + 1 } else { e.ino };
        let header = format!(
            "{}{ino:08x}{:08x}{:08x}{:08x}{:08x}{MTIME:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{sum:08x}",
            if crc { "070702" } else { "070701" },
            e.mode,
            0,
            0,
            e.nlink,
            e.data.len(),
            0,
            0,
            0,
            0,
            e.name.len() + 1
        );
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(e.name.as_bytes());
        out.push(0);
        pad(&mut out, 4);
        out.extend_from_slice(e.data);
        pad(&mut out, 4);
    }
    out
}

fn odc(entries: &[C]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, e) in entries.iter().chain([&TRAILER_ENTRY]).enumerate() {
        let header = format!(
            "070707{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{MTIME:011o}{:06o}{:011o}",
            0,
            i + 1,
            e.mode,
            0,
            0,
            e.nlink,
            0,
            e.name.len() + 1,
            e.data.len()
        );
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(e.name.as_bytes());
        out.push(0);
        out.extend_from_slice(e.data);
    }
    out
}

fn bin(entries: &[C], little: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, e) in entries.iter().chain([&TRAILER_ENTRY]).enumerate() {
        let size = e.data.len() as u32;
        let words = [
            0o070707,
            0,
            i as u16 + 1,
            e.mode as u16,
            0,
            0,
            e.nlink as u16,
            0,
            (MTIME >> 16) as u16,
            MTIME as u16,
            e.name.len() as u16 + 1,
            (size >> 16) as u16,
            size as u16,
        ];
        for w in words {
            out.extend_from_slice(&if little {
                w.to_le_bytes()
            } else {
                w.to_be_bytes()
            });
        }
        out.extend_from_slice(e.name.as_bytes());
        out.push(0);
        pad(&mut out, 2);
        out.extend_from_slice(e.data);
        pad(&mut out, 2);
    }
    out
}

fn sample() -> Vec<C<'static>> {
    vec![
        C {
            name: ".",
            mode: 0o040755,
            data: b"",
            ino: 0,
            nlink: 2,
        },
        C {
            name: "./docs",
            mode: 0o040755,
            data: b"",
            ino: 0,
            nlink: 2,
        },
        file("./docs/readme.txt", b"read me first\n"),
        C {
            name: "./run.sh",
            mode: 0o100755,
            data: b"#!/bin/sh\necho hi\n",
            ino: 0,
            nlink: 1,
        },
        C {
            name: "./latest",
            mode: 0o120777,
            data: b"docs/readme.txt",
            ino: 0,
            nlink: 1,
        },
        C {
            name: "./dev/null",
            mode: 0o020666,
            data: b"",
            ino: 0,
            nlink: 1,
        },
    ]
}

#[test]
fn every_cpio_layout_lists_and_reads_the_same() {
    for (label, bytes) in [
        ("newc", newc(&sample(), false)),
        ("crc", newc(&sample(), true)),
        ("odc", odc(&sample())),
        ("bin le", bin(&sample(), true)),
        ("bin be", bin(&sample(), false)),
    ] {
        let (_dir, a) = open(Format::Cpio, &bytes);
        let a = a.unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(
            names(&a),
            ["docs", "docs/readme.txt", "run.sh", "latest"],
            "{label}"
        );
        assert!(a.entries()[0].is_dir, "{label}");
        assert_eq!(read(&a, "docs/readme.txt"), b"read me first\n", "{label}");
        assert_eq!(a.entries()[1].mtime, Some(i64::from(MTIME)), "{label}");
        assert_eq!(
            a.link(3),
            Some(Link::Symbolic("docs/readme.txt")),
            "{label}"
        );
        assert_eq!(a.notices().len(), 1, "{label}");
        assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 2, "{label}");
        let (dest, out) = extract_all(&a);
        assert_eq!(
            fs::read(dest.path().join("docs/readme.txt")).unwrap(),
            b"read me first\n"
        );
        assert_eq!(out.bytes, 14 + 18, "{label}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dest.path().join("run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{label}");
            assert_eq!(
                fs::read_link(dest.path().join("latest")).unwrap(),
                Path::new("docs/readme.txt")
            );
        }
    }
}

#[test]
fn a_wrong_cpio_crc_sum_is_an_integrity_error() {
    let mut bytes = newc(&[file("a.txt", b"abc")], true);
    let at = bytes.windows(3).position(|w| w == b"abc").unwrap();
    bytes[at] = b'x';
    let (_dir, a) = open(Format::Cpio, &bytes);
    let a = a.unwrap();
    assert!(matches!(
        a.test(&[], &|_, _, _| true),
        Err(Error::Integrity { .. })
    ));
}

#[test]
fn svr4_hard_links_share_the_data_of_the_last_name() {
    let bytes = newc(
        &[
            C {
                name: "first",
                mode: 0o100644,
                data: b"",
                ino: 77,
                nlink: 2,
            },
            C {
                name: "second",
                mode: 0o100644,
                data: b"shared",
                ino: 77,
                nlink: 2,
            },
        ],
        false,
    );
    let (_dir, a) = open(Format::Cpio, &bytes);
    let a = a.unwrap();
    assert_eq!(a.link(0), Some(Link::Hard("second")));
    assert_eq!(a.entries()[0].size, 6);
    assert_eq!(a.read_entry(0).unwrap(), b"shared");
    let (dest, out) = extract_all(&a);
    assert_eq!(fs::read(dest.path().join("first")).unwrap(), b"shared");
    assert_eq!(fs::read(dest.path().join("second")).unwrap(), b"shared");
    assert_eq!(out.files, 2);
}

#[test]
fn a_cpio_entry_that_climbs_out_is_refused() {
    let bytes = newc(&[file("../evil.txt", b"x")], false);
    let (_dir, a) = open(Format::Cpio, &bytes);
    let a = a.unwrap();
    let outer = tempfile::tempdir().unwrap();
    let dest = outer.path().join("inside");
    let result = a.extract(&dest, &[], &|_, _, _| true, &|_| Conflict::Overwrite);
    assert!(result.is_err());
    assert!(!outer.path().join("evil.txt").exists());
}

#[test]
fn a_link_out_of_the_destination_is_left_out() {
    let bytes = newc(
        &[C {
            name: "passwd",
            mode: 0o120777,
            data: b"../../../../etc/passwd",
            ino: 0,
            nlink: 1,
        }],
        false,
    );
    let (_dir, a) = open(Format::Cpio, &bytes);
    let (dest, out) = extract_all(&a.unwrap());
    assert_eq!(out.refused.len(), 1);
    assert!(fs::symlink_metadata(dest.path().join("passwd")).is_err());
}

#[test]
fn cpio_sizes_and_names_are_bounded() {
    let mut huge = newc(&[file("a", b"abcd")], false);
    huge[54..62].copy_from_slice(b"ffffffff");
    let (_dir, a) = open(Format::Cpio, &huge);
    assert!(matches!(a, Err(Error::Format(_))));
    let mut long = newc(&[file("a", b"abcd")], false);
    long[94..102].copy_from_slice(b"7fffffff");
    let (_dir, a) = open(Format::Cpio, &long);
    assert!(matches!(a, Err(Error::Limit(_))));
}

#[test]
fn damaged_cpio_never_panics() {
    survives_damage(Format::Cpio, &newc(&sample(), true), 1);
    survives_damage(Format::Cpio, &odc(&sample()), 1);
    survives_damage(Format::Cpio, &bin(&sample(), false), 1);
}

// --- AR and DEB ------------------------------------------------------------

fn ar(members: &[(&str, &[u8])], bsd: bool) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    let mut table = Vec::new();
    let mut fields = Vec::new();
    for (name, _) in members {
        if bsd {
            fields.push(format!("#1/{}", name.len()));
        } else if name.len() > 15 {
            fields.push(format!("/{}", table.len()));
            table.extend_from_slice(name.as_bytes());
            table.extend_from_slice(b"/\n");
        } else {
            fields.push(format!("{name}/"));
        }
    }
    let header = |out: &mut Vec<u8>, field: &str, size: usize, mode: u32| {
        out.extend_from_slice(
            format!(
                "{field:<16}{MTIME:<12}{:<6}{:<6}{:<8}{size:<10}`\n",
                0,
                0,
                format!("{mode:o}")
            )
            .as_bytes(),
        );
    };
    if !table.is_empty() {
        header(&mut out, "//", table.len(), 0);
        out.extend_from_slice(&table);
        pad(&mut out, 2);
    }
    for ((name, data), field) in members.iter().zip(&fields) {
        let extra = if bsd { name.len() } else { 0 };
        header(&mut out, field, data.len() + extra, 0o100755);
        if bsd {
            out.extend_from_slice(name.as_bytes());
        }
        out.extend_from_slice(data);
        pad(&mut out, 2);
    }
    out
}

#[test]
fn gnu_and_bsd_ar_long_names_are_read() {
    let members: [(&str, &[u8]); 3] = [
        ("short.o", b"odd"),
        ("a_rather_long_object_name.o", b"even"),
        ("x", b""),
    ];
    for bsd in [false, true] {
        let (_dir, a) = open(Format::Ar, &ar(&members, bsd));
        let a = a.unwrap();
        assert_eq!(names(&a), ["short.o", "a_rather_long_object_name.o", "x"]);
        assert_eq!(read(&a, "a_rather_long_object_name.o"), b"even");
        assert_eq!(read(&a, "short.o"), b"odd");
        assert_eq!(a.entries()[0].mtime, Some(i64::from(MTIME)));
        assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 3);
    }
}

#[test]
fn ar_symbol_tables_are_not_files() {
    let mut bytes = b"!<arch>\n".to_vec();
    bytes.extend_from_slice(
        format!("{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n", "/", 0, 0, 0, 0, 4).as_bytes(),
    );
    bytes.extend_from_slice(b"\0\0\0\0");
    bytes.extend_from_slice(&ar(&[("lib.o", b"code")], false)[8..]);
    let (_dir, a) = open(Format::Ar, &bytes);
    assert_eq!(names(&a.unwrap()), ["lib.o"]);
}

#[test]
fn ar_members_and_tables_past_the_end_are_refused() {
    let mut bytes = ar(&[("a", b"data")], false);
    bytes[8 + 48..8 + 58].copy_from_slice(b"9999999999");
    let (_dir, a) = open(Format::Ar, &bytes);
    assert!(matches!(a, Err(Error::Format(_))));
    let (_dir, a) = open(Format::Ar, b"!<thin>\n");
    assert!(matches!(a, Err(Error::Unsupported(_))));
}

#[test]
fn damaged_ar_never_panics() {
    survives_damage(
        Format::Ar,
        &ar(
            &[("a_rather_long_object_name.o", b"even"), ("b", b"x")],
            false,
        ),
        1,
    );
    survives_damage(Format::Ar, &ar(&[("bsd_name.o", b"data")], true), 1);
}

fn tar(entries: &[(&str, &[u8], u32)], links: &[(&str, &str)]) -> Vec<u8> {
    let mut w = arca_tar::TarWriter::new(Vec::new());
    for (name, data, mode) in entries {
        w.add(name, data.len() as u64, i64::from(MTIME), *mode, *data)
            .unwrap();
    }
    for (name, target) in links {
        w.add_link(name, target, i64::from(MTIME), b'2').unwrap();
    }
    w.finish().unwrap()
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn xz(data: &[u8]) -> Vec<u8> {
    compress(Format::Xz, data)
}

fn compress(format: Format, data: &[u8]) -> Vec<u8> {
    let mut e = arca_stream::encoder(
        Vec::new(),
        format,
        arca_core::Level::FAST,
        1,
        Some(data.len() as u64),
    )
    .unwrap();
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn deb_with(data: Vec<u8>, data_name: &str) -> Vec<u8> {
    let control = gzip(&tar(
        &[
            ("./", b"", 0o755),
            ("./control", b"Package: hello\nVersion: 1.0\n", 0o644),
        ],
        &[],
    ));
    ar(
        &[
            ("debian-binary", b"2.0\n"),
            ("control.tar.gz", &control),
            (data_name, &data),
        ],
        false,
    )
}

fn deb() -> Vec<u8> {
    deb_with(
        xz(&tar(
            &[
                ("./", b"", 0o755),
                ("./usr/", b"", 0o755),
                ("./usr/bin/", b"", 0o755),
                ("./usr/bin/hello", b"#!/bin/sh\necho hello\n", 0o755),
                ("./usr/share/doc/hello/copyright", b"Apache-2.0\n", 0o644),
            ],
            &[("./usr/bin/hi", "hello")],
        )),
        "data.tar.xz",
    )
}

#[test]
fn a_deb_lists_its_files_and_its_control_files_under_debian() {
    let (_dir, a) = open(Format::Deb, &deb());
    let a = a.unwrap();
    assert_eq!(
        names(&a),
        [
            "usr",
            "usr/bin",
            "usr/bin/hello",
            "usr/share/doc/hello/copyright",
            "usr/bin/hi",
            "DEBIAN/control",
        ]
    );
    assert_eq!(
        read(&a, "DEBIAN/control"),
        b"Package: hello\nVersion: 1.0\n"
    );
    assert_eq!(read(&a, "usr/share/doc/hello/copyright"), b"Apache-2.0\n");
    assert_eq!(a.link(4), Some(Link::Symbolic("hello")));
    assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 3);
    let (dest, out) = extract_all(&a);
    assert_eq!(out.files, if cfg!(unix) { 4 } else { 3 });
    assert_eq!(
        fs::read(dest.path().join("usr/bin/hello")).unwrap(),
        b"#!/bin/sh\necho hello\n"
    );
    assert!(dest.path().join("DEBIAN/control").is_file());
}

#[test]
fn a_deb_with_an_uncompressed_or_gzip_data_tar_reads_too() {
    let inner = tar(&[("./a", b"plain", 0o644)], &[]);
    for (bytes, name) in [(inner.clone(), "data.tar"), (gzip(&inner), "data.tar.gz")] {
        let (_dir, a) = open(Format::Deb, &deb_with(bytes, name));
        assert_eq!(read(&a.unwrap(), "a"), b"plain");
    }
}

#[test]
fn an_ar_that_is_not_a_package_is_not_a_deb() {
    let (_dir, a) = open(Format::Deb, &ar(&[("x", b"y")], false));
    assert!(matches!(a, Err(Error::Format(_))));
    let (_dir, a) = open(
        Format::Deb,
        &ar(&[("debian-binary", b"3.0\n"), ("data.tar", b"")], false),
    );
    assert!(matches!(a, Err(Error::Unsupported(_))));
}

#[test]
fn damaged_debs_never_panic() {
    survives_damage(Format::Deb, &deb(), 3);
}

// --- RPM -----------------------------------------------------------------

fn rpm(payload: Vec<u8>) -> Vec<u8> {
    let mut out = vec![0xed, 0xab, 0xee, 0xdb, 3, 0];
    out.resize(96, 0);
    out[78..80].copy_from_slice(&5u16.to_be_bytes());
    out.extend_from_slice(&[0x8e, 0xad, 0xe8, 0x01, 0, 0, 0, 0]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    let store = b"cpio\0";
    out.extend_from_slice(&[0x8e, 0xad, 0xe8, 0x01, 0, 0, 0, 0]);
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&(store.len() as u32).to_be_bytes());
    for word in [1124u32, 6, 0, 1] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out.extend_from_slice(store);
    out.extend_from_slice(&payload);
    out
}

fn rpm_payload() -> Vec<u8> {
    newc(
        &[
            C {
                name: "./usr/bin",
                mode: 0o040755,
                data: b"",
                ino: 0,
                nlink: 2,
            },
            C {
                name: "./usr/bin/hello",
                mode: 0o100755,
                data: b"#!/bin/sh\necho hello\n",
                ino: 0,
                nlink: 1,
            },
            file("./usr/share/doc/hello/README", b"read me\n"),
        ],
        false,
    )
}

#[test]
fn an_rpm_lists_its_cpio_payload_compressed_or_not() {
    let payload = rpm_payload();
    for bytes in [
        rpm(payload.clone()),
        rpm(gzip(&payload)),
        rpm(xz(&payload)),
        rpm(compress(Format::Bz2, &payload)),
    ] {
        let (_dir, a) = open(Format::Rpm, &bytes);
        let a = a.unwrap();
        assert_eq!(
            names(&a),
            ["usr/bin", "usr/bin/hello", "usr/share/doc/hello/README"]
        );
        assert_eq!(read(&a, "usr/share/doc/hello/README"), b"read me\n");
        assert_eq!(read(&a, "usr/bin/hello"), b"#!/bin/sh\necho hello\n");
        assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 2);
        assert!(a.notices().iter().any(|n| n.contains("signatures")));
        let (dest, _) = extract_all(&a);
        assert!(dest.path().join("usr/share/doc/hello/README").is_file());
    }
}

#[test]
fn an_rpm_payload_that_declares_more_than_it_holds_fails_without_allocating() {
    let mut payload = rpm_payload();
    let at = payload
        .windows(15)
        .position(|w| w == b"./usr/bin/hello")
        .unwrap()
        - 110
        + 54;
    payload[at..at + 8].copy_from_slice(b"fffffff0");
    let (_dir, a) = open(Format::Rpm, &rpm(gzip(&payload)));
    assert!(a.is_err());
}

#[test]
fn rpm_headers_are_bounded() {
    let mut bytes = rpm(rpm_payload());
    bytes[96 + 8..96 + 12].copy_from_slice(&0x0100_0000u32.to_be_bytes());
    let (_dir, a) = open(Format::Rpm, &bytes);
    assert!(matches!(a, Err(Error::Limit(_))));
    let mut lead = rpm(rpm_payload());
    lead[0] = 0;
    let (_dir, a) = open(Format::Rpm, &lead);
    assert!(matches!(a, Err(Error::Format(_))));
}

#[test]
fn damaged_rpms_never_panic() {
    survives_damage(Format::Rpm, &rpm(gzip(&rpm_payload())), 1);
}

// --- CAB -----------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn big_text() -> Vec<u8> {
    let mut text = String::new();
    for i in 0..2500 {
        text.push_str(&format!(
            "line {i} of the arca cabinet fixture: {}\n",
            &"abcdefghij"[i % 10..]
        ));
    }
    text.into_bytes()
}

#[test]
fn makecab_cabinets_read_with_mszip_and_lzx() {
    for (name, method) in [("mszip.cab", Method::Deflate), ("lzx.cab", Method::Lzx)] {
        let a = ExtraArchive::open(&fixture(name), Format::Cab).unwrap();
        assert_eq!(names(&a), ["a.txt", "docs/big.txt"], "{name}");
        assert_eq!(a.entries()[1].method, method, "{name}");
        assert_eq!(read(&a, "a.txt"), b"hello from a cabinet\n", "{name}");
        assert_eq!(read(&a, "docs/big.txt"), big_text(), "{name}");
        assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 2, "{name}");
        let (dest, out) = extract_all(&a);
        assert_eq!(out.bytes, 21 + big_text().len() as u64);
        assert_eq!(
            fs::read(dest.path().join("docs/big.txt")).unwrap(),
            big_text()
        );
    }
}

fn cab(files: &[(&str, &[u8])], mszip: bool, sums: bool) -> Vec<u8> {
    let names: usize = files.iter().map(|(n, _)| 16 + n.len() + 1).sum();
    let data_at = 36 + 8 + names;
    let all: Vec<u8> = files.iter().flat_map(|(_, d)| d.to_vec()).collect();
    let mut blocks = Vec::new();
    for chunk in all.chunks(cab::MAX_BLOCK) {
        let packed = if mszip {
            let mut e =
                flate2::write::DeflateEncoder::new(b"CK".to_vec(), flate2::Compression::default());
            e.write_all(chunk).unwrap();
            e.finish().unwrap()
        } else {
            chunk.to_vec()
        };
        let mut head = Vec::new();
        head.extend_from_slice(&(packed.len() as u16).to_le_bytes());
        head.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        let sum = if sums {
            cab::checksum_for_tests(&head, cab::checksum_for_tests(&packed, 0))
        } else {
            0
        };
        blocks.extend_from_slice(&sum.to_le_bytes());
        blocks.extend_from_slice(&head);
        blocks.extend_from_slice(&packed);
    }
    let count = all.chunks(cab::MAX_BLOCK).count();
    let mut out = b"MSCF".to_vec();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((data_at + blocks.len()) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&44u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&[3, 1]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&(data_at as u32).to_le_bytes());
    out.extend_from_slice(&(count as u16).to_le_bytes());
    out.extend_from_slice(&u16::from(mszip).to_le_bytes());
    let mut offset = 0u32;
    for (name, data) in files {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x5771u16.to_le_bytes());
        out.extend_from_slice(&0x7a75u16.to_le_bytes());
        out.extend_from_slice(&0x20u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        offset += data.len() as u32;
    }
    assert_eq!(out.len(), data_at);
    out.extend_from_slice(&blocks);
    out
}

#[test]
fn stored_and_mszip_cabinets_built_here_round_trip() {
    let big = big_text();
    let files: [(&str, &[u8]); 3] = [
        ("one.txt", b"first"),
        ("dir\\two.bin", &big),
        ("three", b"3"),
    ];
    for mszip in [false, true] {
        let (_dir, a) = open(Format::Cab, &cab(&files, mszip, true));
        let a = a.unwrap();
        assert_eq!(names(&a), ["one.txt", "dir/two.bin", "three"]);
        assert_eq!(read(&a, "dir/two.bin"), big);
        assert_eq!(read(&a, "three"), b"3");
        let wanted = [false, false, true];
        assert_eq!(a.test(&wanted, &|_, _, _| true).unwrap(), 1);
        assert!(a.entries()[0].mtime.is_some());
        assert_eq!(a.entries()[0].attributes, 0x20);
    }
}

#[test]
fn a_cabinet_block_with_a_bad_checksum_is_refused() {
    let mut bytes = cab(&[("a", b"abcdef")], false, true);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let (_dir, a) = open(Format::Cab, &bytes);
    assert!(a.unwrap().test(&[], &|_, _, _| true).is_err());
}

#[test]
fn a_cabinet_file_outside_its_folder_is_refused() {
    let mut bytes = cab(&[("a", b"abcdef")], false, false);
    bytes[44..48].copy_from_slice(&0x7fff_ffffu32.to_le_bytes());
    let (_dir, a) = open(Format::Cab, &bytes);
    assert!(matches!(a, Err(Error::Format(_))));
}

#[test]
fn a_cabinet_name_that_climbs_out_is_refused() {
    let (_dir, a) = open(Format::Cab, &cab(&[("..\\..\\evil", b"x")], false, false));
    let a = a.unwrap();
    let outer = tempfile::tempdir().unwrap();
    let dest = outer.path().join("in");
    assert!(a
        .extract(&dest, &[], &|_, _, _| true, &|_| Conflict::Overwrite)
        .is_err());
    assert!(!outer.path().join("evil").exists());
}

#[test]
fn damaged_cabinets_never_panic() {
    survives_damage_after(Format::Cab, &fs::read(fixture("lzx.cab")).unwrap(), 160, 19);
    survives_damage_after(
        Format::Cab,
        &fs::read(fixture("mszip.cab")).unwrap(),
        160,
        37,
    );
    survives_damage(
        Format::Cab,
        &cab(&[("a", b"abcdef"), ("b", b"g")], true, true),
        1,
    );
}

#[test]
fn the_cabinet_checksum_matches_the_specification() {
    assert_eq!(
        cab::checksum_for_tests(b"\x0e\0\x0e\0Hello, world!\n", 0),
        0x7f2e1a4c
    );
}

// --- LZH -----------------------------------------------------------------

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn lzh_member(method: &[u8; 5], name: &str, packed: &[u8], original: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(method);
    body.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    body.extend_from_slice(&(original.len() as u32).to_le_bytes());
    body.extend_from_slice(&0x5771_7a75u32.to_le_bytes());
    body.push(0x20);
    body.push(0);
    body.push(name.len() as u8);
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(&crc16(original).to_le_bytes());
    let sum = body.iter().fold(0u8, |s, &b| s.wrapping_add(b));
    let mut out = vec![body.len() as u8, sum];
    out.extend_from_slice(&body);
    out.extend_from_slice(packed);
    out
}

// An lh5 block whose three trees each hold a single symbol, so that every
// symbol costs no bits at all: four times 'A'.
fn lh5_aaaa() -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut put = |value: u32, width: u32| {
        for i in (0..width).rev() {
            bits.push(value >> i & 1 == 1);
        }
    };
    put(4, 16);
    put(0, 5);
    put(0, 5);
    put(0, 9);
    put(u32::from(b'A'), 9);
    put(0, 4);
    put(0, 4);
    let mut out = vec![0u8; bits.len().div_ceil(8) + 2];
    for (i, bit) in bits.iter().enumerate() {
        if *bit {
            out[i / 8] |= 0x80 >> (i % 8);
        }
    }
    out
}

fn lzh() -> Vec<u8> {
    let mut out = lzh_member(b"-lh0-", "docs\\readme.txt", b"stored text", b"stored text");
    out.extend(lzh_member(b"-lh5-", "aaaa.txt", &lh5_aaaa(), b"AAAA"));
    out.extend(lzh_member(b"-lhd-", "empty\\", b"", b""));
    out.push(0);
    out
}

#[test]
fn lzh_stored_and_lh5_members_read_and_check_their_crc() {
    let (_dir, a) = open(Format::Lzh, &lzh());
    let a = a.unwrap();
    assert_eq!(names(&a), ["docs/readme.txt", "aaaa.txt", "empty"]);
    assert!(a.entries()[2].is_dir);
    assert_eq!(a.entries()[0].method, Method::Store);
    assert_eq!(a.entries()[1].method, Method::Lzh);
    assert_eq!(read(&a, "docs/readme.txt"), b"stored text");
    assert_eq!(read(&a, "aaaa.txt"), b"AAAA");
    assert_eq!(a.test(&[], &|_, _, _| true).unwrap(), 2);
    let (dest, _) = extract_all(&a);
    assert_eq!(fs::read(dest.path().join("aaaa.txt")).unwrap(), b"AAAA");
    assert!(dest.path().join("empty").is_dir());

    let mut bad = lzh_member(b"-lh0-", "a", b"stored", b"stored");
    let at = bad.len() - 1;
    bad[at] ^= 1;
    bad.push(0);
    let (_dir, a) = open(Format::Lzh, &bad);
    assert!(a.unwrap().test(&[], &|_, _, _| true).is_err());
}

#[test]
fn an_lzh_name_that_climbs_out_stays_inside() {
    let mut bytes = lzh_member(b"-lh0-", "..\\..\\evil.txt", b"x", b"x");
    bytes.push(0);
    let (_dir, a) = open(Format::Lzh, &bytes);
    let a = a.unwrap();
    let outer = tempfile::tempdir().unwrap();
    let dest = outer.path().join("in");
    assert_eq!(names(&a), ["evil.txt"]);
    a.extract(&dest, &[], &|_, _, _| true, &|_| Conflict::Overwrite)
        .unwrap();
    assert!(dest.join("evil.txt").is_file());
    assert!(!outer.path().join("evil.txt").exists());
    assert!(!outer.path().parent().unwrap().join("evil.txt").exists());
}

#[test]
fn an_lzh_size_bomb_is_bounded() {
    let mut bytes = lzh_member(b"-lh0-", "bomb", b"tiny", b"tiny");
    bytes[11..15].copy_from_slice(&u32::MAX.to_le_bytes());
    let sum = bytes[2..2 + usize::from(bytes[0])]
        .iter()
        .fold(0u8, |s, &b| s.wrapping_add(b));
    bytes[1] = sum;
    bytes.push(0);
    let (_dir, a) = open(Format::Lzh, &bytes);
    let a = a.unwrap();
    assert!(matches!(a.read_entry(0), Err(Error::Limit(_))));
    assert!(a.test(&[], &|_, _, _| true).is_err());
}

#[test]
fn damaged_lzh_never_panics() {
    survives_damage(Format::Lzh, &lzh(), 1);
}

// --- Shared ----------------------------------------------------------------

#[test]
fn stopping_is_reported_as_a_stop() {
    let a = ExtraArchive::open(&fixture("lzx.cab"), Format::Cab).unwrap();
    assert!(matches!(
        a.test(&[], &|_, _, _| false),
        Err(Error::Cancelled)
    ));
}

#[test]
fn formats_this_crate_does_not_read_are_refused() {
    assert!(!handles(Format::Zip));
    assert!(handles(Format::Lzh));
    let (_dir, a) = open(Format::Zip, b"PK");
    assert!(matches!(a, Err(Error::Unsupported(_))));
}

#[test]
fn names_lose_their_dot_slash_and_trailing_slash() {
    assert_eq!(clean("./a/b/"), "a/b");
    assert_eq!(clean("/abs"), "abs");
    assert_eq!(clean("."), "");
    assert_eq!(clean("././x"), "x");
}

// --- Random payloads through the third-party decoders ---------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

// Release builds abort on a panic, so whatever delharc makes of garbage has
// to be an error. Debug assertions are on here, which checks more than a
// release build would.
#[test]
fn every_lzh_method_survives_random_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("random.lzh");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let methods: [&[u8; 5]; 9] = [
        b"-lh1-", b"-lh4-", b"-lh5-", b"-lh6-", b"-lh7-", b"-lzs-", b"-lz5-", b"-pm1-", b"-pm2-",
    ];
    for round in 0..3000 {
        let method = methods[round % methods.len()];
        let len = 1 + (rng.next() % 300) as usize;
        let packed = rng.bytes(len);
        let original = vec![0u8; 4096 + (rng.next() % 61440) as usize];
        let mut bytes = lzh_member(method, "r", &packed, &original);
        bytes.push(0);
        fs::write(&path, &bytes).unwrap();
        let a = ExtraArchive::open(&path, Format::Lzh).unwrap();
        let _ = a.test(&[], &|_, _, _| true);
    }
}

#[test]
fn lzx_survives_random_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("random.cab");
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let template = cab(&[("r", &vec![0u8; 40_000])], false, false);
    let data_at = 36 + 8 + 16 + 2;
    for round in 0..1500 {
        let mut bytes = template.clone();
        let window: u16 = if round % 2 == 0 { 0x1003 } else { 0x1503 };
        bytes[42..44].copy_from_slice(&window.to_le_bytes());
        for at in [data_at + 8, data_at + 8 + cab::MAX_BLOCK + 8] {
            let end = (at + cab::MAX_BLOCK).min(bytes.len());
            let noise = rng.bytes(end - at);
            bytes[at..end].copy_from_slice(&noise);
        }
        fs::write(&path, &bytes).unwrap();
        let a = ExtraArchive::open(&path, Format::Cab).unwrap();
        let _ = a.test(&[], &|_, _, _| true);
    }
}
