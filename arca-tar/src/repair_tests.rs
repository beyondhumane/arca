use super::*;
use std::io::Cursor;

fn sample() -> Vec<u8> {
    let mut w = TarWriter::new(Vec::new());
    w.add("dir/", 0, 1_600_000_000, 0o750, io::empty()).unwrap();
    w.add("dir/one.txt", 700, 1_600_000_001, 0o644, &[b'1'; 700][..])
        .unwrap();
    let long = format!("{}/{}", "d".repeat(150), "f".repeat(120));
    w.add(&long, 3, 1_600_000_002, 0o644, &b"abc"[..]).unwrap();
    w.add_link("dir/hard", "dir/one.txt", 1_600_000_003, b'1')
        .unwrap();
    w.add("two.bin", 3000, 1_600_000_004, 0o600, &[2u8; 3000][..])
        .unwrap();
    w.finish().unwrap()
}

fn long_name() -> String {
    format!("{}/{}", "d".repeat(150), "f".repeat(120))
}

fn salvaged(tar: &[u8]) -> (Vec<u8>, Salvaged) {
    salvaged_from(tar)
}

fn salvaged_from<R: Read>(source: R) -> (Vec<u8>, Salvaged) {
    let mut w = TarWriter::new(Vec::new());
    let mut spool = Cursor::new(Vec::new());
    let report = salvage(source, &mut w, &mut spool, &mut |_, _| true).unwrap();
    (w.finish().unwrap(), report)
}

fn listed(tar: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut r = TarReader::new(tar);
    let mut out = Vec::new();
    while let Some(e) = r.next_entry().unwrap() {
        let mut data = Vec::new();
        r.copy_data(&e, &mut data).unwrap();
        out.push((e.entry.name.clone(), data));
    }
    out
}

fn header_at(tar: &[u8], name: &str) -> usize {
    (0..tar.len())
        .step_by(BLOCK)
        .find(|&at| tar[at..].starts_with(name.as_bytes()) && is_header(&tar[at..at + BLOCK]))
        .unwrap()
}

#[test]
fn a_sound_tar_comes_through_whole() {
    let tar = sample();
    let (out, report) = salvaged(&tar);
    assert!(report.lost.is_empty() && report.stream.is_none());
    assert_eq!(listed(&out), listed(&tar));
    assert_eq!(report.kept.len(), 5);
}

#[test]
fn a_damaged_header_costs_only_its_entry() {
    let mut tar = sample();
    let at = header_at(&tar, "two.bin");
    tar[at + 3] ^= 0x40;
    let at = header_at(&tar, "dir/one.txt");
    tar[at + 130] ^= 0x01;
    let (out, report) = salvaged(&tar);
    let names: Vec<String> = listed(&out).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["dir/".to_string(), long_name()]);
    assert_eq!(report.kept, names);
    assert!(report.stream.is_none());
    assert!(report
        .lost
        .iter()
        .any(|l| l.reason.contains("hard link to 'dir/one.txt'")));
    assert!(report.lost.len() >= 3);
}

#[test]
fn a_run_of_zeros_in_the_middle_is_stepped_over() {
    let tar = sample();
    let at = header_at(&tar, "two.bin");
    let mut holed = tar[..at].to_vec();
    holed.extend_from_slice(&[0u8; BLOCK * 3]);
    holed.extend_from_slice(&tar[at..]);
    let (out, report) = salvaged(&holed);
    assert_eq!(listed(&out), listed(&tar));
    assert!(report.lost.is_empty());
}

#[test]
fn an_entry_cut_short_leaves_nothing_behind() {
    let tar = sample();
    let at = header_at(&tar, "two.bin");
    let (out, report) = salvaged(&tar[..at + BLOCK + 1000]);
    let names: Vec<String> = listed(&out).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names.len(), 4);
    assert!(!names.contains(&"two.bin".to_string()));
    assert_eq!(report.lost.len(), 1);
    assert_eq!(report.lost[0].name.as_deref(), Some("two.bin"));
}

struct Failing {
    data: Vec<u8>,
    at: usize,
    fail_at: usize,
}

impl Read for Failing {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at >= self.fail_at {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "corrupt gzip"));
        }
        let n = buf.len().min(self.fail_at - self.at).min(100);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

#[test]
fn a_source_that_fails_keeps_what_came_before() {
    let tar = sample();
    let fail_at = header_at(&tar, "two.bin") + BLOCK + 10;
    let (out, report) = salvaged_from(Failing {
        data: tar,
        at: 0,
        fail_at,
    });
    assert_eq!(listed(&out).len(), 4);
    assert_eq!(report.stream.as_deref(), Some("corrupt gzip"));
    assert_eq!(report.lost.last().unwrap().name.as_deref(), Some("two.bin"));
}

#[test]
fn stopping_and_a_spool_that_cannot_be_written_are_errors() {
    let tar = sample();
    let mut w = TarWriter::new(Vec::new());
    let mut spool = Cursor::new(Vec::new());
    assert!(matches!(
        salvage(&tar[..], &mut w, &mut spool, &mut |_, _| false),
        Err(Error::Cancelled)
    ));
    let mut tiny = [0u8; 16];
    let mut spool = Cursor::new(&mut tiny[..]);
    assert!(matches!(
        salvage(&tar[..], &mut w, &mut spool, &mut |_, _| true),
        Err(Error::Io(_))
    ));
}

#[test]
fn cut_or_garbled_input_does_not_panic_and_always_reads_back() {
    let tar = sample();
    for cut in 0..tar.len() {
        let (out, _) = salvaged(&tar[..cut]);
        listed(&out);
    }
    for at in (0..tar.len()).step_by(3) {
        for delta in [0x01u8, 0x80, 0xFF] {
            let mut bad = tar.clone();
            bad[at] ^= delta;
            let (out, _) = salvaged(&bad);
            listed(&out);
        }
    }
    let noise: Vec<u8> = (0..40_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let (out, report) = salvaged(&noise);
    assert!(listed(&out).len() <= report.kept.len());
}
