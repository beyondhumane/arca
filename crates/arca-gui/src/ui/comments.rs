//! Reading and changing comments.

use super::theme::{hex, mono_font, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};

/// What the comment window is editing: the archive's own comment, or the
/// one on the entry named `entry`.
pub(crate) struct CommentEdit {
    entry: Option<String>,
    title: String,
    text: String,
    was: String,
    editable: bool,
    focus: bool,
}

const TOOLTIP_CHARS: usize = 600;

impl Shell {
    pub(super) fn begin_archive_comment(&mut self) {
        let s = self.controller.s();
        let was = self.controller.archive_comment();
        let editable = self.controller.can_comment();
        if !editable && was.is_empty() {
            return;
        }
        self.comment_edit = Some(CommentEdit {
            entry: None,
            title: s.archive_comment_word.to_string(),
            text: was.clone(),
            was,
            editable,
            focus: true,
        });
    }

    pub(super) fn begin_entry_comment(&mut self, index: usize) {
        let rows = self.controller.visible_rows();
        let Some(entry) = rows.get(index).and_then(|row| row.entry) else {
            return;
        };
        let Some(name) = self
            .controller
            .state
            .entries
            .get(entry)
            .map(|e| e.name.clone())
        else {
            return;
        };
        if !self.controller.can_comment() {
            return;
        }
        let s = self.controller.s();
        let was = self
            .controller
            .entry_comment(entry)
            .unwrap_or_default()
            .to_string();
        let leaf = name
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&name)
            .to_string();
        self.comment_edit = Some(CommentEdit {
            entry: Some(name),
            title: fill(s.entry_comment_title, &[("name", &leaf)]),
            text: was.clone(),
            was,
            editable: true,
            focus: true,
        });
    }

    pub(super) fn comment_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(edit) = self.comment_edit.as_mut() else {
            return;
        };
        heading(ui, &edit.title);
        if edit.editable {
            ui.label(RichText::new(s.comment_hint).color(hex(p.muted)));
        }
        ui.add_space(6.0);
        let editor = egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut edit.text)
                        .font(mono_font(ui.style(), 0.0))
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .interactive(edit.editable),
                )
            })
            .inner;
        named(&editor, &edit.title);
        if std::mem::take(&mut edit.focus) && edit.editable {
            editor.request_focus();
        }
        let editable = edit.editable;
        let mut save = false;
        let mut close = false;
        Self::footer_buttons(ui, |ui| {
            if editable {
                save = primary_button(ui, s.save_comment, true).clicked();
                close = secondary_button(ui, s.cancel, true).clicked();
            } else {
                close = primary_button(ui, s.close_word, true).clicked();
            }
        });
        if save {
            if let Some(edit) = self.comment_edit.take() {
                if edit.text != edit.was {
                    self.controller.save_comment(edit.entry, edit.text);
                }
            }
        } else if close {
            self.comment_edit = None;
        }
    }

    /// Whether the entry of the row menu is there for this row at all: a
    /// comment only on an entry of a ZIP, a repair only on an archive on disk.
    pub(super) fn row_action_applies(&self, action: RowAction, row: Option<&Row>) -> bool {
        match action {
            RowAction::Comment => {
                self.controller.can_comment() && row.is_some_and(|row| row.entry.is_some())
            }
            RowAction::Repair => self.repair_applies(row),
            _ => true,
        }
    }

    pub(super) fn comment_tooltip(&self, response: &egui::Response, row: &Row) {
        let Some(comment) = row.entry.and_then(|i| self.controller.entry_comment(i)) else {
            return;
        };
        let mut shown: String = comment.chars().take(TOOLTIP_CHARS).collect();
        if comment.chars().count() > TOOLTIP_CHARS {
            shown.push('…');
        }
        response.clone().on_hover_text(shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_offered_on_the_entries_of_an_archive_only() {
        assert!(RowAction::Comment.shown(false) && !RowAction::Comment.shown(true));
        assert!(RowAction::Comment.writable_only());
        assert!(!RowAction::EMPTY_SPACE.contains(&RowAction::Comment));
    }
}
