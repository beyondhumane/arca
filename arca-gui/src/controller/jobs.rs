//! The queue of operations and the threads that run them.
//!
//! Every extraction, test, rewrite and download is a `Task` with its own
//! thread, channel and pause and stop flags. Tasks run side by side unless two
//! of them touch the same file and one of them writes it: a rewrite replaces
//! the archive when it is whole, and nothing else may be reading or rewriting
//! it while that happens.

use super::{AppController, Message};
use crate::archive_ops::Answer;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

pub(crate) type TaskId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
}

pub(crate) type Work =
    Box<dyn FnOnce(Sender<Message>, Receiver<Answer>, Arc<AtomicBool>, Arc<AtomicBool>) + Send>;

pub(crate) struct Task {
    pub(crate) id: TaskId,
    pub(crate) verb: String,
    pub(crate) subject: String,
    pub(crate) status: TaskStatus,
    pub(crate) done: usize,
    pub(crate) total: usize,
    pub(crate) current_file: String,
    pub(crate) in_bytes: bool,
    pub(crate) started: Option<Instant>,
    pub(crate) finished: Option<Instant>,
    pub(crate) result: String,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) hold: Arc<AtomicBool>,
    pub(crate) channel: Option<Receiver<Message>>,
    pub(crate) replies: Option<Sender<Answer>>,
    pub(crate) conflict: Option<String>,
    pub(crate) reads: Vec<PathBuf>,
    pub(crate) writes: Vec<PathBuf>,
    // Folders a task fills with files of its own choosing, such as an
    // extraction's destination. Two of them overlapping may write the same file.
    pub(crate) outputs: Vec<PathBuf>,
    // The archive the window showed when the task was asked for. A result is
    // only opened over the window if it still shows that.
    pub(crate) origin: Option<PathBuf>,
    pub(crate) stop_leaves_files: bool,
    pub(crate) reread_after: Option<(PathBuf, Option<String>, String)>,
    pub(crate) close_when_done: bool,
    pub(crate) quiet: bool,
    work: Option<Work>,
}

impl Task {
    pub(crate) fn new(verb: impl Into<String>, subject: impl Into<String>, work: Work) -> Self {
        Self {
            id: 0,
            verb: verb.into(),
            subject: subject.into(),
            status: TaskStatus::Queued,
            done: 0,
            total: 0,
            current_file: String::new(),
            in_bytes: false,
            started: None,
            finished: None,
            result: String::new(),
            stop: Arc::new(AtomicBool::new(false)),
            hold: Arc::new(AtomicBool::new(false)),
            channel: None,
            replies: None,
            conflict: None,
            reads: Vec::new(),
            writes: Vec::new(),
            outputs: Vec::new(),
            origin: None,
            stop_leaves_files: false,
            reread_after: None,
            close_when_done: false,
            quiet: false,
            work: Some(work),
        }
    }

    pub(crate) fn reading(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.reads.extend(paths);
        self
    }

    pub(crate) fn writing(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.writes.extend(paths);
        self
    }

    pub(crate) fn filling(mut self, folders: impl IntoIterator<Item = PathBuf>) -> Self {
        self.outputs.extend(folders);
        self
    }

    pub(crate) fn total(mut self, total: usize) -> Self {
        self.total = total;
        self
    }

    pub(crate) fn active(&self) -> bool {
        matches!(self.status, TaskStatus::Queued | TaskStatus::Running)
    }

    pub(crate) fn finished(&self) -> bool {
        !self.active()
    }

    pub(crate) fn held(&self) -> bool {
        self.hold.load(Ordering::Relaxed)
    }

    pub(crate) fn stopping(&self) -> bool {
        self.active() && self.stop.load(Ordering::Relaxed)
    }

    pub(crate) fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.done as f64 / self.total as f64
        }
    }

    pub(crate) fn writes_to(&self, path: &Path) -> bool {
        self.writes.iter().any(|w| w == path)
    }

    fn touches(&self, path: &Path) -> bool {
        self.reads
            .iter()
            .chain(self.writes.iter())
            .any(|p| same_file(p, path))
    }

    fn collides(&self, other: &Task) -> bool {
        self.writes.iter().any(|w| other.touches(w))
            || other.writes.iter().any(|w| self.touches(w))
            || self
                .outputs
                .iter()
                .any(|a| other.outputs.iter().any(|b| nested(a, b)))
    }

    fn start(&mut self) {
        let Some(work) = self.work.take() else {
            return;
        };
        let (tx, rx) = channel();
        let (reply_tx, reply_rx) = channel();
        self.channel = Some(rx);
        self.replies = Some(reply_tx);
        self.status = TaskStatus::Running;
        self.started = Some(Instant::now());
        let stop = self.stop.clone();
        let hold = self.hold.clone();
        std::thread::spawn(move || work(tx, reply_rx, stop, hold));
    }
}

fn real(p: &Path) -> PathBuf {
    match std::fs::canonicalize(p) {
        Ok(real) => real,
        Err(_) => match (p.parent(), p.file_name()) {
            (Some(parent), Some(name)) => std::fs::canonicalize(parent)
                .map(|parent| parent.join(name))
                .unwrap_or_else(|_| p.to_path_buf()),
            _ => p.to_path_buf(),
        },
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || real(a) == real(b)
}

fn nested(a: &Path, b: &Path) -> bool {
    let (a, b) = (real(a), real(b));
    a.starts_with(&b) || b.starts_with(&a)
}

impl AppController {
    pub(crate) fn enqueue(&mut self, mut task: Task) -> TaskId {
        self.state.next_task += 1;
        task.id = self.state.next_task;
        task.origin = self.state.archive.clone();
        let id = task.id;
        self.state.tasks.push(task);
        self.schedule();
        id
    }

    /// Starts every queued task that nothing ahead of it is using the same
    /// file as. Order is kept per file: a task waits behind an earlier queued
    /// one it collides with even while that one is itself waiting.
    pub(crate) fn schedule(&mut self) {
        let tasks = &mut self.state.tasks;
        for i in 0..tasks.len() {
            if tasks[i].status != TaskStatus::Queued {
                continue;
            }
            let blocked = tasks.iter().enumerate().any(|(j, other)| {
                j != i
                    && (other.status == TaskStatus::Running
                        || (other.status == TaskStatus::Queued && j < i))
                    && other.collides(&tasks[i])
            });
            if !blocked {
                tasks[i].start();
            }
        }
    }

    pub(crate) fn task(&self, id: TaskId) -> Option<&Task> {
        self.state.tasks.iter().find(|t| t.id == id)
    }

    pub(crate) fn task_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.state.tasks.iter_mut().find(|t| t.id == id)
    }

    pub(crate) fn active_tasks(&self) -> impl Iterator<Item = &Task> {
        self.state.tasks.iter().filter(|t| t.active())
    }

    #[cfg(test)]
    pub(crate) fn pending_reread(&self) -> Option<(PathBuf, Option<String>, String)> {
        self.state
            .tasks
            .iter()
            .rev()
            .find_map(|t| t.reread_after.clone())
    }

    #[cfg(test)]
    pub(crate) fn working(&self) -> bool {
        self.state.tasks.iter().any(|t| t.active())
    }

    /// Whether a task is rewriting the archive the window shows. The listing
    /// cannot be trusted while that is happening and nothing else may change
    /// the file either.
    pub(crate) fn archive_locked(&self) -> bool {
        let Some(archive) = &self.state.archive else {
            return false;
        };
        self.state
            .tasks
            .iter()
            .any(|t| t.active() && t.writes_to(archive))
    }

    /// Whether the window has to wait: a listing on its way, or a rewrite of
    /// the archive it is showing. Extracting, testing and compressing run in
    /// the background and leave the window alone.
    pub(crate) fn blocked(&self) -> bool {
        self.state.busy || self.archive_locked()
    }

    /// The first open question from a task, if any. One at a time: the box
    /// that asks is modal, and the task behind it blocks until it is answered.
    pub(crate) fn conflict(&self) -> Option<(TaskId, &str)> {
        self.state
            .tasks
            .iter()
            .find_map(|t| t.conflict.as_deref().map(|path| (t.id, path)))
    }

    pub(crate) fn answer_conflict(&mut self, answer: Answer) {
        let Some((id, _)) = self.conflict() else {
            return;
        };
        if let Some(task) = self.task_mut(id) {
            task.conflict = None;
            if let Some(replies) = &task.replies {
                let _ = replies.send(answer);
            }
        }
    }

    pub(crate) fn hold_task(&mut self, id: TaskId, held: bool) {
        if let Some(task) = self.task(id) {
            if task.active() && !task.stopping() {
                task.hold.store(held, Ordering::Relaxed);
            }
        }
    }

    pub(crate) fn cancel_task(&mut self, id: TaskId) {
        let stopped = self.s().stopped.to_string();
        let Some(task) = self.task_mut(id) else {
            return;
        };
        task.stop.store(true, Ordering::Relaxed);
        task.hold.store(false, Ordering::Relaxed);
        if task.status == TaskStatus::Queued {
            task.status = TaskStatus::Stopped;
            task.result = stopped;
            task.finished = Some(Instant::now());
            task.work = None;
            self.schedule();
        }
    }

    pub(crate) fn dismiss_task(&mut self, id: TaskId) {
        self.state.tasks.retain(|t| !(t.id == id && t.finished()));
    }

    pub(crate) fn clear_finished_tasks(&mut self) {
        self.state.tasks.retain(|t| t.active());
    }

    /// Reads what every running task has to say. Returns whether a window that
    /// was opened for one task has nothing left to do.
    pub(crate) fn poll_tasks(&mut self) -> bool {
        let mut close_window = false;
        let mut cut_ready = Vec::new();
        let mut installing = false;
        let mut settled = false;
        let words = self.s();
        for i in 0..self.state.tasks.len() {
            let mut messages = Vec::new();
            let mut gone = false;
            if let Some(rx) = &self.state.tasks[i].channel {
                loop {
                    match rx.try_recv() {
                        Ok(m) => messages.push(m),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            gone = true;
                            break;
                        }
                    }
                }
            }
            if messages.is_empty() && !gone {
                continue;
            }
            for m in messages {
                let task = &mut self.state.tasks[i];
                match m {
                    Message::Progress(done, total, name) => {
                        task.done = done;
                        task.total = total;
                        task.current_file = name;
                    }
                    Message::Conflict(path) => task.conflict = Some(path),
                    Message::Created(path, password) => {
                        if !task.close_when_done {
                            task.reread_after = Some((path, password, String::new()));
                        }
                    }
                    Message::CutReady => cut_ready.push(task.id),
                    Message::Done(text) => {
                        task.status = TaskStatus::Done;
                        task.result = text.clone();
                        self.state.notice = text;
                        self.state.error = false;
                        settled = true;
                    }
                    Message::Failed(text) => {
                        let quit = text == arca_core::Error::Cancelled.to_string();
                        let text = if quit && task.stop_leaves_files {
                            words.stopped_partial.to_string()
                        } else if quit {
                            words.stopped.to_string()
                        } else {
                            text
                        };
                        task.status = if quit {
                            TaskStatus::Stopped
                        } else {
                            TaskStatus::Failed
                        };
                        task.result = text.clone();
                        task.reread_after = None;
                        self.state.notice = text;
                        self.state.error = !quit;
                        settled = true;
                    }
                    Message::Downloaded(path) => {
                        task.status = TaskStatus::Done;
                        let version = self
                            .state
                            .update
                            .as_ref()
                            .map(|r| r.tag.clone())
                            .unwrap_or_default();
                        let notice = super::fill(words.update_installing, &[("version", &version)]);
                        match super::install_update(&path) {
                            Ok(()) => {
                                installing = true;
                                self.state.notice = notice;
                            }
                            Err(e) => {
                                self.state.notice = e;
                                self.state.error = true;
                            }
                        }
                        self.state.tasks[i].result = self.state.notice.clone();
                        settled = true;
                    }
                    Message::Listing(..) | Message::AccessChecked(..) => {}
                }
            }
            let task = &mut self.state.tasks[i];
            // A worker that let go of its channel without a word has panicked.
            // Left Running, its row would spin forever and hold every file it
            // touches.
            if gone && task.active() {
                task.status = TaskStatus::Failed;
                task.result = words.task_lost.to_string();
                task.reread_after = None;
                self.state.notice = task.result.clone();
                self.state.error = true;
                settled = true;
            }
            if task.finished() && task.channel.is_some() {
                task.channel = None;
                task.replies = None;
                task.finished = Some(Instant::now());
                if task.status == TaskStatus::Done {
                    if let Some(reread) = task.reread_after.take() {
                        if task.origin == self.state.archive {
                            self.state.reread_queued = Some((task.origin.clone(), reread));
                        }
                    }
                    if task.close_when_done {
                        close_window = true;
                    }
                }
            }
        }
        if let Some((armed_by, _)) = &self.state.cut_armed {
            if cut_ready.contains(armed_by) {
                self.state.cut_pending = self.state.cut_armed.take().map(|(_, cut)| cut);
            }
        }
        if settled {
            self.schedule();
            if !self.state.entries.is_empty() && self.state.notice.is_empty() {
                self.state.notice = self.summary();
            }
            if matches!(self.state.view, super::View::Running) && !close_window {
                self.state.view = super::View::Browse;
            }
        }
        self.reread_when_free();
        installing || close_window
    }

    /// Rereads the archive a finished rewrite left behind, once nothing else
    /// is rewriting it and only if the window still shows it. Kept until then:
    /// a rewrite queued behind may fail and leave nothing newer to show.
    fn reread_when_free(&mut self) {
        if let Some((origin, _)) = &self.state.reread_queued {
            if *origin != self.state.archive {
                self.state.reread_queued = None;
            }
        }
        if self.blocked() {
            return;
        }
        let Some((_, (path, pw, dir))) = self.state.reread_queued.take() else {
            return;
        };
        let notice = std::mem::take(&mut self.state.notice);
        self.open_with_password(path, pw);
        self.state.reread_dir = Some(dir);
        self.state.notice = notice;
        self.state.view = super::View::Browse;
    }
}
