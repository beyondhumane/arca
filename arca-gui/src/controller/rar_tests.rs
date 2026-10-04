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
        let error = result.unwrap_err();
        assert!(error.contains("not modified"), "{error}");
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

fn rar_job(out: PathBuf, inputs: Vec<PathBuf>, level: Level) -> Job {
    Job::Compress {
        out,
        inputs,
        format: Format::Rar,
        codec: Codec::Store,
        level,
        password: None,
        hide_names: false,
    }
}

#[test]
fn rar_selector_offers_rar_only_with_the_feature_and_never_cbr_or_iso() {
    let formats = AppController::create_formats();
    assert_eq!(formats.contains(&Format::Rar), cfg!(feature = "rar"));
    assert!(!formats.contains(&Format::Cbr));
    assert!(!formats.contains(&Format::Iso));
    assert!(Format::WRITABLE.iter().all(|f| formats.contains(f)));
    assert!(formats.iter().all(|f| f.can_create()));
    assert_eq!(formats.first(), Some(&Format::Zip));
}

#[test]
fn rar_create_jobs_carry_only_the_options_the_writer_takes() {
    let mut app = AppController::new(Settings::default());
    app.prepare_compress(vec![PathBuf::from("/source/a.txt")]);
    assert_eq!(app.state.format, Format::Zip);
    app.state.add_password = "secret".into();
    app.set_create_format(Format::SevenZ);
    app.state.hide_names = true;
    app.set_create_format(Format::Rar);
    assert!(app.state.output_name.ends_with(".rar"));
    assert!(!app.state.hide_names);
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
        assert_eq!(out.extension().unwrap(), "rar");
        assert_eq!(format, Format::Rar);
        assert_eq!(actual, level);
        assert!(password.is_none());
        assert!(!hide_names);
    }
    app.set_create_format(Format::Zip);
    let Some(Job::Compress { password, .. }) = app.compression_job() else {
        panic!("compression job")
    };
    assert_eq!(password.as_deref(), Some("secret"));
}

#[test]
fn rar_creation_opens_the_new_archive_read_only_and_tests_it() {
    let room = crate::test_support::Room::new();
    let source = room.path("source");
    fs::create_dir_all(source.join("nested").join("empty")).unwrap();
    fs::write(source.join("a.txt"), b"first contents").unwrap();
    fs::write(source.join("nested").join("b.txt"), vec![b'x'; 70_000]).unwrap();
    let out = room.path("created.rar");
    for level in [Level::Store, Level::Best] {
        let _ = fs::remove_file(&out);
        let mut controller = AppController::new(Settings::default());
        controller.run_job(rar_job(out.clone(), vec![source.clone()], level));
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);
        assert!(controller.state.notice.contains("created.rar"));
        assert_eq!(controller.state.archive.as_ref(), Some(&out));
        assert_eq!(controller.state.format, Format::Rar);
        assert!(controller.state.window_title.contains("(RAR: read-only)"));
        let mut names: Vec<_> = controller
            .state
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "source",
                "source/a.txt",
                "source/nested",
                "source/nested/b.txt",
                "source/nested/empty",
            ]
        );
        controller.run_job(Job::Test {
            archive: out.clone(),
            only: None,
            password: None,
        });
        settle(&mut controller);
        assert!(!controller.state.error, "{}", controller.state.notice);

        let before = fs::read(&out).unwrap();
        controller.run_job(Job::Add {
            archive: out.clone(),
            inputs: vec![source.join("a.txt")],
            dir: String::new(),
            codec: Codec::Store,
            level: Level::Store,
            password: None,
        });
        settle(&mut controller);
        assert!(controller.state.error);
        assert_eq!(fs::read(&out).unwrap(), before);
    }
}

#[test]
fn rar_creation_rejects_unsupported_options_before_writing() {
    let room = crate::test_support::Room::new();
    let input = room.path("input.txt");
    fs::write(&input, b"input").unwrap();
    let out = room.path("options.rar");
    let attempts: [(Format, PathBuf, Option<&str>, bool, &str); 4] = [
        (
            Format::Rar,
            out.clone(),
            Some("secret"),
            false,
            "encryption",
        ),
        (Format::Rar, out.clone(), None, true, "hidden names"),
        (Format::Rar, room.path("comic.cbr"), None, false, "CBR"),
        (
            Format::Cbr,
            room.path("comic.cbr"),
            None,
            false,
            "read-only",
        ),
    ];
    for (format, out, password, hide_names, reason) in attempts {
        let result = run_job_blocking(
            Job::Compress {
                out: out.clone(),
                inputs: vec![input.clone()],
                format,
                codec: Codec::Store,
                level: Level::Normal,
                password: password.map(str::to_string),
                hide_names,
            },
            strings(Lang::En),
            &|_, _, _| true,
            &|_| Answer::Replace,
        );
        let error = result.unwrap_err();
        assert!(error.contains(reason), "{reason}: {error}");
        assert!(!out.exists(), "{reason} left {}", out.display());
    }
    assert!(fs::read_dir(room.path("")).unwrap().count() == 1);
}

#[test]
fn rar_creation_never_replaces_an_existing_file() {
    let room = crate::test_support::Room::new();
    let input = room.path("input.txt");
    fs::write(&input, b"new contents").unwrap();
    let out = room.path("existing.rar");
    fs::write(&out, b"original archive").unwrap();
    for answer in [Answer::Cancel, Answer::Replace, Answer::Rename] {
        let mut app = AppController::new(Settings::default());
        app.run_job(rar_job(out.clone(), vec![input.clone()], Level::Fast));
        let end = Instant::now() + Duration::from_secs(30);
        while app.state.conflict.is_none() {
            app.receive();
            assert!(Instant::now() < end, "conflict was not delivered");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(fs::read(&out).unwrap(), b"original archive");
        app.dispatch(AppAction::AnswerConflict(answer));
        settle(&mut app);
        assert_eq!(fs::read(&out).unwrap(), b"original archive");
        match answer {
            Answer::Rename => {
                assert!(!app.state.error, "{}", app.state.notice);
                assert_eq!(app.state.archive, Some(room.path("existing (1).rar")));
                assert_eq!(app.state.entries[0].name, "input.txt");
            }
            Answer::Replace => {
                assert!(app.state.error);
                assert!(
                    app.state.notice.contains("never replaces"),
                    "{}",
                    app.state.notice
                );
                assert!(app.state.archive.is_none());
            }
            _ => {
                assert!(app.state.archive.is_none());
                assert!(app.state.reread_after.is_none());
            }
        }
    }
    let mut leftovers: Vec<_> = fs::read_dir(room.path(""))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    leftovers.sort();
    assert_eq!(leftovers, ["existing (1).rar", "existing.rar", "input.txt"]);
}

#[test]
fn rar_creation_stops_when_asked_and_leaves_nothing_behind() {
    let room = crate::test_support::Room::new();
    let source = room.path("source");
    fs::create_dir_all(&source).unwrap();
    for i in 0..8 {
        fs::write(source.join(format!("{i}.bin")), vec![i as u8; 20_000]).unwrap();
    }
    let out = room.path("stopped.rar");
    for stop_after in [0usize, 3, 6] {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = run_job_blocking(
            rar_job(out.clone(), vec![source.clone()], Level::Normal),
            strings(Lang::En),
            &|_, _, _| calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < stop_after,
            &|_| Answer::Cancel,
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(!out.exists());
    }
    assert_eq!(fs::read_dir(room.path("")).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn rar_creation_refuses_symbolic_links_without_writing() {
    let room = crate::test_support::Room::new();
    let source = room.path("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("real.txt"), b"real").unwrap();
    std::os::unix::fs::symlink(source.join("real.txt"), source.join("link.txt")).unwrap();
    let out = room.path("links.rar");
    let result = run_job_blocking(
        rar_job(out.clone(), vec![source], Level::Store),
        strings(Lang::En),
        &|_, _, _| true,
        &|_| Answer::Cancel,
    );
    assert!(result.unwrap_err().contains("not followed"));
    assert!(!out.exists());
}
