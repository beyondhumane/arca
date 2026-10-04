use super::*;

impl GpuiShell {
    pub(super) fn job_panel(&self, cx: &mut Context<Self>) -> Option<Stateful<gpui::Div>> {
        let s = self.controller.s();
        if self.controller.state.busy {
            use std::sync::atomic::Ordering;
            let total = self.controller.state.total_count;
            let done = self.controller.state.done_count;
            let fraction = if total == 0 {
                0.0
            } else {
                done as f64 / total as f64
            };
            let progress = match (total, self.controller.state.in_bytes) {
                (0, _) => format!("{}…", s.working_word),
                // A download counts bytes, not files, and nobody reads
                // "2481152 of 4627170".
                (total, true) => format!("{} / {}", human(done as u64), human(total as u64)),
                (total, false) => format!("{done} / {total}"),
            };
            let held = self.controller.state.hold.load(Ordering::Relaxed);
            let asked = self.controller.state.stop.load(Ordering::Relaxed);
            // The clock, and the guess of what is left made from how long the
            // part already done took. Only once enough of it is done for the
            // guess to be worth reading: at two per cent it would say an hour
            // and then a minute. A paused job is not going anywhere.
            let mut timing = self
                .controller
                .state
                .started
                .map(|started| {
                    let gone = started.elapsed().as_secs_f64();
                    let mut text = format!("{} {}", s.elapsed_word, super::clock(gone));
                    if !held && fraction > 0.05 {
                        text.push_str(&format!(
                            " · {} {}",
                            s.time_left,
                            super::clock(gone / fraction - gone)
                        ));
                    }
                    text
                })
                .unwrap_or_default();
            if asked {
                timing.push_str(&format!(" · {}", s.stopping));
            } else if held {
                timing.push_str(&format!(" · {}", s.paused_word));
            }
            // Pausing lets go at the end of an entry, not the end of a byte, so
            // a file that has started still has to finish.
            // Transport marks rather than words: the strip already ends in a
            // line of grey text, and two more words at the end of it read as
            // the rest of the sentence instead of as something to press.
            let hold_button = Self::icon_button(
                cx,
                "pause-job",
                if held {
                    IconName::Play
                } else {
                    IconName::Pause
                },
                if held { s.resume_word } else { s.pause_word }.to_string(),
                !self.background_blocked() && !asked,
            )
            .xsmall()
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.background_blocked() {
                    this.controller.state.hold.store(!held, Ordering::Relaxed);
                    cx.notify();
                }
            }));
            let cancel = Self::icon_button(
                cx,
                "cancel-job",
                IconName::Close,
                s.cancel.to_string(),
                !self.background_blocked() && !asked,
            )
            .xsmall()
            .on_click(cx.listener(|this, _, _, cx| {
                if !this.background_blocked() {
                    this.controller.dispatch(AppAction::CancelJob);
                    cx.notify();
                }
            }));
            // A strip across the top of the content pane rather than another
            // floating row: work happening to the archive belongs above the
            // archive, and it must not shove the list down a line when it
            // appears.
            // The bar owns the accessible name and the numeric value; the
            // strip around it is a status region whose text is read as it
            // changes. Naming both would announce the same thing twice.
            let mut progress_view = div()
                .id("progress")
                .flex_none()
                .h(px(30.))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .bg(cx.theme().secondary)
                .border_b_1()
                .border_color(cx.theme().border)
                .child(progress)
                .child(
                    Progress::new("job-progress")
                        .accessibility_label(s.progress_region)
                        .value(fraction as f32 * 100.)
                        .loading(total == 0)
                        .w(px(96.))
                        .flex_none(),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.controller.state.current_file.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(cx.theme().muted_foreground)
                        .child(timing),
                )
                .child(hold_button)
                .child(cancel);
            if !self.background_blocked() {
                progress_view = progress_view.role(Role::Status);
            }
            return Some(progress_view);
        }

        None
    }
}
