use super::jobs::Work;
use super::*;
use crate::test_support::Room;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

fn settle(app: &mut AppController) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.state.busy || app.working() {
        assert!(Instant::now() < deadline, "tasks did not finish");
        app.receive();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.receive();
}

fn settle_until(app: &mut AppController, mut ready: impl FnMut(&AppController) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready(app) {
        assert!(Instant::now() < deadline, "condition never came");
        app.receive();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A task that reports it has started and then waits to be let go, so a test
/// can hold the queue in whatever shape it wants to look at.
fn gate() -> (Work, Sender<()>, Receiver<()>) {
    let (release_tx, release_rx) = channel::<()>();
    let (started_tx, started_rx) = channel::<()>();
    let work: Work = Box::new(move |tx, _, stop, hold| {
        let notify = worker_notify(&tx, &stop, &hold);
        let _ = notify(0, 2, "first");
        let _ = started_tx.send(());
        let _ = release_rx.recv();
        let _ = tx.send(if notify(1, 2, "second") {
            Message::Done("finished".into())
        } else {
            Message::Failed(arca_core::Error::Cancelled.to_string())
        });
    });
    (work, release_tx, started_rx)
}

fn status(app: &AppController, id: TaskId) -> TaskStatus {
    app.task(id).expect("task listed").status
}

#[test]
fn tasks_on_different_files_run_side_by_side() {
    let mut app = AppController::new(Settings::default());
    let (a, release_a, started_a) = gate();
    let (b, release_b, started_b) = gate();
    let one = app.enqueue(Task::new("A", "one", a).writing([PathBuf::from("/tmp/one.zip")]));
    let two = app.enqueue(Task::new("B", "two", b).writing([PathBuf::from("/tmp/two.zip")]));
    assert_eq!(status(&app, one), TaskStatus::Running);
    assert_eq!(status(&app, two), TaskStatus::Running);
    started_a.recv().unwrap();
    started_b.recv().unwrap();
    release_a.send(()).unwrap();
    release_b.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, one), TaskStatus::Done);
    assert_eq!(status(&app, two), TaskStatus::Done);
    assert_eq!(app.task(one).unwrap().result, "finished");
    assert_eq!(app.task(two).unwrap().result, "finished");
}

#[test]
fn a_second_rewrite_of_the_same_archive_waits_its_turn() {
    let mut app = AppController::new(Settings::default());
    let archive = PathBuf::from("/tmp/same.zip");
    let (a, release_a, started_a) = gate();
    let (b, release_b, started_b) = gate();
    let (c, release_c, started_c) = gate();
    let first = app.enqueue(Task::new("Delete", "same", a).writing([archive.clone()]));
    let second = app.enqueue(Task::new("Add", "same", b).writing([archive.clone()]));
    let reader = app.enqueue(Task::new("Extract", "same", c).reading([archive.clone()]));
    started_a.recv().unwrap();
    assert_eq!(status(&app, first), TaskStatus::Running);
    assert_eq!(status(&app, second), TaskStatus::Queued);
    assert_eq!(status(&app, reader), TaskStatus::Queued);
    assert!(started_b.try_recv().is_err());

    release_a.send(()).unwrap();
    settle_until(&mut app, |app| status(app, first) == TaskStatus::Done);
    assert_eq!(status(&app, second), TaskStatus::Running);
    assert_eq!(status(&app, reader), TaskStatus::Queued);
    started_b.recv().unwrap();
    release_b.send(()).unwrap();
    settle_until(&mut app, |app| status(app, second) == TaskStatus::Done);
    assert_eq!(status(&app, reader), TaskStatus::Running);
    started_c.recv().unwrap();
    release_c.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, reader), TaskStatus::Done);
}

#[test]
fn two_readers_of_one_archive_do_not_wait_for_each_other() {
    let mut app = AppController::new(Settings::default());
    let archive = PathBuf::from("/tmp/shared.zip");
    let (a, release_a, started_a) = gate();
    let (b, release_b, started_b) = gate();
    let extract = app.enqueue(Task::new("Extract", "shared", a).reading([archive.clone()]));
    let test = app.enqueue(Task::new("Test", "shared", b).reading([archive]));
    assert_eq!(status(&app, extract), TaskStatus::Running);
    assert_eq!(status(&app, test), TaskStatus::Running);
    started_a.recv().unwrap();
    started_b.recv().unwrap();
    release_a.send(()).unwrap();
    release_b.send(()).unwrap();
    settle(&mut app);
}

#[test]
fn stopping_one_row_leaves_the_other_alone() {
    let mut app = AppController::new(Settings::default());
    let (a, release_a, started_a) = gate();
    let (b, release_b, started_b) = gate();
    let stopped = app.enqueue(Task::new("A", "one", a).writing([PathBuf::from("/tmp/x.zip")]));
    let kept = app.enqueue(Task::new("B", "two", b).writing([PathBuf::from("/tmp/y.zip")]));
    started_a.recv().unwrap();
    started_b.recv().unwrap();
    app.hold_task(kept, true);
    assert!(app.task(kept).unwrap().held());
    assert!(!app.task(stopped).unwrap().held());
    app.hold_task(kept, false);
    app.cancel_task(stopped);
    assert!(app.task(stopped).unwrap().stopping());
    assert!(!app.task(kept).unwrap().stopping());
    release_a.send(()).unwrap();
    release_b.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, stopped), TaskStatus::Stopped);
    assert_eq!(status(&app, kept), TaskStatus::Done);
    let words = app.s();
    assert_eq!(app.task(stopped).unwrap().result, words.stopped);
    assert!(!app.state.error);
}

#[test]
fn a_queued_row_can_be_taken_back_before_it_starts() {
    let mut app = AppController::new(Settings::default());
    let archive = PathBuf::from("/tmp/queue.zip");
    let (a, release_a, started_a) = gate();
    let (b, _release_b, started_b) = gate();
    let (c, release_c, started_c) = gate();
    let first = app.enqueue(Task::new("A", "q", a).writing([archive.clone()]));
    let second = app.enqueue(Task::new("B", "q", b).writing([archive.clone()]));
    let third = app.enqueue(Task::new("C", "q", c).writing([archive]));
    started_a.recv().unwrap();
    app.cancel_task(second);
    assert_eq!(status(&app, second), TaskStatus::Stopped);
    assert_eq!(status(&app, third), TaskStatus::Queued);
    release_a.send(()).unwrap();
    settle_until(&mut app, |app| status(app, third) == TaskStatus::Running);
    assert!(started_b.try_recv().is_err());
    started_c.recv().unwrap();
    release_c.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, first), TaskStatus::Done);
    app.dismiss_task(second);
    assert!(app.task(second).is_none());
    app.clear_finished_tasks();
    assert!(app.state.tasks.is_empty());
}

#[test]
fn only_a_rewrite_of_the_open_archive_blocks_the_window() {
    let mut app = AppController::new(Settings::default());
    let archive = PathBuf::from("/tmp/open.zip");
    app.state.archive = Some(archive.clone());
    let (a, release_a, started_a) = gate();
    let reading = app.enqueue(Task::new("Extract", "open", a).reading([archive.clone()]));
    started_a.recv().unwrap();
    assert!(!app.blocked());
    assert!(!app.archive_locked());
    let (b, release_b, started_b) = gate();
    let writing = app.enqueue(Task::new("Delete", "open", b).writing([archive]));
    assert_eq!(status(&app, writing), TaskStatus::Queued);
    assert!(app.blocked(), "a queued rewrite already owns the archive");
    release_a.send(()).unwrap();
    settle_until(&mut app, |app| status(app, reading) == TaskStatus::Done);
    started_b.recv().unwrap();
    assert!(app.archive_locked());
    release_b.send(()).unwrap();
    settle(&mut app);
    assert!(!app.blocked());
}

#[test]
fn each_row_keeps_its_own_outcome() {
    let mut app = AppController::new(Settings::default());
    let good: Work = Box::new(|tx, _, _, _| {
        let _ = tx.send(Message::Done("all good".into()));
    });
    let bad: Work = Box::new(|tx, _, _, _| {
        let _ = tx.send(Message::Failed("went wrong".into()));
    });
    let one = app.enqueue(Task::new("A", "x", good));
    let two = app.enqueue(Task::new("B", "y", bad));
    settle(&mut app);
    assert_eq!(status(&app, one), TaskStatus::Done);
    assert_eq!(app.task(one).unwrap().result, "all good");
    assert_eq!(status(&app, two), TaskStatus::Failed);
    assert_eq!(app.task(two).unwrap().result, "went wrong");
    assert!(app.task(one).unwrap().finished.is_some());
}

#[test]
fn a_question_from_one_task_is_answered_to_that_task() {
    let mut app = AppController::new(Settings::default());
    let (answered_tx, answered_rx) = channel();
    let asker: Work = Box::new(move |tx, replies, _, _| {
        let _ = tx.send(Message::Conflict("there.txt".into()));
        let _ = answered_tx.send(replies.recv().unwrap());
        let _ = tx.send(Message::Done(String::new()));
    });
    let (other, release, started) = gate();
    let asking = app.enqueue(Task::new("Extract", "a", asker));
    let _quiet = app.enqueue(Task::new("Extract", "b", other));
    started.recv().unwrap();
    settle_until(&mut app, |app| app.conflict().is_some());
    assert_eq!(app.conflict(), Some((asking, "there.txt")));
    app.answer_conflict(Answer::SkipAll);
    assert_eq!(answered_rx.recv().unwrap(), Answer::SkipAll);
    assert!(app.conflict().is_none());
    release.send(()).unwrap();
    settle(&mut app);
}

#[test]
fn two_real_extractions_finish_with_their_own_results() {
    let room = Room::new();
    let first = room.archive(None, false);
    let second_dir = room.path("second-src");
    std::fs::create_dir_all(&second_dir).unwrap();
    std::fs::write(second_dir.join("c.txt"), b"third contents").unwrap();
    let second = room.path("second.zip");
    compress(
        &second,
        &[second_dir],
        Format::Zip,
        Codec::Deflate,
        Level::Normal,
        &|_, _, _| true,
        (None, false),
    )
    .unwrap();
    let mut app = AppController::new(Settings::default());
    app.run_job(Job::Extract {
        archives: vec![first.clone()],
        dest: Destination::Subfolder,
        password: None,
    });
    settle_until(&mut app, |app| app.state.tasks.len() == 1);
    app.run_job(Job::Extract {
        archives: vec![second.clone()],
        dest: Destination::Subfolder,
        password: None,
    });
    settle_until(&mut app, |app| app.state.tasks.len() == 2);
    settle(&mut app);
    let outcomes: Vec<_> = app
        .state
        .tasks
        .iter()
        .map(|t| (t.status, t.result.clone()))
        .collect();
    assert!(
        outcomes
            .iter()
            .all(|(status, _)| *status == TaskStatus::Done),
        "{outcomes:?}"
    );
    assert!(room
        .path(&format!("{}/source/a.txt", archive_stem(&first)))
        .exists());
    assert!(room.path("second/second-src/c.txt").exists());
}

#[test]
fn extractions_into_the_same_folder_take_turns() {
    let mut app = AppController::new(Settings::default());
    let (a, release_a, started_a) = gate();
    let (b, release_b, started_b) = gate();
    let (c, release_c, started_c) = gate();
    let into = |work, archive: &str, folder: &str| {
        Task::new("Extract", "x", work)
            .reading([PathBuf::from(archive)])
            .filling([PathBuf::from(folder)])
    };
    let first = app.enqueue(into(a, "/tmp/q/one.zip", "/tmp/q"));
    let nested = app.enqueue(into(b, "/tmp/q/two.zip", "/tmp/q/two"));
    let apart = app.enqueue(into(c, "/tmp/q/one.zip", "/tmp/q-apart"));
    started_a.recv().unwrap();
    started_c.recv().unwrap();
    assert_eq!(status(&app, first), TaskStatus::Running);
    assert_eq!(status(&app, nested), TaskStatus::Queued);
    assert_eq!(status(&app, apart), TaskStatus::Running);
    release_a.send(()).unwrap();
    settle_until(&mut app, |app| status(app, first) == TaskStatus::Done);
    assert_eq!(status(&app, nested), TaskStatus::Running);
    started_b.recv().unwrap();
    release_b.send(()).unwrap();
    release_c.send(()).unwrap();
    settle(&mut app);
}

#[test]
fn an_extraction_fills_the_folder_it_extracts_into() {
    let archive = PathBuf::from("/tmp/dl/pack.zip");
    let job = |dest| Job::Extract {
        archives: vec![archive.clone()],
        dest,
        password: None,
    };
    let (reads, writes, beside) = touched_by(&job(Destination::Beside));
    assert_eq!(reads, vec![archive.clone()]);
    assert!(writes.is_empty());
    assert_eq!(beside, vec![PathBuf::from("/tmp/dl")]);
    let (_, _, sub) = touched_by(&job(Destination::Subfolder));
    assert_eq!(sub, vec![PathBuf::from("/tmp/dl/pack")]);
}

#[test]
fn only_the_copy_that_staged_a_cut_can_arm_it() {
    let mut app = AppController::new(Settings::default());
    let ready: fn() -> Work = || {
        Box::new(|tx, _, _, _| {
            let _ = tx.send(Message::CutReady);
            let _ = tx.send(Message::Done(String::new()));
        })
    };
    let cut = Cut {
        archive: PathBuf::from("/tmp/cut.zip"),
        paths: vec![PathBuf::from("/tmp/clip/a.txt")],
        names: vec!["a.txt".into()],
    };
    let (older, release, started) = gate();
    let older = app.enqueue(Task::new("Cut", "old", older));
    started.recv().unwrap();
    app.state.cut_armed = Some((older + 100, cut));
    let stray = app.enqueue(Task::new("Cut", "stray", ready()));
    release.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, stray), TaskStatus::Done);
    assert!(app.state.cut_pending.is_none());
    assert!(app.state.cut_armed.is_some());

    let (_, cut) = app.state.cut_armed.take().unwrap();
    let mine = app.enqueue(Task::new("Cut", "mine", ready()));
    app.state.cut_armed = Some((mine, cut));
    settle(&mut app);
    assert!(app.state.cut_armed.is_none());
    assert_eq!(
        app.state.cut_pending.as_ref().map(|c| c.names.clone()),
        Some(vec!["a.txt".to_string()])
    );
}

#[test]
fn a_rewrite_that_failed_behind_keeps_the_reread_of_the_one_before() {
    let room = Room::new();
    let archive = room.archive(None, false);
    let mut app = AppController::new(Settings::default());
    app.open_with_password(archive.clone(), None);
    settle(&mut app);
    assert!(!app.state.entries.is_empty());
    app.state.entries.clear();

    let (a, release_a, started_a) = gate();
    let mut first = Task::new("Rename", "x", a).writing([archive.clone()]);
    first.reread_after = Some((archive.clone(), None, String::new()));
    let first = app.enqueue(first);
    let (b, release_b, started_b) = gate();
    let second = app.enqueue(Task::new("Delete", "x", b).writing([archive.clone()]));
    started_a.recv().unwrap();
    release_a.send(()).unwrap();
    settle_until(&mut app, |app| status(app, first) == TaskStatus::Done);
    started_b.recv().unwrap();
    assert!(
        app.state.reread_queued.is_some(),
        "kept while the archive is busy"
    );
    assert!(app.state.entries.is_empty());
    app.cancel_task(second);
    release_b.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, second), TaskStatus::Stopped);
    assert!(app.state.reread_queued.is_none());
    assert!(
        !app.state.entries.is_empty(),
        "the first rewrite was reread"
    );
}

#[test]
fn a_finished_rewrite_does_not_pull_the_window_back() {
    let mut app = AppController::new(Settings::default());
    let there = PathBuf::from("/tmp/there.zip");
    let here = PathBuf::from("/tmp/here.zip");
    app.state.archive = Some(there.clone());
    let (a, release, started) = gate();
    let mut task = Task::new("Rename", "there", a).writing([there.clone()]);
    task.reread_after = Some((there, None, String::new()));
    let id = app.enqueue(task);
    started.recv().unwrap();
    app.state.archive = Some(here.clone());
    release.send(()).unwrap();
    settle(&mut app);
    assert_eq!(status(&app, id), TaskStatus::Done);
    assert!(app.state.reread_queued.is_none());
    assert_eq!(app.state.archive, Some(here));
    assert!(!app.state.busy);
}

#[test]
fn a_worker_that_dies_without_a_word_fails_its_row() {
    let mut app = AppController::new(Settings::default());
    let archive = PathBuf::from("/tmp/dies.zip");
    let dies: Work = Box::new(|_, _, _, _| panic!("worker fell over"));
    let id = app.enqueue(Task::new("Delete", "dies", dies).writing([archive.clone()]));
    let (b, release, started) = gate();
    let next = app.enqueue(Task::new("Add", "dies", b).writing([archive]));
    settle_until(&mut app, |app| status(app, id) == TaskStatus::Failed);
    assert_eq!(app.task(id).unwrap().result, app.s().task_lost);
    started.recv().unwrap();
    assert_eq!(status(&app, next), TaskStatus::Running);
    release.send(()).unwrap();
    settle(&mut app);
}

#[test]
fn an_update_check_that_ends_without_an_answer_stops_being_waited_for() {
    let mut app = AppController::new(Settings::default());
    let (tx, rx) = channel();
    app.state.update_rx = Some(rx);
    app.receive();
    assert!(app.state.update_rx.is_some());
    drop::<Sender<Release>>(tx);
    app.receive();
    assert!(app.state.update_rx.is_none());
    assert!(app.state.update.is_none());
}
