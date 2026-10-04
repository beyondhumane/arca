use super::*;
use crate::test_support::Room;

fn settle(controller: &mut AppController) {
    let end = Instant::now() + std::time::Duration::from_secs(30);
    while controller.state.busy || controller.state.preview.status == PreviewStatus::Loading {
        assert!(!controller.receive());
        controller.poll_preview();
        assert!(Instant::now() < end, "worker did not complete");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn hidden_headers_retry_without_listing_and_cancel_cleanly() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    assert!(app.state.busy);
    settle(&mut app);
    assert!(app.state.entries.is_empty());
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
    app.submit_password("wrong".into());
    assert!(app.state.busy);
    settle(&mut app);
    assert!(app.state.entries.is_empty());
    assert!(app.state.password_wrong);
    assert_eq!(app.state.notice, app.s().password_or_corrupt);
    app.cancel_password();
    assert!(app.state.waiting_on_password.is_none());
    assert!(app.state.password_input.is_empty());
    assert!(app.state.archive_password.is_none());
    assert!(app.state.reread_after.is_none());
    app.open(archive);
    settle(&mut app);
    app.submit_password("secret".into());
    settle(&mut app);
    assert_eq!(app.state.entries.len(), 4);
    assert!(app.state.waiting_on_password.is_none());
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
}

#[test]
fn clear_headers_keep_names_and_resume_the_original_preview_after_retry() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let mut app = AppController::new(Settings::default());
    app.open(archive);
    settle(&mut app);
    assert_eq!(app.state.entries.len(), 4);
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::OpenArchive)
    ));
    app.cancel_password();
    let index = app
        .state
        .entries
        .iter()
        .position(|e| e.name == "source/b.txt")
        .unwrap();
    app.view_entry(index);
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::Read(_))
    ));
    app.submit_password("wrong".into());
    settle(&mut app);
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::Read(_))
    ));
    assert!(app.state.viewing.is_none());
    app.submit_password("secret".into());
    settle(&mut app);
    assert_eq!(
        app.state.viewing.as_ref().unwrap().bytes.as_ref(),
        b"second contents"
    );
    assert!(app.state.waiting_on_password.is_none());
}

#[test]
fn hidden_header_extraction_job_retries_then_continues_without_early_writes() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), true);
    let dest = room.path(&archive_stem(&archive));
    let mut app = AppController::new(Settings::default());
    app.run_job(Job::Extract {
        archives: vec![archive],
        dest: Destination::Subfolder,
        password: None,
    });
    settle(&mut app);
    assert!(!dest.exists());
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::Extract(_))
    ));
    app.submit_password("wrong".into());
    settle(&mut app);
    assert!(!dest.exists());
    assert!(app.state.password_wrong);
    app.submit_password("secret".into());
    settle(&mut app);
    assert!(!app.state.error, "{}", app.state.notice);
    assert_eq!(
        fs::read(dest.join("source/a.txt")).unwrap(),
        b"first contents"
    );
    assert!(app.state.waiting_on_password.is_none());
}

#[test]
fn opening_another_archive_discards_entries_pending_actions_and_old_responses() {
    let room = Room::new();
    let archive = room.archive(None, false);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    app.state.waiting_on_password = Some(Pending::Read(Box::new(AppAction::Preview(1))));
    app.state.archive_password = Some("old".into());
    app.state.reread_after = Some((room.path("old.7z"), None, String::new()));
    let (old_tx, old_rx) = channel();
    app.state.channel = Some(old_rx);
    let old_stop = app.state.stop.clone();
    app.open(archive.clone());
    assert!(old_stop.load(std::sync::atomic::Ordering::Relaxed));
    assert!(app.state.entries.is_empty());
    assert!(app.state.checked.is_empty());
    assert!(app.state.waiting_on_password.is_none());
    assert!(app.state.archive_password.is_none());
    assert!(app.state.reread_after.is_none());
    assert!(old_tx
        .send(Message::AccessChecked(
            Pending::Read(Box::new(AppAction::Preview(1))),
            Some("old".into()),
            Ok(())
        ))
        .is_err());
    assert!(old_tx
        .send(Message::Listing(
            room.path("old.7z"),
            Ok(vec![]),
            Some("old".into())
        ))
        .is_err());
    settle(&mut app);
    assert_eq!(app.state.archive, Some(archive));
    assert_eq!(app.state.entries.len(), 4);
    assert!(app.state.viewing.is_none());
}

#[test]
fn listing_errors_and_cancelled_access_do_not_leave_pending_jobs() {
    let room = Room::new();
    let mut app = AppController::new(Settings::default());
    app.open(room.path("missing.7z"));
    settle(&mut app);
    assert!(app.state.error);
    assert!(app.state.entries.is_empty());
    assert!(app.state.waiting_on_password.is_none());
    app.access_failed(
        Pending::Read(Box::new(AppAction::Preview(0))),
        None,
        arca_core::Error::Cancelled,
    );
    assert!(!app.state.error);
    assert_eq!(app.state.notice, app.s().stopped);
    assert!(app.state.waiting_on_password.is_none());
}

#[test]
fn sevenz_creation_preserves_levels_password_and_hidden_names() {
    let mut app = AppController::new(Settings::default());
    app.prepare_compress(vec![PathBuf::from("/source/a.txt")]);
    app.set_create_format(Format::SevenZ);
    app.state.add_password = "secret".into();
    app.state.hide_names = true;
    for level in [Level::Store, Level::Fast, Level::Normal, Level::Best] {
        app.state.level = level;
        let Some(Job::Compress {
            out,
            format,
            level: actual,
            password,
            hide_names,
            ..
        }) = app.compression_job()
        else {
            panic!("compression job")
        };
        assert_eq!(out.extension().unwrap(), "7z");
        assert_eq!(format, Format::SevenZ);
        assert_eq!(actual, level);
        assert_eq!(password.as_deref(), Some("secret"));
        assert!(hide_names);
    }
    app.set_create_format(Format::TarGz);
    let Some(Job::Compress {
        password,
        hide_names,
        ..
    }) = app.compression_job()
    else {
        panic!("compression job")
    };
    assert!(password.is_none());
    assert!(!hide_names);
    app.set_create_format(Format::Zip);
    assert!(app.state.output_name.ends_with(".zip"));
    app.state.output_name = "/output/custom.tar.gz".into();
    app.set_create_format(Format::SevenZ);
    assert_eq!(
        Path::new(&app.state.output_name),
        Path::new("/output/custom.7z")
    );
}

#[test]
fn sevenz_creation_conflicts_preserve_the_original_and_open_the_renamed_output() {
    for answer in [Answer::Cancel, Answer::Rename] {
        let room = Room::new();
        let out = room.path("existing.7z");
        fs::write(&out, b"original archive").unwrap();
        let input = room.path("input.txt");
        fs::write(&input, b"new contents").unwrap();
        let mut app = AppController::new(Settings::default());
        app.run_job(Job::Compress {
            out: out.clone(),
            inputs: vec![input],
            format: Format::SevenZ,
            codec: Codec::Deflate,
            level: Level::Normal,
            password: None,
            hide_names: false,
        });
        let end = Instant::now() + std::time::Duration::from_secs(30);
        while app.state.conflict.is_none() {
            app.receive();
            assert!(Instant::now() < end, "conflict was not delivered");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(fs::read(&out).unwrap(), b"original archive");
        app.dispatch(AppAction::AnswerConflict(answer));
        settle(&mut app);
        assert_eq!(fs::read(&out).unwrap(), b"original archive");
        if answer == Answer::Rename {
            assert_eq!(app.state.archive, Some(room.path("existing (1).7z")));
            assert_eq!(app.state.entries[0].name, "input.txt");
        } else {
            assert!(app.state.archive.is_none());
            assert!(app.state.reread_after.is_none());
        }
    }
}

#[test]
fn selected_extraction_resumes_with_the_selection_after_password_entry() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let mut app = AppController::new(Settings::default());
    app.open(archive);
    settle(&mut app);
    app.cancel_password();
    app.state.into_subfolder = false;
    let index = app
        .state
        .entries
        .iter()
        .position(|e| e.name == "source/b.txt")
        .unwrap();
    app.state.checked[index] = true;
    let dest = room.path("selected");
    app.start_extract_to(true, dest.clone());
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::Read(_))
    ));
    assert!(!dest.exists());
    app.submit_password("secret".into());
    settle(&mut app);
    assert_eq!(
        fs::read(dest.join("source/b.txt")).unwrap(),
        b"second contents"
    );
    assert!(!dest.join("source/a.txt").exists());
}

#[test]
fn sevenz_mutations_are_rejected_before_workers_start() {
    let mut app = AppController::new(Settings::default());
    app.state.archive = Some(PathBuf::from("readonly.7z"));
    app.begin_password_change();
    assert!(app.state.waiting_on_password.is_none());
    app.run_job(Job::Delete {
        archive: PathBuf::from("readonly.7z"),
        names: vec!["a".into()],
        password: None,
    });
    assert!(!app.state.busy);
    assert_eq!(app.state.notice, app.s().only_zip_can_change);
}
