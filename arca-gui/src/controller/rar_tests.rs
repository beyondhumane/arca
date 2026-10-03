use super::*;
use crate::i18n::Lang;
use std::time::Duration;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../arca-rar/tests/fixtures")
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
fn encrypted_header_listing_prompts_then_retries_in_a_worker() {
    let mut controller = AppController::new(Settings::default());
    let archive = fixture("headers.rar");
    controller.load_listing(archive.clone());
    settle(&mut controller);
    assert!(controller.state.entries.is_empty());
    assert!(
        matches!(&controller.state.waiting_on_password, Some(Pending::ListArchive(path)) if path == &archive)
    );
    assert!(!controller.state.password_wrong);
    controller.submit_password("wrong".into());
    settle(&mut controller);
    assert!(controller.state.password_wrong);
    assert!(controller.state.entries.is_empty());
    assert!(controller.state.archive_password.is_none());
    controller.submit_password("arca-test-only".into());
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert_eq!(
        controller
            .state
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .count(),
        2
    );
    assert!(controller.state.waiting_on_password.is_none());
    assert!(!controller.state.password_wrong);
    assert!(controller.state.window_title.contains("read-only"));
    controller.load_listing(archive);
    settle(&mut controller);
    assert!(controller.state.waiting_on_password.is_none());
    assert!(!controller.state.error);
}

#[test]
fn visible_encrypted_entries_use_the_rar_password_flow() {
    let mut controller = AppController::new(Settings::default());
    controller.load_listing(fixture("encrypted.rar"));
    settle(&mut controller);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::ListArchive(_))
    ));
    controller.submit_password("arca-test-only".into());
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert!(controller.state.waiting_on_password.is_none());
    controller.run_job(Job::Test {
        archive: fixture("encrypted.rar"),
        only: None,
        password: None,
    });
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert!(controller.state.waiting_on_password.is_none());
}

#[test]
fn standalone_rar_test_prompts_from_the_worker() {
    let mut controller = AppController::new(Settings::default());
    controller.run_job(Job::Test {
        archive: fixture("headers.rar"),
        only: None,
        password: None,
    });
    settle(&mut controller);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::TestArchive(_))
    ));
    controller.submit_password("arca-test-only".into());
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert!(controller.state.waiting_on_password.is_none());
}

#[test]
fn rar_mutation_jobs_are_rejected_before_touching_the_archive() {
    let archive = fixture("plain.rar");
    let before = fs::read(&archive).unwrap();
    let jobs = vec![
        Job::Password {
            archive: archive.clone(),
            current: None,
            new: Some("no".into()),
        },
        Job::Delete {
            archive: archive.clone(),
            names: vec!["first.txt".into()],
            password: None,
        },
        Job::Rename {
            archive: archive.clone(),
            from: "first.txt".into(),
            to: "new.txt".into(),
            folder: false,
            password: None,
        },
        Job::Move {
            archive: archive.clone(),
            moves: vec![("first.txt".into(), "folder/new.txt".into())],
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
    controller.state.format = Format::Rar;
    controller.prepare_compress(Vec::new());
    assert_eq!(controller.state.format, Format::Zip);
}
