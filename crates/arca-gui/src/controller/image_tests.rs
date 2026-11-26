use super::*;
use crate::i18n::Lang;
use std::time::Duration;

// The NTFS image arca-disk tests with, unpacked next to the test.
fn ntfs(dir: &Path) -> PathBuf {
    let packed =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../arca-disk/tests/fixtures/ntfs.img.xz");
    let mut data = Vec::new();
    std::io::copy(
        &mut arca_stream::open(&packed, Format::Xz).unwrap(),
        &mut data,
    )
    .unwrap();
    let path = dir.join("volume.img");
    fs::write(&path, data).unwrap();
    path
}

fn settle(controller: &mut AppController) {
    let limit = Instant::now() + Duration::from_secs(10);
    while controller.state.busy || controller.working() {
        controller.receive();
        assert!(Instant::now() < limit, "listing worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_disk_image_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(ntfs(dir.path()), None);
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert_eq!(controller.state.format, Format::Img);
    assert!(controller.state.window_title.contains("IMG: read-only"));
    assert!(controller
        .state
        .entries
        .iter()
        .any(|e| e.name == "many_subdirs/512"));
}

#[test]
fn disk_image_entries_read_extract_and_test() {
    let dir = tempfile::tempdir().unwrap();
    let archive = ntfs(dir.path());
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap().0;
    let index = entries
        .iter()
        .position(|e| e.name == "file-with-12345")
        .unwrap();
    let mut out = Vec::new();
    read_entry(&archive, index, &mut out, None, &|_, _, _| true).unwrap();
    assert_eq!(out, b"12345");
    let path = extract_one(&archive, &entries[index], None, &|_, _, _| true).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"12345");
    let dest = tempfile::tempdir().unwrap();
    let mut left_out = 0;
    let bytes = crate::archive_ops::extract_reporting(
        &archive,
        dest.path(),
        &[],
        &|_, _, _| true,
        &|_| Answer::Replace,
        None,
        &mut left_out,
    )
    .unwrap();
    assert_eq!(bytes, 5 + 1000 + 500_005);
    assert!(dest.path().join("many_subdirs/512").is_dir());
    let (good, bad) = test_archive(&archive, None, None, &|_, _, _| true).unwrap();
    assert_eq!((good, bad.len()), (4, 0));
}

#[test]
fn disk_image_mutation_jobs_are_rejected_before_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    let archive = ntfs(dir.path());
    let before = fs::read(&archive).unwrap();
    let jobs = vec![
        Job::Delete {
            archive: archive.clone(),
            names: vec!["empty-file".into()],
            password: None,
        },
        Job::Add {
            archive: archive.clone(),
            inputs: vec![],
            dir: String::new(),
            codec: Codec::Store,
            level: Level::STORE,
            password: None,
        },
    ];
    for job in jobs {
        let result = run_job_blocking(job, strings(Lang::En), &|_, _, _| true, &|_| Answer::Cancel);
        assert!(result.unwrap_err().contains("read-only"));
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
}
