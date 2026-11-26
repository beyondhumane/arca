use super::AppController;
use arca_bench::{Event, Options, Progress, Report, Row};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

enum Update {
    Progress(Progress),
    Row(Row),
    Finished(Result<Report, String>),
}

struct Running {
    stop: Arc<AtomicBool>,
    updates: Receiver<Update>,
}

/// The benchmark window: what it is set to, what the run has measured so
/// far and, once it ends, the whole report. The run is its own thread, not
/// a task in the queue, so its progress shows here and nowhere else.
#[derive(Default)]
pub(crate) struct BenchState {
    pub(crate) open: bool,
    pub(crate) threads: usize,
    pub(crate) rows: Vec<Row>,
    pub(crate) progress: Option<Progress>,
    pub(crate) report: Option<Report>,
    pub(crate) error: Option<String>,
    running: Option<Running>,
}

pub(crate) fn cores() -> usize {
    static CORES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CORES.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
}

impl BenchState {
    pub(crate) fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The finished report as text, the same the command line prints.
    pub(crate) fn text(&self) -> Option<String> {
        self.report.as_ref().map(arca_bench::table)
    }

    fn take(&mut self, update: Update) {
        match update {
            Update::Progress(p) => self.progress = Some(p),
            Update::Row(row) => match self.rows.iter_mut().find(|r| r.codec == row.codec) {
                Some(old) => *old = row,
                None => self.rows.push(row),
            },
            Update::Finished(outcome) => {
                self.running = None;
                self.progress = None;
                match outcome {
                    Ok(report) => {
                        self.rows = report.rows.clone();
                        self.report = Some(report);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }
}

impl AppController {
    pub(crate) fn bench(&self) -> &BenchState {
        &self.benchmark
    }

    pub(crate) fn open_bench(&mut self) {
        let state = &mut self.benchmark;
        if state.threads == 0 {
            state.threads = cores();
        }
        state.open = true;
    }

    pub(crate) fn set_bench_threads(&mut self, threads: usize) {
        if !self.benchmark.running() {
            self.benchmark.threads = threads.clamp(1, cores().max(1) * 4);
        }
    }

    pub(crate) fn start_bench(&mut self) {
        let state = &mut self.benchmark;
        if state.running() {
            return;
        }
        state.rows.clear();
        state.report = None;
        state.error = None;
        state.progress = None;
        let options = Options::new(state.threads.max(1));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, updates) = channel();
        let flag = Arc::clone(&stop);
        let spawned = std::thread::Builder::new()
            .name("arca-bench".into())
            .spawn(move || {
                let events = |event: Event<'_>| {
                    let _ = tx.send(match event {
                        Event::Progress(p) => Update::Progress(p),
                        Event::Row(row) => Update::Row(row.clone()),
                    });
                };
                let outcome = arca_bench::run(&options, &flag, &events).map_err(|e| e.to_string());
                let _ = tx.send(Update::Finished(outcome));
            });
        match spawned {
            Ok(_) => state.running = Some(Running { stop, updates }),
            Err(e) => state.error = Some(e.to_string()),
        }
    }

    pub(crate) fn stop_bench(&mut self) {
        if let Some(run) = &self.benchmark.running {
            run.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Closing the window stops the run; what it measured is dropped with it.
    pub(crate) fn close_bench(&mut self) {
        self.stop_bench();
        self.benchmark.open = false;
    }

    pub(crate) fn poll_bench(&mut self) {
        let state = &mut self.benchmark;
        while let Some(run) = &state.running {
            match run.updates.try_recv() {
                Ok(update) => state.take(update),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    state.running = None;
                    state.progress = None;
                }
            }
        }
    }
}

#[cfg(test)]
impl AppController {
    pub(crate) fn bench_mut(&mut self) -> &mut BenchState {
        &mut self.benchmark
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;
    use std::time::{Duration, Instant};

    fn wait(app: &mut AppController) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while app.bench().running() {
            assert!(Instant::now() < deadline, "the benchmark did not finish");
            app.poll_bench();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_stopped_run_reports_what_it_measured_and_can_start_again() {
        let mut app = AppController::new(Settings::default());
        app.open_bench();
        assert!(app.bench().open);
        assert!(app.bench().threads >= 1);
        app.set_bench_threads(2);
        app.start_bench();
        assert!(app.bench().running());
        app.stop_bench();
        wait(&mut app);
        let report = app.bench().report.as_ref().expect("a report");
        assert!(report.stopped);
        assert_eq!(report.options.threads, 2);
        assert!(app
            .bench()
            .text()
            .unwrap()
            .contains("Stopped before the end"));
        assert!(app.bench().progress.is_none());

        app.start_bench();
        assert!(app.bench().report.is_none());
        app.close_bench();
        wait(&mut app);
        assert!(!app.bench().open);
        assert!(app.bench().report.as_ref().unwrap().stopped);
    }

    #[test]
    fn the_thread_count_stays_put_while_a_run_goes() {
        let mut app = AppController::new(Settings::default());
        app.open_bench();
        app.set_bench_threads(1);
        app.start_bench();
        app.set_bench_threads(3);
        assert_eq!(app.bench().threads, 1);
        app.close_bench();
        wait(&mut app);
        app.set_bench_threads(0);
        assert_eq!(app.bench().threads, 1);
    }
}
