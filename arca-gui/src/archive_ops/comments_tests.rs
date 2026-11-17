use super::*;
use crate::i18n::{strings, Lang};
use crate::test_support::Room;
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

fn comment_job(archive: &Path, entry: Option<&str>, text: &str) -> Job {
    Job::Comment {
        archive: archive.to_path_buf(),
        entry: entry.map(str::to_string),
        text: text.to_string(),
        password: None,
    }
}

#[test]
fn a_comment_job_rewrites_the_zip_and_keeps_the_old_one_for_undo() {
    let room = Room::new();
    let archive = zip(&room, "c.zip");
    let before = fs::read(&archive).unwrap();
    assert_eq!(
        run(comment_job(
            &archive,
            None,
            "l\u{ed}nea uno\nl\u{ed}nea dos"
        ))
        .unwrap(),
        strings(Lang::En).comment_saved
    );
    assert_eq!(fs::read(undo_path(&archive)).unwrap(), before);
    run(comment_job(&archive, Some("dir/b.txt"), "nota")).unwrap();
    let a = ZipArchive::open(fs::File::open(&archive).unwrap()).unwrap();
    assert_eq!(a.comment(), "l\u{ed}nea uno\nl\u{ed}nea dos");
    assert_eq!(a.entry_comment(1).as_deref(), Some("nota"));
    assert_eq!(a.entry_comment(0), None);

    let comments = read_comments(&archive, None).unwrap();
    assert_eq!(comments.archive, "l\u{ed}nea uno\nl\u{ed}nea dos");
    let offset = a.entries()[1].offset;
    assert_eq!(
        comments.entries.get(&offset).map(String::as_str),
        Some("nota")
    );
    assert_eq!(comments.entries.len(), 1);

    let error = run(comment_job(&archive, Some("missing.txt"), "x")).unwrap_err();
    assert!(error.contains("no entry named"), "{error}");
    assert!(!room.path("c.zip.arca-new").exists());
}

#[test]
fn only_a_zip_takes_a_comment_and_comments_never_stop_a_listing() {
    let room = Room::new();
    let seven = room.archive(None, false);
    assert_eq!(
        run(comment_job(&seven, None, "x")).unwrap_err(),
        strings(Lang::En).comment_only_zip
    );
    assert!(!can_edit(&comment_job(&seven, None, "x")));
    assert_eq!(read_comments(&seven, None), None);
    let broken = room.path("broken.zip");
    fs::write(&broken, b"PK\x05\x06 not a whole end record").unwrap();
    assert_eq!(read_comments(&broken, None), None);
    assert_eq!(notice_line(" \r\n\t", "C"), None);
    assert_eq!(notice_line("a\nb", "C").as_deref(), Some("C: a b"));
}
