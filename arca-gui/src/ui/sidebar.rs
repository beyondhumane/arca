use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use crate::tree::Folder;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

pub(crate) fn place_glyph(kind: PlaceKind) -> &'static str {
    match kind {
        PlaceKind::Home => icon::HOUSE,
        PlaceKind::Desktop => icon::DESKTOP,
        PlaceKind::Documents => icon::FILE_TEXT,
        PlaceKind::Downloads => icon::DOWNLOAD_SIMPLE,
        PlaceKind::Pictures => icon::IMAGE,
        PlaceKind::Music => icon::MUSIC_NOTE,
        PlaceKind::Videos => icon::FILM_STRIP,
        PlaceKind::Computer => icon::MONITOR,
        PlaceKind::Drive => icon::HARD_DRIVE,
        PlaceKind::Pinned => icon::PUSH_PIN,
    }
}

pub(crate) fn folder_children<'a>(
    folder: &'a Folder,
    path: &str,
) -> Vec<(String, String, &'a Folder)> {
    folder
        .kids
        .iter()
        .map(|(label, kid)| (label.clone(), format!("{path}{label}/"), kid))
        .collect()
}

enum TreeEvent {
    Navigate(String),
    Action(RowAction, String),
    Drop(Vec<String>, String),
}

impl Shell {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, rail: bool) {
        let s = self.controller.s();
        let here = self
            .controller
            .on_disk()
            .then(|| disk_path(&self.controller.state.current_dir));
        let mut browse = None;
        let mut unpin = None;
        let mut open = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (title, places, pinned) in [
                    (s.places_group, self.controller.places(), false),
                    (s.pinned_group, self.controller.pinned_places(), true),
                    (s.devices_group, self.controller.devices(), false),
                ] {
                    if places.is_empty() {
                        continue;
                    }
                    if rail {
                        ui.separator();
                    } else {
                        group_label(ui, title);
                    }
                    for place in places {
                        let selected = here.as_ref() == Some(&place.path);
                        let response =
                            nav_item(ui, place_glyph(place.kind), &place.label, selected, rail);
                        if response.clicked() {
                            browse = Some(place.path.clone());
                        }
                        if pinned {
                            response.context_menu(|ui| {
                                if ui.button(s.unpin_word).clicked() {
                                    unpin = Some(place.path.clone());
                                }
                            });
                        }
                    }
                }
                let recent = &self.controller.state.settings.recent;
                if !rail && !recent.is_empty() {
                    group_label(ui, s.recent_group);
                    for path in recent.iter().take(RECENT_MAX) {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        let open_now = self
                            .controller
                            .state
                            .archive
                            .as_ref()
                            .is_some_and(|archive| archive == std::path::Path::new(path));
                        if nav_item(ui, icon::CLOCK_COUNTER_CLOCKWISE, &name, open_now, false)
                            .on_hover_text(path)
                            .clicked()
                        {
                            open = Some(PathBuf::from(path));
                        }
                    }
                }
                if !rail && self.controller.state.archive.is_some() && !self.controller.on_disk() {
                    self.archive_tree(ui);
                }
            });
        if !self.background_idle() {
            return;
        }
        if let Some(path) = browse {
            self.controller.dispatch(AppAction::Browse(path));
        }
        if let Some(path) = unpin {
            self.controller.dispatch(AppAction::TogglePinned(path));
        }
        if let Some(path) = open {
            self.controller.dispatch(AppAction::Open(path));
        }
    }

    fn archive_tree(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        group_label(ui, s.archive_folders);
        let root = self
            .controller
            .state
            .archive
            .as_ref()
            .and_then(|archive| archive.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let current = self.controller.state.current_dir.clone();
        let mut event = None;
        if nav_item(ui, icon::ARCHIVE, &root, current.is_empty(), false).clicked() {
            event = Some(TreeEvent::Navigate(String::new()));
        }
        let context = TreeContext {
            s,
            writable: self.controller.writable(),
            on_disk: self.controller.on_disk(),
            current: &current,
            renaming: self
                .controller
                .state
                .renaming
                .as_ref()
                .filter(|_| self.renaming_in_tree)
                .map(|(path, _)| path.clone()),
            muted: hex(p.muted),
        };
        let mut rename = RenameField {
            value: &mut self.rename_value,
            focus: &mut self.focus_rename,
            outcome: None,
        };
        tree(
            ui,
            &self.controller.state.folders,
            "",
            &context,
            &mut event,
            &mut rename,
        );
        match rename.outcome {
            Some(true) => self.commit_rename(),
            Some(false) => self.cancel_rename(),
            None => {}
        }
        match event {
            Some(TreeEvent::Navigate(path)) => {
                self.controller.dispatch(AppAction::Navigate(path));
            }
            Some(TreeEvent::Action(action, path)) => self.folder_action(action, &path),
            Some(TreeEvent::Drop(roots, target))
                if self.controller.writable() && self.background_idle() =>
            {
                self.controller.move_into(&roots, &target);
            }
            _ => {}
        }
    }
}

struct TreeContext<'a> {
    s: &'static Strings,
    writable: bool,
    on_disk: bool,
    current: &'a str,
    renaming: Option<String>,
    muted: egui::Color32,
}

struct RenameField<'a> {
    value: &'a mut String,
    focus: &'a mut bool,
    outcome: Option<bool>,
}

fn tree(
    ui: &mut egui::Ui,
    folder: &Folder,
    path: &str,
    context: &TreeContext,
    event: &mut Option<TreeEvent>,
    rename: &mut RenameField,
) {
    for (label, id, kid) in folder_children(folder, path) {
        let open_by_default = context.current.starts_with(&id);
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            ui.make_persistent_id(("tree", &id)),
            open_by_default,
        );
        let row = |ui: &mut egui::Ui, event: &mut Option<TreeEvent>, rename: &mut RenameField| {
            if context.renaming.as_deref() == Some(id.as_str()) {
                let response =
                    ui.add(egui::TextEdit::singleline(rename.value).desired_width(f32::INFINITY));
                if *rename.focus {
                    response.request_focus();
                    *rename.focus = false;
                }
                if response.lost_focus() {
                    let entered = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    rename.outcome = Some(entered);
                }
                return;
            }
            let selected = context.current == id;
            let glyph = if selected {
                icon::FOLDER_OPEN
            } else {
                icon::FOLDER
            };
            let response = nav_item(ui, glyph, &label, selected, false);
            if response.clicked() {
                *event = Some(TreeEvent::Navigate(id.clone()));
            }
            if let Some(payload) = response.dnd_release_payload::<DraggedRows>() {
                *event = Some(TreeEvent::Drop(payload.0.clone(), id.clone()));
            }
            response.context_menu(|ui| {
                let mut first = true;
                for action in RowAction::ALL {
                    if !action.offered()
                        || !action.shown(context.on_disk)
                        || matches!(action, RowAction::View | RowAction::Pin)
                        || (action.writable_only() && !context.writable)
                    {
                        continue;
                    }
                    if action.starts_group() && !first {
                        ui.separator();
                    }
                    first = false;
                    let (text, keys) = action.label(context.s);
                    if ui
                        .add(
                            egui::Button::new(text)
                                .shortcut_text(RichText::new(keys).color(context.muted)),
                        )
                        .clicked()
                    {
                        *event = Some(TreeEvent::Action(action, id.clone()));
                    }
                }
            });
        };
        if kid.kids.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(ui.spacing().indent);
                row(ui, event, rename);
            });
        } else {
            state
                .show_header(ui, |ui| row(ui, event, rename))
                .body(|ui| tree(ui, kid, &id, context, event, rename));
        }
    }
}
