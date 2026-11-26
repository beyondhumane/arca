use arca_rar::{Conflict, RarArchive};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn main() {
    let input = fs::read(std::env::args_os().nth(1).expect("case file")).unwrap();
    if input.len() > 128 * 1024 || input.len() < 2 {
        return;
    }
    let mode = input[0];
    let password = match input[1] % 4 {
        0 => None,
        1 => Some("arca-test-only"),
        2 => Some("password"),
        _ => Some("wrong"),
    };
    let dir = tempfile::tempdir().unwrap();
    let inputs = dir.path().join("input");
    fs::create_dir(&inputs).unwrap();
    let mut rest = &input[2..];
    let mut paths = Vec::new();
    while rest.len() >= 4 && paths.len() < 4 {
        let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        rest = &rest[4..];
        if len > 32768 || len > rest.len() {
            return;
        }
        let index = paths.len();
        let name = match (mode & 3) % 3 {
            0 => format!("set.part{}.rar", index + 1),
            1 => format!("set.part{:02}.rar", index + 1),
            _ if index == 0 => "set.rar".into(),
            _ => format!("set.r{:02}", index - 1),
        };
        let path = inputs.join(name);
        fs::write(&path, &rest[..len]).unwrap();
        paths.push(path);
        rest = &rest[len..];
    }
    if paths.is_empty() || !rest.is_empty() {
        return;
    }
    let deadline = || Instant::now() + Duration::from_millis(250);
    let until = deadline();
    let selected = if mode & 4 == 0 {
        &paths[0]
    } else {
        paths.last().unwrap()
    };
    let Ok(archive) =
        RarArchive::open_with_progress(selected, password, &|_, _, _| Instant::now() < until)
    else {
        println!("rejected");
        return;
    };
    println!("opened");
    // These additional harness budgets do not change the production adapter's limits.
    if archive.entries().len() > 128
        || archive.entries().iter().map(|e| e.size).sum::<u64>() > 256 * 1024
    {
        println!("budget-skipped");
        return;
    }
    let until = deadline();
    let verified = archive
        .test(password, &|_, _, _| Instant::now() < until)
        .is_ok();
    if verified {
        println!("verified");
    }
    let dest = dir.path().join("output");
    let sentinel = dir.path().join("sentinel");
    fs::write(&sentinel, b"never overwrite outside the destination").unwrap();
    let publication = AtomicBool::new(false);
    let until = deadline();
    let result = archive.extract(
        &dest,
        &[],
        password,
        &|_, total, name| {
            if name == "RAR" && total > 0 {
                publication.store(true, Ordering::SeqCst);
            }
            Instant::now() < until && (mode & 8 == 0 || name == "RAR")
        },
        &|_| match (mode >> 4) % 4 {
            0 => Conflict::Overwrite,
            1 => Conflict::Rename,
            2 => Conflict::Skip,
            _ => Conflict::Cancel,
        },
    );
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"never overwrite outside the destination"
    );
    if result.is_err() && !publication.load(Ordering::SeqCst) {
        assert!(!dest.exists(), "verification failure published output");
    }
    if result.is_ok() {
        println!("extracted");
        // Inspect published paths, not just the decoder's error code.
        for (index, entry) in archive.entries().iter().enumerate() {
            let path = dest.join(&entry.name);
            let canonical = path.canonicalize().unwrap();
            assert!(canonical.starts_with(dest.canonicalize().unwrap()));
            assert!(!fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink());
            if !entry.is_dir {
                let bytes = fs::read(path).unwrap();
                assert_eq!(bytes.len() as u64, entry.size);
                if index < 4 {
                    assert_eq!(archive.read_entry(index, password).unwrap(), bytes);
                }
            }
        }
    }
}
