use super::*;
use crate::i18n::Lang;
use std::time::Duration;

fn cabinet() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../arca-extra/tests/fixtures/lzx.cab")
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
fn a_cabinet_opens_read_only() {
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(cabinet(), None);
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert_eq!(controller.state.format, Format::Cab);
    assert!(controller.state.window_title.contains("CAB: read-only"));
    assert!(controller
        .state
        .entries
        .iter()
        .any(|e| e.name == "docs/big.txt"));
}

#[test]
fn cabinet_entries_read_extract_and_test() {
    let archive = cabinet();
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap().0;
    let mut out = Vec::new();
    read_entry(&archive, 0, &mut out, None, &|_, _, _| true).unwrap();
    assert_eq!(out, b"hello from a cabinet\n");
    let path = extract_one(&archive, &entries[0], None, &|_, _, _| true).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"hello from a cabinet\n");
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
    assert!(bytes > 100_000);
    assert!(dest.path().join("docs/big.txt").is_file());
    let (good, bad) = test_archive(&archive, None, None, &|_, _, _| true).unwrap();
    assert_eq!((good, bad.len()), (2, 0));
    let only: HashSet<String> = ["a.txt".to_string()].into();
    let (good, bad) = test_archive(&archive, Some(&only), None, &|_, _, _| true).unwrap();
    assert_eq!((good, bad.len()), (1, 0));
}

#[test]
fn cabinet_mutation_jobs_are_rejected_before_touching_it() {
    let archive = cabinet();
    let before = fs::read(&archive).unwrap();
    let jobs = vec![
        Job::Delete {
            archive: archive.clone(),
            names: vec!["a.txt".into()],
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
