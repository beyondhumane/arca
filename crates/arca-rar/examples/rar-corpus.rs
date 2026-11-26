//! Writes the RAR benchmark corpus to a directory.
//!
//! ```sh
//! cargo run --release -p arca-rar --example rar-corpus -- <dir> [scale] [kind...]
//! ```
//!
//! The same scale writes the same bytes everywhere. Each kind lands in its own
//! subdirectory, so a run can time one kind or the whole tree.

#[path = "../tests/corpus/mod.rs"]
mod corpus;

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next().map(PathBuf::from) else {
        eprintln!("usage: rar-corpus <dir> [scale] [kind...]");
        std::process::exit(2);
    };
    let scale = args
        .next()
        .map(|s| s.parse::<f64>().expect("scale is a number"))
        .unwrap_or(1.0);
    let kinds: Vec<String> = args.collect();
    let kinds: Vec<&str> = if kinds.is_empty() {
        corpus::KINDS.to_vec()
    } else {
        kinds.iter().map(String::as_str).collect()
    };
    let mut total = 0usize;
    let mut files = 0usize;
    for kind in kinds {
        let mut bytes = 0usize;
        let generated = corpus::kind(kind, scale);
        for file in &generated {
            let path = dir.join(&file.path);
            std::fs::create_dir_all(path.parent().unwrap()).expect("create directory");
            std::fs::write(&path, &file.data).expect("write file");
            bytes += file.data.len();
        }
        println!("{kind:8} {:6} files {bytes:12} bytes", generated.len());
        total += bytes;
        files += generated.len();
    }
    println!("{:8} {files:6} files {total:12} bytes", "total");
}
