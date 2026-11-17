use arca_core::hash::{Algorithm, Hasher};
use arca_core::Result;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const BUF: usize = 256 * 1024;

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Kind {
    Crc32,
    Sha1,
    Sha256,
}

impl Kind {
    fn algorithm(self) -> Algorithm {
        match self {
            Kind::Crc32 => Algorithm::Crc32,
            Kind::Sha1 => Algorithm::Sha1,
            Kind::Sha256 => Algorithm::Sha256,
        }
    }
}

/// Answers whether every file was read and matched.
pub(crate) fn run(paths: &[PathBuf], kind: Option<Kind>, check: Option<&Path>) -> Result<bool> {
    let forced = kind.map(Kind::algorithm);
    match check {
        Some(sums) => verify(sums, forced),
        None => print_sums(paths, forced.unwrap_or(Algorithm::Sha256)),
    }
}

fn print_sums(paths: &[PathBuf], algorithm: Algorithm) -> Result<bool> {
    let (files, mut failed) = collect(paths);
    for (path, error) in &failed {
        eprintln!("arca: {}: {error}", shown(path));
    }
    let mut out = BufWriter::new(io::stdout().lock());
    hash_in_order(
        &files,
        |_| algorithm,
        |path, result| {
            match result {
                Ok(hex) => writeln!(out, "{}", sum_line(&hex, &shown(path)))?,
                Err(error) => {
                    out.flush()?;
                    eprintln!("arca: {}: {error}", shown(path));
                    failed.push((path.to_path_buf(), error));
                }
            }
            Ok(())
        },
    )?;
    out.flush()?;
    Ok(failed.is_empty())
}

fn verify(sums: &Path, forced: Option<Algorithm>) -> Result<bool> {
    let bytes = fs::read(sums)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut listed = Vec::new();
    let mut malformed = 0usize;
    for (n, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(line, forced) {
            Some(entry) => listed.push(entry),
            None => {
                malformed += 1;
                eprintln!("arca: {}:{}: not a checksum line", sums.display(), n + 1);
            }
        }
    }
    if listed.is_empty() {
        eprintln!(
            "arca: {}: no properly formatted checksum lines found",
            sums.display()
        );
        return Ok(false);
    }
    let paths: Vec<PathBuf> = listed
        .iter()
        .map(|(_, _, name)| PathBuf::from(name))
        .collect();
    let mut mismatched = 0usize;
    let mut unreadable = 0usize;
    let mut out = BufWriter::new(io::stdout().lock());
    let mut next = 0usize;
    hash_in_order(
        &paths,
        |i| listed[i].0,
        |path, result| {
            let (_, wanted, name) = &listed[next];
            let name = super::printable(name);
            next += 1;
            match result {
                Ok(found) if found == *wanted => writeln!(out, "{name}: OK")?,
                Ok(_) => {
                    mismatched += 1;
                    writeln!(out, "{name}: FAILED")?;
                }
                Err(error) => {
                    unreadable += 1;
                    writeln!(out, "{name}: FAILED open or read")?;
                    out.flush()?;
                    eprintln!(
                        "arca: {}: {error}",
                        super::printable(&path.to_string_lossy())
                    );
                }
            }
            Ok(())
        },
    )?;
    out.flush()?;
    if malformed > 0 {
        eprintln!("arca: WARNING: {malformed} lines are improperly formatted");
    }
    if unreadable > 0 {
        eprintln!("arca: WARNING: {unreadable} listed files could not be read");
    }
    if mismatched > 0 {
        eprintln!("arca: WARNING: {mismatched} computed checksums did NOT match");
    }
    Ok(mismatched == 0 && unreadable == 0)
}

// Files are hashed a batch at a time on every core and handed back in the
// order they were listed, so the output is the same on every run and starts
// before the last file is read.
fn hash_in_order(
    files: &[PathBuf],
    algorithm: impl Fn(usize) -> Algorithm + Sync,
    mut each: impl FnMut(&Path, io::Result<String>) -> Result<()>,
) -> Result<()> {
    let batch = rayon::current_num_threads().max(1) * 4;
    let mut start = 0usize;
    for chunk in files.chunks(batch) {
        let results: Vec<_> = chunk
            .par_iter()
            .enumerate()
            .map(|(i, path)| hash_file(path, algorithm(start + i)))
            .collect();
        for (path, result) in chunk.iter().zip(results) {
            each(path, result)?;
        }
        start += chunk.len();
    }
    Ok(())
}

fn hash_file(path: &Path, algorithm: Algorithm) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new(&[algorithm]);
    let mut buf = vec![0u8; BUF];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish().remove(0).1)
}

// Paths named on the command line are followed when they are links, as
// sha256sum does. Inside a folder a linked file is followed too, but a linked
// folder is not, so a link back up the tree cannot make the walk endless.
fn collect(inputs: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, io::Error)>) {
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for input in inputs {
        match fs::metadata(input) {
            Ok(meta) if meta.is_dir() => walk(input, &mut files, &mut failed),
            Ok(_) => files.push(input.clone()),
            Err(e) => failed.push((input.clone(), e)),
        }
    }
    (files, failed)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>, failed: &mut Vec<(PathBuf, io::Error)>) {
    let mut children: Vec<_> = match fs::read_dir(dir) {
        Ok(read) => read.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            failed.push((dir.to_path_buf(), e));
            return;
        }
    };
    children.sort_by_key(|d| d.file_name());
    for child in children {
        let path = child.path();
        match child.file_type() {
            Ok(kind) if kind.is_dir() => walk(&path, files, failed),
            Ok(kind) if kind.is_file() => files.push(path),
            Ok(kind) if kind.is_symlink() => {
                if fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                    files.push(path);
                }
            }
            Ok(_) => {}
            Err(e) => failed.push((path, e)),
        }
    }
}

fn shown(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

// The sha256sum line: a name holding a backslash or a line break is written
// escaped, and the line then starts with a backslash to say so.
fn sum_line(hex: &str, name: &str) -> String {
    if name.contains(['\\', '\n', '\r']) {
        let escaped = name
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        format!("\\{hex}  {escaped}")
    } else {
        format!("{hex}  {name}")
    }
}

fn parse_line(line: &str, forced: Option<Algorithm>) -> Option<(Algorithm, String, String)> {
    let (escaped, line) = match line.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (hex, rest) = line.split_once(' ')?;
    let name = rest.strip_prefix([' ', '*'])?;
    if name.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let algorithm = match forced {
        Some(a) if a.hex_len() == hex.len() => a,
        Some(_) => return None,
        None => Algorithm::from_hex_len(hex.len())?,
    };
    let name = if escaped {
        unescape(name)?
    } else {
        name.to_string()
    };
    Some((algorithm, hex.to_ascii_lowercase(), name))
}

fn unescape(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sum_lines_are_read_the_way_sha256sum_writes_them() {
        let read = |line: &str| parse_line(line, None);
        assert_eq!(
            read(&format!("{ABC}  a b.txt")),
            Some((Algorithm::Sha256, ABC.to_string(), "a b.txt".to_string()))
        );
        assert_eq!(read(&format!("{ABC} *bin")).unwrap().2, "bin");
        assert_eq!(
            read("CBF43926  x").unwrap(),
            (Algorithm::Crc32, "cbf43926".to_string(), "x".to_string())
        );
        assert_eq!(read(&format!("\\{ABC}  a\\\\b\\nc")).unwrap().2, "a\\b\nc");
        assert_eq!(sum_line(ABC, "a\\b\nc"), format!("\\{ABC}  a\\\\b\\nc"));
    }

    #[test]
    fn malformed_and_truncated_sum_lines_are_refused() {
        let truncated = &ABC[..63];
        for line in [
            "",
            " ",
            "abc",
            ABC,
            &format!("{ABC} "),
            &format!("{ABC}  "),
            &format!("{ABC}\tname"),
            &format!("{ABC} name"),
            &format!("{truncated}  name"),
            &format!("{}g  name", &ABC[..63]),
            &format!("\\{ABC}  bad\\escape"),
            &format!("\\{ABC}  trailing\\"),
            "\u{e9}",
            "\\",
        ] {
            assert_eq!(parse_line(line, None), None, "{line:?}");
        }
        assert_eq!(
            parse_line(&format!("{ABC}  x"), Some(Algorithm::Crc32)),
            None
        );
    }

    #[test]
    fn a_file_that_changes_or_disappears_after_the_walk_is_reported_not_fatal() {
        let room = tempfile::tempdir().unwrap();
        let dir = room.path();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/gone.txt"), b"soon gone").unwrap();
        fs::write(dir.join("sub/shrinks.txt"), b"abc and then some").unwrap();
        let (files, failed) = collect(&[dir.join("sub"), dir.join("missing")]);
        assert_eq!(files.len(), 2);
        assert_eq!(failed.len(), 1);

        fs::remove_file(dir.join("sub/gone.txt")).unwrap();
        fs::write(dir.join("sub/shrinks.txt"), b"abc").unwrap();
        let mut seen = Vec::new();
        hash_in_order(
            &files,
            |_| Algorithm::Sha256,
            |path, result| {
                seen.push((path.to_path_buf(), result.ok()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(seen[0], (dir.join("sub/gone.txt"), None));
        assert_eq!(
            seen[1],
            (dir.join("sub/shrinks.txt"), Some(ABC.to_string()))
        );
    }
}
