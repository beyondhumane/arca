use crate::{
    app::{step_states, Setup, HEADER},
    assets,
    engine::{Choices, Mode},
    theme::{self, color, LINE, MUTED, SLATE, WHITE},
    ui::{self, Glyph},
};
use eframe::egui::{self, pos2, vec2, Align, Layout, Rect, RichText, Stroke, Ui, UiBuilder};

pub(crate) const RELEASES: &str = "https://github.com/beyondhumane/arca/releases/latest";
const ERROR_CHARS: usize = 260;

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Begin,
    Customize,
    Back,
    Close,
    Finish,
    Releases,
    Toggle(fn(&mut Choices) -> &mut bool),
}

fn heading(ui: &mut Ui, text: &str, size: f32) {
    ui.label(
        RichText::new(text)
            .font(theme::display(size))
            .line_height(Some(size + 6.)),
    );
}

fn body(ui: &mut Ui, text: &str, size: f32, tint: u32) {
    ui.label(
        RichText::new(text)
            .font(theme::text(size))
            .line_height(Some(size + 7.5))
            .color(color(tint)),
    );
}

fn truncated(text: &str) -> String {
    if text.chars().count() <= ERROR_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(ERROR_CHARS).collect();
    format!("{head}...")
}

fn column<R>(
    ui: &mut Ui,
    area: Rect,
    align: Align,
    centered: bool,
    salt: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let id = ui.id().with(salt);
    let last: f32 = ui.ctx().data(|data| data.get_temp(id)).unwrap_or(0.);
    let top = if centered {
        (area.center().y - last / 2.).max(area.top())
    } else {
        area.top()
    };
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(pos2(area.left(), top), area.max))
            .layout(Layout::top_down(align)),
    );
    child.spacing_mut().item_spacing = vec2(0., 0.);
    let result = add(&mut child);
    let height = child.min_rect().height();
    if (height - last).abs() > 0.5 {
        ui.ctx().data_mut(|data| data.insert_temp(id, height));
        ui.ctx().request_repaint();
    }
    result
}

fn row<R>(ui: &mut Ui, gap: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        add(ui)
    })
    .inner
}

impl Setup {
    pub(crate) fn welcome(&self, ui: &mut Ui, area: Rect) -> Option<Action> {
        let s = self.lang.strings();
        let uninstalling = self.mode == Mode::Uninstall;
        let (title, text) = if uninstalling {
            (s.confirm_title, s.confirm_body)
        } else {
            (s.welcome_title, s.welcome_body)
        };
        let area = Rect::from_min_max(
            pos2(area.left() + 64., area.top()),
            pos2(area.right(), area.bottom() - 8.),
        );
        column(ui, area, Align::Min, true, "welcome", |ui| {
            ui.add(assets::image("logo-horizontal.svg").fit_to_exact_size(vec2(300., 65.6)));
            ui.add_space(34.);
            heading(ui, title, 28.);
            ui.add_space(10.);
            ui.scope(|ui| {
                ui.set_max_width(420.);
                body(ui, text, 14.5, SLATE);
            });
            ui.add_space(28.);
            row(ui, 12., |ui| {
                let primary = if uninstalling { s.uninstall } else { s.install };
                if ui::primary_button(ui, primary, None, !uninstalling).clicked() {
                    return Some(Action::Begin);
                }
                if uninstalling {
                    ui::secondary_button(ui, s.cancel, None)
                        .clicked()
                        .then_some(Action::Close)
                } else {
                    ui::secondary_button(ui, s.customize, None)
                        .clicked()
                        .then_some(Action::Customize)
                }
            })
        })
    }

    fn option_row(
        &self,
        ui: &mut Ui,
        title: &str,
        hint: &str,
        on: bool,
        toggle: fn(&mut Choices) -> &mut bool,
    ) -> Option<Action> {
        row(ui, 16., |ui| {
            let clicked = ui::switch(ui, on, title).clicked();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.;
                ui.label(RichText::new(title).font(theme::display(14.)));
                ui.label(
                    RichText::new(hint)
                        .font(theme::text(12.5))
                        .color(color(MUTED)),
                );
            });
            clicked.then_some(Action::Toggle(toggle))
        })
    }

    pub(crate) fn options(&self, ui: &mut Ui, area: Rect) -> Option<Action> {
        let s = self.lang.strings();
        let area = Rect::from_min_max(
            pos2(area.left() + 64., area.top() + HEADER + 4.),
            pos2(area.right() - 64., area.bottom()),
        );
        column(ui, area, Align::Min, false, "options", |ui| {
            let mut action = None;
            heading(ui, s.options_title, 24.);
            ui.add_space(4.);
            body(ui, s.options_body, 14., SLATE);
            ui.add_space(14.);
            ui::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 12.;
                for (title, hint, on, toggle) in [
                    (
                        s.option_menu,
                        s.option_menu_hint,
                        self.choices.menu,
                        (|choices| &mut choices.menu) as fn(&mut Choices) -> &mut bool,
                    ),
                    (
                        s.option_assoc,
                        s.option_assoc_hint,
                        self.choices.assoc,
                        |choices| &mut choices.assoc,
                    ),
                    (
                        s.option_path,
                        s.option_path_hint,
                        self.choices.path,
                        |choices| &mut choices.path,
                    ),
                ] {
                    if let Some(chosen) = self.option_row(ui, title, hint, on, toggle) {
                        action = Some(chosen);
                    }
                }
                let (line, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.), egui::Sense::hover());
                ui.painter().rect_filled(line, 0, color(LINE));
                row(ui, 10., |ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(16., 16.), egui::Sense::hover());
                    ui::glyph(ui.painter(), rect, Glyph::Folder, color(MUTED));
                    ui.label(
                        RichText::new(s.location)
                            .font(theme::display(12.5))
                            .color(color(SLATE)),
                    );
                    ui.label(
                        RichText::new(self.dir.display().to_string())
                            .font(theme::text(12.5))
                            .color(color(MUTED)),
                    );
                });
            });
            ui.add_space(16.);
            row(ui, 12., |ui| {
                if ui::secondary_button(ui, s.back, Some(Glyph::ChevronLeft)).clicked() {
                    action = Some(Action::Back);
                }
                if ui::primary_button(ui, s.install, None, true).clicked() {
                    action = Some(Action::Begin);
                }
            });
            action
        })
    }

    pub(crate) fn installing(&self, ui: &mut Ui, area: Rect) {
        let s = self.lang.strings();
        let (title, text, labels) = if self.mode == Mode::Uninstall {
            (s.uninstalling_title, s.uninstalling_body, s.uninstall_steps)
        } else {
            (s.installing_title, s.installing_body, s.steps)
        };
        let area = Rect::from_min_max(pos2(area.left(), area.top() + 52.), area.max);
        column(ui, area, Align::Center, false, "installing", |ui| {
            heading(ui, title, 22.);
            ui.add_space(4.);
            body(ui, text, 13.5, SLATE);
            ui.add_space(20.);
            ui.add(assets::image("mark-sm.svg").fit_to_exact_size(vec2(96., 60.6)));
            ui.add_space(24.);
            ui.allocate_ui_with_layout(
                vec2(400., 20.),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 12.;
                    ui::progress_bar(ui, self.percent, 352.);
                    ui.allocate_ui_with_layout(
                        vec2(36., 20.),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(format!("{}%", self.percent.floor() as u32))
                                    .font(theme::display(13.)),
                            );
                        },
                    );
                },
            );
            ui.add_space(20.);
            ui.allocate_ui_with_layout(vec2(400., 140.), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 10.;
                for (state, label) in step_states(self.percent).into_iter().zip(labels) {
                    ui::step_row(ui, state, label);
                }
            });
        });
    }

    pub(crate) fn done(&self, ui: &mut Ui, area: Rect) -> Option<Action> {
        let s = self.lang.strings();
        let uninstalled = self.mode == Mode::Uninstall;
        let (title, text, label) = if uninstalled {
            (s.uninstalled_title, s.uninstalled_body, s.close)
        } else {
            (s.done_title, s.done_body, s.start)
        };
        let area = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - 8.));
        column(ui, area, Align::Center, true, "done", |ui| {
            let mut action = None;
            heading(ui, title, 28.);
            ui.add_space(4.);
            body(ui, text, 14., SLATE);
            ui.add_space(22.);
            ui.add(assets::image("mark-lg.svg").fit_to_exact_size(vec2(140., 88.5)));
            ui.add_space(26.);
            if ui::primary_button(ui, label, Some(340.), !uninstalled).clicked() {
                action = Some(Action::Finish);
            }
            if !uninstalled {
                ui.add_space(16.);
                if ui::link(ui, s.whats_new).clicked() {
                    action = Some(Action::Releases);
                }
            }
            action
        })
    }

    pub(crate) fn failed(&self, ui: &mut Ui, area: Rect) -> Option<Action> {
        let s = self.lang.strings();
        let title = if self.mode == Mode::Uninstall {
            s.failed_uninstall
        } else {
            s.failed_install
        };
        let detail = truncated(self.error.as_deref().unwrap_or_default());
        let area = Rect::from_min_max(
            pos2(area.left() + 64., area.top()),
            pos2(area.right() - 64., area.bottom() - 8.),
        );
        column(ui, area, Align::Center, true, "failed", |ui| {
            ui.add(assets::image("mark-sm.svg").fit_to_exact_size(vec2(80., 50.5)));
            ui.add_space(16.);
            heading(ui, title, 24.);
            ui.add_space(4.);
            body(ui, s.failed_hint, 14., SLATE);
            ui.add_space(14.);
            egui::Frame::new()
                .fill(color(WHITE))
                .stroke(Stroke::new(1., color(LINE)))
                .corner_radius(10)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(496.);
                    ui.label(
                        RichText::new(detail)
                            .font(theme::text(12.))
                            .line_height(Some(17.))
                            .color(color(SLATE)),
                    );
                });
            ui.add_space(20.);
            row(ui, 12., |ui| {
                if ui::primary_button(ui, s.retry, None, false).clicked() {
                    return Some(Action::Begin);
                }
                ui::secondary_button(ui, s.close, None)
                    .clicked()
                    .then_some(Action::Close)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_error_is_cut_and_a_short_one_is_kept() {
        assert_eq!(truncated("disk full"), "disk full");
        let long = "x".repeat(ERROR_CHARS + 50);
        let cut = truncated(&long);
        assert_eq!(cut.chars().count(), ERROR_CHARS + 3);
        assert!(cut.ends_with("..."));
    }
}
