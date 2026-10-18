mod actions;
mod browser;
mod dialogs;
mod header;
mod highlight;
mod jobs;
mod layout;
mod pathbar;
mod preview;
mod search;
mod sidebar;
#[cfg(test)]
mod tests;
mod theme;
mod themes;
mod widgets;

use crate::controller::disk::Volume;
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
    OutputDir(Option<PathBuf>),
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

/// A drop that is waiting for the user to say move or copy.
pub(crate) struct PendingDrop {
    pub(crate) roots: Vec<String>,
    pub(crate) target: String,
    pub(crate) at: egui::Pos2,
}

fn look_of(s: &Settings) -> String {
    format!(
        "{:?}|{}|{}|{:?}|{}|{}",
        s.theme, s.dark_theme, s.light_theme, s.font, s.text_size, s.motion
    )
}

/// A rectangle being dragged out over the rows from the blank space, which
/// picks every row it crosses. `base` is what was checked when it started,
/// kept when Ctrl is held so the sweep adds to it.
pub(crate) struct Marquee {
    from: egui::Pos2,
    to: egui::Pos2,
    pane: Option<usize>,
    base: Vec<bool>,
}

impl Marquee {
    pub(crate) fn rect(&self) -> egui::Rect {
        egui::Rect::from_two_pos(self.from, self.to)
    }
}

/// What the Properties dialog says about a path on the disk.
pub(crate) struct Props {
    pub(crate) name: String,
    pub(crate) path: std::path::PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) files: usize,
    pub(crate) folders: usize,
    /// The walk stopped early, so size and counts are lower bounds.
    pub(crate) capped: bool,
    pub(crate) modified: Option<std::time::SystemTime>,
    pub(crate) hidden: bool,
    pub(crate) pinned: bool,
    /// Unix mode bits and the owner's names, where there are such things.
    pub(crate) mode: Option<u32>,
    pub(crate) owner: String,
    pub(crate) group: String,
}

const PROPS_WALK_MOST: usize = 50_000;

/// Sizes up a path without going on for ever: deep trees are cut off at
/// `PROPS_WALK_MOST` entries and said to be at least that big.
pub(crate) fn inspect(path: &std::path::Path, pinned: bool) -> Props {
    let meta = std::fs::symlink_metadata(path).ok();
    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let mut props = Props {
        name,
        path: path.to_path_buf(),
        is_dir,
        size: meta
            .as_ref()
            .map_or(0, |m| if m.is_dir() { 0 } else { m.len() }),
        files: 0,
        folders: 0,
        capped: false,
        modified: meta.as_ref().and_then(|m| m.modified().ok()),
        hidden: path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.')),
        pinned,
        mode: None,
        owner: String::new(),
        group: String::new(),
    };
    #[cfg(unix)]
    if let Some(meta) = &meta {
        use std::os::unix::fs::MetadataExt;
        props.mode = Some(meta.mode() & 0o777);
        props.owner = unix_name("/etc/passwd", meta.uid());
        props.group = unix_name("/etc/group", meta.gid());
    }
    if is_dir {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                if props.files + props.folders >= PROPS_WALK_MOST {
                    props.capped = true;
                    return props;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    props.folders += 1;
                    stack.push(entry.path());
                } else {
                    props.files += 1;
                    props.size += entry.metadata().map_or(0, |m| m.len());
                }
            }
        }
    }
    props
}

/// The name for an id in a passwd/group style file, or the number itself.
#[cfg(unix)]
fn unix_name(file: &str, id: u32) -> String {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut parts = line.split(':');
                let name = parts.next()?;
                let found: u32 = parts.nth(1)?.parse().ok()?;
                (found == id).then(|| name.to_string())
            })
        })
        .unwrap_or_else(|| id.to_string())
}

pub(crate) struct Shell {
    controller: AppController,
    ctx: egui::Context,
    dialog: Option<Receiver<DialogResult>>,
    filter_value: String,
    focus_filter: bool,
    filter_open: bool,
    marquee: Option<Marquee>,
    pane_action: Option<PaneAction>,
    properties: Option<Props>,
    last_active_pane: Option<usize>,
    // Whether the last thing the user did was press a key. While it was,
    // the pointer resting on another pane does not steal the focus.
    keyboard_drove: bool,
    // The folder row the pointer has rested on, and since when; the peek
    // card comes up once it has rested long enough.
    peek: Option<(String, egui::Rect, f64)>,
    peek_seen: bool,
    peek_loaded: Option<String>,
    // Set when the pointer, not a navigation, made a pane active: the row
    // of panes then stays where it is instead of scrolling to it.
    pointer_focused: bool,
    last_pane_count: usize,
    // The pane whose filter row is open, and what is typed in it.
    pane_filter: Option<(usize, String)>,
    device_label: Option<Volume>,
    device_format: Option<(Volume, usize, String)>,
    // What was cut or copied on the disk, as roots, and whether it was cut.
    disk_clip: Option<(Vec<String>, bool)>,
    // The roots to move into the folder being named, for "New folder with
    // selection".
    new_folder_with: Option<Vec<String>>,
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
    wrap_lines: bool,
    texture: Option<(u64, egui::TextureHandle)>,
    scroll_to_cursor: bool,
    width: f32,
    title: String,
    look: String,
    path_edit: Option<String>,
    path_pick: Option<usize>,
    path_missing: bool,
    focus_path: bool,
    path_grace: u8,
    path_cache: Option<(String, Vec<String>)>,
    themes: Vec<themes::Theme>,
    theme_query: String,
    theme_filter: usize,
    theme_draft: Option<themes::Theme>,
    theme_error: Option<String>,
    carrying: bool,
    /// A drop waiting on the Move / Copy / Cancel menu.
    pending_drop: Option<PendingDrop>,
    /// The blank of a list under carried rows this frame: its folder and rect.
    blank_drop: Option<(String, egui::Rect)>,
    /// A folder row is under the carried rows this frame.
    row_drop_hover: bool,
    /// The new name typed in the conflict box, keyed by the path it is for.
    conflict_name: Option<(String, String)>,
    /// "Apply to all" in the conflict box.
    conflict_all: bool,
    /// The password in the compress box shown in clear.
    add_show_password: bool,
    anchor: Option<(egui::Id, egui::Pos2)>,
    search: Option<search::Search>,
    decorated: bool,
}

impl Shell {
    fn new(ctx: &egui::Context, startup: Startup) -> Self {
        let mut controller = AppController::new(Settings::load());
        let view = controller.state.settings.browser_view;
        let columns = view != crate::settings::BrowserView::Details;
        let flat = controller.state.settings.flat;
        controller.transition_browser_view(columns, flat);
        controller.set_free_panes(!controller.state.settings.next_pane);
        apply_startup(&mut controller, startup);
        let library = themes::all();
        theme::install(ctx, &controller.state.settings, &library);
        let look = look_of(&controller.state.settings);
        let settings_now = controller.state.settings.title_bar;
        egui_extras::install_image_loaders(ctx);
        Self {
            filter_value: controller.state.filter.clone(),
            controller,
            ctx: ctx.clone(),
            dialog: None,
            focus_filter: false,
            filter_open: false,
            marquee: None,
            pane_action: None,
            properties: None,
            last_active_pane: None,
            keyboard_drove: false,
            peek: None,
            peek_seen: false,
            peek_loaded: None,
            pointer_focused: false,
            last_pane_count: 0,
            pane_filter: None,
            device_label: None,
            device_format: None,
            disk_clip: None,
            new_folder_with: None,
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
            wrap_lines: false,
            texture: None,
            scroll_to_cursor: false,
            width: NORMAL_SIZE.0,
            title: String::new(),
            look,
            path_edit: None,
            path_pick: None,
            path_missing: false,
            focus_path: false,
            path_grace: 0,
            path_cache: None,
            themes: library,
            theme_query: String::new(),
            theme_filter: 0,
            theme_draft: None,
            theme_error: None,
            carrying: false,
            pending_drop: None,
            blank_drop: None,
            row_drop_hover: false,
            conflict_name: None,
            conflict_all: false,
            add_show_password: false,
            anchor: None,
            search: None,
            decorated: system_title_bar(&settings_now),
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
        let look = look_of(&self.controller.state.settings);
        if look != self.look {
            theme::install(ctx, &self.controller.state.settings, &self.themes);
            self.look = look;
        }
        let decorated = system_title_bar(&self.controller.state.settings.title_bar);
        if decorated != self.decorated {
            self.decorated = decorated;
            if !cfg!(target_os = "macos") {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(decorated));
            }
        }
    }

    /// Whether Arca is drawing the title bar itself right now, with the
    /// window buttons in its toolbar.
    pub(super) fn own_title_bar(&self) -> bool {
        !self.decorated && !cfg!(target_os = "macos")
    }

    /// Without the system's frame the window has no edges to grab, so the
    /// strip along the border takes that over, and a hairline marks where
    /// the window ends. The pointer is read directly rather than through a
    /// widget: the strip is thinner than egui's hit radius, and whatever
    /// sits inside the edge would win the hit test on the bottom and right.
    fn window_chrome(&self, ctx: &egui::Context) {
        if !self.own_title_bar() {
            return;
        }
        let p = theme::palette_of(ctx);
        let rect = ctx.content_rect();
        let id = egui::Id::new("arca-window-chrome");
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, id))
            .rect_stroke(
                rect.shrink(0.5),
                0,
                egui::Stroke::new(1.0, theme::alpha(p.border, 0.9)),
                egui::StrokeKind::Inside,
            );
        let fixed = ctx.input(|i| {
            i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
        });
        if fixed {
            return;
        }
        let (pos, pressed, down) = ctx.input(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.primary_pressed(),
                i.pointer.any_down(),
            )
        });
        let Some(pos) = pos else {
            return;
        };
        if let Some((direction, cursor)) = resize_edge(rect, pos) {
            if !down || pressed {
                ctx.set_cursor_icon(cursor);
            }
            if pressed {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
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
        } else if self.device_label.is_some() {
            Some(ModalKind::Label)
        } else if self.device_format.is_some() {
            Some(ModalKind::Format)
        } else if self.properties.is_some() {
            Some(ModalKind::Properties)
        } else if state.show_update && state.update.is_some() {
            Some(ModalKind::Update)
        } else if state.show_settings {
            Some(ModalKind::Settings)
        } else if state.show_shortcuts {
            Some(ModalKind::Shortcuts)
        } else if self.search.is_some() {
            Some(ModalKind::Search)
        } else {
            None
        }
    }

    fn cancel_modal(&mut self, kind: ModalKind) {
        match kind {
            ModalKind::Password => self.controller.dispatch(AppAction::CancelPassword),
            ModalKind::Conflict => self.answer_conflict(Answer::Cancel, None),
            ModalKind::Delete => self.controller.dispatch(AppAction::ConfirmDelete(false)),
            ModalKind::Drop => self
                .controller
                .dispatch(AppAction::AnswerDrop(DropChoice::Cancel)),
            ModalKind::Add => self.controller.cancel_compress(),
            ModalKind::NewFolder | ModalKind::Mask => self.close_name(),
            ModalKind::Label => {
                self.device_label = None;
                self.name_value.clear();
            }
            ModalKind::Format => self.device_format = None,
            ModalKind::Settings => self.controller.state.show_settings = false,
            ModalKind::Shortcuts => self.controller.state.show_shortcuts = false,
            ModalKind::Search => self.search = None,
            ModalKind::Properties => self.properties = None,
            ModalKind::Update => self.controller.state.show_update = false,
        }
    }

    fn answer_conflict(&mut self, answer: Answer, name: Option<String>) {
        self.controller
            .dispatch(AppAction::AnswerConflict(answer, name));
    }

    fn background_blocked(&self) -> bool {
        !background_event_allowed(self.modal_kind().is_some(), self.dialog.is_some())
    }

    /// Rows let go over a folder. Ctrl copies and Shift moves whatever the
    /// default would be; otherwise a drop moves within a volume and copies
    /// across one, as the Finder and the Explorer do. With "ask when
    /// dropping" on, a small menu asks instead. Inside an archive there is
    /// only moving.
    pub(crate) fn drop_rows(&mut self, roots: Vec<String>, target: String) {
        if roots.is_empty() || !self.controller.can_drop() || !self.background_idle() {
            return;
        }
        if !self.controller.on_disk() {
            self.controller.move_into(&roots, &target);
            return;
        }
        let (modifiers, at) = self.ctx.input(|i| {
            (
                i.modifiers,
                i.pointer.interact_pos().or_else(|| i.pointer.latest_pos()),
            )
        });
        if modifiers.ctrl {
            self.controller.copy_on_disk(&roots, &target);
        } else if modifiers.shift {
            self.controller.move_into(&roots, &target);
        } else if self.controller.state.settings.ask_on_drop {
            self.pending_drop = Some(PendingDrop {
                roots,
                target,
                at: at.unwrap_or_default(),
            });
        } else if self.controller.same_volume(&roots, &target) {
            self.controller.move_into(&roots, &target);
        } else {
            self.controller.copy_on_disk(&roots, &target);
        }
    }

    /// A drop on the blank of a list goes into the folder it shows, but only
    /// once every row has had its turn: the blank sits behind the rows and
    /// would otherwise take drops meant for a folder row.
    fn settle_blank_drop(&mut self, ctx: &egui::Context) {
        let row_hover = std::mem::take(&mut self.row_drop_hover);
        let Some((target, rect)) = self.blank_drop.take() else {
            return;
        };
        if row_hover {
            return;
        }
        let p = theme::palette_of(ctx);
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("arca-blank-drop"),
        ))
        .rect_stroke(
            rect.shrink(1.0),
            6,
            egui::Stroke::new(1.5, theme::hex(p.focus)),
            egui::StrokeKind::Inside,
        );
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<DraggedRows>(ctx) {
                self.drop_rows(payload.0.clone(), target);
            }
        }
    }

    /// The shadow that follows the pointer while rows are carried: icon,
    /// name (or a count) and what letting go would do.
    fn drag_shadow(&self, ctx: &egui::Context) {
        let Some(payload) = egui::DragAndDrop::payload::<DraggedRows>(ctx) else {
            return;
        };
        let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        let s = self.controller.s();
        let p = theme::palette_of(ctx);
        let first = payload.0.first().cloned().unwrap_or_default();
        let is_dir = first.ends_with('/');
        let leaf = first.trim_end_matches('/');
        let leaf = leaf.rsplit('/').next().unwrap_or(leaf);
        let name = if payload.0.len() > 1 {
            fill(s.items_carried, &[("n", &payload.0.len().to_string())])
        } else {
            leaf.to_string()
        };
        let glyph = if is_dir {
            egui_phosphor::regular::FOLDER
        } else {
            egui_phosphor::regular::FILE
        };
        let copying =
            self.controller.on_disk() && ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift);
        let badge = if copying { s.copy_badge } else { s.move_badge };
        egui::Area::new(egui::Id::new("arca-drag-shadow"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + egui::vec2(14.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::alpha(p.raised, 0.94))
                    .stroke(egui::Stroke::new(1.0, theme::alpha(p.focus, 0.6)))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                theme::icon_text(glyph)
                                    .size(16.0)
                                    .color(theme::hex(p.focus)),
                            );
                            ui.label(egui::RichText::new(name).color(theme::hex(p.text)));
                            ui.label(
                                egui::RichText::new(badge)
                                    .size(11.0)
                                    .color(theme::hex(p.muted)),
                            );
                        });
                    });
            });
    }

    /// The Move here / Copy here / Cancel menu after a drop, when asked for.
    fn drop_menu(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_drop.as_ref() else {
            return;
        };
        let at = pending.at;
        let s = self.controller.s();
        let mut choice: Option<Option<bool>> = None;
        let area = egui::Area::new(egui::Id::new("arca-drop-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    // Justified, as egui's own menus are, so each entry runs
                    // the whole width and the hover and click do too.
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                        let entry = widgets::menu_entry;
                        if entry(
                            ui,
                            egui_phosphor::regular::ARROW_RIGHT,
                            s.move_here,
                            "",
                            true,
                        )
                        .clicked()
                        {
                            choice = Some(Some(false));
                        }
                        if entry(ui, egui_phosphor::regular::COPY, s.copy_here, "", true).clicked()
                        {
                            choice = Some(Some(true));
                        }
                        ui.separator();
                        if entry(ui, egui_phosphor::regular::X, s.cancel, "Esc", true).clicked() {
                            choice = Some(None);
                        }
                    });
                });
            });
        let away = ctx.input(|i| {
            i.key_pressed(egui::Key::Escape)
                || (i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|pos| !area.response.rect.contains(pos)))
        });
        if away {
            choice = Some(None);
        }
        if let Some(choice) = choice {
            let pending = self.pending_drop.take();
            if let (Some(copy), Some(pending)) = (choice, pending) {
                if copy {
                    self.controller
                        .copy_on_disk(&pending.roots, &pending.target);
                } else {
                    self.controller.move_into(&pending.roots, &pending.target);
                }
            }
        }
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
        if self.controller.on_disk() {
            return self.background_idle() && self.selected_count() > 0;
        }
        clipboard_action_allowed(
            clipboard::AVAILABLE,
            self.background_idle(),
            self.controller.state.archive.is_some(),
            self.selected_count(),
            true,
        )
    }

    fn can_paste_files(&self) -> bool {
        if self.controller.on_disk() {
            return self.background_idle() && self.disk_clip.is_some();
        }
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
            (DialogKind::PickInputs { .. } | DialogKind::PickOutputDir, Some(ModalKind::Add)) => {
                true
            }
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
                DialogKind::PickOutputDir => {
                    DialogResult::OutputDir(rfd::FileDialog::new().pick_folder())
                }
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
            DialogResult::OutputDir(Some(dir)) => {
                self.controller.state.output_dir = Some(dir);
            }
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
    }

    fn overflow_action(&mut self, action: OverflowAction) {
        match action {
            OverflowAction::Release => self.controller.state.show_update = true,
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
        if !self.controller.can_change() {
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
            ModalKind::Label => {
                if let Some(volume) = self.device_label.take() {
                    self.controller.label_volume(&volume, &name);
                }
                self.name_value.clear();
            }
            ModalKind::NewFolder => {
                let Some(archive) = self.controller.state.archive.clone() else {
                    let carried = self.new_folder_with.take();
                    self.close_name();
                    if !self.controller.on_disk() {
                        return;
                    }
                    if name.is_empty() || name.contains('/') || name.contains('\\') {
                        self.controller.state.notice = s.bad_name.to_string();
                        self.controller.state.error = true;
                        return;
                    }
                    if let Some(dir) = self.controller.create_disk_folder(&name) {
                        if let Some(roots) = carried.filter(|roots| !roots.is_empty()) {
                            self.controller.move_on_disk(&roots, &dir);
                        }
                    }
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
        if action.writable_only() && !self.controller.can_change() {
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
            RowAction::Trash => {
                let roots = self.controller.selected_roots();
                self.controller.remove_on_disk(&roots, true);
            }
            RowAction::Duplicate => {
                let roots = self.controller.selected_roots();
                self.controller.duplicate_on_disk(&roots);
            }
            RowAction::MoveTo | RowAction::CopyTo => {
                let roots = self.controller.selected_roots();
                if roots.is_empty() {
                    return;
                }
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    let target = crate::controller::disk::disk_dir(&dir);
                    if action == RowAction::MoveTo {
                        self.controller.move_on_disk(&roots, &target);
                    } else {
                        self.controller.copy_on_disk(&roots, &target);
                    }
                }
            }
            RowAction::OpenWith => {
                if let Some(row) = row.filter(|row| !row.is_dir) {
                    self.controller.open_with(&disk_path(&row.path), None);
                }
            }
            RowAction::NewFolderWith => {
                let roots = self.controller.selected_roots();
                if roots.is_empty() {
                    return;
                }
                self.new_folder_with = Some(roots);
                self.overflow_action(OverflowAction::NewFolder);
            }
            RowAction::Properties => {
                let path = match row {
                    Some(row) if !row.up => disk_path(&row.path),
                    _ => disk_path(&self.controller.state.current_dir),
                };
                let pinned = self.controller.is_pinned(&path);
                self.properties = Some(inspect(&path, pinned));
            }
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
            RowAction::ExtractAll => self.begin_dialog(DialogKind::Extract {
                only_checked: false,
            }),
            RowAction::TestArchive => self.shortcut(Shortcut::Test),
            RowAction::OpenInPane => {
                if let Some(row) = row.filter(|row| row.is_dir && !row.up) {
                    self.controller.open_in_pane(&row.path);
                }
            }
            RowAction::Pin => match row {
                Some(row) if row.is_dir && !row.up => {
                    self.controller
                        .dispatch(AppAction::TogglePinned(disk_path(&row.path)));
                }
                Some(_) => {}
                None if self.controller.on_disk() => {
                    let here = disk_path(&self.controller.state.current_dir);
                    self.controller.dispatch(AppAction::TogglePinned(here));
                }
                None => {}
            },
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
        if self.controller.on_disk() {
            let roots = self.controller.selected_roots();
            if !roots.is_empty() {
                self.disk_clip = Some((roots, cut));
            }
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
        if self.controller.on_disk() {
            let Some((roots, cut)) = self.disk_clip.clone() else {
                return;
            };
            let target = self.controller.state.current_dir.clone();
            if cut {
                self.controller.move_on_disk(&roots, &target);
                self.disk_clip = None;
            } else {
                self.controller.copy_on_disk(&roots, &target);
            }
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

    fn set_next_pane(&mut self, on: bool) {
        self.controller.state.settings.next_pane = on;
        self.controller.set_free_panes(!on);
        self.controller.state.settings.save();
    }

    fn switch_view(&mut self, view: crate::settings::BrowserView, flat: bool) {
        use crate::settings::BrowserView;
        let columns = view != BrowserView::Details;
        self.controller.transition_browser_view(columns, flat);
        self.controller
            .set_free_panes(!self.controller.state.settings.next_pane);
        self.controller.state.settings.browser_view =
            if columns { view } else { BrowserView::Details };
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
            self.controller.dispatch(AppAction::SelectOnly(target));
        }
        self.controller
            .set_pane_cursor(self.controller.state.browser.active, Some(index));
        self.release_focus();
    }

    /// Hands the keyboard back to the list, out of whatever field had it.
    /// Nudge the text size by `step` points, or back to the default for 0.
    pub(super) fn step_text_size(&mut self, step: f32) {
        use crate::settings::{TEXT_DEFAULT, TEXT_LEAST, TEXT_MOST};
        let settings = &mut self.controller.state.settings;
        let next = if step == 0.0 {
            TEXT_DEFAULT
        } else {
            (settings.text_size + step).clamp(TEXT_LEAST, TEXT_MOST)
        };
        if next != settings.text_size {
            settings.text_size = next;
            settings.save();
        }
    }

    fn release_focus(&self) {
        self.ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }

    fn input(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.pointer.delta() != egui::Vec2::ZERO) {
            self.keyboard_drove = false;
        }
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
            // Windows keeps the pointer captured while a button is down, so
            // it never "leaves": it just reports positions past the edge.
            let outside = ctx.input(|input| {
                input.pointer.primary_down()
                    && input
                        .pointer
                        .latest_pos()
                        .is_none_or(|pos| !input.content_rect().contains(pos))
            });
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

    /// Tab and Shift+Tab walk the panes; Left and Right step to the one
    /// beside. Only with more than one pane, so that a single list keeps
    /// the keys for whatever else wants them.
    fn pane_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        let browser = &self.controller.state.browser;
        if !browser.columns || browser.panes.len() < 2 || modifiers.command || modifiers.alt {
            return false;
        }
        let count = browser.panes.len();
        let active = browser.active;
        let target = match key {
            egui::Key::Tab if modifiers.shift => Some((active + count - 1) % count),
            egui::Key::Tab => Some((active + 1) % count),
            egui::Key::ArrowLeft => active.checked_sub(1),
            egui::Key::ArrowRight => (active + 1 < count).then_some(active + 1),
            _ => None,
        };
        let Some(target) = target else {
            return false;
        };
        self.controller.activate_pane(target);
        self.scroll_to_cursor = true;
        self.release_focus();
        true
    }

    /// Whether the pointer may move the focus between panes: it is not
    /// pressing, dragging or carrying anything, and no menu, text field or
    /// box is up.
    fn pointer_may_focus(&self) -> bool {
        let ctx = &self.ctx;
        self.controller.state.settings.pointer_focus
            && !self.keyboard_drove
            && !ctx.any_popup_open()
            && !ctx.egui_is_using_pointer()
            && !ctx.input(|i| i.pointer.any_down())
            && !self.typing()
            && self.marquee.is_none()
            && !self.carrying
            && self.controller.state.renaming.is_none()
            && self.modal_kind().is_none()
    }

    fn key_down(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        if !modifiers.command && self.search_key(key, modifiers) {
            return;
        }
        // The palette is a modal, which silences shortcuts below; Ctrl+K has
        // to get through so the same key opens and closes it.
        // A file picker that never shows up must not hold the window
        // hostage; Esc lets go of it and ignores whatever it sends later.
        if key == egui::Key::Escape && self.dialog.is_some() && self.modal_kind().is_none() {
            self.dialog = None;
            return;
        }
        if modifiers.command && key == egui::Key::K && self.search.is_some() {
            self.search = None;
            return;
        }
        if self.background_blocked() {
            return;
        }
        let name = key.name().to_ascii_lowercase();
        let typing = self.typing();
        if !typing {
            self.keyboard_drove = true;
            if self.pane_key(key, modifiers) {
                return;
            }
        }
        if modifiers.command && !modifiers.shift && key == egui::Key::F {
            if self.panes_view() {
                self.open_pane_filter(self.controller.state.browser.active);
            } else {
                self.filter_open = true;
                self.focus_filter = true;
            }
            return;
        }
        if modifiers.command && !typing {
            let step = match key {
                egui::Key::Plus | egui::Key::Equals => Some(1.0),
                egui::Key::Minus => Some(-1.0),
                egui::Key::Num0 => Some(0.0),
                _ => None,
            };
            if let Some(step) = step {
                self.step_text_size(step);
                return;
            }
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
            Shortcut::EditPath => self.start_path_edit(),
            Shortcut::Search => self.toggle_search(),
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
        if key == "delete" && self.background_idle() && self.controller.can_change() {
            if self.controller.on_disk() && !modifiers.shift {
                let roots = self.controller.selected_roots();
                self.controller.remove_on_disk(&roots, true);
            } else {
                self.controller.dispatch(AppAction::RequestDelete);
            }
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
        // After the lists, so every row has had its say about a drop.
        self.settle_blank_drop(&ctx);
        self.drag_shadow(&ctx);
        self.drop_menu(&ctx);
        self.jobs(&ctx);
        self.dialogs(&ctx);
        self.window_chrome(&ctx);
    }
}

/// Which way the window would grow from a pointer at `pos`, if `pos` is on
/// the border of `rect`: corners first, then the four edges.
fn resize_edge(
    rect: egui::Rect,
    pos: egui::Pos2,
) -> Option<(egui::ResizeDirection, egui::CursorIcon)> {
    use egui::{CursorIcon as C, ResizeDirection as D};
    const EDGE: f32 = 6.0;
    const CORNER: f32 = 16.0;
    if !rect.contains(pos) {
        return None;
    }
    let left = pos.x - rect.left();
    let right = rect.right() - pos.x;
    let top = pos.y - rect.top();
    let bottom = rect.bottom() - pos.y;
    let (w, e) = (left < EDGE, right < EDGE);
    let (n, s) = (top < EDGE, bottom < EDGE);
    let (near_w, near_e) = (left < CORNER, right < CORNER);
    let (near_n, near_s) = (top < CORNER, bottom < CORNER);
    Some(match (n, s, w, e) {
        (true, _, _, _) if near_w => (D::NorthWest, C::ResizeNwSe),
        (true, _, _, _) if near_e => (D::NorthEast, C::ResizeNeSw),
        (_, true, _, _) if near_w => (D::SouthWest, C::ResizeNeSw),
        (_, true, _, _) if near_e => (D::SouthEast, C::ResizeNwSe),
        (_, _, true, _) if near_n => (D::NorthWest, C::ResizeNwSe),
        (_, _, true, _) if near_s => (D::SouthWest, C::ResizeNeSw),
        (_, _, _, true) if near_n => (D::NorthEast, C::ResizeNeSw),
        (_, _, _, true) if near_s => (D::SouthEast, C::ResizeNwSe),
        (true, _, _, _) => (D::North, C::ResizeVertical),
        (_, true, _, _) => (D::South, C::ResizeVertical),
        (_, _, true, _) => (D::West, C::ResizeHorizontal),
        (_, _, _, true) => (D::East, C::ResizeHorizontal),
        _ => return None,
    })
}

/// Whether the window system is asked for its own title bar. macOS keeps its
/// frame either way: hiding the strip there means hiding only the strip, and
/// the traffic lights stay where they are.
fn system_title_bar(title_bar: &crate::settings::TitleBar) -> bool {
    *title_bar == crate::settings::TitleBar::System
}

pub(crate) fn run() {
    let startup = crate::parse_args();
    let compact = !matches!(startup, Startup::Browse(_));
    let (width, height) = shell_size(compact);
    let floor = if compact { COMPACT_SIZE } else { MINIMUM_SIZE };
    let icon = eframe::icon_data::from_png_bytes(crate::assets::ICON).ok();
    let decorated = system_title_bar(&Settings::load().title_bar);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Arca")
        .with_app_id("arca")
        .with_inner_size([width, height])
        .with_min_inner_size([floor.0, floor.1])
        .with_drag_and_drop(true);
    if cfg!(target_os = "macos") {
        viewport = viewport
            .with_titlebar_shown(decorated)
            .with_title_shown(decorated)
            .with_fullsize_content_view(!decorated);
    } else {
        viewport = viewport.with_decorations(decorated);
    }
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
