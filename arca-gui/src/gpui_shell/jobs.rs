use super::*;
use crate::controller::{Task, TaskStatus};
use gpui::{AnyElement, SharedString};

const HEADER_HEIGHT: f32 = 34.;
const ROW_HEIGHT: f32 = 56.;
const RUNNING_ROW_HEIGHT: f32 = 80.;
const PANEL_MAX_HEIGHT: f32 = 360.;

impl GpuiShell {
    /// The tasks worth a row: everything but a clipboard copy that is going
    /// well, which is over before a row has finished appearing.
    pub(super) fn shown_tasks(&self) -> Vec<&Task> {
        self.controller
            .state
            .tasks
            .iter()
            .filter(|t| !t.quiet || t.status == TaskStatus::Failed)
            .collect()
    }

    /// Opens the panel when something new has been asked for. Called from
    /// the tick so a job started from a menu, a drop or the command line all
    /// show up the same way.
    pub(super) fn notice_new_tasks(&mut self) {
        let newest = self
            .controller
            .state
            .tasks
            .iter()
            .filter(|t| !t.quiet)
            .map(|t| t.id)
            .max()
            .unwrap_or(0);
        if newest > self.jobs_seen {
            self.jobs_open = true;
        }
        self.jobs_seen = self.jobs_seen.max(newest);
    }

    /// The list of operations: docked as the whole content of a window that
    /// was opened for one job, floating over the corner of the workspace
    /// otherwise so the list underneath stays usable while work goes on.
    pub(super) fn jobs_panel(&self, docked: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tasks = self.shown_tasks();
        if tasks.is_empty() || (!docked && !self.jobs_open) {
            return None;
        }
        let s = self.controller.s();
        let running = tasks.iter().filter(|t| t.active()).count();
        let any_finished = tasks.iter().any(|t| t.finished());
        let title = if running == 0 {
            s.operations_title.to_string()
        } else {
            format!(
                "{} · {}",
                s.operations_title,
                fill(s.running_count, &[("n", &running.to_string())])
            )
        };
        let enabled = !self.background_blocked();
        let mut header = div()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            );
        if any_finished {
            header = header.child(
                Self::button(
                    "jobs-clear",
                    s.clear_finished,
                    s.clear_finished.to_string(),
                    enabled,
                )
                .xsmall()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller.dispatch(AppAction::ClearFinishedTasks);
                    cx.notify();
                })),
            );
        }
        if !docked {
            header = header.child(
                Self::icon_button(
                    cx,
                    "jobs-hide",
                    IconName::Close,
                    s.hide_word.to_string(),
                    enabled,
                )
                .xsmall()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.jobs_open = false;
                    cx.notify();
                })),
            );
        }
        let rows: Vec<AnyElement> = tasks
            .iter()
            .map(|task| self.task_row(task, enabled, cx))
            .collect();
        // Floating, the panel has no parent to take a height from, so it is
        // told its own: the header plus the rows it holds, up to a limit the
        // list then scrolls inside.
        let wanted: f32 = tasks.iter().map(|task| Self::row_height(task)).sum();
        let height = (HEADER_HEIGHT + wanted).min(PANEL_MAX_HEIGHT);
        let list = div()
            .id("jobs-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .children(rows);
        let mut panel = div()
            .id("jobs-panel")
            .flex()
            .flex_col()
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground);
        if enabled {
            panel = panel.role(Role::Status).aria_label(s.operations_title);
        }
        panel = if docked {
            panel.flex_1().min_h_0().w_full()
        } else {
            panel
                .absolute()
                .bottom(px(8.))
                .right(px(8.))
                .w(px(420.))
                .h(px(height))
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .shadow_md()
        };
        Some(panel.child(header).child(list).into_any_element())
    }

    fn row_height(task: &Task) -> f32 {
        if task.status == TaskStatus::Running {
            RUNNING_ROW_HEIGHT
        } else {
            ROW_HEIGHT
        }
    }

    fn task_row(&self, task: &Task, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let s = self.controller.s();
        let id = task.id;
        let name = |what: &str| SharedString::from(format!("task-{what}-{id}"));
        let mut top = div().flex().items_center().gap_2().child(
            div()
                .flex_1()
                .truncate()
                .child(format!("{} · {}", task.verb, task.subject)),
        );
        if task.active() {
            let held = task.held();
            let stopping = task.stopping();
            top = top
                .child(
                    Self::icon_button(
                        cx,
                        name("hold"),
                        if held {
                            IconName::Play
                        } else {
                            IconName::Pause
                        },
                        if held { s.resume_word } else { s.pause_word }.to_string(),
                        enabled && !stopping && task.status == TaskStatus::Running,
                    )
                    .xsmall()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controller.dispatch(AppAction::HoldTask(id, !held));
                        cx.notify();
                    })),
                )
                .child(
                    Self::icon_button(
                        cx,
                        name("cancel"),
                        IconName::Close,
                        s.cancel.to_string(),
                        enabled && !stopping,
                    )
                    .xsmall()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controller.dispatch(AppAction::CancelTask(id));
                        cx.notify();
                    })),
                );
        } else {
            top = top.child(
                Self::icon_button(
                    cx,
                    name("dismiss"),
                    IconName::Close,
                    s.dismiss_word.to_string(),
                    enabled,
                )
                .xsmall()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.controller.dispatch(AppAction::DismissTask(id));
                    cx.notify();
                })),
            );
        }
        let detail: AnyElement = match task.status {
            TaskStatus::Queued => div()
                .text_color(cx.theme().muted_foreground)
                .child(s.queued_word)
                .into_any_element(),
            TaskStatus::Running => {
                let fraction = task.fraction();
                let count = match (task.total, task.in_bytes) {
                    (0, _) => format!("{}…", s.working_word),
                    (total, true) => {
                        format!("{} / {}", human(task.done as u64), human(total as u64))
                    }
                    (total, false) => format!("{} / {total}", task.done),
                };
                let mut timing = task
                    .started
                    .map(|started| {
                        let gone = started.elapsed().as_secs_f64();
                        let mut text = format!("{} {}", s.elapsed_word, super::clock(gone));
                        if !task.held() && fraction > 0.05 {
                            text.push_str(&format!(
                                " · {} {}",
                                s.time_left,
                                super::clock(gone / fraction - gone)
                            ));
                        }
                        text
                    })
                    .unwrap_or_default();
                if task.stopping() {
                    timing.push_str(&format!(" · {}", s.stopping));
                } else if task.held() {
                    timing.push_str(&format!(" · {}", s.paused_word));
                }
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Progress::new(name("progress"))
                                    .accessibility_label(s.progress_region)
                                    .value(fraction as f32 * 100.)
                                    .loading(task.total == 0)
                                    .flex_1(),
                            )
                            .child(div().flex_none().child(count)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .text_color(cx.theme().muted_foreground)
                            .child(div().flex_1().truncate().child(task.current_file.clone()))
                            .child(div().flex_none().child(timing)),
                    )
                    .into_any_element()
            }
            TaskStatus::Done | TaskStatus::Stopped => div()
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .child(task.result.clone())
                .into_any_element(),
            TaskStatus::Failed => div()
                .truncate()
                .text_color(cx.theme().danger)
                .child(task.result.clone())
                .into_any_element(),
        };
        div()
            .id(name("row"))
            .h(px(Self::row_height(task)))
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .overflow_hidden()
            .text_xs()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(top)
            .child(detail)
            .into_any_element()
    }
}
