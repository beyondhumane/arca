use crate::{
    app::{step_states, Screen, Setup, HEADER},
    engine::{Choices, Mode},
    theme::{self, color, LINE, MUTED, SLATE, WHITE},
    ui,
};
use gpui::{div, img, prelude::*, px, Context, Div, FontWeight, IntoElement};

const RELEASES: &str = "https://github.com/THIONG/arca/releases/latest";
const ERROR_CHARS: usize = 260;

fn heading(text: &'static str, size: f32) -> Div {
    div()
        .font_family(theme::display_font())
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(size))
        .line_height(px(size + 6.))
        .child(text)
}

fn truncated(text: &str) -> String {
    if text.chars().count() <= ERROR_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(ERROR_CHARS).collect();
    format!("{head}...")
}

impl Setup {
    pub(crate) fn welcome(&self, cx: &mut Context<Self>) -> Div {
        let s = self.lang.strings();
        let uninstalling = self.mode == Mode::Uninstall;
        let (title, body) = if uninstalling {
            (s.confirm_title, s.confirm_body)
        } else {
            (s.welcome_title, s.welcome_body)
        };
        let primary = if uninstalling {
            ui::primary_button("install", s.uninstall, None, false)
        } else {
            ui::primary_button("install", s.install, None, true)
        };
        let secondary = if uninstalling {
            ui::secondary_button("customize", s.cancel, None)
                .on_click(cx.listener(|_, _, window, _| window.remove_window()))
        } else {
            ui::secondary_button("customize", s.customize, None).on_click(cx.listener(
                |setup, _, _, cx| {
                    setup.screen = Screen::Options;
                    cx.notify();
                },
            ))
        };
        div()
            .flex()
            .flex_col()
            .justify_center()
            .size_full()
            .pl(px(64.))
            .pb(px(8.))
            .child(img("logo-horizontal.svg").w(px(300.)).h(px(65.6)))
            .child(div().mt(px(34.)).child(heading(title, 28.)))
            .child(
                div()
                    .mt(px(10.))
                    .w(px(420.))
                    .text_size(px(14.5))
                    .line_height(px(22.))
                    .text_color(color(SLATE))
                    .child(body),
            )
            .child(
                div()
                    .mt(px(28.))
                    .flex()
                    .flex_row()
                    .gap(px(12.))
                    .child(
                        primary
                            .on_click(cx.listener(|setup, _, window, cx| setup.begin(window, cx))),
                    )
                    .child(secondary),
            )
    }

    fn option_row(
        &self,
        id: &'static str,
        title: &'static str,
        hint: &'static str,
        on: bool,
        toggle: fn(&mut Choices) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(16.))
            .child(
                ui::switch(id, on).on_click(cx.listener(move |setup, _, _, cx| {
                    setup.toggle(toggle, cx);
                })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(theme::display_font())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(14.))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(color(MUTED))
                            .child(hint),
                    ),
            )
    }

    pub(crate) fn options(&self, cx: &mut Context<Self>) -> Div {
        let s = self.lang.strings();
        div()
            .flex()
            .flex_col()
            .size_full()
            .pt(px(HEADER + 4.))
            .px(px(64.))
            .child(heading(s.options_title, 24.))
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(14.))
                    .text_color(color(SLATE))
                    .child(s.options_body),
            )
            .child(
                ui::card()
                    .mt(px(14.))
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(self.option_row(
                        "opt-menu",
                        s.option_menu,
                        s.option_menu_hint,
                        self.choices.menu,
                        |choices| &mut choices.menu,
                        cx,
                    ))
                    .child(self.option_row(
                        "opt-assoc",
                        s.option_assoc,
                        s.option_assoc_hint,
                        self.choices.assoc,
                        |choices| &mut choices.assoc,
                        cx,
                    ))
                    .child(self.option_row(
                        "opt-path",
                        s.option_path,
                        s.option_path_hint,
                        self.choices.path,
                        |choices| &mut choices.path,
                        cx,
                    ))
                    .child(div().h(px(1.)).bg(color(LINE)))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .text_size(px(12.5))
                            .text_color(color(SLATE))
                            .child(ui::icon("icons/folder.svg", 16., MUTED))
                            .child(
                                div()
                                    .font_family(theme::display_font())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(s.location),
                            )
                            .child(
                                div()
                                    .text_color(color(MUTED))
                                    .child(self.dir.display().to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(16.))
                    .flex()
                    .flex_row()
                    .gap(px(12.))
                    .child(
                        ui::secondary_button("back", s.back, Some("icons/chevron-left.svg"))
                            .on_click(cx.listener(|setup, _, _, cx| {
                                setup.screen = Screen::Welcome;
                                cx.notify();
                            })),
                    )
                    .child(
                        ui::primary_button("install-now", s.install, None, true)
                            .on_click(cx.listener(|setup, _, window, cx| setup.begin(window, cx))),
                    ),
            )
    }

    pub(crate) fn installing(&self) -> Div {
        let s = self.lang.strings();
        let (title, body, labels) = if self.mode == Mode::Uninstall {
            (s.uninstalling_title, s.uninstalling_body, s.uninstall_steps)
        } else {
            (s.installing_title, s.installing_body, s.steps)
        };
        let mut steps = div().flex().flex_col().gap(px(10.)).w(px(400.));
        for (state, label) in step_states(self.percent).into_iter().zip(labels) {
            steps = steps.child(ui::step_row(state, label));
        }
        div()
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .pt(px(52.))
            .child(heading(title, 22.))
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(13.5))
                    .text_color(color(SLATE))
                    .child(body),
            )
            .child(img("mark-sm.svg").mt(px(20.)).w(px(96.)).h(px(60.6)))
            .child(
                div()
                    .mt(px(24.))
                    .w(px(400.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.))
                    .child(ui::progress_bar(self.percent))
                    .child(
                        div()
                            .w(px(36.))
                            .text_right()
                            .font_family(theme::display_font())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(format!("{}%", self.percent.floor() as u32)),
                    ),
            )
            .child(div().mt(px(20.)).child(steps))
    }

    pub(crate) fn done(&self, cx: &mut Context<Self>) -> Div {
        let s = self.lang.strings();
        let uninstalled = self.mode == Mode::Uninstall;
        let (title, body, action) = if uninstalled {
            (s.uninstalled_title, s.uninstalled_body, s.close)
        } else {
            (s.done_title, s.done_body, s.start)
        };
        let mut column = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .pb(px(8.))
            .child(heading(title, 28.))
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(14.))
                    .text_color(color(SLATE))
                    .child(body),
            )
            .child(img("mark-lg.svg").mt(px(22.)).w(px(140.)).h(px(88.5)))
            .child(
                div().mt(px(26.)).child(
                    ui::primary_button("start", action, Some(340.), !uninstalled)
                        .on_click(cx.listener(|setup, _, window, _| setup.finish(window))),
                ),
            );
        if !uninstalled {
            column = column.child(
                div().mt(px(16.)).child(
                    ui::link("news", s.whats_new)
                        .on_click(cx.listener(|_, _, _, cx| cx.open_url(RELEASES))),
                ),
            );
        }
        column
    }

    pub(crate) fn failed(&self, cx: &mut Context<Self>) -> Div {
        let s = self.lang.strings();
        let title = if self.mode == Mode::Uninstall {
            s.failed_uninstall
        } else {
            s.failed_install
        };
        let detail = truncated(self.error.as_deref().unwrap_or_default());
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .px(px(64.))
            .pb(px(8.))
            .child(img("mark-sm.svg").w(px(80.)).h(px(50.5)))
            .child(div().mt(px(16.)).child(heading(title, 24.)))
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(14.))
                    .text_color(color(SLATE))
                    .child(s.failed_hint),
            )
            .child(
                div()
                    .mt(px(14.))
                    .w(px(520.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(color(WHITE))
                    .border_1()
                    .border_color(color(LINE))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(color(SLATE))
                    .child(detail),
            )
            .child(
                div()
                    .mt(px(20.))
                    .flex()
                    .flex_row()
                    .gap(px(12.))
                    .child(
                        ui::primary_button("retry", s.retry, None, false)
                            .on_click(cx.listener(|setup, _, window, cx| setup.begin(window, cx))),
                    )
                    .child(
                        ui::secondary_button("close", s.close, None)
                            .on_click(cx.listener(|_, _, window, _| window.remove_window())),
                    ),
            )
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

    #[test]
    fn the_cut_never_splits_a_character() {
        let long = "ñ".repeat(ERROR_CHARS + 10);
        assert!(truncated(&long).starts_with('ñ'));
    }
}
