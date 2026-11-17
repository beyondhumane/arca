use super::*;
use crate::corpus::{generate, Kind, CHUNK, SEED};
use std::sync::atomic::AtomicUsize;

fn tiny(kind: Kind) -> Options {
    Options {
        threads: 2,
        seconds: 0.0,
        size_mib: 2,
        codecs: Codec::ALL.to_vec(),
        kind,
    }
}

fn quiet(_: Event<'_>) {}

#[test]
fn the_corpus_is_the_same_everywhere() {
    let a = generate(Kind::Mixed, SEED, 3);
    let b = generate(Kind::Mixed, SEED, 2);
    assert_eq!(a.len(), 3);
    assert!(a.iter().all(|c| c.len() == CHUNK));
    assert_eq!(a[..2], b[..]);
    assert_ne!(a[0], a[1]);
    assert_ne!(generate(Kind::Mixed, SEED ^ 1, 1)[0], a[0]);
    let crcs: Vec<u32> = a.iter().map(|c| arca_core::crc32(c)).collect();
    assert_eq!(
        crcs, GOLDEN,
        "the corpus changed: published ratings no longer compare"
    );
}

const GOLDEN: [u32; 3] = [4258600235, 296701467, 1912548333];

#[test]
fn text_is_text_and_random_is_random() {
    let text = &generate(Kind::Text, SEED, 1)[0];
    assert!(text
        .iter()
        .all(|&b| b == b'\n' || (b' '..=b'~').contains(&b)));
    let mixed = &generate(Kind::Mixed, SEED, 1)[0];
    let random_stretch = &mixed[3 * corpus::STRETCH..4 * corpus::STRETCH];
    assert!(random_stretch.iter().any(|&b| b >= 0x80));
    assert!(mixed[..corpus::STRETCH].is_ascii());
}

fn check(report: &Report, ratios: impl Fn(Codec, f64) -> bool) {
    assert!(!report.stopped);
    let codecs: Vec<Codec> = report.rows.iter().map(|r| r.codec).collect();
    assert_eq!(codecs, Codec::ALL);
    for row in &report.rows {
        let Outcome::Measured {
            ratio,
            single,
            multi,
        } = &row.outcome
        else {
            assert!(matches!(row.codec, Codec::Zstd | Codec::Rar), "{:?}", row);
            continue;
        };
        assert!(ratios(row.codec, *ratio), "{:?} ratio {ratio}", row.codec);
        for s in [single, multi] {
            let s = s.expect("both thread counts measured");
            assert!(s.compress.is_finite() && s.compress > 0.0);
            assert!(s.decompress.is_finite() && s.decompress > 0.0);
            assert!(s.compress_runs >= 1 && s.decompress_runs >= 1);
        }
    }
    for codec in RATED {
        assert!(report
            .rows
            .iter()
            .any(|r| r.codec == codec && r.ratio().is_some()));
    }
    assert!(report.rating(false).is_some_and(|r| r > 0.0));
    assert!(report.rating(true).is_some_and(|r| r > 0.0));
}

#[test]
fn a_short_run_measures_every_codec_on_text() {
    let report = run(&tiny(Kind::Text), &AtomicBool::new(false), &quiet).unwrap();
    check(&report, |codec, ratio| {
        if codec == Codec::Store {
            (1.0..1.01).contains(&ratio)
        } else {
            ratio < 0.6
        }
    });
    let text = table(&report);
    assert!(text.contains("Reproduce: arca bench --codecs --threads 2 --seconds 0 --size 2"));
    assert!(text.contains("of the Deflate, LZMA2 and bzip2 speeds"));
    for codec in Codec::ALL {
        assert!(text.contains(codec.label()));
    }
}

#[test]
fn random_data_does_not_compress() {
    let report = run(&tiny(Kind::Random), &AtomicBool::new(false), &quiet).unwrap();
    check(&report, |_, ratio| (0.98..1.06).contains(&ratio));
}

#[test]
fn every_codec_gives_back_what_it_was_given_and_refuses_a_cut_copy() {
    let chunks = generate(Kind::Mixed, SEED, 2);
    let sizes = [CHUNK, CHUNK];
    for codec in Codec::ALL {
        if codecs::missing(codec).is_some() {
            continue;
        }
        let packed = codecs::pack(codec, &chunks).unwrap();
        let out = codecs::unpack(codec, &packed, &sizes).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().zip(&chunks).all(|(o, c)| o[..] == c[..]));
        let cut = match packed {
            codecs::Packed::Zip(b) => codecs::Packed::Zip(b[..b.len() / 2].to_vec()),
            codecs::Packed::Rar(b) => codecs::Packed::Rar(b[..b.len() / 2].to_vec()),
            codecs::Packed::Streams(s) => {
                codecs::Packed::Streams(s.iter().map(|s| s[..s.len() / 2].to_vec()).collect())
            }
        };
        assert!(codecs::unpack(codec, &cut, &sizes).is_err(), "{codec:?}");
    }
}

#[test]
fn a_stop_ends_the_run_with_what_was_measured() {
    let stop = AtomicBool::new(true);
    let report = run(&tiny(Kind::Mixed), &stop, &quiet).unwrap();
    assert!(report.stopped);
    assert!(report.rows.iter().all(|r| r.ratio().is_none()));
    assert!(report.rating(false).is_none());

    let stop = AtomicBool::new(false);
    let rows = AtomicUsize::new(0);
    let events = |e: Event<'_>| {
        if let Event::Row(row) = e {
            if row.ratio().is_some() {
                rows.fetch_add(1, Ordering::Relaxed);
                stop.store(true, Ordering::Relaxed);
            }
        }
    };
    let report = run(&tiny(Kind::Mixed), &stop, &events).unwrap();
    assert!(report.stopped);
    assert_eq!(rows.load(Ordering::Relaxed), 1);
    assert_eq!(report.rows.len(), 1);
    assert!(report.rows[0].speeds(false).is_some());
    assert!(report.rows[0].speeds(true).is_none());
    assert!(table(&report).contains("Stopped before the end"));
    parse(&json(&report));
}

#[test]
fn progress_counts_every_measurement() {
    let mut options = tiny(Kind::Mixed);
    options.codecs = vec![Codec::Store, Codec::Deflate];
    let last = std::sync::Mutex::new(Vec::new());
    let events = |e: Event<'_>| {
        if let Event::Progress(p) = e {
            last.lock().unwrap().push((p.step, p.steps, p.phase));
        }
    };
    run(&options, &AtomicBool::new(false), &events).unwrap();
    let seen = last.into_inner().unwrap();
    assert_eq!(seen[0], (0, 8, Phase::Generating));
    assert_eq!(seen.len(), 9);
    assert_eq!(seen[8], (7, 8, Phase::Decompressing));
}

#[test]
fn the_rating_is_the_geometric_mean_of_the_rated_speeds() {
    let speeds = |v: f64| Speeds {
        compress: v,
        decompress: 4.0 * v,
        compress_runs: 1,
        decompress_runs: 1,
    };
    let rows = Codec::ALL
        .iter()
        .map(|&codec| Row {
            codec,
            threads: 4,
            outcome: Outcome::Measured {
                ratio: 0.5,
                single: Some(speeds(if codec == Codec::Store { 1000.0 } else { 10.0 })),
                multi: None,
            },
        })
        .collect();
    let report = Report {
        options: Options::new(4),
        rows,
        stopped: false,
        native: true,
    };
    assert!((report.rating(false).unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(report.rating(true), None);
    let value = parse(&json(&report));
    assert_eq!(value.get("rating").get("single"), &Json::Number(20.0));
    assert_eq!(value.get("rating").get("multi"), &Json::Null);
}

#[test]
fn the_json_parses_and_says_what_the_table_says() {
    let mut options = tiny(Kind::Text);
    options.codecs = vec![Codec::Deflate, Codec::Zstd, Codec::Rar];
    let report = run(&options, &AtomicBool::new(false), &quiet).unwrap();
    let value = parse(&json(&report));
    assert_eq!(value.get("threads"), &Json::Number(2.0));
    assert_eq!(value.get("level"), &Json::Number(6.0));
    let Json::Array(codecs) = value.get("codecs") else {
        panic!("codecs is not an array");
    };
    assert_eq!(codecs.len(), 3);
    assert_eq!(codecs[0].get("codec"), &Json::String("deflate".into()));
    let Json::Number(ratio) = codecs[0].get("ratio") else {
        panic!("deflate has no ratio");
    };
    assert!(*ratio > 0.0 && *ratio < 1.0);
    assert!(matches!(
        codecs[0].get("multi").get("compress"),
        Json::Number(_)
    ));
    for c in &codecs[1..] {
        match c.get("available") {
            Json::Bool(true) => assert!(matches!(c.get("ratio"), Json::Number(_))),
            Json::Bool(false) => assert!(matches!(c.get("reason"), Json::String(_))),
            other => panic!("available is {other:?}"),
        }
    }
    assert_eq!(
        value.get("command"),
        &Json::String("arca bench --codecs --threads 2 --seconds 0 --size 2".into())
    );
}

#[derive(Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> &Json {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("no field {key}")),
            _ => panic!("not an object looking for {key}"),
        }
    }
}

fn parse(text: &str) -> Json {
    let mut p = Parser {
        s: text.as_bytes(),
        at: 0,
    };
    let v = p.value();
    p.space();
    assert_eq!(p.at, p.s.len(), "trailing text in {text}");
    v
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn eat(&mut self, b: u8) {
        self.space();
        assert_eq!(self.s.get(self.at), Some(&b), "at {}", self.at);
        self.at += 1;
    }

    fn word(&mut self, w: &str, v: Json) -> Json {
        assert!(self.s[self.at..].starts_with(w.as_bytes()));
        self.at += w.len();
        v
    }

    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = String::new();
        loop {
            let b = self.s[self.at];
            self.at += 1;
            match b {
                b'"' => return out,
                b'\\' => {
                    let e = self.s[self.at];
                    self.at += 1;
                    match e {
                        b'u' => {
                            let hex = std::str::from_utf8(&self.s[self.at..self.at + 4]).unwrap();
                            out.push(
                                char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap(),
                            );
                            self.at += 4;
                        }
                        b'n' => out.push('\n'),
                        b'"' | b'\\' | b'/' => out.push(e as char),
                        _ => panic!("bad escape"),
                    }
                }
                b if b < 0x20 => panic!("control character in a string"),
                b => out.push(b as char),
            }
        }
    }

    fn value(&mut self) -> Json {
        self.space();
        match self.s[self.at] {
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.space();
                if self.s[self.at] == b'}' {
                    self.at += 1;
                    return Json::Object(fields);
                }
                loop {
                    let k = self.string();
                    self.eat(b':');
                    fields.push((k, self.value()));
                    self.space();
                    self.at += 1;
                    match self.s[self.at - 1] {
                        b',' => continue,
                        b'}' => return Json::Object(fields),
                        _ => panic!("bad object at {}", self.at),
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.s[self.at] == b']' {
                    self.at += 1;
                    return Json::Array(items);
                }
                loop {
                    items.push(self.value());
                    self.space();
                    self.at += 1;
                    match self.s[self.at - 1] {
                        b',' => continue,
                        b']' => return Json::Array(items),
                        _ => panic!("bad array at {}", self.at),
                    }
                }
            }
            b'"' => Json::String(self.string()),
            b't' => self.word("true", Json::Bool(true)),
            b'f' => self.word("false", Json::Bool(false)),
            b'n' => self.word("null", Json::Null),
            _ => {
                let start = self.at;
                while self.at < self.s.len()
                    && matches!(
                        self.s[self.at],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.at += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.at]).unwrap();
                Json::Number(text.parse().unwrap_or_else(|_| panic!("bad number {text}")))
            }
        }
    }
}
