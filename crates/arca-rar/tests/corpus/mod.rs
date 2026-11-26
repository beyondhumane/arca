//! A deterministic corpus for measuring and testing the RAR 5 compressor.
//!
//! Every byte comes from a fixed seed through SplitMix64, so the same scale
//! gives the same files on every machine and every run, with nothing
//! downloaded. Each kind stands in for a kind of data archivers meet: prose,
//! logs, source code, x86-64 machine code, PCM audio and bitmaps, data that is
//! already compressed, and many small files. They are stand-ins: synthetic
//! machine code exercises the same E8/E9 call patterns a real executable has,
//! but it is not one.

#![allow(dead_code)]

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (((self.next() >> 32) * n as u64) >> 32) as usize
    }

    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

pub struct Vocabulary {
    words: Vec<String>,
    cumulative: Vec<u64>,
}

impl Vocabulary {
    pub fn new(rng: &mut Rng, size: usize) -> Self {
        const ONSETS: [&str; 24] = [
            "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w",
            "st", "tr", "pl", "ch", "sh", "th", "gr",
        ];
        const VOWELS: [&str; 10] = ["a", "e", "i", "o", "u", "ea", "ou", "ai", "io", "y"];
        const CODAS: [&str; 12] = ["", "", "", "n", "r", "s", "t", "l", "nd", "st", "ng", "m"];
        let mut words = Vec::with_capacity(size);
        let mut seen = std::collections::HashSet::new();
        while words.len() < size {
            let syllables = 1 + rng.below(3) + usize::from(words.len() > size / 4);
            let mut word = String::new();
            for _ in 0..syllables {
                word.push_str(ONSETS[rng.below(ONSETS.len())]);
                word.push_str(VOWELS[rng.below(VOWELS.len())]);
                word.push_str(CODAS[rng.below(CODAS.len())]);
            }
            if seen.insert(word.clone()) {
                words.push(word);
            }
        }
        let mut cumulative = Vec::with_capacity(size);
        let mut total = 0u64;
        for rank in 0..size {
            total += 1_000_000 / (rank as u64 + 2);
            cumulative.push(total);
        }
        Vocabulary { words, cumulative }
    }

    pub fn word(&self, rng: &mut Rng) -> &str {
        let total = *self.cumulative.last().unwrap();
        let pick = rng.next() % total;
        let index = self.cumulative.partition_point(|&c| c <= pick);
        &self.words[index]
    }
}

pub fn prose(seed: u64, size: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let vocabulary = Vocabulary::new(&mut rng, 6000);
    let mut out = String::with_capacity(size + 256);
    let mut column = 0usize;
    while out.len() < size {
        let sentences = rng.range(2, 7);
        for _ in 0..sentences {
            let words = rng.range(4, 22);
            for i in 0..words {
                let mut word = vocabulary.word(&mut rng).to_string();
                if i == 0 {
                    word[..1].make_ascii_uppercase();
                }
                if i + 1 == words {
                    word.push(if rng.chance(85) { '.' } else { '?' });
                } else if rng.chance(8) {
                    word.push(',');
                }
                if column + word.len() + 1 > 76 {
                    out.push('\n');
                    column = 0;
                } else if column > 0 {
                    out.push(' ');
                    column += 1;
                }
                out.push_str(&word);
                column += word.len();
            }
        }
        out.push_str("\n\n");
        column = 0;
    }
    out.truncate(size);
    out.into_bytes()
}

pub fn log(seed: u64, size: usize) -> Vec<u8> {
    use std::fmt::Write;
    const LEVELS: [&str; 4] = ["INFO ", "INFO ", "DEBUG", "WARN "];
    const PATHS: [&str; 8] = [
        "/api/v1/items",
        "/api/v1/users",
        "/api/v2/search",
        "/static/app.js",
        "/static/style.css",
        "/health",
        "/api/v1/orders",
        "/login",
    ];
    const STATUS: [u32; 6] = [200, 200, 200, 304, 404, 500];
    let mut rng = Rng::new(seed);
    let mut out = String::with_capacity(size + 256);
    let mut millis: u64 = 1_763_380_800_000;
    while out.len() < size {
        millis += rng.range(0, 40) as u64;
        let seconds = millis / 1000;
        let (h, m, s) = ((seconds / 3600) % 24, (seconds / 60) % 60, seconds % 60);
        let _ = writeln!(
            out,
            "2026-11-17T{h:02}:{m:02}:{s:02}.{:03}Z {} [worker-{:02}] {} {}/{} status={} bytes={} dur={}ms client=10.{}.{}.{}",
            millis % 1000,
            LEVELS[rng.below(LEVELS.len())],
            rng.below(16),
            if rng.chance(80) { "GET" } else { "POST" },
            PATHS[rng.below(PATHS.len())],
            rng.below(5000),
            STATUS[rng.below(STATUS.len())],
            rng.range(100, 90_000),
            rng.range(1, 400),
            rng.below(4),
            rng.below(256),
            rng.below(256),
        );
    }
    out.truncate(size);
    out.into_bytes()
}

pub fn source(seed: u64, size: usize) -> Vec<u8> {
    use std::fmt::Write;
    const TYPES: [&str; 8] = [
        "u32",
        "usize",
        "u8",
        "bool",
        "String",
        "&str",
        "Vec<u8>",
        "Option<u64>",
    ];
    let mut rng = Rng::new(seed);
    let vocabulary = Vocabulary::new(&mut rng, 1500);
    let ident = |rng: &mut Rng| -> String {
        let parts = rng.range(1, 3);
        (0..parts)
            .map(|_| vocabulary.word(rng).to_string())
            .collect::<Vec<_>>()
            .join("_")
    };
    let mut out = String::with_capacity(size + 4096);
    let _ = writeln!(
        out,
        "use std::collections::HashMap;\nuse std::io::{{self, Read, Write}};\n"
    );
    while out.len() < size {
        let name = ident(&mut rng);
        let _ = writeln!(
            out,
            "/// {} {} {}.",
            ident(&mut rng),
            ident(&mut rng),
            ident(&mut rng)
        );
        let args = rng.range(0, 3);
        let params: Vec<String> = (0..args)
            .map(|_| format!("{}: {}", ident(&mut rng), TYPES[rng.below(TYPES.len())]))
            .collect();
        let _ = writeln!(
            out,
            "pub fn {name}({}) -> {} {{",
            params.join(", "),
            TYPES[rng.below(TYPES.len())]
        );
        let statements = rng.range(3, 18);
        let mut depth = 1usize;
        for _ in 0..statements {
            let indent = "    ".repeat(depth);
            match rng.below(7) {
                0 => {
                    let _ = writeln!(
                        out,
                        "{indent}let {} = {}({});",
                        ident(&mut rng),
                        ident(&mut rng),
                        rng.below(1024)
                    );
                }
                1 if depth < 4 => {
                    let _ = writeln!(
                        out,
                        "{indent}if {} > {} {{",
                        ident(&mut rng),
                        rng.below(256)
                    );
                    depth += 1;
                }
                2 if depth > 1 => {
                    depth -= 1;
                    let _ = writeln!(out, "{}}}", "    ".repeat(depth));
                }
                3 => {
                    let _ = writeln!(
                        out,
                        "{indent}// {} {} {}",
                        ident(&mut rng),
                        ident(&mut rng),
                        ident(&mut rng)
                    );
                }
                4 => {
                    let _ = writeln!(
                        out,
                        "{indent}{}.push(\"{}\".to_string());",
                        ident(&mut rng),
                        ident(&mut rng)
                    );
                }
                5 => {
                    let _ = writeln!(
                        out,
                        "{indent}for {} in 0..{} {{",
                        ident(&mut rng),
                        rng.below(64)
                    );
                    let _ = writeln!(
                        out,
                        "{indent}    {} += {};",
                        ident(&mut rng),
                        ident(&mut rng)
                    );
                    let _ = writeln!(out, "{indent}}}");
                }
                _ => {
                    let _ = writeln!(
                        out,
                        "{indent}self.{} = {}?;",
                        ident(&mut rng),
                        ident(&mut rng)
                    );
                }
            }
        }
        while depth > 1 {
            depth -= 1;
            let _ = writeln!(out, "{}}}", "    ".repeat(depth));
        }
        let _ = writeln!(out, "    {}\n}}\n", ident(&mut rng));
    }
    out.truncate(size);
    out.into_bytes()
}

pub fn x86(seed: u64, size: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let code_size = size * 3 / 4;
    let mut out = Vec::with_capacity(size + 64);
    let mut functions: Vec<usize> = Vec::new();
    let data_base = code_size as u32;
    while out.len() < code_size {
        functions.push(out.len());
        out.extend_from_slice(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x83, 0xEC]);
        out.push((rng.range(1, 8) * 8) as u8);
        let body = rng.range(6, 60);
        for _ in 0..body {
            match rng.below(10) {
                0 | 1 => {
                    let target = if functions.len() > 1 && rng.chance(90) {
                        functions[rng.below(functions.len())]
                    } else {
                        rng.below(code_size)
                    };
                    let rel = (target as i64 - (out.len() as i64 + 5)) as i32;
                    out.push(0xE8);
                    out.extend_from_slice(&rel.to_le_bytes());
                }
                2 => {
                    out.extend_from_slice(&[0x48, 0x8B, 0x45]);
                    out.push(0u8.wrapping_sub((rng.range(1, 16) * 8) as u8));
                }
                3 => {
                    out.extend_from_slice(&[0x48, 0x89, 0x45]);
                    out.push(0u8.wrapping_sub((rng.range(1, 16) * 8) as u8));
                }
                4 => {
                    out.push(0xB8 + rng.below(8) as u8);
                    out.extend_from_slice(&(rng.below(4096) as u32).to_le_bytes());
                }
                5 => {
                    let rel = data_base
                        .wrapping_add(rng.below(size / 4) as u32)
                        .wrapping_sub(out.len() as u32 + 7);
                    out.extend_from_slice(&[0x48, 0x8D, 0x05]);
                    out.extend_from_slice(&rel.to_le_bytes());
                }
                6 => {
                    out.extend_from_slice(&[0x0F, 0x84]);
                    out.extend_from_slice(&(rng.range(8, 400) as i32).to_le_bytes());
                }
                7 => {
                    out.extend_from_slice(&[0x85, 0xC0, 0x74, rng.range(2, 80) as u8]);
                }
                8 => {
                    out.extend_from_slice(&[0x89, 0xC7 + rng.below(8) as u8]);
                }
                _ => {
                    out.extend_from_slice(&[0x48, 0x83, 0xC4, (rng.range(1, 8) * 8) as u8]);
                }
            }
        }
        out.extend_from_slice(&[0x48, 0x89, 0xEC, 0x5D, 0xC3]);
        while !out.len().is_multiple_of(16) {
            out.push(0xCC);
        }
    }
    out.truncate(code_size);
    let vocabulary = Vocabulary::new(&mut rng, 800);
    let strings_end = code_size + (size - code_size) / 2;
    while out.len() < strings_end {
        let word = vocabulary.word(&mut rng);
        out.extend_from_slice(word.as_bytes());
        out.push(if rng.chance(30) { 0 } else { b'_' });
    }
    out.truncate(strings_end);
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
    while out.len() < size {
        let target = functions[rng.below(functions.len())] as u64 + 0x1_4000_1000;
        out.extend_from_slice(&target.to_le_bytes());
    }
    out.truncate(size);
    out
}

pub fn pcm(seed: u64, size: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(size);
    let mut phase = [0f64; 3];
    let steps = [0.0137, 0.0291, 0.0043];
    let mut t = 0usize;
    while out.len() + 4 <= size {
        if t.is_multiple_of(44_100) {
            for step in phase.iter_mut() {
                *step += rng.below(1000) as f64 / 100.0;
            }
        }
        for channel in 0..2 {
            let mut value = 0f64;
            for (i, p) in phase.iter().enumerate() {
                value += ((t as f64) * steps[i] * (1.0 + channel as f64 * 0.01) + p).sin() * 6000.0;
            }
            value += (rng.below(200) as f64) - 100.0;
            out.extend_from_slice(&(value as i16).to_le_bytes());
        }
        t += 1;
    }
    out.resize(size, 0);
    out
}

pub fn bitmap(seed: u64, size: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let width = 1024usize;
    let mut out = Vec::with_capacity(size);
    let mut y = 0usize;
    while out.len() < size {
        for x in 0..width {
            let noise = rng.below(9) as i32 - 4;
            let r = ((x * 255 / width) as i32 + noise).clamp(0, 255) as u8;
            let g = (((y * 3) % 256) as i32 + noise).clamp(0, 255) as u8;
            let b = ((((x + y) / 4) % 256) as i32 + noise).clamp(0, 255) as u8;
            out.extend_from_slice(&[b, g, r]);
        }
        y += 1;
    }
    out.truncate(size);
    out
}

pub fn random(seed: u64, size: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(size + 8);
    while out.len() < size {
        out.extend_from_slice(&rng.next().to_le_bytes());
    }
    out.truncate(size);
    out
}

pub fn small_file(seed: u64) -> (String, Vec<u8>) {
    let mut rng = Rng::new(seed);
    match rng.below(4) {
        0 => {
            let mut text = String::from("{\n");
            let fields = rng.range(2, 30);
            for i in 0..fields {
                text.push_str(&format!(
                    "  \"field_{}\": {}{}\n",
                    rng.below(50),
                    rng.below(100_000),
                    if i + 1 == fields { "" } else { "," }
                ));
            }
            text.push_str("}\n");
            ("json".into(), text.into_bytes())
        }
        1 => ("txt".into(), prose(rng.next(), rng.range(60, 6000))),
        2 => ("rs".into(), source(rng.next(), rng.range(200, 8000))),
        _ => ("bin".into(), x86(rng.next(), rng.range(256, 4096))),
    }
}

/// One file of the corpus: its path inside the corpus and its bytes.
pub struct File {
    pub path: String,
    pub data: Vec<u8>,
}

pub const KINDS: [&str; 7] = ["text", "log", "source", "exe", "media", "random", "small"];

/// The files of one kind, at `scale` (1.0 is the published corpus, 77 MiB
/// in 3,313 files; a test uses a few hundredths of it).
pub fn kind(name: &str, scale: f64) -> Vec<File> {
    let mib = |n: f64| ((n * scale * 1024.0 * 1024.0) as usize).max(1024);
    let file = |path: String, data: Vec<u8>| File { path, data };
    match name {
        "text" => (0..4)
            .map(|i| file(format!("text/book{i}.txt"), prose(100 + i, mib(3.0))))
            .collect(),
        "log" => vec![file("log/server.log".into(), log(200, mib(20.0)))],
        "source" => {
            let count = ((300.0 * scale) as u64).max(2);
            (0..count)
                .map(|i| {
                    let mut rng = Rng::new(300 + i);
                    let size = rng.range(4096, 40_960);
                    let size = ((size as f64) * scale.clamp(0.05, 1.0)) as usize;
                    file(
                        format!("source/m{:02}/f{i:03}.rs", i % 12),
                        source(rng.next(), size.max(512)),
                    )
                })
                .collect()
        }
        "exe" => (0..4)
            .map(|i| file(format!("exe/prog{i}.exe"), x86(400 + i, mib(4.0))))
            .collect(),
        "media" => vec![
            file("media/tone.wav".into(), pcm(500, mib(4.0))),
            file("media/image.bmp".into(), bitmap(501, mib(4.0))),
        ],
        "random" => (0..2)
            .map(|i| file(format!("random/packed{i}.bin"), random(600 + i, mib(4.0))))
            .collect(),
        "small" => {
            let count = ((3000.0 * scale) as u64).max(8);
            (0..count)
                .map(|i| {
                    let (ext, data) = small_file(700 + i);
                    file(format!("small/d{:02}/s{i:04}.{ext}", i % 30), data)
                })
                .collect()
        }
        _ => panic!("unknown corpus kind {name}"),
    }
}
