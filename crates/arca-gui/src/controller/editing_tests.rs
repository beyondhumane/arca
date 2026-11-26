use super::*;
use crate::test_support::Room;

fn settle(controller: &mut AppController) {
    let end = Instant::now() + std::time::Duration::from_secs(60);
    while controller.state.busy || controller.working() {
        assert!(!controller.receive());
        assert!(Instant::now() < end, "worker did not complete");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn names(app: &AppController) -> Vec<String> {
    let mut names: Vec<String> = app.state.entries.iter().map(|e| e.name.clone()).collect();
    names.sort();
    names
}

fn tar_gz(room: &Room) -> PathBuf {
    let source = room.path("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), b"first").unwrap();
    fs::write(source.join("b.txt"), b"second").unwrap();
    let out = room.path("bundle.tar.gz");
    compress(
        &out,
        &[source],
        Format::TarGz,
        Codec::Deflate,
        Level::FAST,
        &|_, _, _| true,
        (None, false),
    )
    .unwrap();
    out
}

#[test]
fn a_tar_gz_can_be_changed_and_the_change_taken_back() {
    let room = Room::new();
    let archive = tar_gz(&room);
    let before = fs::read(&archive).unwrap();
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    assert!(app.writable());
    assert!(app.can_drop());
    app.run_job(Job::Delete {
        archive: archive.clone(),
        names: vec!["source/a.txt".into()],
        password: None,
    });
    settle(&mut app);
    assert!(!app.state.error, "{}", app.state.notice);
    assert_eq!(names(&app), ["source/b.txt"]);
    assert!(app.state.undo.is_some());
    app.undo_last();
    settle(&mut app);
    assert_eq!(fs::read(&archive).unwrap(), before);
    assert_eq!(names(&app), ["source/a.txt", "source/b.txt"]);
}

#[test]
fn an_encrypted_7z_asks_for_its_password_before_it_is_changed() {
    let room = Room::new();
    let archive = room.archive(Some("secret"), false);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    app.cancel_password();
    assert!(app.state.archive_password.is_none());
    assert!(app.writable());
    app.run_job(Job::Delete {
        archive: archive.clone(),
        names: vec!["source/a.txt".into()],
        password: None,
    });
    assert!(matches!(
        app.state.waiting_on_password,
        Some(Pending::Read(_))
    ));
    assert!(!app.working());
    app.submit_password("secret".into());
    settle(&mut app);
    assert!(!app.state.error, "{}", app.state.notice);
    assert_eq!(app.state.archive_password.as_deref(), Some("secret"));
    assert!(!names(&app).contains(&"source/a.txt".to_string()));
    assert!(names(&app).contains(&"source/b.txt".to_string()));
}

#[test]
fn the_password_stays_a_zip_thing() {
    let room = Room::new();
    let archive = tar_gz(&room);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    app.begin_password_change();
    assert!(app.state.waiting_on_password.is_none());
    assert!(app.state.error);
    assert_eq!(app.state.notice, app.s().only_zip_can_change);
    app.state.error = false;
    app.run_job(Job::Password {
        archive,
        current: None,
        new: Some("x".into()),
    });
    assert!(!app.working());
    assert_eq!(app.state.notice, app.s().only_zip_can_change);
}

#[test]
fn a_comment_refused_on_a_tar_says_only_zip_comments_change() {
    let room = Room::new();
    let archive = tar_gz(&room);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    app.run_job(Job::Comment {
        archive,
        entry: None,
        text: "hello".into(),
        password: None,
    });
    assert!(!app.working());
    assert!(app.state.error);
    assert_eq!(app.state.notice, app.s().comment_only_zip);
}
