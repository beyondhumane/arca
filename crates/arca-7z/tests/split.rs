use arca_7z::{create_7z_into, CreateOptions, SevenZArchive, Source};
use arca_core::volumes::{self, part_path};
use arca_core::Level;
use std::fs;
use std::io::BufWriter;

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
fn a_7z_written_into_small_parts_reads_back_from_any_of_them() {
    let temp = tempfile::tempdir().unwrap();
    let mut sources = vec![Source {
        path: temp.path().join("dir"),
        name: "dir/".into(),
    }];
    fs::create_dir(temp.path().join("dir")).unwrap();
    let mut expected = Vec::new();
    for (i, len) in [90_000usize, 0, 40_000].into_iter().enumerate() {
        let path = temp.path().join("dir").join(format!("f{i}"));
        let data = noise(len, i as u32 + 7);
        fs::write(&path, &data).unwrap();
        sources.push(Source {
            path,
            name: format!("dir/f{i}"),
        });
        expected.push((format!("dir/f{i}"), data));
    }
    let out = temp.path().join("out.7z");
    for password in [None, Some("secret")] {
        let options = CreateOptions {
            level: Level::FAST,
            password,
            hide_names: password.is_some(),
            ..Default::default()
        };
        volumes::staged(&out, Some(10_000), true, |w| {
            create_7z_into(BufWriter::new(w), &sources, &options, &mut |_| true)?;
            Ok(())
        })
        .unwrap();
        assert!(part_path(&out, 10).exists());
        let last = (1..).take_while(|&i| part_path(&out, i).exists()).count() as u32;
        for opened in [1, last] {
            let source = volumes::open(&part_path(&out, opened)).unwrap();
            let mut archive = SevenZArchive::open(source, password).unwrap();
            for (name, data) in &expected {
                let at = archive
                    .entries()
                    .iter()
                    .position(|e| &e.name == name)
                    .unwrap();
                assert_eq!(
                    &archive.read_entry(at, u64::MAX, &mut |_| true).unwrap(),
                    data
                );
            }
            archive.test(&mut |_| true).unwrap();
        }
        let second = fs::read(part_path(&out, 2)).unwrap();
        fs::write(part_path(&out, 2), &second[..100]).unwrap();
        assert!(volumes::open(&part_path(&out, 1)).is_err());
        fs::write(part_path(&out, 2), &second).unwrap();
    }
}
