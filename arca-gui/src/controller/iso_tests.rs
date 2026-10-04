use super::*;
use crate::i18n::Lang;
use std::time::Duration;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../arca-iso/tests/fixtures")
        .join(name)
}

fn settle(controller: &mut AppController) {
    let limit = Instant::now() + Duration::from_secs(10);
    while controller.state.busy {
        controller.receive();
        assert!(Instant::now() < limit, "listing worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn an_iso_opens_read_only_and_says_what_it_left_out() {
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(fixture("rockridge.iso"), None);
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert_eq!(controller.state.format, Format::Iso);
    assert!(controller.state.window_title.contains("ISO: read-only"));
    assert!(controller.state.notice.contains("link-to-readme"));
    assert!(controller
        .state
        .entries
        .iter()
        .any(|e| e.name == "docs/deep/er/leaf.bin"));
}

#[test]
fn single_entries_preview_and_extract_by_index() {
    let archive = fixture("rockridge.iso");
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap().0;
    let index = entries.iter().position(|e| e.name == "readme.txt").unwrap();
    let mut out = Vec::new();
    read_entry(&archive, index, &mut out, None, &|_, _, _| true).unwrap();
    assert_eq!(out, b"hello from arca\n");
    let path = extract_one(&archive, &entries[index], None, &|_, _, _| true).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"hello from arca\n");
    let (good, bad) = test_archive(&archive, None, None, &|_, _, _| true).unwrap();
    assert_eq!((good, bad.len()), (5, 0));
    let only: HashSet<String> = ["readme.txt".to_string()].into();
    let (good, bad) = test_archive(&archive, Some(&only), None, &|_, _, _| true).unwrap();
    assert_eq!((good, bad.len()), (1, 0));
}

#[test]
fn iso_mutation_jobs_are_rejected_before_touching_the_image() {
    let archive = fixture("plain.iso");
    let before = fs::read(&archive).unwrap();
    let jobs = vec![
        Job::Password {
            archive: archive.clone(),
            current: None,
            new: Some("no".into()),
        },
        Job::Delete {
            archive: archive.clone(),
            names: vec!["README.TXT".into()],
            password: None,
        },
        Job::Rename {
            archive: archive.clone(),
            from: "README.TXT".into(),
            to: "NEW.TXT".into(),
            folder: false,
            password: None,
        },
        Job::Move {
            archive: archive.clone(),
            moves: vec![("README.TXT".into(), "DOCS/README.TXT".into())],
            password: None,
        },
        Job::NewFolder {
            archive: archive.clone(),
            name: "new/".into(),
            password: None,
        },
        Job::Add {
            archive: archive.clone(),
            inputs: vec![],
            dir: String::new(),
            codec: Codec::Store,
            level: Level::Store,
            password: None,
        },
    ];
    for job in jobs {
        let result = run_job_blocking(job, strings(Lang::En), &|_, _, _| true, &|_| Answer::Cancel);
        assert!(result.unwrap_err().contains("read-only"));
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
    let mut controller = AppController::new(Settings::default());
    controller.state.format = Format::Iso;
    controller.prepare_compress(Vec::new());
    assert_eq!(controller.state.format, Format::Zip);
}

#[test]
fn cancelling_compress_keeps_the_open_iso_read_only() {
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(fixture("rockridge.iso"), None);
    settle(&mut controller);
    controller.prepare_compress(Vec::new());
    assert_eq!(controller.state.format, Format::Zip);
    controller.cancel_compress();
    assert!(matches!(controller.state.view, View::Browse));
    assert_eq!(controller.state.format, Format::Iso);
}
