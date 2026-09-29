use crate::{
    assets::Assets,
    cli::{self, Options},
    engine::{self, Choices, Event, Job, Mode},
    i18n::Lang,
    theme::{self, color, FOG, INK},
    ui::{self, StepState},
};
use gpui::{
    div, img, point, prelude::*, px, size, App, Bounds, Context, FocusHandle, KeyDownEvent,
    ObjectFit, Render, TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions,
};
use gpui_platform::application;
use std::{
    io::Write as _,
    path::PathBuf,
    sync::mpsc::{channel, Receiver, Sender, TryRecvError},
    thread,
    time::Duration,
};

pub const WIDTH: f32 = 720.;
pub const HEIGHT: f32 = 420.;
pub const HEADER: f32 = 40.;

const BACKGROUNDS: [&str; 3] = [
    "instalador-fondo-bienvenida.svg",
    "instalador-fondo-progreso.svg",
    "instalador-fondo-listo.svg",
];
const PRELOADED: [&str; 3] = ["logo-horizontal.svg", "mark-sm.svg", "mark-lg.svg"];

const TICK: Duration = Duration::from_millis(16);
const CATCH_UP_PER_TICK: f32 = 0.9;
const HOLD_TICKS: u32 = 40;
const PREVIEW_STEPS: u32 = 50;
const PREVIEW_STEP_TIME: Duration = Duration::from_millis(140);
const LOG_NAME: &str = "arca-setup.log";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Welcome,
    Options,
    Installing,
    Done,
    Failed,
}

impl Screen {
    pub fn from_name(name: &str) -> Option<Screen> {
        match name {
            "welcome" => Some(Screen::Welcome),
            "options" => Some(Screen::Options),
            "installing" => Some(Screen::Installing),
            "done" => Some(Screen::Done),
            "failed" => Some(Screen::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyAction {
    Begin,
    Back,
    Close,
    Ignore,
}

fn key_action(screen: Screen, key: &str) -> KeyAction {
    match (screen, key) {
        (Screen::Welcome | Screen::Options | Screen::Failed, "enter") => KeyAction::Begin,
        (Screen::Options, "escape") => KeyAction::Back,
        (Screen::Welcome | Screen::Done | Screen::Failed, "escape") => KeyAction::Close,
        (Screen::Done, "enter") => KeyAction::Close,
        _ => KeyAction::Ignore,
    }
}

pub fn step_states(percent: f32) -> [StepState; 4] {
    let mut states = [StepState::Pending; 4];
    for (index, state) in states.iter_mut().enumerate() {
        let start = index as f32 * 25.;
        *state = if percent >= start + 25. {
            StepState::Done
        } else if percent >= start {
            StepState::Active
        } else {
            StepState::Pending
        };
    }
    states
}

fn simulate(report: &mut dyn FnMut(Event)) {
    for step in 0..=PREVIEW_STEPS {
        report(Event::Progress(step as f32 / PREVIEW_STEPS as f32 * 100.));
        thread::sleep(PREVIEW_STEP_TIME);
    }
}

fn work(job: Job, preview: bool, tx: Sender<Event>) {
    let mut report = |event: Event| {
        let _ = tx.send(event);
    };
    let outcome = if preview {
        simulate(&mut report);
        Ok(())
    } else {
        engine::run(&job, &mut report)
    };
    report(match outcome {
        Ok(()) => Event::Finished,
        Err(message) => Event::Failed(message),
    });
}

fn job_from(options: &Options) -> Job {
    let dir = options.dir.clone().unwrap_or_else(|| match options.mode {
        Mode::Install => engine::default_location(),
        Mode::Uninstall => engine::uninstall_location(),
    });
    Job {
        mode: options.mode,
        choices: engine::initial_choices(&dir),
        dir,
        quiet_update: options.update,
        files_only: options.files_only,
    }
}

fn log_failure(message: &str) {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let line = format!("{seconds} arca-setup {}: {message}\n", engine::VERSION);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join(LOG_NAME))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

fn run_silent(options: &Options) -> i32 {
    match engine::run(&job_from(options), &mut |_| {}) {
        Ok(()) => 0,
        Err(message) => {
            log_failure(&message);
            1
        }
    }
}

pub struct Setup {
    pub(crate) focus: Option<FocusHandle>,
    pub(crate) screen: Screen,
    pub(crate) lang: Lang,
    pub(crate) mode: Mode,
    pub(crate) choices: Choices,
    pub(crate) dir: PathBuf,
    pub(crate) percent: f32,
    pub(crate) error: Option<String>,
    pub(crate) preview: bool,
    files_only: bool,
    target: f32,
    finished: bool,
    hold: u32,
    events: Option<Receiver<Event>>,
}

impl Setup {
    pub fn new(options: &Options, lang: Lang) -> Self {
        let job = job_from(options);
        let screen = options.screen.unwrap_or(if options.percent.is_some() {
            Screen::Installing
        } else {
            Screen::Welcome
        });
        let percent = match (screen, options.percent) {
            (_, Some(percent)) => percent,
            (Screen::Done, None) => 100.,
            _ => 0.,
        };
        Setup {
            focus: None,
            screen,
            lang,
            mode: job.mode,
            choices: if options.preview {
                Choices::default()
            } else {
                job.choices
            },
            dir: job.dir,
            percent,
            error: (screen == Screen::Failed)
                .then(|| "preview: there is not enough space on the disk".to_string()),
            preview: options.preview,
            files_only: options.files_only,
            target: percent,
            finished: false,
            hold: 0,
            events: None,
        }
    }

    fn job(&self) -> Job {
        Job {
            mode: self.mode,
            dir: self.dir.clone(),
            choices: self.choices,
            quiet_update: false,
            files_only: self.files_only,
        }
    }

    pub(crate) fn begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Installing;
        self.percent = 0.;
        self.target = 0.;
        self.finished = false;
        self.hold = 0;
        self.error = None;
        let (tx, rx) = channel();
        self.events = Some(rx);
        let (job, preview) = (self.job(), self.preview);
        thread::spawn(move || work(job, preview, tx));
        cx.notify();
        let view = cx.weak_entity();
        window
            .spawn(cx, async move |async_cx| loop {
                async_cx.background_executor().timer(TICK).await;
                let running = view
                    .update_in(async_cx, |setup, _window, cx| {
                        let running = setup.tick();
                        cx.notify();
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            })
            .detach();
    }

    fn fail(&mut self, message: String) -> bool {
        self.error = Some(message);
        self.screen = Screen::Failed;
        self.events = None;
        false
    }

    fn tick(&mut self) -> bool {
        let Some(events) = &self.events else {
            return false;
        };
        let mut failure = None;
        loop {
            match events.try_recv() {
                Ok(Event::Progress(percent)) => self.target = percent.clamp(0., 100.),
                Ok(Event::Finished) => {
                    self.finished = true;
                    self.target = 100.;
                }
                Ok(Event::Failed(message)) => {
                    failure = Some(message);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.finished {
                        failure = Some("the installer stopped unexpectedly".to_string());
                    }
                    break;
                }
            }
        }
        if let Some(message) = failure {
            return self.fail(message);
        }
        if self.percent < self.target {
            self.percent = (self.percent + CATCH_UP_PER_TICK).min(self.target);
        }
        if self.finished && self.percent >= 100. {
            self.hold += 1;
            if self.hold >= HOLD_TICKS {
                self.screen = Screen::Done;
                self.events = None;
                return false;
            }
        }
        true
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match key_action(self.screen, event.keystroke.key.as_str()) {
            KeyAction::Begin => self.begin(window, cx),
            KeyAction::Back => {
                self.screen = Screen::Welcome;
                cx.notify();
            }
            KeyAction::Close => window.remove_window(),
            KeyAction::Ignore => {}
        }
    }

    pub(crate) fn toggle(&mut self, which: fn(&mut Choices) -> &mut bool, cx: &mut Context<Self>) {
        let value = which(&mut self.choices);
        *value = !*value;
        cx.notify();
    }

    pub(crate) fn finish(&mut self, window: &mut Window) {
        if !self.preview && self.mode == Mode::Install {
            engine::launch_app(&self.dir);
        }
        window.remove_window();
    }

    fn background(&self) -> &'static str {
        match self.screen {
            Screen::Welcome => BACKGROUNDS[0],
            Screen::Options | Screen::Installing | Screen::Failed => BACKGROUNDS[1],
            Screen::Done => BACKGROUNDS[2],
        }
    }

    fn backdrop(&self) -> impl IntoElement {
        let current = self.background();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(BACKGROUNDS.map(|path| {
                img(path)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(WIDTH))
                    .h(px(HEIGHT))
                    .object_fit(ObjectFit::Cover)
                    .opacity(if path == current { 1. } else { 0. })
            }))
            .children(PRELOADED.map(|path| img(path).absolute().w(px(1.)).h(px(1.)).opacity(0.)))
    }

    fn header(&self) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(HEADER))
            .flex()
            .flex_row()
            .items_center()
            .child(ui::drag_area("drag"))
            .when(!cfg!(target_os = "macos"), |bar| {
                bar.child(ui::window_control(
                    "minimize",
                    "icons/minus.svg",
                    WindowControlArea::Min,
                    false,
                ))
                .child(ui::window_control(
                    "close",
                    "icons/x.svg",
                    WindowControlArea::Close,
                    true,
                ))
            })
    }
}

impl Render for Setup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let strings = self.lang.strings();
        window.set_window_title(if self.mode == Mode::Uninstall {
            strings.uninstall_window_title
        } else {
            strings.window_title
        });
        let focus = match &self.focus {
            Some(focus) => focus.clone(),
            None => {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                self.focus = Some(focus.clone());
                focus
            }
        };
        let content = match self.screen {
            Screen::Welcome => self.welcome(cx),
            Screen::Options => self.options(cx),
            Screen::Installing => self.installing(),
            Screen::Done => self.done(cx),
            Screen::Failed => self.failed(cx),
        };
        div()
            .id("setup")
            .track_focus(&focus)
            .on_key_down(cx.listener(Self::on_key))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(color(FOG))
            .font_family(theme::text_font())
            .text_color(color(INK))
            .text_size(px(14.))
            .child(self.backdrop())
            .child(div().absolute().top_0().left_0().size_full().child(content))
            .child(self.header())
    }
}

pub fn run() {
    let options = cli::parse(std::env::args().skip(1));
    if options.mode == Mode::Uninstall && !options.relaunched && !options.preview {
        let dir = options
            .dir
            .clone()
            .unwrap_or_else(engine::uninstall_location);
        let mut args: Vec<String> = std::env::args().skip(1).collect();
        args.push("--relaunched".to_string());
        args.push(format!("--dir={}", dir.display()));
        if engine::relaunch_for_uninstall(&args).is_ok() {
            return;
        }
    }
    if options.silent && !options.preview {
        std::process::exit(run_silent(&options));
    }
    application().with_assets(Assets).run(move |cx: &mut App| {
        theme::resolve_fonts(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let lang = options.lang.unwrap_or_else(Lang::from_system);
        let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(WIDTH), px(HEIGHT))),
                is_resizable: false,
                app_owns_titlebar_drag: true,
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(9.), px(9.))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Setup::new(&options, lang)),
        )
        .expect("open the setup window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(list: &[&str]) -> Options {
        cli::parse(list.iter().map(|arg| arg.to_string()))
    }

    fn setup(list: &[&str]) -> Setup {
        Setup::new(&options(list), Lang::En)
    }

    #[test]
    fn a_percent_alone_opens_on_the_progress_screen() {
        let with_percent = setup(&["--percent=40"]);
        assert_eq!(with_percent.screen, Screen::Installing);
        assert_eq!(with_percent.percent, 40.);
        assert_eq!(setup(&["--preview"]).screen, Screen::Welcome);
    }

    #[test]
    fn the_failure_preview_carries_a_message() {
        let setup = setup(&["--screen=failed"]);
        assert_eq!(setup.screen, Screen::Failed);
        assert!(setup.error.is_some());
    }

    #[test]
    fn keys_drive_each_screen() {
        assert_eq!(key_action(Screen::Welcome, "enter"), KeyAction::Begin);
        assert_eq!(key_action(Screen::Options, "enter"), KeyAction::Begin);
        assert_eq!(key_action(Screen::Failed, "enter"), KeyAction::Begin);
        assert_eq!(key_action(Screen::Options, "escape"), KeyAction::Back);
        assert_eq!(key_action(Screen::Done, "enter"), KeyAction::Close);
        assert_eq!(key_action(Screen::Welcome, "escape"), KeyAction::Close);
        assert_eq!(key_action(Screen::Failed, "escape"), KeyAction::Close);
        assert_eq!(key_action(Screen::Installing, "enter"), KeyAction::Ignore);
        assert_eq!(key_action(Screen::Installing, "escape"), KeyAction::Ignore);
        assert_eq!(key_action(Screen::Welcome, "x"), KeyAction::Ignore);
    }

    #[test]
    fn steps_finish_one_after_another() {
        use StepState::{Active, Done, Pending};
        assert_eq!(step_states(0.), [Active, Pending, Pending, Pending]);
        assert_eq!(step_states(30.), [Done, Active, Pending, Pending]);
        assert_eq!(step_states(68.), [Done, Done, Active, Pending]);
        assert_eq!(step_states(99.), [Done, Done, Done, Active]);
        assert_eq!(step_states(100.), [Done, Done, Done, Done]);
    }

    fn feed(setup: &mut Setup, events: Vec<Event>) -> Sender<Event> {
        let (tx, rx) = channel();
        for event in events {
            tx.send(event).unwrap();
        }
        setup.events = Some(rx);
        tx
    }

    fn run_ticks(setup: &mut Setup, ticks: usize) {
        for _ in 0..ticks {
            if !setup.tick() {
                break;
            }
        }
    }

    #[test]
    fn the_bar_catches_up_with_the_work_instead_of_jumping() {
        let mut setup = setup(&["--preview"]);
        let _worker = feed(&mut setup, vec![Event::Progress(60.)]);
        run_ticks(&mut setup, 10);
        assert!(setup.percent > 0. && setup.percent < 60.);
        assert_eq!(setup.screen, Screen::Welcome);
        run_ticks(&mut setup, 200);
        assert_eq!(setup.percent, 60.);
    }

    #[test]
    fn a_finished_job_lands_on_the_last_screen_once_the_bar_is_full() {
        let mut setup = setup(&["--preview"]);
        setup.screen = Screen::Installing;
        let _worker = feed(&mut setup, vec![Event::Finished]);
        run_ticks(&mut setup, 10_000);
        assert_eq!(setup.screen, Screen::Done);
        assert_eq!(setup.percent, 100.);
    }

    #[test]
    fn a_failed_job_shows_its_message() {
        let mut setup = setup(&["--preview"]);
        setup.screen = Screen::Installing;
        let _worker = feed(
            &mut setup,
            vec![Event::Progress(10.), Event::Failed("disk full".to_string())],
        );
        run_ticks(&mut setup, 5);
        assert_eq!(setup.screen, Screen::Failed);
        assert_eq!(setup.error.as_deref(), Some("disk full"));
    }

    #[test]
    fn a_worker_that_vanishes_is_a_failure_not_a_hang() {
        let mut setup = setup(&["--preview"]);
        setup.screen = Screen::Installing;
        drop(feed(&mut setup, vec![Event::Progress(10.)]));
        run_ticks(&mut setup, 5);
        assert_eq!(setup.screen, Screen::Failed);
    }
}
