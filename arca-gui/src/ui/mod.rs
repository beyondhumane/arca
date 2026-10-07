mod actions;
mod browser;
mod dialogs;
mod header;
mod jobs;
mod layout;
mod preview;
mod sidebar;
#[cfg(test)]
mod tests;
mod theme;
mod widgets;

use crate::tree::children_of;
use crate::*;
use actions::*;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const COMPACT_SIZE: (f32, f32) = (660.0, 380.0);
const NORMAL_SIZE: (f32, f32) = (1000.0, 660.0);
// Below this the header, the list and a dialog can no longer share a window.
const MINIMUM_SIZE: (f32, f32) = (800.0, 320.0);
const RECENT_MAX: usize = 10;
// How often to look at work that runs on other threads while it runs. Nothing
// is repainted on a clock once the window has nothing left to show.
const TICK: Duration = Duration::from_millis(100);

enum DialogResult {
    Open(Option<PathBuf>),
    Inputs(Option<Vec<PathBuf>>),
    Extract {
        only_checked: bool,
        destination: Option<PathBuf>,
    },
    AddFiles(Option<Vec<PathBuf>>),
    SaveCopy(Option<PathBuf>),
}

/// Rows carried by a drag inside the window, as the roots they stand for.
#[derive(Clone)]
pub(crate) struct DraggedRows(pub Vec<String>);

pub(crate) struct Shell {
    controller: AppController,
    ctx: egui::Context,
    dialog: Option<Receiver<DialogResult>>,
    filter_value: String,
    focus_filter: bool,
    name_value: String,
    focus_name: bool,
    rename_value: String,
    focus_rename: bool,
    renaming_in_tree: bool,
    settings_section: SettingsSection,
    settings_query: String,
    jobs_open: bool,
    jobs_seen: TaskId,
    preview_password: String,
    texture: Option<(u64, egui::TextureHandle)>,
    scroll_to_cursor: bool,
    width: f32,
    title: String,
    theme: ThemePreference,
    carrying: bool,
    anchor: Option<(egui::Id, egui::Pos2)>,
}

impl Shell {
    fn new(ctx: &egui::Context, startup: Startup) -> Self {
        let mut controller = AppController::new(Settings::load());
        let columns =
            controller.state.settings.browser_view == crate::settings::BrowserView::Columns;
        let flat = controller.state.settings.flat;
        controller.transition_browser_view(columns, flat);
        apply_startup(&mut controller, startup);
        let theme = controller.state.settings.theme;
        theme::install(ctx, theme);
        egui_extras::install_image_loaders(ctx);
        Self {
            filter_value: controller.state.filter.clone(),
            controller,
            ctx: ctx.clone(),
            dialog: None,
            focus_filter: false,
            name_value: String::new(),
            focus_name: false,
            rename_value: String::new(),
            focus_rename: false,
            renaming_in_tree: false,
            settings_section: SettingsSection::General,
            settings_query: String::new(),
            jobs_open: false,
            jobs_seen: 0,
            preview_password: String::new(),
            texture: None,
            scroll_to_cursor: false,
            width: NORMAL_SIZE.0,
            title: String::new(),
            theme,
            carrying: false,
            anchor: None,
        }
    }

    /// Everything that happened on other threads since the last frame.
    fn poll(&mut self, ctx: &egui::Context) {
        self.poll_dialog();
        self.controller.ask_about_updates();
        let conflict_was_open = self.controller.conflict().is_some();
        if self.controller.receive() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !conflict_was_open && self.controller.conflict().is_some() {
            ctx.memory_mut(|memory| {
                if let Some(id) = memory.focused() {
                    memory.surrender_focus(id);
                }
            });
        }
        if self.controller.state.cut_pending.is_some() {
            self.controller.cut_landed();
        }
        self.notice_new_tasks();
        let state = &self.controller.state;
        let waiting = self.dialog.is_some()
            || state.busy
            || state.listing
            || state.channel.is_some()
            || state.update_rx.is_some()
            || state.cut_pending.is_some()
            || matches!(state.preview.status, PreviewStatus::Loading)
            || state
                .tasks
                .iter()
                .any(|task| matches!(task.status, TaskStatus::Queued | TaskStatus::Running));
        if waiting {
            ctx.request_repaint_after(TICK);
        }
    }

    fn sync_window(&mut self, ctx: &egui::Context) {
        let title = if self.controller.state.window_title.is_empty() {
            "Arca".to_string()
        } else {
            self.controller.state.window_title.clone()
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        let theme = self.controller.state.settings.theme;
        if theme != self.theme {
            theme::apply(ctx, theme);
            self.theme = theme;
        }
    }

    fn notice_new_tasks(&mut self) {
        let newest = self
            .controller
            .state
            .tasks
            .iter()
            .filter(|t| !t.quiet)
            .map(|t| t.id)
            .max()
            .unwrap_or(0);
        if newest > self.jobs_seen {
            self.jobs_open = true;
        }
        self.jobs_seen = self.jobs_seen.max(newest);
    }

    pub(crate) fn shown_tasks(&self) -> Vec<&Task> {
        self.controller
            .state
            .tasks
            .iter()
            .filter(|t| !t.quiet || t.status == TaskStatus::Failed)
            .collect()
    }

    fn modal_kind(&self) -> Option<ModalKind> {
        let state = &self.controller.state;
        if state.waiting_on_password.is_some() {
            Some(ModalKind::Password)
        } else if self.controller.conflict().is_some() {
            Some(ModalKind::Conflict)
        } else if state.confirm_delete.is_some() {
            Some(ModalKind::Delete)
        } else if state.confirm_drop.is_some() {
            Some(ModalKind::Drop)
        } else if matches!(state.view, View::Add) {
            Some(ModalKind::Add)
        } else if state.asking_folder {
            Some(ModalKind::NewFolder)
        } else if state.picking_group.is_some() {
            Some(ModalKind::Mask)
        } else if state.show_settings {
            Some(ModalKind::Settings)
        } else if state.show_shortcuts {
            Some(ModalKind::Shortcuts)
        } else {
            None
        }
    }

    fn cancel_modal(&mut self, kind: ModalKind) {
        match kind {
            ModalKind::Password => self.controller.dispatch(AppAction::CancelPassword),
            ModalKind::Conflict => self.answer_conflict(Answer::Cancel),
            ModalKind::Delete => self.controller.dispatch(AppAction::ConfirmDelete(false)),
            ModalKind::Drop => self
                .controller
                .dispatch(AppAction::AnswerDrop(DropChoice::Cancel)),
            ModalKind::Add => self.controller.cancel_compress(),
            ModalKind::NewFolder | ModalKind::Mask => self.close_name(),
            ModalKind::Settings => self.controller.state.show_settings = false,
            ModalKind::Shortcuts => self.controller.state.show_shortcuts = false,
        }
    }

    fn answer_conflict(&mut self, answer: Answer) {
        self.controller.dispatch(AppAction::AnswerConflict(answer));
    }

    fn background_blocked(&self) -> bool {
        !background_event_allowed(self.modal_kind().is_some(), self.dialog.is_some())
    }

    fn background_idle(&self) -> bool {
        !self.controller.blocked() && self.modal_kind().is_none() && self.dialog.is_none()
    }

    fn accepts_drop(&self) -> bool {
        if self.controller.blocked() || self.dialog.is_some() {
            return false;
        }
        matches!(self.modal_kind(), None | Some(ModalKind::Add))
    }

    fn selected_count(&self) -> usize {
        self.controller
            .state
            .checked
            .iter()
            .filter(|checked| **checked)
            .count()
    }

    fn can_copy_files(&self) -> bool {
        clipboard_action_allowed(
            clipboard::AVAILABLE,
            self.background_idle(),
            self.controller.state.archive.is_some(),
            self.selected_count(),
            true,
        )
    }

    fn can_paste_files(&self) -> bool {
        clipboard_action_allowed(
            clipboard::AVAILABLE,
            self.background_idle(),
            self.controller.state.archive.is_some(),
            self.selected_count(),
            false,
        )
    }

    fn typing(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    fn begin_dialog(&mut self, kind: DialogKind) {
        let modal_allows = match (&kind, self.modal_kind()) {
            (DialogKind::PickInputs { .. }, Some(ModalKind::Add)) => true,
            (_, modal) => modal.is_none(),
        };
        if self.dialog.is_some() || self.controller.blocked() || !modal_allows {
            return;
        }
        if matches!(kind, DialogKind::Extract { .. })
            && (self.controller.state.archive.is_none()
                || (matches!(kind, DialogKind::Extract { only_checked: true })
                    && !self.controller.state.checked.iter().any(|checked| *checked)))
        {
            return;
        }
        let (tx, rx) = channel();
        self.dialog = Some(rx);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = match kind {
                DialogKind::Open => {
                    let dialog =
                        rfd::FileDialog::new().add_filter("Archives", &crate::open_filter());
                    let dialog = if cfg!(feature = "rar") {
                        dialog.add_filter("All files (including RAR volumes)", &["*"])
                    } else {
                        dialog
                    };
                    DialogResult::Open(dialog.pick_file())
                }
                DialogKind::PickInputs { folders } => DialogResult::Inputs(if folders {
                    rfd::FileDialog::new().pick_folders()
                } else {
                    rfd::FileDialog::new().pick_files()
                }),
                DialogKind::Extract { only_checked } => DialogResult::Extract {
                    only_checked,
                    destination: rfd::FileDialog::new().pick_folder(),
                },
                DialogKind::AddFiles { folders } => DialogResult::AddFiles(if folders {
                    rfd::FileDialog::new().pick_folders()
                } else {
                    rfd::FileDialog::new().pick_files()
                }),
                DialogKind::SaveCopy { name, directory } => DialogResult::SaveCopy(
                    rfd::FileDialog::new()
                        .set_file_name(&name)
                        .set_directory(&directory)
                        .save_file(),
                ),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_dialog(&mut self) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let result = match dialog.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.dialog = None;
                return;
            }
        };
        self.dialog = None;
        match result {
            DialogResult::Open(Some(path)) => self.controller.dispatch(AppAction::Open(path)),
            DialogResult::Inputs(Some(paths)) if !paths.is_empty() => {
                self.controller.dispatch(AppAction::PrepareCompress(paths))
            }
            DialogResult::Extract {
                only_checked,
                destination: Some(dest),
            } => self
                .controller
                .dispatch(AppAction::ExtractTo { only_checked, dest }),
            DialogResult::AddFiles(Some(paths)) if !paths.is_empty() => {
                self.controller.dispatch(AppAction::Add(paths))
            }
            DialogResult::SaveCopy(Some(dest)) => {
                if let Some(archive) = self.controller.state.archive.clone() {
                    if dest != archive {
                        self.controller
                            .dispatch(AppAction::Run(Job::CopyTo { archive, dest }));
                    }
                }
            }
            _ => {}
        }
    }

    fn set_theme(&mut self, theme: ThemePreference) {
        self.controller.dispatch(AppAction::SetTheme(theme));
        theme::apply(&self.ctx, theme);
        self.theme = theme;
    }

    fn overflow_action(&mut self, action: OverflowAction) {
        match action {
            OverflowAction::Release => self.controller.start_update(),
            OverflowAction::AddFiles => self.begin_dialog(DialogKind::AddFiles { folders: false }),
            OverflowAction::AddFolders => self.begin_dialog(DialogKind::AddFiles { folders: true }),
            OverflowAction::NewFolder => {
                self.name_value.clear();
                self.focus_name = true;
                self.controller.state.asking_folder = true;
            }
            OverflowAction::Undo => self.controller.undo_last(),
            OverflowAction::SaveCopy => {
                let Some(archive) = self.controller.state.archive.clone() else {
                    return;
                };
                let name = archive
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let directory = archive
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                self.begin_dialog(DialogKind::SaveCopy { name, directory });
            }
        }
    }

    fn close_name(&mut self) {
        self.controller.state.asking_folder = false;
        self.controller.state.renaming = None;
        self.controller.state.picking_group = None;
        self.name_value.clear();
    }

    fn begin_rename(&mut self, path: &str, in_tree: bool) {
        if !self.controller.writable() {
            return;
        }
        let trimmed = path.trim_end_matches('/');
        let label = trimmed.rsplit('/').next().unwrap_or(trimmed).to_string();
        self.renaming_in_tree = in_tree;
        self.rename_value = label.clone();
        self.focus_rename = true;
        self.controller.state.renaming = Some((path.to_string(), label));
    }

    fn commit_rename(&mut self) {
        let Some((path, _)) = self.controller.state.renaming.clone() else {
            return;
        };
        let name = self.rename_value.trim().to_string();
        let rows = children_of(&self.controller.state.entries, &parent_of(&path));
        self.controller.state.renaming = None;
        self.renaming_in_tree = false;
        self.controller.rename_to(&rows, &path, &name);
    }

    fn cancel_rename(&mut self) {
        self.renaming_in_tree = false;
        self.controller.state.renaming = None;
    }

    fn confirm_name(&mut self, kind: ModalKind) {
        let s = self.controller.s();
        let name = self.name_value.trim().to_string();
        match kind {
            ModalKind::NewFolder => {
                let Some(archive) = self.controller.state.archive.clone() else {
                    self.close_name();
                    return;
                };
                if name.is_empty() || name.contains('/') || name.contains('\\') {
                    self.controller.state.notice = s.bad_name.to_string();
                    self.controller.state.error = true;
                    self.close_name();
                    return;
                }
                if self
                    .controller
                    .visible_rows()
                    .iter()
                    .any(|row| row.label.eq_ignore_ascii_case(&name))
                {
                    self.controller.state.notice = fill(s.name_taken, &[("name", &name)]);
                    self.controller.state.error = true;
                    self.close_name();
                    return;
                }
                let full = format!("{}{name}/", self.controller.state.current_dir);
                self.close_name();
                self.controller.dispatch(AppAction::Run(Job::NewFolder {
                    archive,
                    name: full,
                    password: self.controller.state.archive_password.clone(),
                }));
            }
            ModalKind::Mask => {
                let adding = self.controller.state.picking_group.unwrap_or(true);
                let rows = self.controller.visible_rows();
                self.close_name();
                if name.is_empty() {
                    return;
                }
                for row in &rows {
                    if matches_mask(&name, &row.label) {
                        self.controller.dispatch(AppAction::SetChecked {
                            row: row.clone(),
                            value: adding,
                        });
                    }
                }
            }
            _ => self.close_name(),
        }
    }

    fn start_add(&mut self) {
        if let Some(job) = self.controller.compression_job() {
            self.controller.dispatch(AppAction::Run(job));
        }
    }

    fn copy_text(&self, lines: Vec<String>) {
        if !lines.is_empty() {
            self.ctx.copy_text(lines.join("\r\n"));
        }
    }

    fn row_action(&mut self, action: RowAction, index: usize) {
        if !self.background_idle() {
            return;
        }
        if action.writable_only() && !self.controller.writable() {
            return;
        }
        if !action.shown(self.controller.on_disk()) {
            return;
        }
        let rows = self.controller.visible_rows();
        let row = rows.get(index).cloned();
        if row.as_ref().is_some_and(|row| row.up) && !action.about_the_place() {
            return;
        }
        if let Some(destination) =
            action.destination(row.as_ref(), &self.controller.state.current_dir)
        {
            if destination != self.controller.state.current_dir {
                self.controller.go_to(destination);
            }
        }
        match action {
            RowAction::Open => {
                if let Some(row) = row {
                    self.activate(&row);
                }
            }
            RowAction::ExtractSelection => {
                self.begin_dialog(DialogKind::Extract { only_checked: true })
            }
            RowAction::ExtractHere => self.controller.extract_here(),
            RowAction::View => {
                if let Some(entry) = row.as_ref().and_then(|row| row.entry) {
                    self.show_preview(entry);
                }
            }
            RowAction::TestSelection => {
                let names = self.controller.selected_names();
                if let Some(archive) = self.controller.state.archive.clone() {
                    self.controller.dispatch(AppAction::Run(Job::Test {
                        archive,
                        only: (!names.is_empty()).then(|| names.into_iter().collect()),
                        password: None,
                    }));
                }
            }
            RowAction::Rename => {
                if let Some(row) = row {
                    self.begin_rename(&row.path, false);
                }
            }
            RowAction::Delete => self.controller.dispatch(AppAction::RequestDelete),
            RowAction::Copy => self.dispatch_clipboard(false),
            RowAction::Cut => self.dispatch_clipboard(true),
            RowAction::Paste => self.dispatch_paste(),
            RowAction::CopyNames => self.copy_text(self.controller.selected_names()),
            RowAction::SelectAll => self.controller.dispatch(AppAction::SelectAllVisible),
            RowAction::NewFolder => self.overflow_action(OverflowAction::NewFolder),
            RowAction::Compress => {
                let inputs = self.controller.selected_disk_paths();
                self.controller.dispatch(AppAction::PrepareCompress(inputs));
            }
            RowAction::Pin => {
                if let Some(row) = row.filter(|row| row.is_dir && !row.up) {
                    self.controller
                        .dispatch(AppAction::TogglePinned(disk_path(&row.path)));
                }
            }
            RowAction::CopyPath => {
                let paths = self
                    .controller
                    .selected_disk_paths()
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                self.copy_text(paths);
            }
        }
    }

    fn folder_action(&mut self, action: RowAction, path: &str) {
        if !self.background_idle() {
            return;
        }
        if action == RowAction::Rename {
            self.begin_rename(path, true);
            return;
        }
        if action.about_the_place() {
            self.controller
                .dispatch(AppAction::Navigate(path.to_string()));
        } else {
            self.controller.pick_folder(path);
        }
        self.row_action(action, usize::MAX);
    }

    /// Goes into a folder, or opens a file with what the system opens it with.
    fn activate(&mut self, row: &Row) {
        if row.is_dir {
            self.controller
                .dispatch(AppAction::Navigate(row.path.clone()));
        } else if let Some(entry) = row.entry {
            self.controller.dispatch(AppAction::OpenFile(entry));
        }
    }

    fn drop_external(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() || !self.accepts_drop() {
            return;
        }
        if matches!(self.controller.state.view, View::Add) {
            self.controller.dispatch(AppAction::PrepareCompress(paths));
        } else {
            self.controller.dispatch(AppAction::Drop(paths));
        }
    }

    fn dispatch_clipboard(&mut self, cut: bool) {
        if self.typing() || !self.background_idle() {
            return;
        }
        if self.controller.state.archive.is_none() || self.selected_count() == 0 {
            return;
        }
        if !clipboard::AVAILABLE {
            self.controller.state.notice =
                "File clipboard integration is available on Windows only.".into();
            self.controller.state.error = true;
            return;
        }
        self.controller.state.notice = if cut {
            "Preparing selected files to move…".into()
        } else {
            "Preparing selected files to copy…".into()
        };
        self.controller.state.error = false;
        self.controller.dispatch(AppAction::Copy { cut });
    }

    fn dispatch_paste(&mut self) {
        if self.typing() || !self.background_idle() {
            return;
        }
        if self.controller.state.archive.is_none() {
            return;
        }
        if !clipboard::AVAILABLE {
            self.controller.state.notice =
                "File clipboard integration is available on Windows only.".into();
            self.controller.state.error = true;
            return;
        }
        self.controller.dispatch(AppAction::Paste);
    }

    fn switch_view(&mut self, columns: bool, flat: bool) {
        self.controller.transition_browser_view(columns, flat);
        self.controller.state.settings.browser_view = if columns {
            crate::settings::BrowserView::Columns
        } else {
            crate::settings::BrowserView::Details
        };
        if flat {
            self.controller
                .state
                .settings
                .columns
                .set(SortColumn::Path, true);
        }
        self.controller.state.settings.save();
        self.scroll_to_cursor = true;
    }

    fn show_preview(&mut self, entry: usize) {
        self.controller.state.settings.preview_visible = true;
        self.controller.state.settings.save();
        self.controller.view_entry(entry);
    }

    fn close_preview(&mut self) {
        self.controller.cancel_preview();
        self.texture = None;
    }

    fn select_row(&mut self, index: usize, modifiers: egui::Modifiers) {
        if self.background_blocked() {
            return;
        }
        let rows = self.controller.visible_rows();
        let Some(target) = rows.get(index).cloned() else {
            return;
        };
        if modifiers.shift {
            let from = self.controller.state.cursor.unwrap_or(index);
            let (lo, hi) = if from <= index {
                (from, index)
            } else {
                (index, from)
            };
            for row in &rows[lo..=hi.min(rows.len() - 1)] {
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: true,
                });
            }
        } else if modifiers.command {
            let value = !self.controller.is_checked(&target);
            self.controller
                .dispatch(AppAction::SetChecked { row: target, value });
        } else {
            self.controller.state.checked.fill(false);
            self.controller.dispatch(AppAction::SetChecked {
                row: target,
                value: true,
            });
        }
        self.controller
            .set_pane_cursor(self.controller.state.browser.active, Some(index));
        self.release_focus();
    }

    /// Hands the keyboard back to the list, out of whatever field had it.
    fn release_focus(&self) {
        self.ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }

    fn input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => self.key_down(key, modifiers),
                egui::Event::Copy if !self.typing() => {
                    if ctx.input(|input| input.modifiers.shift) {
                        self.shortcut(Shortcut::CopyNames);
                    } else {
                        self.dispatch_clipboard(false);
                    }
                }
                egui::Event::Cut if !self.typing() => self.dispatch_clipboard(true),
                egui::Event::Paste(_) if !self.typing() => self.dispatch_paste(),
                egui::Event::Text(text) if !self.typing() && !self.background_blocked() => {
                    self.jump_to(&text)
                }
                _ => {}
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.drop_external(dropped);
        }
        if self.carrying {
            let outside = ctx
                .input(|input| input.pointer.primary_down() && input.pointer.hover_pos().is_none());
            if outside {
                self.carrying = false;
                egui::DragAndDrop::clear_payload(ctx);
                if self.controller.state.format == Format::SevenZ {
                    self.controller.state.notice = self.controller.s().sevenz_copy.to_string();
                } else {
                    self.controller.drag_out();
                }
            } else if !ctx.input(|input| input.pointer.primary_down()) {
                self.carrying = false;
            }
        }
    }

    /// The first row after the cursor whose name starts with what was typed.
    fn jump_to(&mut self, text: &str) {
        let Some(letter) = text.chars().next().filter(|c| c.is_alphanumeric()) else {
            return;
        };
        let letter = letter.to_lowercase().to_string();
        let rows = self.controller.visible_rows();
        let start = self.controller.state.cursor.map_or(0, |cursor| cursor + 1);
        let found = (0..rows.len())
            .map(|step| (start + step) % rows.len().max(1))
            .find(|index| {
                rows.get(*index)
                    .is_some_and(|row| !row.up && row.label.to_lowercase().starts_with(&letter))
            });
        if let Some(index) = found {
            self.controller
                .set_pane_cursor(self.controller.state.browser.active, Some(index));
            self.scroll_to_cursor = true;
        }
    }

    fn key_down(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        if self.background_blocked() {
            return;
        }
        let name = key.name().to_ascii_lowercase();
        let typing = self.typing();
        if modifiers.command && !modifiers.shift && key == egui::Key::F {
            self.focus_filter = true;
            return;
        }
        if let Some(shortcut) = shortcut_for(
            modifiers.command,
            modifiers.shift,
            modifiers.alt,
            &name,
            typing,
        ) {
            self.shortcut(shortcut);
            return;
        }
        if typing || self.ctx.memory(|memory| memory.focused().is_some()) {
            return;
        }
        self.list_key(&name, modifiers);
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        let archive = self.controller.state.archive.clone();
        if !self.background_idle()
            && !matches!(
                shortcut,
                Shortcut::Shortcuts | Shortcut::Back | Shortcut::Forward | Shortcut::View
            )
        {
            return;
        }
        match shortcut {
            Shortcut::Back => self.controller.dispatch(AppAction::Back),
            Shortcut::Forward => self.controller.dispatch(AppAction::Forward),
            Shortcut::Open => self.begin_dialog(DialogKind::Open),
            Shortcut::Compress => {
                let inputs = self.controller.selected_disk_paths();
                self.controller.dispatch(AppAction::PrepareCompress(inputs));
            }
            Shortcut::ExtractAll if archive.is_some() => self.begin_dialog(DialogKind::Extract {
                only_checked: false,
            }),
            Shortcut::ExtractHere if archive.is_some() => self.controller.extract_here(),
            Shortcut::Test => {
                if let Some(archive) = archive {
                    self.controller.dispatch(AppAction::Run(Job::Test {
                        archive,
                        only: None,
                        password: None,
                    }));
                }
            }
            Shortcut::Refresh => self.controller.dispatch(AppAction::Refresh),
            Shortcut::Invert => self.controller.dispatch(AppAction::InvertVisible),
            Shortcut::ClearSelection => {
                if self.carrying {
                    self.carrying = false;
                    egui::DragAndDrop::clear_payload(&self.ctx);
                } else {
                    self.controller.dispatch(AppAction::ClearSelection);
                }
            }
            Shortcut::CopyNames => self.copy_text(self.controller.selected_names()),
            Shortcut::Shortcuts => {
                self.controller.state.show_shortcuts = !self.controller.state.show_shortcuts;
            }
            Shortcut::Undo if self.controller.state.undo.is_some() => self.controller.undo_last(),
            Shortcut::Rename if archive.is_some() => {
                let rows = self.controller.visible_rows();
                if let Some(row) = self.controller.state.cursor.and_then(|i| rows.get(i)) {
                    let path = row.path.clone();
                    self.begin_rename(&path, false);
                }
            }
            Shortcut::PickGroup(adding) if archive.is_some() => {
                self.name_value.clear();
                self.focus_name = true;
                self.controller.state.picking_group = Some(adding);
            }
            Shortcut::View if archive.is_some() || self.controller.on_disk() => {
                let rows = self.controller.visible_rows();
                if let Some(entry) = self
                    .controller
                    .state
                    .cursor
                    .and_then(|index| rows.get(index))
                    .and_then(|row| row.entry)
                {
                    self.show_preview(entry);
                }
            }
            _ => {}
        }
    }

    fn list_key(&mut self, key: &str, modifiers: egui::Modifiers) {
        if key == "delete" && self.background_idle() && self.controller.writable() {
            self.controller.dispatch(AppAction::RequestDelete);
            return;
        }
        let rows = self.controller.visible_rows();
        let back = matches!(key, "backspace" | "left" | "arrowleft") && !modifiers.alt;
        if rows.is_empty() {
            if back && self.controller.can_go_up() {
                self.controller.dispatch(AppAction::Up);
            }
            return;
        }
        if modifiers.command && key == "a" {
            self.controller.dispatch(AppAction::SelectAllVisible);
            return;
        }
        if key == "space" {
            if let Some(row) = self.controller.state.cursor.and_then(|i| rows.get(i)) {
                let value = !self.controller.is_checked(row);
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value,
                });
            }
            return;
        }
        if matches!(key, "enter" | "right" | "arrowright") && !modifiers.alt {
            if let Some(row) = self.controller.state.cursor.and_then(|i| rows.get(i)) {
                if row.is_dir || key == "enter" {
                    let row = row.clone();
                    self.activate(&row);
                    self.scroll_to_cursor = true;
                }
            }
            return;
        }
        if back {
            if self.controller.can_go_up() {
                self.controller.dispatch(AppAction::Up);
                self.scroll_to_cursor = true;
            }
            return;
        }
        let last = rows.len() - 1;
        let current = self.controller.state.cursor;
        let Some(index) = cursor_step(key, current, last) else {
            return;
        };
        if modifiers.shift {
            let from = current.unwrap_or(index);
            let (lo, hi) = if from <= index {
                (from, index)
            } else {
                (index, from)
            };
            for row in &rows[lo..=hi] {
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: true,
                });
            }
        }
        self.controller
            .set_pane_cursor(self.controller.state.browser.active, Some(index));
        self.scroll_to_cursor = true;
    }

    pub(crate) fn crumbs(&self) -> Vec<(String, String)> {
        let root = if let Some(archive) = &self.controller.state.archive {
            archive
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        } else if self.controller.on_disk() {
            self.controller.s().computer_word.to_string()
        } else {
            return Vec::new();
        };
        let mut crumbs = vec![(root, String::new())];
        let mut walked = String::new();
        for part in self
            .controller
            .state
            .current_dir
            .split('/')
            .filter(|part| !part.is_empty())
        {
            walked.push_str(part);
            walked.push('/');
            crumbs.push((part.to_string(), walked.clone()));
        }
        crumbs
    }

    pub(crate) fn status(&self) -> String {
        let s = self.controller.s();
        let state = &self.controller.state;
        let active: Vec<_> = self
            .controller
            .active_tasks()
            .filter(|t| !t.quiet)
            .collect();
        match active.as_slice() {
            [] => {}
            [task] => {
                let progress = match (task.status, task.total) {
                    (TaskStatus::Queued, _) => s.queued_word.to_string(),
                    (_, 0) => s.working_word.to_string(),
                    (_, total) if task.in_bytes => {
                        format!("{} / {}", human(task.done as u64), human(total as u64))
                    }
                    (_, total) => format!("{} / {total}", task.done),
                };
                return format!("{} · {}", task.verb, progress);
            }
            many => return fill(s.operations_count, &[("n", &many.len().to_string())]),
        }
        if state.busy || (matches!(state.view, View::Running) && state.notice.is_empty()) {
            let progress = if state.total_count == 0 {
                s.working_word.to_string()
            } else {
                format!("{} / {}", state.done_count, state.total_count)
            };
            return format!("{} · {}", state.title, progress);
        }
        if !state.notice.is_empty() {
            return state.notice.clone();
        }
        if matches!(state.view, View::Add) {
            return format!("{}: {}", s.add_to_archive, state.pending_inputs.len());
        }
        if state.entries.is_empty() {
            return s.drop_here.into();
        }
        self.controller.summary()
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.sync_window(&ctx);
        self.input(&ctx);
        self.width = ui.available_width();
        let only_form = self.controller.state.one_shot && self.modal_kind() == Some(ModalKind::Add);
        if !only_form {
            self.workspace(ui);
        }
        self.jobs(&ctx);
        self.dialogs(&ctx);
    }
}

pub(crate) fn run() {
    let startup = crate::parse_args();
    let compact = !matches!(startup, Startup::Browse(_));
    let (width, height) = shell_size(compact);
    let floor = if compact { COMPACT_SIZE } else { MINIMUM_SIZE };
    let icon = eframe::icon_data::from_png_bytes(crate::assets::ICON).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Arca")
        .with_app_id("arca")
        .with_inner_size([width, height])
        .with_min_inner_size([floor.0, floor.1])
        .with_drag_and_drop(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let result = eframe::run_native(
        "Arca",
        options,
        Box::new(move |creation| Ok(Box::new(Shell::new(&creation.egui_ctx, startup)))),
    );
    if let Err(error) = result {
        eprintln!("arca-gui: {error}");
        std::process::exit(1);
    }
}

pub(crate) fn cursor_step(key: &str, current: Option<usize>, last: usize) -> Option<usize> {
    let page = 12;
    match key {
        "down" | "arrowdown" => Some(current.map_or(0, |index| (index + 1).min(last))),
        "up" | "arrowup" => Some(current.map_or(0, |index| index.saturating_sub(1))),
        "pagedown" => Some(current.map_or(0, |index| (index + page).min(last))),
        "pageup" => Some(current.map_or(0, |index| index.saturating_sub(page))),
        "home" => Some(0),
        "end" => Some(last),
        _ => None,
    }
}
