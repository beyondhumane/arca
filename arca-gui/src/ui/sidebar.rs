use super::widgets::*;
use super::*;
use crate::controller::disk::Volume;
use crate::tree::Folder;
use eframe::egui;
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
        PlaceKind::Removable => icon::USB,
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
            .then(|| disk_path(&self.controller.state.active_pane().directory));
        let mut browse = None;
        let mut unpin = None;
        let mut open = None;
        let mut new_pane: Option<PathBuf> = None;
        let mut copy: Option<String> = None;
        let mut props: Option<PathBuf> = None;
        let mut forget: Option<String> = None;
        let mut mount: Option<Volume> = None;
        let mut unmount: Option<Volume> = None;
        let mut relabel: Option<Volume> = None;
        let mut format: Option<Volume> = None;
        let free = self.controller.state.browser.free;
        self.controller.refresh_volumes();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut first = true;
                for (title, places, pinned) in [
                    (s.places_group, self.controller.places(), false),
                    (s.pinned_group, self.controller.pinned_places(), true),
                    (s.devices_group, self.controller.devices(), false),
                ] {
                    if places.is_empty() {
                        continue;
                    }
                    // The rail has no titles, so a rule stands between the
                    // groups; there is nothing above the first to divide.
                    if !rail {
                        group_label(ui, title);
                    } else if !first {
                        ui.separator();
                    }
                    first = false;
                    for place in places {
                        let selected = here.as_ref() == Some(&place.path);
                        let response =
                            nav_item(ui, place_glyph(place.kind), &place.label, selected, rail);
                        if response.clicked() {
                            browse = Some(place.path.clone());
                        }
                        if let Some(payload) = response.dnd_release_payload::<DraggedRows>() {
                            if self.controller.on_disk() {
                                let target = crate::controller::disk::disk_dir(&place.path);
                                self.drop_rows(payload.0.clone(), target);
                            }
                        }
                        let is_computer = place.kind == PlaceKind::Computer;
                        let is_pinned = pinned || self.controller.is_pinned(&place.path);
                        response.context_menu(|ui| {
                            if menu_entry(ui, icon::ARROW_SQUARE_OUT, s.open_word, "", true)
                                .clicked()
                            {
                                browse = Some(place.path.clone());
                                ui.close();
                            }
                            if free
                                && menu_entry(
                                    ui,
                                    icon::COLUMNS_PLUS_RIGHT,
                                    s.open_in_pane,
                                    "",
                                    true,
                                )
                                .clicked()
                            {
                                new_pane = Some(place.path.clone());
                                ui.close();
                            }
                            ui.separator();
                            if !is_computer {
                                let (glyph, label) = if is_pinned {
                                    (icon::PUSH_PIN_SLASH, s.unpin_word)
                                } else {
                                    (icon::PUSH_PIN, s.pin_word)
                                };
                                if menu_entry(ui, glyph, label, "", true).clicked() {
                                    unpin = Some(place.path.clone());
                                    ui.close();
                                }
                            }
                            if menu_entry(ui, icon::LINK, s.copy_path, "", true).clicked() {
                                copy = Some(place.path.display().to_string());
                                ui.close();
                            }
                            let volume =
                                matches!(place.kind, PlaceKind::Drive | PlaceKind::Removable)
                                    .then(|| self.controller.volume_at(&place.path).cloned())
                                    .flatten()
                                    .filter(|v| !v.device.is_empty());
                            if let Some(volume) = volume {
                                ui.separator();
                                let (glyph, label) = if volume.mount.is_some() {
                                    (icon::EJECT, s.unmount_word)
                                } else {
                                    (icon::PLUG, s.mount_word)
                                };
                                if menu_entry(ui, glyph, label, "", true).clicked() {
                                    if volume.mount.is_some() {
                                        unmount = Some(volume.clone());
                                    } else {
                                        mount = Some(volume.clone());
                                    }
                                    ui.close();
                                }
                                if menu_entry(ui, icon::ERASER, s.format_word, "", true).clicked() {
                                    format = Some(volume.clone());
                                    ui.close();
                                }
                                if menu_entry(ui, icon::TAG, s.set_label, "", true).clicked() {
                                    relabel = Some(volume.clone());
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if menu_entry(ui, icon::INFO, s.properties_word, "", true).clicked() {
                                props = Some(place.path.clone());
                                ui.close();
                            }
                        });
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
                        let item =
                            nav_item(ui, icon::CLOCK_COUNTER_CLOCKWISE, &name, open_now, false)
                                .on_hover_text(path);
                        if item.clicked() {
                            open = Some(PathBuf::from(path));
                        }
                        item.context_menu(|ui| {
                            if menu_entry(ui, icon::ARROW_SQUARE_OUT, s.open_word, "", true)
                                .clicked()
                            {
                                open = Some(PathBuf::from(path));
                                ui.close();
                            }
                            if menu_entry(ui, icon::LINK, s.copy_path, "", true).clicked() {
                                copy = Some(path.clone());
                                ui.close();
                            }
                            if menu_entry(ui, icon::X_CIRCLE, s.remove_recent, "", true).clicked() {
                                forget = Some(path.clone());
                                ui.close();
                            }
                            ui.separator();
                            if menu_entry(ui, icon::INFO, s.properties_word, "", true).clicked() {
                                props = Some(PathBuf::from(path));
                                ui.close();
                            }
                        });
                    }
                }
                if !rail && self.controller.state.archive.is_some() && !self.controller.on_disk() {
                    self.archive_tree(ui);
                }
            });
        if !self.background_idle() {
            return;
        }
        if let Some(volume) = mount {
            browse = self.controller.mount_volume(&volume);
        }
        if let Some(volume) = unmount {
            self.controller.unmount_volume(&volume);
        }
        if let Some(volume) = relabel {
            self.name_value = volume.label.clone();
            self.focus_name = true;
            self.device_label = Some(volume);
        }
        if let Some(volume) = format {
            self.device_format = Some((volume, 0, String::new()));
        }
        if let Some(path) = browse {
            // A click on a volume that is not mounted yet mounts it first.
            let unmounted = self
                .controller
                .volume_at(&path)
                .filter(|v| v.mount.is_none() && !v.device.is_empty())
                .cloned();
            let path = match unmounted {
                Some(volume) => self.controller.mount_volume(&volume),
                None => Some(path),
            };
            if let Some(path) = path {
                self.controller.dispatch(AppAction::Browse(path));
            }
        }
        if let Some(path) = unpin {
            self.controller.dispatch(AppAction::TogglePinned(path));
        }
        if let Some(path) = open {
            self.controller.dispatch(AppAction::Open(path));
        }
        if let Some(path) = new_pane {
            let dir = crate::controller::disk::disk_dir(&path);
            self.controller.open_in_pane(&dir);
        }
        if let Some(text) = copy {
            self.copy_text(vec![text]);
        }
        if let Some(path) = props {
            let pinned = self.controller.is_pinned(&path);
            self.properties = Some(super::inspect(&path, pinned));
        }
        if let Some(path) = forget {
            self.controller
                .update_settings(|settings| settings.recent.retain(|p| *p != path));
        }
    }

    fn archive_tree(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        group_label(ui, s.archive_folders);
        let root = self
            .controller
            .state
            .archive
            .as_ref()
            .and_then(|archive| archive.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let current = self.controller.state.active_pane().directory.clone();
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
            Some(TreeEvent::Drop(roots, target)) => self.drop_rows(roots, target),
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
                    ui.add(super::widgets::field(ui, rename.value).desired_width(f32::INFINITY));
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
                    if !action.offered(context.on_disk)
                        || !action.shown(context.on_disk)
                        || matches!(
                            action,
                            RowAction::View
                                | RowAction::OpenInPane
                                | RowAction::Pin
                                | RowAction::OpenWith
                                | RowAction::ExtractAll
                                | RowAction::TestArchive
                        )
                        || (action.writable_only() && !context.writable)
                    {
                        continue;
                    }
                    if action.starts_group() && !first {
                        ui.separator();
                    }
                    first = false;
                    let (text, keys) = action.label(context.s);
                    let keys = super::dialogs::shortcut_label(keys, context.s);
                    if menu_entry(ui, action.glyph(), text, &keys, true).clicked() {
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
