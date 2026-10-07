use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

impl Shell {
    pub(super) fn jobs(&mut self, ctx: &egui::Context) {
        if self.controller.state.one_shot || !self.jobs_open || self.shown_tasks().is_empty() {
            return;
        }
        let p = palette_of(ctx);
        egui::Area::new(egui::Id::new("arca-jobs"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -40.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(hex(p.surface))
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(380.0);
                        self.jobs_body(ui, false);
                    });
            });
    }

    pub(super) fn jobs_body(&mut self, ui: &mut egui::Ui, docked: bool) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let enabled = !self.background_blocked();
        let mut actions = Vec::new();
        let running = self
            .controller
            .state
            .tasks
            .iter()
            .filter(|t| t.active() && !t.quiet)
            .count();
        let any_finished = self.shown_tasks().iter().any(|t| t.finished());
        ui.horizontal(|ui| {
            heading(ui, s.operations_title);
            if running > 0 {
                muted(ui, fill(s.running_count, &[("n", &running.to_string())]));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !docked && icon_button(ui, icon::X, s.hide_word, true).clicked() {
                    self.jobs_open = false;
                }
                if any_finished && ui.button(s.clear_finished).clicked() {
                    actions.push(AppAction::ClearFinishedTasks);
                }
            });
        });
        ui.separator();
        let tallest = if docked { f32::INFINITY } else { 320.0 };
        let measured = ui.id().with("jobs-content");
        let wanted = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(measured))
            .unwrap_or(0.0)
            .min(tallest);
        let shown = egui::ScrollArea::vertical()
            .max_height(tallest)
            .min_scrolled_height(if docked { 64.0 } else { wanted.max(64.0) })
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for task in self.shown_tasks() {
                    ui.push_id(task.id, |ui| {
                        task_row(ui, task, s, p, enabled, &mut actions)
                    });
                    ui.add_space(6.0);
                }
            });
        ui.ctx()
            .data_mut(|data| data.insert_temp(measured, shown.content_size.y));
        for action in actions {
            self.controller.dispatch(action);
        }
    }
}

fn task_row(
    ui: &mut egui::Ui,
    task: &Task,
    s: &'static Strings,
    p: theme::Palette,
    enabled: bool,
    actions: &mut Vec<AppAction>,
) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(RichText::new(format!("{} · {}", task.verb, task.subject)).strong())
                .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if task.active() {
                let stopping = task.stopping();
                if icon_button(ui, icon::STOP, s.cancel, enabled && !stopping).clicked() {
                    actions.push(AppAction::CancelTask(task.id));
                }
                let (glyph, tip) = if task.held() {
                    (icon::PLAY, s.resume_word)
                } else {
                    (icon::PAUSE, s.pause_word)
                };
                let can_hold = enabled && !stopping && task.status == TaskStatus::Running;
                if icon_button(ui, glyph, tip, can_hold).clicked() {
                    actions.push(AppAction::HoldTask(task.id, !task.held()));
                }
            } else if icon_button(ui, icon::X, s.dismiss_word, enabled).clicked() {
                actions.push(AppAction::DismissTask(task.id));
            }
        });
    });
    if task.active() {
        let fraction = task.fraction();
        let count = match (task.status, task.total, task.in_bytes) {
            (TaskStatus::Queued, _, _) => s.queued_word.to_string(),
            (_, 0, _) => format!("{}…", s.working_word),
            (_, total, true) => format!("{} / {}", human(task.done as u64), human(total as u64)),
            (_, total, false) => format!("{} / {total}", task.done),
        };
        let mut timing = task
            .started
            .map(|started| {
                let gone = started.elapsed().as_secs_f64();
                let mut text = format!("{} {}", s.elapsed_word, clock(gone));
                if !task.held() && fraction > 0.05 {
                    text.push_str(&format!(
                        " · {} {}",
                        s.time_left,
                        clock(gone / fraction - gone)
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
        let bar = progress(
            ui,
            (task.total > 0 && task.status == TaskStatus::Running).then_some(fraction as f32),
        );
        ui.ctx().accesskit_node_builder(bar.id, |node| {
            node.set_label(s.progress_region.to_string());
        });
        ui.horizontal(|ui| {
            muted(ui, count);
            if !timing.is_empty() {
                muted(ui, timing);
            }
        });
        if !task.current_file.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(&task.current_file)
                        .small()
                        .color(hex(p.muted)),
                )
                .truncate(),
            );
        }
    } else {
        let color = match task.status {
            TaskStatus::Failed => p.danger,
            TaskStatus::Done => p.success,
            _ => p.muted,
        };
        ui.add(egui::Label::new(RichText::new(&task.result).color(hex(color))).wrap());
    }
}
