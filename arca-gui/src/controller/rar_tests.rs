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
fn encrypted_header_listing_prompts_retries_and_refreshes_in_a_worker() {
    let mut controller = AppController::new(Settings::default());
    let archive = fixture("headers.rar");
    controller.open(archive.clone());
    settle(&mut controller);
    assert!(controller.state.entries.is_empty());
    assert!(matches!(
        &controller.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
    assert!(!controller.state.password_wrong);
    controller.submit_password("wrong".into());
    settle(&mut controller);
    assert!(controller.state.password_wrong);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
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
    controller.dispatch(AppAction::Refresh);
    settle(&mut controller);
    assert!(controller.state.waiting_on_password.is_none());
    assert!(!controller.state.error);
    assert_eq!(controller.state.archive.as_ref(), Some(&archive));
    assert_eq!(
        controller.state.archive_password.as_deref(),
        Some("arca-test-only")
    );
    assert_eq!(
        controller
            .state
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .count(),
        2
    );
}

#[test]
fn a_failed_rar_password_can_be_cancelled_before_opening_another_archive() {
    for name in ["headers.rar", "encrypted.rar"] {
        let mut controller = AppController::new(Settings::default());
        controller.dispatch(AppAction::Open(fixture(name)));
        settle(&mut controller);
        for _ in 0..2 {
            controller.dispatch(AppAction::SubmitPassword("wrong".into()));
            settle(&mut controller);
            assert!(controller.state.password_wrong);
            assert!(matches!(
                controller.state.waiting_on_password,
                Some(Pending::OpenArchive)
            ));
            assert!(!controller.state.busy);
        }
        controller.dispatch(AppAction::CancelPassword);
        assert!(controller.state.waiting_on_password.is_none());
        assert!(!controller.state.password_wrong);
        assert!(controller.state.password_input.is_empty());
        controller.dispatch(AppAction::Open(fixture("plain.rar")));
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert!(controller.state.waiting_on_password.is_none());
        assert!(!controller.state.entries.is_empty());
    }
}

#[test]
fn opening_another_archive_does_not_reuse_the_cached_password() {
    let mut controller = AppController::new(Settings::default());
    controller.state.archive = Some(fixture("encrypted.rar"));
    controller.state.archive_password = Some("arca-test-only".into());
    controller.dispatch(AppAction::Open(fixture("headers.rar")));
    settle(&mut controller);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
    assert!(controller.state.archive_password.is_none());
    assert!(controller.state.entries.is_empty());
}

#[test]
fn cancelling_standalone_encrypted_test_returns_to_browsing() {
    let mut controller = AppController::new(Settings::default());
    controller.dispatch(AppAction::Run(Job::Test {
        archive: fixture("headers.rar"),
        only: None,
        password: None,
    }));
    settle(&mut controller);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::Extract(_))
    ));
    assert!(matches!(controller.state.view, View::Running));
    controller.dispatch(AppAction::SetPasswordInput("unfinished".into()));
    controller.dispatch(AppAction::CancelPassword);
    assert!(matches!(controller.state.view, View::Browse));
    assert!(controller.state.waiting_on_password.is_none());
    assert!(controller.state.password_input.is_empty());
    assert!(!controller.state.busy);
    assert!(controller.state.archive.is_none());
    assert!(controller.state.archive_password.is_none());
}

#[test]
fn visible_encrypted_entries_use_the_rar_password_flow() {
    let mut controller = AppController::new(Settings::default());
    controller.open(fixture("encrypted.rar"));
    settle(&mut controller);
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::OpenArchive)
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
        Some(Pending::Extract(_))
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

#[test]
fn rar_and_cbr_keep_their_read_only_titles() {
    let room = crate::test_support::Room::new();
    for (name, format, label) in [
        ("archive.RAR", Format::Rar, "RAR"),
        ("comic.CBR", Format::Cbr, "CBR"),
    ] {
        let archive = room.path(name);
        fs::copy(fixture("plain.rar"), &archive).unwrap();
        let mut controller = AppController::new(Settings::default());
        controller.open(archive);
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert_eq!(controller.state.format, format);
        assert!(controller
            .state
            .window_title
            .contains(&format!("({label}: read-only)")));
        assert!(!controller.state.entries.is_empty());
    }
}

#[test]
fn opening_later_modern_and_legacy_volumes_lists_and_tests_the_complete_set() {
    let dir = std::env::temp_dir().join(format!(
        "arca-rar-volumes-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    for (i, name) in ["volume.rar", "volume.r00", "volume.r01", "volume.r02"]
        .iter()
        .enumerate()
    {
        fs::copy(
            fixture(&format!("volume.part{}.rar", i + 1)),
            dir.join(name),
        )
        .unwrap();
    }
    for archive in [fixture("volume.part4.rar"), dir.join("volume.r02")] {
        let mut controller = AppController::new(Settings::default());
        controller.dispatch(AppAction::Open(archive.clone()));
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert_eq!(controller.state.entries.len(), 4);
        assert!(controller.state.window_title.contains("read-only"));
        assert!(!controller.state.window_title.contains("experimental"));
        controller.run_job(Job::Test {
            archive,
            only: None,
            password: None,
        });
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn switching_between_rar_sevenz_and_zip_keeps_passwords_scoped_to_the_archive() {
    let room = crate::test_support::Room::new();
    let sevenz = room.archive(Some("secret"), true);
    let input = room.path("zip-input.txt");
    fs::write(&input, b"zip contents").unwrap();
    let zip = room.path("encrypted.zip");
    compress(
        &zip,
        &[input],
        Format::Zip,
        Codec::Store,
        Level::Store,
        &|_, _, _| true,
        (Some("zip-secret"), false),
    )
    .unwrap();
    let mut controller = AppController::new(Settings::default());
    for (archive, format, password) in [
        (fixture("headers.rar"), Format::Rar, "arca-test-only"),
        (sevenz, Format::SevenZ, "secret"),
        (zip, Format::Zip, "zip-secret"),
        (fixture("encrypted.rar"), Format::Rar, "arca-test-only"),
    ] {
        controller.open(archive.clone());
        settle(&mut controller);
        assert!(controller.state.archive_password.is_none());
        assert!(matches!(
            controller.state.waiting_on_password,
            Some(Pending::OpenArchive)
        ));
        controller.submit_password("wrong".into());
        settle(&mut controller);
        assert!(controller.state.password_wrong);
        controller.submit_password(password.into());
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert_eq!(controller.state.format, format);
        assert_eq!(controller.state.archive_password.as_deref(), Some(password));
        assert!(controller.state.waiting_on_password.is_none());
        controller.dispatch(AppAction::Refresh);
        settle(&mut controller);
        assert_eq!(controller.state.archive.as_ref(), Some(&archive));
        assert!(controller.state.waiting_on_password.is_none());
        assert_eq!(
            controller.state.window_title.contains("read-only"),
            format == Format::Rar
        );
    }
}

#[test]
fn rar_selected_extraction_retries_without_writing_and_resumes_only_the_selection() {
    let room = crate::test_support::Room::new();
    let dest = room.path("selected-rar");
    let mut controller = AppController::new(Settings::default());
    controller.open(fixture("encrypted.rar"));
    settle(&mut controller);
    controller.cancel_password();
    controller.state.into_subfolder = false;
    let index = controller
        .state
        .entries
        .iter()
        .position(|e| e.name == "folder/second.txt")
        .unwrap();
    controller.state.checked[index] = true;
    controller.start_extract_to(true, dest.clone());
    assert!(matches!(
        controller.state.waiting_on_password,
        Some(Pending::Read(_))
    ));
    controller.submit_password("wrong".into());
    settle(&mut controller);
    assert!(controller.state.password_wrong);
    assert!(!dest.exists());
    controller.submit_password("arca-test-only".into());
    settle(&mut controller);
    assert!(!controller.state.error, "{}", controller.state.notice);
    assert!(!dest.join("first.txt").exists());
    assert_eq!(
        fs::read(dest.join("folder/second.txt")).unwrap(),
        b"Arca RAR fixture beta\n".repeat(64)
    );
}

#[test]
fn stale_rar_listing_cannot_replace_a_new_sevenz_archive() {
    let room = crate::test_support::Room::new();
    let sevenz = room.archive(None, false);
    let mut controller = AppController::new(Settings::default());
    controller.open(fixture("headers.rar"));
    let (old_tx, old_rx) = channel();
    controller.state.channel = Some(old_rx);
    controller.open(sevenz.clone());
    assert!(old_tx
        .send(Message::Listing(
            fixture("headers.rar"),
            Err(arca_core::Error::PasswordRequired),
            None
        ))
        .is_err());
    settle(&mut controller);
    assert_eq!(controller.state.archive, Some(sevenz));
    assert_eq!(controller.state.format, Format::SevenZ);
    assert!(controller.state.waiting_on_password.is_none());
    assert_eq!(controller.state.entries.len(), 4);
}

#[test]
fn rar_open_file_obeys_the_worker_cancellation_callback() {
    let room = crate::test_support::Room::new();
    let archive = room.path("cancel-open.rar");
    fs::copy(fixture("plain.rar"), &archive).unwrap();
    let entries = list_entries(&archive, None, &|_, _, _| true).unwrap().0;
    let entry = entries.iter().find(|e| !e.is_dir).unwrap();
    assert!(matches!(
        extract_one(&archive, entry, None, &|_, _, _| false),
        Err(arca_core::Error::Cancelled)
    ));
}
