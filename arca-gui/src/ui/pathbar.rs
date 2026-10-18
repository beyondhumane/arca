use super::theme::{hex, palette_of, text_font, text_size};
use super::Shell;
use crate::controller::disk_path;
use crate::tree::Folder;
use crate::AppAction;
use eframe::egui;
use egui_phosphor::regular as icon;
use std::path::PathBuf;

const PATH_ID: &str = "arca-path";
const SUGGESTIONS: usize = 8;
const LISTING_LIMIT: usize = 5000;

/// The folder part of what was typed, up to its last separator, and the
/// start of the name after it.
pub(crate) fn split_typed(text: &str) -> (&str, &str) {
    match text.rfind(['/', '\\']) {
        Some(i) => (&text[..=i], &text[i + 1..]),
        None => ("", text),
    }
}

fn joined(parts: &[&str]) -> String {
    parts.iter().map(|part| format!("{part}/")).collect()
}

fn walked(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    for part in text.split(['/', '\\']).filter(|p| !p.is_empty()) {
        match part {
            "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// A typed disk path in the form the browser keeps folders in, or `None`
/// when it cannot name a folder on this disk.
pub(crate) fn disk_dir_of(text: &str, home: Option<PathBuf>) -> Option<String> {
    let text = text.trim();
    let expanded = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            format!("{}{rest}", home?.display())
        }
        _ => text.to_string(),
    };
    if !cfg!(windows) && !expanded.starts_with('/') {
        return None;
    }
    Some(joined(&walked(&expanded)))
}

/// A typed path inside the open archive, from its root.
pub(crate) fn archive_dir_of(text: &str) -> String {
    joined(&walked(text.trim()))
}

fn folder_at<'a>(root: &'a Folder, dir: &str) -> Option<&'a Folder> {
    dir.split('/')
        .filter(|part| !part.is_empty())
        .try_fold(root, |folder, part| folder.kids.get(part))
}

pub(crate) fn matching(names: &[String], stem: &str, hidden: bool) -> Vec<String> {
    let stem = stem.to_lowercase();
    names
        .iter()
        .filter(|name| hidden || stem.starts_with('.') || !name.starts_with('.'))
        .filter(|name| name.to_lowercase().starts_with(&stem))
        .take(SUGGESTIONS)
        .cloned()
        .collect()
}

impl Shell {
    fn shown_path(&self) -> String {
        let dir = &self.controller.state.active_pane().directory;
        if self.controller.on_disk() {
            let mut text = disk_path(dir).display().to_string();
            if !text.is_empty() && !text.ends_with(['/', '\\']) {
                text.push(std::path::MAIN_SEPARATOR);
            }
            text
        } else {
            format!("/{dir}")
        }
    }

    pub(super) fn start_path_edit(&mut self) {
        if self.crumbs().is_empty() {
            return;
        }
        self.path_edit = Some(self.shown_path());
        self.focus_path = true;
        self.path_pick = None;
        self.path_missing = false;
    }

    pub(super) fn copy_current_path(&self) {
        let text = self.shown_path();
        let trimmed = text.trim_end_matches(['/', '\\']);
        self.ctx.copy_text(if trimmed.is_empty() {
            text.clone()
        } else {
            trimmed.to_string()
        });
    }

    fn folder_names(&mut self, parent: &str) -> Vec<String> {
        if !self.controller.on_disk() {
            return folder_at(&self.controller.state.folders, &archive_dir_of(parent))
                .map(|folder| folder.kids.keys().cloned().collect())
                .unwrap_or_default();
        }
        let Some(dir) = disk_dir_of(parent, home()) else {
            return Vec::new();
        };
        if let Some((cached, names)) = &self.path_cache {
            if *cached == dir {
                return names.clone();
            }
        }
        let mut names: Vec<String> = std::fs::read_dir(disk_path(&dir))
            .map(|read| {
                read.flatten()
                    .take(LISTING_LIMIT)
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|name| name.to_lowercase());
        self.path_cache = Some((dir, names.clone()));
        names
    }

    fn go_typed(&mut self, typed: &str) -> bool {
        let dir = if self.controller.on_disk() {
            match disk_dir_of(typed, home()) {
                Some(dir) => dir,
                None => return false,
            }
        } else {
            archive_dir_of(typed)
        };
        let exists = if self.controller.on_disk() {
            dir.is_empty() || disk_path(&dir).is_dir()
        } else {
            folder_at(&self.controller.state.folders, &dir).is_some()
        };
        if exists {
            self.controller.dispatch(AppAction::Navigate(dir));
        }
        exists
    }

    pub(super) fn path_field(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let id = egui::Id::new(PATH_ID);
        let focused = ui.memory(|m| m.has_focus(id));
        let mut text = self.path_edit.take().unwrap_or_default();
        let (parent, stem) = split_typed(&text);
        let parent = parent.to_string();
        let names = self.folder_names(&parent);
        let suggestions = matching(&names, stem, self.controller.state.settings.show_hidden);
        if self.path_pick.is_some_and(|i| i >= suggestions.len()) {
            self.path_pick = None;
        }
        let (mut tab, mut up, mut down) = (false, false, false);
        if focused {
            ui.input_mut(|i| {
                tab = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
            });
        }
        let count = suggestions.len();
        if count > 0 && down {
            self.path_pick = Some(self.path_pick.map_or(0, |i| (i + 1) % count));
        }
        if count > 0 && up {
            self.path_pick = Some(
                self.path_pick
                    .map_or(count - 1, |i| (i + count - 1) % count),
            );
        }
        let mut to_end = self.focus_path;
        if tab {
            if let Some(name) = suggestions.get(self.path_pick.unwrap_or(0)) {
                text = format!("{parent}{name}/");
                self.path_pick = None;
                self.path_missing = false;
                to_end = true;
            }
        }
        let response = ui.add(
            super::widgets::field(ui, &mut text)
                .id(id)
                // Keep Tab for completion; the key itself is consumed above.
                .lock_focus(true)
                .hint_text(s.edit_path)
                .background_color(hex(p.raised))
                .desired_width(ui.available_width()),
        );
        if self.path_missing {
            ui.painter().rect_stroke(
                response.rect,
                4,
                egui::Stroke::new(1.0, hex(p.danger)),
                egui::StrokeKind::Outside,
            );
        }
        let response = if self.path_missing {
            response.on_hover_text(s.no_such_folder)
        } else {
            response
        };
        if response.changed() {
            self.path_pick = None;
            self.path_missing = false;
        }
        if self.focus_path {
            response.request_focus();
            self.focus_path = false;
            self.path_grace = 2;
        }
        if to_end {
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                let end = egui::text::CCursor::new(text.chars().count());
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(end)));
                state.store(ui.ctx(), id);
            }
        }
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
        let pointer = ui.input(|i| i.pointer.any_down() || i.pointer.any_released());
        let mut clicked = None;
        if (focused || pointer || self.path_grace > 0) && !suggestions.is_empty() && !escape {
            // As wide as the field, with where each folder lives on the
            // right and the keys that work along the bottom.
            let width = response.rect.width();
            let home = crate::controller::disk::home_dir()
                .map(|home| crate::controller::disk::disk_dir(&home))
                .unwrap_or_default();
            let home = home.trim_matches('/');
            let here = parent.trim_matches('/');
            let place = match here.strip_prefix(home) {
                Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
                    format!("~{rest}")
                }
                _ => format!("/{here}"),
            };
            let frame = egui::Frame::popup(ui.style());
            let inner_width = width - frame.inner_margin.sum().x;
            egui::Area::new(id.with("suggestions"))
                .order(egui::Order::Foreground)
                .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 4.0))
                .show(ui.ctx(), |ui| {
                    frame.show(ui, |ui| {
                        ui.set_min_width(inner_width);
                        ui.set_max_width(inner_width);
                        let row_width = ui.available_width();
                        for (i, name) in suggestions.iter().enumerate() {
                            let (rect, row) = ui.allocate_exact_size(
                                egui::vec2(row_width, 28.0),
                                egui::Sense::click(),
                            );
                            let lit = ui
                                .ctx()
                                .animate_bool(row.id, row.hovered() || self.path_pick == Some(i));
                            ui.painter().rect_filled(
                                rect,
                                4,
                                super::theme::alpha(p.focus, 0.22 * lit),
                            );
                            ui.painter().text(
                                rect.left_center() + egui::vec2(10.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                icon::FOLDER,
                                super::theme::icon_font(15.0),
                                hex(p.link),
                            );
                            ui.painter().text(
                                rect.left_center() + egui::vec2(34.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{name}/"),
                                egui::TextStyle::Body.resolve(ui.style()),
                                hex(p.text),
                            );
                            ui.painter().text(
                                rect.right_center() - egui::vec2(10.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                &place,
                                text_font(ui.style(), -1.0),
                                hex(p.muted),
                            );
                            if row.clicked() {
                                clicked = Some(i);
                            }
                        }
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            for (keys, what) in [
                                (&["tab"][..], s.complete_word),
                                (&["up", "down"], s.choose_word),
                                (&["enter"], s.open_word),
                                (&["escape"], s.cancel),
                            ] {
                                for key in keys {
                                    super::widgets::kbd(
                                        ui,
                                        &super::dialogs::shortcut_label(key, s),
                                    );
                                }
                                ui.label(
                                    egui::RichText::new(what)
                                        .size(text_size(ui.style(), -1.0))
                                        .color(hex(p.muted)),
                                );
                                ui.add_space(10.0);
                            }
                        });
                        ui.add_space(2.0);
                    });
                });
        }
        let target = match clicked {
            Some(i) => Some(format!("{parent}{}/", suggestions[i])),
            None if enter => Some(match self.path_pick.and_then(|i| suggestions.get(i)) {
                Some(name) => format!("{parent}{name}/"),
                None => text.clone(),
            }),
            None => None,
        };
        if let Some(target) = target {
            if !self.go_typed(&target) {
                self.path_missing = true;
                self.focus_path = true;
                self.path_edit = Some(target);
            }
            return;
        }
        if escape {
            return;
        }
        if self.path_grace > 0 {
            self.path_grace -= 1;
        } else if !response.has_focus() && !ui.input(|i| i.pointer.any_down()) {
            return;
        }
        self.path_edit = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_splits_at_the_last_separator() {
        assert_eq!(split_typed("/home/ubu"), ("/home/", "ubu"));
        assert_eq!(split_typed("C:\\Users\\Pu"), ("C:\\Users\\", "Pu"));
        assert_eq!(split_typed("docs"), ("", "docs"));
    }

    #[test]
    fn archive_paths_are_kept_inside_the_archive() {
        assert_eq!(archive_dir_of("/a/b"), "a/b/");
        assert_eq!(archive_dir_of("/a/../../b/./c/"), "b/c/");
        assert_eq!(archive_dir_of("/"), "");
    }

    #[cfg(not(windows))]
    #[test]
    fn disk_paths_need_a_root_and_know_the_home() {
        assert_eq!(
            disk_dir_of("/home//ubuntu/", None).as_deref(),
            Some("home/ubuntu/")
        );
        assert_eq!(disk_dir_of("/", None).as_deref(), Some(""));
        assert_eq!(disk_dir_of("relative/dir", None), None);
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(disk_dir_of("~/src", home).as_deref(), Some("home/me/src/"));
        assert_eq!(disk_dir_of("~/src", None), None);
    }

    #[test]
    fn suggestions_start_with_what_was_typed_and_skip_hidden_ones() {
        let names: Vec<String> = [".config", "Documents", "Downloads", "music"]
            .map(String::from)
            .to_vec();
        assert_eq!(matching(&names, "do", false), ["Documents", "Downloads"]);
        assert_eq!(matching(&names, "", false).len(), 3);
        assert_eq!(matching(&names, ".", false), [".config"]);
        assert_eq!(matching(&names, "", true).len(), 4);
    }
}
