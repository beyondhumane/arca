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

fn zip(room: &Room) -> PathBuf {
    let path = room.path("notes.zip");
    let mut w = arca_zip::ZipWriter::new(fs::File::create(&path).unwrap());
    w.add("a.txt", &b"alpha"[..], Codec::Store, Level::STORE, None)
        .unwrap();
    w.add("b.txt", &b"beta"[..], Codec::Store, Level::STORE, None)
        .unwrap();
    w.finish().unwrap();
    path
}

#[test]
fn a_comment_is_shown_saved_and_taken_back() {
    let room = Room::new();
    let archive = zip(&room);
    let mut app = AppController::new(Settings::default());
    app.open(archive.clone());
    settle(&mut app);
    assert!(app.can_comment());
    assert_eq!(app.archive_comment(), "");
    let b = app
        .state
        .entries
        .iter()
        .position(|e| e.name == "b.txt")
        .unwrap();
    assert_eq!(app.entry_comment(b), None);

    app.save_comment(Some("b.txt".into()), "segunda".into());
    settle(&mut app);
    assert!(!app.state.error, "{}", app.state.notice);
    let b = app
        .state
        .entries
        .iter()
        .position(|e| e.name == "b.txt")
        .unwrap();
    assert_eq!(app.entry_comment(b), Some("segunda"));
    app.save_comment(None, "del archivo".into());
    settle(&mut app);
    assert_eq!(app.archive_comment(), "del archivo");
    assert!(app.state.undo.is_some());
    app.undo_last();
    settle(&mut app);
    assert_eq!(app.archive_comment(), "");
}
