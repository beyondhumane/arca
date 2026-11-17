//! Repairing a damaged archive, and offering the copy that comes out.

use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};

impl Shell {
    pub(super) fn repaired_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(offer) = self.controller.repair_offer() else {
            return;
        };
        heading(ui, s.repair_title);
        ui.label(RichText::new(&offer.text).color(hex(p.muted)));
        let mut answer = None;
        Self::footer_buttons(ui, |ui| {
            if primary_button(ui, s.open_repaired, true).clicked() {
                answer = Some(true);
            }
            if secondary_button(ui, s.close_word, true).clicked() {
                answer = Some(false);
            }
        });
        if let Some(open) = answer {
            self.controller.answer_repair_offer(open);
        }
    }

    pub(super) fn begin_repair_of_archive(&mut self) {
        if let Some(archive) = self.controller.state.archive.clone() {
            self.controller.start_repair(archive);
        }
    }

    pub(super) fn begin_repair_of_row(&mut self, index: usize) {
        let rows = self.controller.visible_rows();
        let Some(row) = rows.get(index) else {
            return;
        };
        let path = crate::controller::disk::disk_path(&row.path);
        if AppController::can_repair(&path) {
            self.controller.start_repair(path);
        }
    }

    /// A repair is offered on a file on the disk that is an archive it takes.
    pub(super) fn repair_applies(&self, row: Option<&Row>) -> bool {
        row.is_some_and(|row| {
            !row.is_dir && AppController::can_repair(&crate::controller::disk::disk_path(&row.path))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repair_is_offered_on_the_disk_only() {
        assert!(RowAction::Repair.shown(true) && !RowAction::Repair.shown(false));
        assert!(!RowAction::Repair.writable_only());
        assert!(!RowAction::EMPTY_SPACE.contains(&RowAction::Repair));
    }
}
