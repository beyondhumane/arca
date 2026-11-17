//! Writes a deterministic tree of files, for measuring the per-file cost of
//! `arca create` and `arca extract`:
//!
//! ```sh
//! cargo run --release -p arca-bench --example small_files -- <dir> [count] [max] [mixed|text]
//! ```
//!
//! `count` files (50,000 by default) of 0 to `max` bytes each (16,384 by
//! default), 500 to a folder, cut one after another out of the seeded corpus
//! `arca bench --codecs` uses: by default three parts prose-like text to one
//! part random bytes, or only the text with `text`. The same arguments give
//! the same bytes on every machine. Every file gets the same modification
//! time, so archives of the tree compare by hash. The directory must not
//! exist yet.

use arca_bench::corpus::{generate, Kind, CHUNK, SEED};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

const PER_DIR: usize = 500;

fn number(arg: Option<std::ffi::OsString>, default: u64) -> u64 {
    match arg {
        Some(n) => n.to_string_lossy().parse().unwrap_or_else(|_| {
            eprintln!("count and max must be numbers");
            std::process::exit(2)
        }),
        None => default,
    }
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: small_files <dir> [count] [max] [mixed|text]");
        std::process::exit(2);
    };
    let count = number(args.next(), 50_000) as usize;
    let max = number(args.next(), 16 * 1024);
    let kind = match args.next().as_ref().and_then(|k| k.to_str()) {
        None | Some("mixed") => Kind::Mixed,
        Some("text") => Kind::Text,
        Some(other) => {
            eprintln!("'{other}' is not mixed or text");
            std::process::exit(2);
        }
    };
    if root.exists() {
        eprintln!("{} already exists", root.display());
        std::process::exit(2);
    }

    let mut state = SEED ^ 0x736d_616c_6c5f_6669;
    let sizes: Vec<usize> = (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((state >> 33) % (max + 1)) as usize
        })
        .collect();
    let total: usize = sizes.iter().sum();
    let chunks = generate(kind, SEED, total.div_ceil(CHUNK).max(1));
    let data: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();

    let stamp = UNIX_EPOCH + Duration::from_secs(1_577_836_800);
    let mut at = 0;
    for (i, size) in sizes.iter().enumerate() {
        let dir = root.join(format!("d{:03}", i / PER_DIR));
        if i % PER_DIR == 0 {
            fs::create_dir_all(&dir)?;
        }
        let path = dir.join(format!("f{i:06}.txt"));
        fs::write(&path, &data[at..at + size])?;
        fs::File::options()
            .write(true)
            .open(&path)?
            .set_modified(stamp)?;
        at += size;
    }
    println!("{count} files, {total} bytes in {}", root.display());
    Ok(())
}
