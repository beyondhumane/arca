use super::theme::{hex, mono_font, palette_of, text_size};
use super::widgets::*;
use super::*;
use crate::controller::bench::cores;
use arca_bench::{speed_text, Codec, Options, Outcome, Phase, Speeds};
use eframe::egui::{self, RichText};

/// Thread counts offered: the powers of two below the cores, then the cores.
fn thread_choices() -> Vec<usize> {
    let all = cores();
    let mut v: Vec<usize> = std::iter::successors(Some(1usize), |n| n.checked_mul(2))
        .take_while(|&n| n < all)
        .collect();
    v.push(all);
    v
}

fn threads_text(s: &'static Strings, n: usize) -> String {
    if n == 1 {
        s.bench_one_thread.to_string()
    } else {
        fill(s.bench_n_threads, &[("n", &n.to_string())])
    }
}

impl Shell {
    pub(super) fn bench_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let state = self.controller.bench();
        let running = state.running();
        let threads = state.threads;
        let shown = state.report.as_ref().map_or(threads, |r| r.options.threads);
        heading(ui, s.bench_title);
        ui.add(egui::Label::new(RichText::new(s.bench_intro).color(hex(p.muted))).wrap());
        ui.add_space(10.0);

        let choices = thread_choices();
        let labels: Vec<String> = choices.iter().map(|n| n.to_string()).collect();
        let picked = ui
            .horizontal(|ui| {
                ui.label(s.bench_threads);
                ui.add_space(6.0);
                ui.add_enabled_ui(!running, |ui| {
                    let marks: Vec<(&str, bool)> = labels
                        .iter()
                        .zip(&choices)
                        .map(|(l, &n)| (l.as_str(), n == threads))
                        .collect();
                    segmented(ui, &marks)
                })
                .inner
            })
            .inner;
        ui.add_space(10.0);

        let multi = shown > 1;
        let mono = mono_font(ui.style(), 0.0);
        let cell = |text: String| RichText::new(text).font(mono.clone());
        egui::Grid::new("arca-bench-table")
            .num_columns(if multi { 6 } else { 4 })
            .spacing([18.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.label("");
                ui.strong(threads_text(s, 1));
                ui.label("");
                if multi {
                    ui.strong(threads_text(s, shown));
                    ui.label("");
                }
                ui.end_row();
                ui.strong(s.bench_codec);
                ui.strong(s.bench_ratio);
                for _ in 0..if multi { 2 } else { 1 } {
                    ui.strong(s.bench_compress);
                    ui.strong(s.bench_decompress);
                }
                ui.end_row();
                let speeds = |ui: &mut egui::Ui, v: &Option<Speeds>| {
                    ui.label(cell(format!("{:>8}", speed_text(v.map(|v| v.compress)))));
                    ui.label(cell(format!("{:>8}", speed_text(v.map(|v| v.decompress)))));
                };
                for codec in Codec::ALL {
                    ui.label(codec.label());
                    match state
                        .rows
                        .iter()
                        .find(|r| r.codec == codec)
                        .map(|r| &r.outcome)
                    {
                        Some(Outcome::Unavailable(why)) => {
                            muted(ui, s.bench_missing).on_hover_text(why);
                        }
                        Some(Outcome::Measured {
                            ratio,
                            single,
                            multi: many,
                        }) => {
                            ui.label(cell(format!("{:>6.1}%", ratio * 100.0)));
                            speeds(ui, single);
                            if multi {
                                speeds(ui, many);
                            }
                        }
                        None => {
                            ui.label(cell(format!("{:>7}", "-")));
                            speeds(ui, &None);
                            if multi {
                                speeds(ui, &None);
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        ui.add_space(8.0);

        if let Some(at) = state.progress {
            let fraction = at.step as f32 / at.steps.max(1) as f32;
            progress(ui, Some(fraction));
            let what = match (at.phase, at.codec) {
                (Phase::Generating, _) | (_, None) => s.bench_generating.to_string(),
                (phase, Some(codec)) => format!(
                    "{} \u{b7} {} \u{b7} {}",
                    codec.label(),
                    threads_text(s, at.threads),
                    if phase == Phase::Compressing {
                        s.bench_compressing
                    } else {
                        s.bench_decompressing
                    }
                ),
            };
            muted(ui, what);
        }
        if let Some(report) = &state.report {
            let mut line = format!(
                "{}: {} {}",
                s.bench_rating,
                threads_text(s, 1),
                speed_text(report.rating(false))
            );
            if report.options.threads > 1 {
                line.push_str(&format!(
                    "   {} {}",
                    threads_text(s, report.options.threads),
                    speed_text(report.rating(true))
                ));
            }
            ui.label(RichText::new(line).strong());
            if report.stopped {
                muted(ui, s.bench_stopped);
            }
        }
        if let Some(error) = &state.error {
            ui.label(RichText::new(error).color(hex(p.danger)));
        }
        ui.add_space(4.0);
        let small = text_size(ui.style(), -1.0);
        ui.add(
            egui::Label::new(RichText::new(s.bench_units).size(small).color(hex(p.muted))).wrap(),
        );
        let command = state
            .report
            .as_ref()
            .map_or_else(|| Options::new(threads).command(), |r| r.options.command());
        ui.add(
            egui::Label::new(
                RichText::new(fill(s.bench_command, &[("command", &command)]))
                    .size(small)
                    .color(hex(p.muted)),
            )
            .wrap(),
        );

        let text = state.text();
        let (mut close, mut toggle) = (false, false);
        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y + 8.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if secondary_button(ui, s.close_word, true).clicked() {
                    close = true;
                }
                let label = if running { s.bench_stop } else { s.bench_start };
                if primary_button(ui, label, true).clicked() {
                    toggle = true;
                }
                if secondary_button(ui, s.copy_word, text.is_some() && !running).clicked() {
                    if let Some(text) = &text {
                        ui.ctx().copy_text(text.clone());
                    }
                }
            },
        );
        if let Some(i) = picked {
            self.controller.set_bench_threads(choices[i]);
        }
        if toggle {
            if running {
                self.controller.stop_bench();
            } else {
                self.controller.start_bench();
            }
        }
        if close {
            self.controller.close_bench();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arca_bench::{Report, Row};

    fn draw(shell: &mut Shell, ctx: &egui::Context) {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| shell.bench_dialog(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn the_window_draws_empty_measured_and_finished() {
        assert_eq!(thread_choices().first(), Some(&1));
        assert_eq!(thread_choices().last(), Some(&cores()));
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, crate::Startup::Browse(None));
        shell.controller.open_bench();
        shell.controller.set_bench_threads(4);
        draw(&mut shell, &ctx);
        let speeds = Some(Speeds {
            compress: 12.5,
            decompress: 1250.0,
            compress_runs: 3,
            decompress_runs: 9,
        });
        let rows = vec![
            Row {
                codec: Codec::Deflate,
                threads: 4,
                outcome: Outcome::Measured {
                    ratio: 0.31,
                    single: speeds,
                    multi: None,
                },
            },
            Row {
                codec: Codec::Zstd,
                threads: 4,
                outcome: Outcome::Unavailable("not built".into()),
            },
        ];
        shell.controller.bench_mut().rows = rows.clone();
        draw(&mut shell, &ctx);
        shell.controller.bench_mut().report = Some(Report {
            options: Options::new(4),
            rows,
            stopped: true,
            native: false,
        });
        shell.controller.bench_mut().error = Some("broken".into());
        draw(&mut shell, &ctx);
        assert!(shell
            .controller
            .bench()
            .text()
            .unwrap()
            .contains("Reproduce: arca bench --codecs --threads 4"));
        assert!(shell.modal_kind() == Some(ModalKind::Benchmark));
        shell.cancel_modal(ModalKind::Benchmark);
        assert!(!shell.controller.bench().open);
    }
}
