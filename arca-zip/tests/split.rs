use arca_core::volumes::{self, part_path};
use arca_core::{Codec, Level};
use arca_zip::{create_zip_into, Source, ZipArchive};
use std::fs;
use std::io::BufWriter;
use std::path::PathBuf;

fn room(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arca-zip-split-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn noise(len: usize, seed: u32) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn a_zip_written_into_small_parts_reads_back_from_any_of_them() {
    let dir = room("parts");
    let mut sources = Vec::new();
    for (i, len) in [70_000usize, 0, 5, 130_000].into_iter().enumerate() {
        let path = dir.join(format!("in{i}"));
        fs::write(&path, noise(len, i as u32 + 1)).unwrap();
        sources.push(Source {
            path,
            name: format!("dir/f{i}.bin"),
            size: len as u64,
            mtime: 1_700_000_000,
            codec: if i == 3 { Codec::Store } else { Codec::Deflate },
            level: Level::NORMAL,
        });
    }
    let out = dir.join("out.zip");
    let (_, written) = volumes::staged(&out, Some(16 * 1024), false, |w| {
        create_zip_into(BufWriter::new(w), &sources, 2, None, &|_, _, _| true)?;
        Ok(())
    })
    .unwrap();
    assert!(!out.exists());
    let parts = (1..).take_while(|&i| part_path(&out, i).exists()).count();
    assert!(parts > 10, "{parts}");
    let total: u64 = (1..=parts as u32)
        .map(|i| fs::metadata(part_path(&out, i)).unwrap().len())
        .sum();
    assert_eq!(total, written);
    for opened in [1, 2, parts as u32] {
        let mut a = ZipArchive::open(volumes::open(&part_path(&out, opened)).unwrap()).unwrap();
        assert_eq!(a.len(), 4);
        for (i, s) in sources.iter().enumerate() {
            let mut data = Vec::new();
            a.extract_to_with(i, &mut data, None).unwrap();
            assert_eq!(data, fs::read(&s.path).unwrap());
        }
    }
    fs::write(part_path(&out, parts as u32), b"").unwrap();
    assert!(volumes::open(&part_path(&out, 1)).is_err());
    fs::remove_file(part_path(&out, parts as u32)).unwrap();
    let truncated = volumes::open(&part_path(&out, 1)).unwrap();
    assert!(ZipArchive::open(truncated).is_err());
    let _ = fs::remove_dir_all(&dir);
}
