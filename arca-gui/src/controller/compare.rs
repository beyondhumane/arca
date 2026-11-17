use super::{worker_notify, AppAction, AppController, Message, Task};
use crate::fill;
use arca_compare::{compare, Change, Difference, Options, Side};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum CompareFilter {
    #[default]
    All,
    OnlyFirst,
    OnlySecond,
    Changed,
}

impl CompareFilter {
    pub(crate) const ALL: [CompareFilter; 4] = [
        CompareFilter::All,
        CompareFilter::OnlyFirst,
        CompareFilter::OnlySecond,
        CompareFilter::Changed,
    ];

    pub(crate) fn keeps(self, change: Change) -> bool {
        match self {
            CompareFilter::All => true,
            CompareFilter::OnlyFirst => change == Change::Removed,
            CompareFilter::OnlySecond => change == Change::Added,
            CompareFilter::Changed => {
                matches!(change, Change::Changed | Change::Type | Change::Time)
            }
        }
    }
}

pub(crate) struct Comparison {
    pub(crate) first: String,
    pub(crate) second: String,
    pub(crate) rows: Vec<Difference>,
    pub(crate) filter: CompareFilter,
}

impl Comparison {
    pub(crate) fn shown(&self) -> Vec<&Difference> {
        self.rows
            .iter()
            .filter(|row| self.filter.keeps(row.change))
            .collect()
    }

    pub(crate) fn count(&self, filter: CompareFilter) -> usize {
        self.rows
            .iter()
            .filter(|row| filter.keeps(row.change))
            .count()
    }

    pub(crate) fn lines(&self) -> String {
        self.shown()
            .iter()
            .map(|row| format!("{}\n", row.line()))
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct CompareState {
    jobs: Vec<Receiver<Comparison>>,
    shown: Vec<Comparison>,
}

fn label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

impl AppController {
    pub(crate) fn start_compare(&mut self, first: Option<PathBuf>, second: PathBuf) {
        let (first, password) = match first {
            Some(first) => (first, None),
            None => {
                let again = AppAction::Compare {
                    first: None,
                    second: second.clone(),
                };
                if !self.read_access(again) {
                    return;
                }
                let Some(archive) = self.state.archive.clone() else {
                    return;
                };
                (archive, self.state.archive_password.clone())
            }
        };
        let s = self.s();
        let (names, other) = (label(&first), label(&second));
        let subject = fill(s.compare_subject, &[("a", &names), ("b", &other)]);
        let (report, reports) = channel();
        self.compare.jobs.push(reports);
        let reads = [first.clone(), second.clone()];
        let task = Task::new(
            s.comparing,
            subject,
            Box::new(move |tx, _, stop, hold| {
                let notify = worker_notify(&tx, &stop, &hold);
                let found = Side::open(&first, password.as_deref(), &notify)
                    .and_then(|a| Ok((a, Side::open(&second, password.as_deref(), &notify)?)))
                    .and_then(|(a, b)| compare(&a, &b, Options::default(), &notify));
                let _ = tx.send(match found {
                    Ok(rows) => {
                        let done = if rows.is_empty() {
                            s.compare_identical.to_string()
                        } else {
                            fill(s.compare_done, &[("n", &rows.len().to_string())])
                        };
                        let _ = report.send(Comparison {
                            first: names,
                            second: other,
                            rows,
                            filter: CompareFilter::All,
                        });
                        Message::Done(done)
                    }
                    Err(e) => Message::Failed(e.to_string()),
                });
            }),
        )
        .reading(reads);
        self.enqueue(task);
    }

    pub(crate) fn compare_picked(&mut self) -> bool {
        let mut picked = self.selected_disk_paths();
        if picked.len() != 2 {
            return false;
        }
        let second = picked.pop().unwrap_or_default();
        let first = picked.pop();
        self.dispatch(AppAction::Compare { first, second });
        true
    }

    pub(crate) fn poll_compare(&mut self) {
        let state = &mut self.compare;
        state.jobs.retain(|rx| match rx.try_recv() {
            Ok(found) => {
                state.shown.push(found);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
    }

    pub(crate) fn comparison(&self) -> Option<&Comparison> {
        self.compare.shown.first()
    }

    pub(crate) fn comparison_mut(&mut self) -> Option<&mut Comparison> {
        self.compare.shown.first_mut()
    }

    pub(crate) fn close_comparison(&mut self) {
        if !self.compare.shown.is_empty() {
            self.compare.shown.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Settings, TaskStatus};
    use arca_core::{Codec, Level};
    use std::io::Write;
    use std::time::{Duration, Instant};

    fn settle(app: &mut AppController) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.state.busy || app.working() {
            assert!(Instant::now() < deadline, "tasks did not finish");
            app.receive();
            std::thread::sleep(Duration::from_millis(5));
        }
        app.receive();
    }

    fn zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = arca_zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            w.add(name, *data, Codec::Store, Level::STORE, None)
                .unwrap();
        }
        w.finish().unwrap().flush().unwrap();
    }

    #[test]
    fn the_open_archive_is_compared_in_the_background_and_filtered() {
        let room = tempfile::tempdir().unwrap();
        let (a, b) = (room.path().join("a.zip"), room.path().join("b.zip"));
        zip(
            &a,
            &[("same.txt", b"same"), ("old.txt", b"old"), ("x.txt", b"1")],
        );
        zip(
            &b,
            &[("same.txt", b"same"), ("new.txt", b"new"), ("x.txt", b"2")],
        );
        let mut app = AppController::new(Settings::default());
        app.load_listing(a.clone(), None);
        settle(&mut app);

        app.dispatch(AppAction::Compare {
            first: None,
            second: b.clone(),
        });
        settle(&mut app);
        assert!(app.state.tasks.iter().any(|t| t.status == TaskStatus::Done));
        let found = app.comparison().expect("a comparison");
        assert_eq!(
            (found.first.as_str(), found.second.as_str()),
            ("a.zip", "b.zip")
        );
        assert_eq!(found.lines(), "+ new.txt\n- old.txt\nM x.txt\n");
        assert_eq!(found.count(CompareFilter::OnlyFirst), 1);
        assert_eq!(found.count(CompareFilter::OnlySecond), 1);
        assert_eq!(found.count(CompareFilter::Changed), 1);
        app.comparison_mut().unwrap().filter = CompareFilter::OnlySecond;
        assert_eq!(app.comparison().unwrap().lines(), "+ new.txt\n");
        app.close_comparison();
        assert!(app.comparison().is_none());

        let copy = room.path().join("copy.zip");
        std::fs::copy(&a, &copy).unwrap();
        app.dispatch(AppAction::Compare {
            first: Some(a.clone()),
            second: copy,
        });
        settle(&mut app);
        assert!(app.comparison().expect("a comparison").rows.is_empty());
    }

    #[test]
    fn a_damaged_side_fails_the_task_and_shows_nothing() {
        let room = tempfile::tempdir().unwrap();
        let a = room.path().join("a.zip");
        zip(&a, &[("x.txt", b"1")]);
        let damaged = room.path().join("damaged.zip");
        let data = std::fs::read(&a).unwrap();
        std::fs::write(&damaged, &data[..data.len() / 2]).unwrap();
        let mut app = AppController::new(Settings::default());
        app.dispatch(AppAction::Compare {
            first: Some(a),
            second: damaged,
        });
        settle(&mut app);
        assert!(app.comparison().is_none());
        assert!(app
            .state
            .tasks
            .iter()
            .any(|t| t.status == TaskStatus::Failed));
    }

    #[test]
    fn a_stopped_comparison_is_stopped_and_shows_nothing() {
        let room = tempfile::tempdir().unwrap();
        let dir = room.path().join("dir");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("big.bin"), vec![7u8; 16 << 20]).unwrap();
        let a = room.path().join("a.zip");
        zip(&a, &[("big.bin", &vec![7u8; 16 << 20])]);
        let mut app = AppController::new(Settings::default());
        app.dispatch(AppAction::Compare {
            first: Some(a),
            second: dir,
        });
        let id = app.state.tasks.last().expect("a task").id;
        app.cancel_task(id);
        settle(&mut app);
        assert!(app.comparison().is_none());
        assert_eq!(app.task(id).unwrap().status, TaskStatus::Stopped);
    }
}
