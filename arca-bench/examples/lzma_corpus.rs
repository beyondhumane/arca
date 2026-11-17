//! Writes the deterministic corpus the LZMA measurements in
//! docs/guide/benchmarks.md use: prose, source code, machine code and random
//! bytes, generated from fixed seeds so every machine gets the same files.
//!
//! cargo run --release -p arca-bench --example lzma_corpus -- DIR [MIB]

use arca_bench::corpus::{generate, Kind, SEED};
use std::fs;
use std::io::Write;
use std::path::Path;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn skewed(&mut self, n: usize) -> usize {
        let a = self.below(n);
        let b = self.below(n);
        a * b / n
    }
}

const KEYWORDS: [&str; 16] = [
    "let", "if", "else", "for", "while", "return", "match", "fn", "struct", "impl", "pub", "mut",
    "const", "use", "self", "where",
];
const PARTS: [&str; 32] = [
    "buf", "len", "pos", "count", "index", "value", "size", "data", "entry", "name", "path",
    "offset", "limit", "state", "error", "result", "reader", "writer", "block", "chunk", "header",
    "flags", "mode", "next", "prev", "total", "start", "end", "table", "node", "key", "item",
];
const TYPES: [&str; 10] = [
    "u8",
    "u16",
    "u32",
    "u64",
    "usize",
    "bool",
    "String",
    "Vec<u8>",
    "Option<u64>",
    "Result<()>",
];

fn ident(rng: &mut Rng) -> String {
    let n = 1 + rng.below(3);
    (0..n)
        .map(|_| PARTS[rng.skewed(PARTS.len())])
        .collect::<Vec<_>>()
        .join("_")
}

fn source(len: usize) -> Vec<u8> {
    let mut rng = Rng(SEED ^ 0x736f_7572_6365);
    let mut out = String::with_capacity(len + 4096);
    while out.len() < len {
        let name = ident(&mut rng);
        out.push_str(&format!(
            "/// Returns the {} of the {}.\npub fn {}(",
            PARTS[rng.below(PARTS.len())],
            PARTS[rng.below(PARTS.len())],
            name
        ));
        for i in 0..rng.below(4) {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!(
                "{}: {}",
                ident(&mut rng),
                TYPES[rng.skewed(TYPES.len())]
            ));
        }
        out.push_str(&format!(") -> {} {{\n", TYPES[rng.skewed(TYPES.len())]));
        let mut depth = 1;
        for _ in 0..4 + rng.below(24) {
            let indent = "    ".repeat(depth);
            match rng.below(6) {
                0 if depth < 4 => {
                    out.push_str(&format!(
                        "{indent}{} {} < {} {{\n",
                        KEYWORDS[1 + rng.below(4)],
                        ident(&mut rng),
                        rng.below(4096)
                    ));
                    depth += 1;
                }
                1 if depth > 1 => {
                    depth -= 1;
                    out.push_str(&format!("{}}}\n", "    ".repeat(depth)));
                }
                2 => out.push_str(&format!(
                    "{indent}{}.{}({}, {});\n",
                    ident(&mut rng),
                    ident(&mut rng),
                    ident(&mut rng),
                    rng.below(256)
                )),
                3 => out.push_str(&format!(
                    "{indent}{} = {} + {}[{}];\n",
                    ident(&mut rng),
                    ident(&mut rng),
                    ident(&mut rng),
                    rng.below(64)
                )),
                _ => out.push_str(&format!(
                    "{indent}let {} = {}::{}(&{})?;\n",
                    ident(&mut rng),
                    TYPES[rng.skewed(TYPES.len())],
                    ident(&mut rng),
                    ident(&mut rng)
                )),
            }
        }
        while depth > 1 {
            depth -= 1;
            out.push_str(&format!("{}}}\n", "    ".repeat(depth)));
        }
        out.push_str(&format!("    {}\n}}\n\n", ident(&mut rng)));
    }
    out.truncate(len);
    out.into_bytes()
}

fn machine(len: usize) -> Vec<u8> {
    const OPCODES: [&[u8]; 16] = [
        &[0x48, 0x89, 0xC7],
        &[0x48, 0x8B, 0x45],
        &[0x48, 0x83, 0xEC],
        &[0x55],
        &[0x5D],
        &[0xC3],
        &[0x31, 0xC0],
        &[0x48, 0x85, 0xC0],
        &[0x0F, 0x84],
        &[0x74],
        &[0x75],
        &[0x89, 0x44, 0x24],
        &[0x8B, 0x4C, 0x24],
        &[0x41, 0x57],
        &[0x4C, 0x8D, 0x05],
        &[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00],
    ];
    let mut rng = Rng(SEED ^ 0x6269_6e61_7279);
    let mut out = Vec::with_capacity(len + 4096);
    while out.len() < len {
        match rng.below(8) {
            0 => {
                let base = rng.next() as u32 & 0x00FF_FFF0;
                for i in 0..64 + rng.below(512) {
                    let v = base + i as u32 * (8 + 8 * rng.below(3) as u32);
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            1 => out.resize(out.len() + 16 * (1 + rng.below(64)), 0),
            _ => {
                let at = out.len() as u32;
                for _ in 0..256 + rng.below(4096) {
                    let op = OPCODES[rng.skewed(OPCODES.len())];
                    out.extend_from_slice(op);
                    match rng.below(5) {
                        0 => {
                            out.push(0xE8);
                            let target = (rng.below(4096) as u32 * 16).wrapping_sub(at);
                            out.extend_from_slice(&target.to_le_bytes());
                        }
                        1 => out.push(rng.below(128) as u8 & 0xF8),
                        _ => {}
                    }
                }
            }
        }
    }
    out.truncate(len);
    out
}

fn random(len: usize) -> Vec<u8> {
    let mut rng = Rng(SEED ^ 0x7261_6e64);
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        out.extend_from_slice(&rng.next().to_le_bytes());
    }
    out.truncate(len);
    out
}

fn write(dir: &Path, name: &str, data: &[u8]) {
    let mut file = fs::File::create(dir.join(name)).expect("create corpus file");
    file.write_all(data).expect("write corpus file");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .expect("usage: lzma_corpus DIR [MIB per kind, default 24]");
    let mib: usize = args.next().map_or(24, |s| s.parse().expect("MIB"));
    let dir = Path::new(&dir);
    fs::create_dir_all(dir).expect("create corpus directory");
    let text: Vec<u8> = generate(Kind::Text, SEED, mib).concat();
    write(dir, "text.txt", &text);
    write(dir, "source.rs", &source(mib << 20));
    write(dir, "machine.bin", &machine(mib << 20));
    write(dir, "random.bin", &random((mib << 20) / 3));
}
