use super::{worker_notify, AppAction, AppController, Message, Task};
use crate::{archive_stem, collect_files, detect, fill};
use arca_core::hash::Algorithm;
use arca_core::{Entry, Error, Result};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};

type Notify<'a> = arca_compare::Notify<'a>;

/// One file: its name and, in `Algorithm::ALL` order, its checksums, or why
/// it could not be read.
pub(crate) struct Checksum {
    pub(crate) name: String,
    pub(crate) sums: std::result::Result<Vec<String>, String>,
}

pub(crate) struct Checksums {
    pub(crate) source: String,
    pub(crate) rows: Vec<Checksum>,
    pub(crate) algorithm: Algorithm,
}

impl Checksums {
    pub(crate) fn lines(&self) -> String {
        let at = Algorithm::ALL
            .iter()
            .position(|a| *a == self.algorithm)
            .unwrap_or(0);
        let mut out = String::new();
        for row in &self.rows {
            if let Ok(sums) = &row.sums {
                out.push_str(&format!("{}  {}\n", sums[at], row.name));
            }
        }
        out
    }

    pub(crate) fn failed(&self) -> usize {
        self.rows.iter().filter(|row| row.sums.is_err()).count()
    }
}

#[derive(Default)]
pub(crate) struct ChecksumState {
    jobs: Vec<Receiver<Checksums>>,
    shown: Vec<Checksums>,
}

/// A failure that belongs to one file goes in its row; a stop ends the task.
fn row(name: &str, sums: Result<Vec<String>>) -> Result<Checksum> {
    match sums {
        Err(Error::Cancelled) => Err(Error::Cancelled),
        sums => Ok(Checksum {
            name: name.to_string(),
            sums: sums.map_err(|e| e.to_string()),
        }),
    }
}

/// Files on the disk, folders walked whole, several at a time across the
/// cores. Names are relative to the folder the picks were made in.
pub(crate) fn hash_files(roots: &[PathBuf], notify: Notify<'_>) -> Result<Vec<Checksum>> {
    let files = collect_files(roots)?;
    let total = files.len();
    let done = AtomicUsize::new(0);
    let rows = files
        .par_iter()
        .map(|(path, name)| {
            let at = (done.load(Ordering::Relaxed), total, name.as_str());
            if !notify(at.0, total, name) {
                return Err(Error::Cancelled);
            }
            let sums = arca_compare::digest_file(path, &Algorithm::ALL, notify, at);
            let checksum = row(name, sums)?;
            done.fetch_add(1, Ordering::Relaxed);
            Ok(checksum)
        })
        .collect::<Result<Vec<_>>>()?;
    let _ = notify(total, total, "");
    Ok(rows)
}

/// Entries of an archive, each streamed through the hasher by the same
/// readers testing and previewing use. `wanted` is indexed like `entries`.
pub(crate) fn hash_entries(
    archive: &Path,
    entries: &[Entry],
    wanted: &[bool],
    password: Option<&str>,
    notify: Notify<'_>,
) -> Result<Vec<Checksum>> {
    let Some(format) = detect(archive) else {
        return Err(Error::Unsupported("unknown format".into()));
    };
    let read = arca_compare::digest_entries(
        archive,
        format,
        entries,
        wanted,
        password,
        &Algorithm::ALL,
        notify,
    )?;
    read.into_iter().map(|d| row(&d.name, d.sums)).collect()
}

type Work = Box<dyn for<'a> FnOnce(Notify<'a>) -> Result<Vec<Checksum>> + Send>;

impl AppController {
    /// Checksums of what is picked: files and folders on the disk, or entries
    /// in the open archive, all of them when nothing is picked there.
    pub(crate) fn start_checksums(&mut self) {
        let job = if self.on_disk() {
            self.disk_checksums()
        } else {
            self.archive_checksums()
        };
        let Some((subject, reads, total, work)) = job else {
            return;
        };
        let s = self.s();
        let (report, reports) = channel();
        self.checksums.jobs.push(reports);
        let source = subject.clone();
        let task = Task::new(
            s.computing_checksums,
            subject,
            Box::new(move |tx, _, stop, hold| {
                let notify = worker_notify(&tx, &stop, &hold);
                let _ = tx.send(match work(&notify) {
                    Ok(rows) => {
                        let n = rows.len().to_string();
                        let _ = report.send(Checksums {
                            source,
                            rows,
                            algorithm: Algorithm::Sha256,
                        });
                        Message::Done(fill(s.checksums_done, &[("n", &n)]))
                    }
                    Err(e) => Message::Failed(e.to_string()),
                });
            }),
        )
        .reading(reads)
        .total(total);
        self.enqueue(task);
    }

    fn disk_checksums(&self) -> Option<(String, Vec<PathBuf>, usize, Work)> {
        let roots = self.selected_disk_paths();
        let folder = roots.first()?.parent()?;
        let subject = folder
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| folder.display().to_string());
        let reads = roots.clone();
        Some((
            subject,
            reads,
            0,
            Box::new(move |notify| hash_files(&roots, notify)),
        ))
    }

    fn archive_checksums(&mut self) -> Option<(String, Vec<PathBuf>, usize, Work)> {
        if !self.read_access(AppAction::Checksums) {
            return None;
        }
        let archive = self.state.archive.clone()?;
        let checked = &self.state.active_pane().checked;
        let entries = self.state.entries.clone();
        let mut wanted: Vec<bool> = (0..entries.len())
            .map(|i| checked.get(i).copied().unwrap_or(false) && !entries[i].is_dir)
            .collect();
        if !wanted.contains(&true) {
            wanted = entries.iter().map(|e| !e.is_dir).collect();
        }
        let total = wanted.iter().filter(|w| **w).count();
        let password = self.state.archive_password.clone();
        let source = archive.clone();
        Some((
            archive_stem(&archive),
            vec![archive],
            total,
            Box::new(move |notify| {
                hash_entries(&source, &entries, &wanted, password.as_deref(), notify)
            }),
        ))
    }

    pub(crate) fn poll_checksums(&mut self) {
        let state = &mut self.checksums;
        state.jobs.retain(|rx| match rx.try_recv() {
            Ok(found) => {
                state.shown.push(found);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
    }

    pub(crate) fn checksums(&self) -> Option<&Checksums> {
        self.checksums.shown.first()
    }

    pub(crate) fn checksums_mut(&mut self) -> Option<&mut Checksums> {
        self.checksums.shown.first_mut()
    }

    pub(crate) fn close_checksums(&mut self) {
        if !self.checksums.shown.is_empty() {
            self.checksums.shown.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;
    use arca_core::{Codec, Level};
    use std::time::{Duration, Instant};

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn settle(app: &mut AppController) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.state.busy || app.working() {
            assert!(Instant::now() < deadline, "tasks did not finish");
            app.receive();
            std::thread::sleep(Duration::from_millis(5));
        }
        app.receive();
    }

    fn zip(dir: &Path) -> PathBuf {
        let mut sources = Vec::new();
        for (name, data) in [("abc.txt", &b"abc"[..]), ("digits.txt", b"123456789")] {
            let path = dir.join(name);
            std::fs::write(&path, data).unwrap();
            sources.push(arca_zip::Source {
                path,
                name: format!("inner/{name}"),
                size: data.len() as u64,
                mtime: 0,
                codec: Codec::Store,
                level: Level::STORE,
            });
        }
        let archive = dir.join("sums.zip");
        arca_zip::create_zip(&archive, &sources, 1, None, &|_, _, _| true).unwrap();
        archive
    }

    #[test]
    fn entries_inside_a_zip_are_hashed_without_extracting_them() {
        let room = tempfile::tempdir().unwrap();
        let archive = zip(room.path());
        let mut app = AppController::new(Settings::default());
        app.load_listing(archive.clone(), None);
        settle(&mut app);
        let digits = app
            .state
            .entries
            .iter()
            .position(|e| e.name == "inner/digits.txt")
            .unwrap();
        app.state.active_pane_mut().checked[digits] = true;
        let before: Vec<_> = std::fs::read_dir(room.path()).unwrap().collect();

        app.dispatch(AppAction::Checksums);
        settle(&mut app);
        let found = app.checksums().expect("a report");
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.rows[0].name, "inner/digits.txt");
        assert_eq!(found.rows[0].sums.as_ref().unwrap()[0], "cbf43926");
        assert_eq!(
            std::fs::read_dir(room.path()).unwrap().count(),
            before.len()
        );
        app.close_checksums();
        assert!(app.checksums().is_none());

        app.clear_picked();
        app.dispatch(AppAction::Checksums);
        settle(&mut app);
        let found = app.checksums().expect("a report");
        assert_eq!(found.rows.len(), 2);
        assert_eq!(
            found.lines(),
            format!(
                "{ABC}  inner/abc.txt\n{}  inner/digits.txt\n",
                found.rows[1].sums.as_ref().unwrap()[2]
            )
        );
    }

    #[test]
    fn a_damaged_entry_is_a_row_with_its_error_and_a_stop_is_a_stop() {
        let room = tempfile::tempdir().unwrap();
        let archive = zip(room.path());
        let entries = crate::list_entries(&archive, None, &|_, _, _| true)
            .unwrap()
            .0;
        let mut data = std::fs::read(&archive).unwrap();
        let at = data.windows(9).position(|w| w == b"123456789").unwrap();
        data[at] ^= 0xff;
        let damaged = room.path().join("damaged.zip");
        std::fs::write(&damaged, &data).unwrap();
        let wanted = vec![true; entries.len()];
        let rows = hash_entries(&damaged, &entries, &wanted, None, &|_, _, _| true).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.sums.is_err()));
        assert!(matches!(
            hash_entries(&archive, &entries, &wanted, None, &|_, _, _| false),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn files_on_the_disk_are_named_from_the_folder_they_were_picked_in() {
        let room = tempfile::tempdir().unwrap();
        let dir = room.path().join("pick");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/abc.txt"), b"abc").unwrap();
        std::fs::write(dir.join("top.txt"), b"abc").unwrap();
        let rows = hash_files(&[dir.join("sub"), dir.join("top.txt")], &|_, _, _| true).unwrap();
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["sub/abc.txt", "top.txt"]);
        assert!(rows.iter().all(|row| row.sums.as_ref().unwrap()[2] == ABC));
    }
}
