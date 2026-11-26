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
fn a_repair_offers_its_copy_and_opening_it_lists_it() {
    let room = Room::new();
    let good = fs::read(zip(&room)).unwrap();
    let cd = good
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    let broken = room.path("broken.zip");
    fs::write(&broken, &good[..cd]).unwrap();
    assert!(AppController::can_repair(&broken));
    assert!(!AppController::can_repair(&room.path("missing.zip")));
    let mut app = AppController::new(Settings::default());
    app.start_repair(broken.clone());
    settle(&mut app);
    assert!(!app.state.error, "{}", app.state.notice);
    let offer = app
        .repair_offer()
        .expect("a repair that wrote a copy offers it");
    let out = offer.path.clone();
    assert_eq!(out, room.path("broken (repaired).zip"));
    assert!(offer.text.contains("2 entries recovered"), "{}", offer.text);
    app.answer_repair_offer(true);
    settle(&mut app);
    assert!(app.repair_offer().is_none());
    assert_eq!(app.state.archive.as_ref(), Some(&out));
    assert_eq!(app.state.entries.len(), 2);
    assert_eq!(fs::read(&broken).unwrap(), &good[..cd]);
}
