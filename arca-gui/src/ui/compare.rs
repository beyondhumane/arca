use super::theme::{hex, mono_font, palette_of, text_size};
use super::widgets::*;
use super::*;
use arca_compare::Change;
use eframe::egui::{self, RichText};

/// The native picker for the other side of a comparison, opened beside
/// `first` or, failing that, beside the open archive.
pub(super) fn picked(
    first: Option<PathBuf>,
    folder: bool,
    beside: Option<PathBuf>,
) -> DialogResult {
    let near = first
        .as_deref()
        .and_then(Path::parent)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or(beside);
    let picker = rfd::FileDialog::new();
    let picker = match near {
        Some(dir) => picker.set_directory(dir),
        None => picker,
    };
    let second = if folder {
        picker.pick_folder()
    } else {
        let picker = picker.add_filter("Archives", &crate::open_filter());
        let picker = if cfg!(feature = "rar") {
            picker.add_filter("All files (including RAR volumes)", &["*"])
        } else {
            picker
        };
        picker.pick_file()
    };
    DialogResult::Compare { first, second }
}

impl Shell {
    /// Two things picked on the disk are compared with each other at once;
    /// one thing, or the open archive, is compared with what gets picked next.
    pub(super) fn begin_compare(&mut self, folder: bool) {
        if self.controller.compare_picked() {
            return;
        }
        let first = if self.controller.on_disk() {
            match self.controller.selected_disk_paths().as_slice() {
                [one] => Some(one.clone()),
                _ => return,
            }
        } else if self.controller.state.archive.is_some() {
            None
        } else {
            return;
        };
        self.begin_dialog(DialogKind::Compare { first, folder });
    }

    pub(super) fn compare_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(found) = self.controller.comparison() else {
            return;
        };
        heading(ui, s.compare_word);
        let summary = if found.rows.is_empty() {
            s.compare_identical.to_string()
        } else {
            fill(s.compare_done, &[("n", &found.rows.len().to_string())])
        };
        muted(
            ui,
            format!(
                "{} \u{b7} {summary}",
                fill(
                    s.compare_subject,
                    &[("a", &found.first), ("b", &found.second)]
                )
            ),
        );
        ui.add_space(8.0);
        let words = [
            s.compare_all,
            s.compare_only_first,
            s.compare_only_second,
            s.compare_changed,
        ];
        let labels: Vec<String> = CompareFilter::ALL
            .iter()
            .zip(words)
            .map(|(filter, word)| format!("{word} ({})", found.count(*filter)))
            .collect();
        let choices: Vec<(&str, bool)> = CompareFilter::ALL
            .iter()
            .zip(&labels)
            .map(|(filter, label)| (label.as_str(), *filter == found.filter))
            .collect();
        let picked = segmented(ui, &choices);
        ui.add_space(8.0);
        let shown = found.shown();
        if shown.is_empty() {
            muted(
                ui,
                if found.rows.is_empty() {
                    s.compare_identical
                } else {
                    s.compare_none_shown
                },
            );
        } else {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
            egui::ScrollArea::both()
                .id_salt("arca-compare")
                .max_height(320.0)
                .auto_shrink([false, true])
                .show_rows(ui, row_height, shown.len(), |ui, range| {
                    for row in &shown[range] {
                        let color = match row.change {
                            Change::Added => p.success,
                            Change::Removed => p.danger,
                            Change::Type => p.spark,
                            Change::Changed | Change::Time => p.warning,
                        };
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(
                                RichText::new(row.change.marker().to_string())
                                    .font(mono_font(ui.style(), 0.0))
                                    .color(hex(color)),
                            ));
                            ui.add_space(8.0);
                            ui.add(egui::Label::new(&row.path).extend());
                        });
                    }
                });
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(s.compare_legend)
                .size(text_size(ui.style(), -1.0))
                .color(hex(p.muted)),
        );
        let lines = found.lines();
        let mut close = false;
        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y + 8.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if primary_button(ui, s.close_word, true).clicked() {
                    close = true;
                }
                if secondary_button(ui, s.copy_word, !lines.is_empty()).clicked() {
                    ui.ctx().copy_text(lines);
                }
            },
        );
        if let (Some(i), Some(found)) = (picked, self.controller.comparison_mut()) {
            found.filter = CompareFilter::ALL[i];
        }
        if close {
            self.controller.close_comparison();
        }
    }
}
