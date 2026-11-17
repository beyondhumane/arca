use super::theme::{hex, mono_font, palette_of, text_size};
use super::widgets::*;
use super::*;
use arca_core::hash::Algorithm;
use eframe::egui::{self, RichText};

impl Shell {
    pub(super) fn checksums_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(found) = self.controller.checksums() else {
            return;
        };
        heading(ui, s.checksums_word);
        muted(
            ui,
            format!(
                "{} \u{b7} {}",
                found.source,
                fill(s.checksums_done, &[("n", &found.rows.len().to_string())])
            ),
        );
        ui.add_space(8.0);
        let choices: Vec<(&str, bool)> = Algorithm::ALL
            .iter()
            .map(|a| (a.label(), *a == found.algorithm))
            .collect();
        let picked = segmented(ui, &choices);
        let at = Algorithm::ALL
            .iter()
            .position(|a| *a == found.algorithm)
            .unwrap_or(0);
        ui.add_space(8.0);
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
        egui::ScrollArea::both()
            .id_salt("arca-checksums")
            .max_height(320.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_height, found.rows.len(), |ui, range| {
                for row in &found.rows[range] {
                    ui.horizontal(|ui| {
                        let (text, color) = match &row.sums {
                            Ok(sums) => (sums[at].as_str(), hex(p.text)),
                            Err(error) => (error.as_str(), hex(p.danger)),
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(text)
                                    .font(mono_font(ui.style(), -1.0))
                                    .color(color),
                            )
                            .extend(),
                        );
                        ui.add_space(12.0);
                        ui.add(egui::Label::new(&row.name).extend());
                    });
                }
            });
        let failed = found.failed();
        if failed > 0 {
            ui.add_space(6.0);
            ui.label(
                RichText::new(fill(s.checksums_unreadable, &[("n", &failed.to_string())]))
                    .size(text_size(ui.style(), -1.0))
                    .color(hex(p.danger)),
            );
        }
        let mut close = false;
        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y + 8.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if primary_button(ui, s.close_word, true).clicked() {
                    close = true;
                }
                if secondary_button(ui, s.copy_word, found.any_read()).clicked() {
                    ui.ctx().copy_text(found.lines());
                }
            },
        );
        if let (Some(i), Some(found)) = (picked, self.controller.checksums_mut()) {
            found.algorithm = Algorithm::ALL[i];
        }
        if close {
            self.controller.close_checksums();
        }
    }
}
