use super::*;
use crate::i18n::{strings, Lang};
use crate::test_support::Room;
use arca_tar::TarWriter;
use arca_zip::ZipArchive;

const GO: &(dyn Fn(usize, usize, &str) -> bool + Sync) = &|_, _, _| true;

fn run(job: Job) -> std::result::Result<String, String> {
    run_job_blocking(job, strings(Lang::En), GO, &|_| Answer::Cancel)
}

fn zip(room: &Room, name: &str) -> PathBuf {
    let path = room.path(name);
    let mut w = arca_zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (entry, data) in [("a.txt", &b"first file"[..]), ("dir/b.txt", b"second file")] {
        w.add(entry, data, Codec::Deflate, Level::NORMAL, None)
            .unwrap();
    }
    w.set_comment("antes".as_bytes()).unwrap();
    w.finish().unwrap();
    path
}

#[test]
fn the_repaired_copy_is_named_after_the_archive_and_never_takes_a_name() {
    let room = Room::new();
    let tgz = room.path("photos.tar.gz");
    fs::write(&tgz, b"").unwrap();
    let first = repaired_name(&tgz, "repaired");
    assert_eq!(first, room.path("photos (repaired).tar.gz"));
    fs::write(&first, b"").unwrap();
    assert_eq!(
        repaired_name(&tgz, "repaired"),
        room.path("photos (repaired 2).tar.gz")
    );
    assert_eq!(
        repaired_name(&room.path("notes"), "reparado"),
        room.path("notes (reparado)")
    );
}

fn repair_job(archive: &Path, out: &Path) -> Job {
    Job::Repair {
        archive: archive.to_path_buf(),
        out: out.to_path_buf(),
        password: None,
    }
}

#[test]
fn a_zip_without_its_directory_is_repaired_into_a_new_file() {
    let room = Room::new();
    let good = fs::read(zip(&room, "good.zip")).unwrap();
    let cd = good
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    let broken = room.path("broken.zip");
    fs::write(&broken, &good[..cd]).unwrap();
    let out = room.path("fixed.zip");
    let text = run(repair_job(&broken, &out)).unwrap();
    assert_eq!(text, "fixed.zip: 2 entries recovered, 0 lost");
    assert_eq!(fs::read(&broken).unwrap(), &good[..cd]);
    let a = ZipArchive::open(fs::File::open(&out).unwrap()).unwrap();
    assert_eq!(a.len(), 2);
    assert!(!room.path("fixed.zip.arca-new").exists());

    let error = run(repair_job(&broken, &broken)).unwrap_err();
    assert!(error.contains("cannot replace"), "{error}");
    let nothing = room.path("nothing.zip");
    fs::write(&nothing, b"PK\x03\x04 hardly an archive").unwrap();
    assert!(run(repair_job(&nothing, &room.path("none.zip"))).is_err());
    assert!(!room.path("none.zip").exists());
    let iso = room.path("disc.iso");
    fs::write(&iso, b"").unwrap();
    assert_eq!(
        run(repair_job(&iso, &room.path("disc2.iso"))).unwrap_err(),
        strings(Lang::En).repair_cannot
    );
}

#[test]
fn a_tar_gz_with_a_bad_header_keeps_the_rest() {
    let room = Room::new();
    let mut w = TarWriter::new(Vec::new());
    for (name, data) in [
        ("a.txt", &b"alpha"[..]),
        ("b.txt", b"beta"),
        ("c.txt", b"gamma"),
    ] {
        w.add(name, data.len() as u64, 1_600_000_000, 0o644, data)
            .unwrap();
    }
    let mut tar = w.finish().unwrap();
    tar[1024 + 1] ^= 0x20;
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(&tar).unwrap();
    let archive = room.path("broken.tar.gz");
    fs::write(&archive, e.finish().unwrap()).unwrap();
    let out = room.path("fixed.tar.gz");
    let text = run(repair_job(&archive, &out)).unwrap();
    assert_eq!(text, "fixed.tar.gz: 2 entries recovered, 1 lost");
    let mut r = arca_tar::TarReader::new(open_source(&out, Format::TarGz).unwrap());
    let mut names = Vec::new();
    while let Some(e) = r.next_entry().unwrap() {
        names.push(e.entry.name.clone());
        r.skip_data(&e).unwrap();
    }
    assert_eq!(names, ["a.txt", "c.txt"]);
    assert!(!room.path("fixed.tar.gz.arca-new.spool").exists());
}
