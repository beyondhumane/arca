use super::layout::{breadcrumb_indices, cycle_sidebar, decide};
use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

const FILTER_ID: &str = "arca-filter";

impl Shell {
    pub(super) fn workspace(&mut self, ui: &mut egui::Ui) {
        let p = palette_of(ui.ctx());
        let layout = decide(self.width, &self.controller.state.settings);
        let one_shot = self.controller.state.one_shot;
        let browsing = self.controller.state.archive.is_some() || self.controller.on_disk();
        let bar = egui::Frame::new()
            .fill(hex(p.surface))
            .inner_margin(egui::Margin::symmetric(8, 5));
        let s = self.controller.s();
        egui::Panel::top("arca-header").frame(bar).show(ui, |ui| {
            region(ui, egui::accesskit::Role::Toolbar, s.toolbar_region);
            self.header(ui);
        });
        egui::Panel::bottom("arca-footer")
            .frame(bar.inner_margin(egui::Margin::symmetric(10, 3)))
            .show(ui, |ui| {
                region(ui, egui::accesskit::Role::Status, s.status_region);
                self.footer(ui);
            });
        if layout.sidebar && !one_shot {
            let panel = egui::Panel::left("arca-sidebar").frame(
                egui::Frame::new()
                    .fill(hex(p.surface))
                    .inner_margin(egui::Margin::symmetric(6, 8)),
            );
            let panel = if layout.rail {
                panel.exact_size(44.0).resizable(false)
            } else {
                panel
                    .default_size(self.controller.state.settings.sidebar)
                    .size_range(SIDEBAR_LEAST..=SIDEBAR_MOST)
                    .resizable(true)
            };
            let shown = panel.show(ui, |ui| self.sidebar(ui, layout.rail));
            if !layout.rail {
                let width = shown.response.rect.width();
                self.remember_width(width, |settings| &mut settings.sidebar);
            }
        }
        if layout.preview && browsing && !one_shot {
            self.preview_for_cursor();
            let shown = egui::Panel::right("arca-preview")
                .frame(
                    egui::Frame::new()
                        .fill(hex(p.surface))
                        .inner_margin(egui::Margin::same(10)),
                )
                .default_size(self.controller.state.settings.preview_width)
                .size_range(280.0..=800.0)
                .resizable(true)
                .show(ui, |ui| self.preview_panel(ui));
            let width = shown.response.rect.width();
            self.remember_width(width, |settings| &mut settings.preview_width);
        } else if self.controller.state.preview.index.is_some() {
            self.close_preview();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(hex(p.background)))
            .show(ui, |ui| {
                if one_shot {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(14))
                        .show(ui, |ui| self.jobs_body(ui, true));
                } else {
                    if browsing {
                        region(ui, egui::accesskit::Role::Region, s.archive_contents);
                    }
                    self.browser(ui);
                }
                let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
                if hovering && self.accepts_drop() {
                    let rect = ui.max_rect().shrink(8.0);
                    ui.painter()
                        .rect_filled(rect, 10, theme::alpha(p.focus, 0.12));
                    ui.painter().rect_stroke(
                        rect,
                        10,
                        egui::Stroke::new(2.0, hex(p.focus)),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        self.controller.s().drop_here,
                        theme::display(18.0),
                        hex(p.text),
                    );
                }
            });
    }

    fn remember_width(&mut self, width: f32, slot: impl Fn(&mut Settings) -> &mut f32) {
        let settings = &mut self.controller.state.settings;
        if (*slot(settings) - width).abs() > 0.5 && !self.ctx.input(|i| i.pointer.any_down()) {
            *slot(settings) = width;
            settings.save();
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let idle = self.background_idle();
        let archive = self.controller.state.archive.is_some();
        let wide = self.width >= 1000.0;
        ui.horizontal(|ui| {
            let button = |ui: &mut egui::Ui, glyph: &str, label: &str, enabled: bool| {
                if wide {
                    labelled_button(ui, glyph, label, enabled)
                } else {
                    icon_button(ui, glyph, label, enabled)
                }
            };
            if button(ui, icon::FOLDER_OPEN, s.open, idle).clicked() {
                self.begin_dialog(DialogKind::Open);
            }
            if button(ui, icon::PACKAGE, s.compress, idle).clicked() {
                let inputs = self.controller.selected_disk_paths();
                self.controller.dispatch(AppAction::PrepareCompress(inputs));
            }
            ui.add_enabled_ui(idle && archive, |ui| {
                let title = if wide {
                    format!("{}  {}", icon::EXPORT, s.extract_all)
                } else {
                    icon::EXPORT.to_string()
                };
                ui.menu_button(title, |ui| {
                    if ui.button(s.extract_all).clicked() {
                        self.begin_dialog(DialogKind::Extract {
                            only_checked: false,
                        });
                    }
                    let any = self.selected_count() > 0;
                    let (label, _) = RowAction::ExtractSelection.label(s);
                    if ui.add_enabled(any, egui::Button::new(label)).clicked() {
                        self.begin_dialog(DialogKind::Extract { only_checked: true });
                    }
                    if ui
                        .add(egui::Button::new(s.extract_here).shortcut_text("Alt+W"))
                        .clicked()
                    {
                        self.controller.extract_here();
                    }
                })
                .response
                .on_hover_text(s.extract_all);
            });
            if button(ui, icon::CHECK_CIRCLE, s.test_word, idle && archive).clicked() {
                self.shortcut(Shortcut::Test);
            }
            if let Some(release) = &self.controller.state.update {
                let label = fill(s.update_ready, &[("version", &release.tag)]);
                if ui
                    .add(egui::Button::new(
                        RichText::new(format!("{}  {label}", icon::DOWNLOAD_SIMPLE))
                            .color(hex(palette_of(ui.ctx()).success)),
                    ))
                    .clicked()
                {
                    self.controller.start_update();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.overflow_menu(ui);
                if icon_button(ui, icon::GEAR_SIX, s.settings, idle).clicked() {
                    self.controller.state.show_settings = true;
                }
                let preview = self.controller.state.settings.preview_visible;
                if icon_toggle(ui, icon::EYE, s.view_word, true, preview).clicked() {
                    self.controller.state.settings.preview_visible = !preview;
                    self.controller.state.settings.save();
                }
                ui.separator();
                let columns = self.controller.state.browser.columns;
                let flat = self.controller.state.settings.flat;
                for (glyph, label, want_columns, want_flat) in [
                    (icon::ROWS, s.flat_view, false, true),
                    (icon::LIST, s.details_view, false, false),
                    (icon::COLUMNS, s.column_view, true, false),
                ] {
                    let on = columns == want_columns && (want_columns || flat == want_flat);
                    if icon_toggle(ui, glyph, label, idle, on).clicked() && !on {
                        self.switch_view(want_columns, want_flat);
                    }
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let folders = self.controller.state.settings.folders;
            if icon_toggle(ui, icon::SIDEBAR_SIMPLE, s.folder_tree, true, folders).clicked() {
                cycle_sidebar(&mut self.controller.state.settings);
                self.controller.state.settings.save();
            }
            if icon_button(ui, icon::ARROW_LEFT, s.back, self.controller.can_go_back()).clicked() {
                self.controller.dispatch(AppAction::Back);
            }
            if icon_button(
                ui,
                icon::ARROW_RIGHT,
                s.forward,
                self.controller.can_go_forward(),
            )
            .clicked()
            {
                self.controller.dispatch(AppAction::Forward);
            }
            if icon_button(
                ui,
                icon::ARROW_UP,
                s.up,
                idle && self.controller.can_go_up(),
            )
            .clicked()
            {
                self.controller.dispatch(AppAction::Up);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.filter(ui);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.breadcrumbs(ui);
                });
            });
        });
    }

    fn filter(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let id = egui::Id::new(FILTER_ID);
        let focused = ui.memory(|m| m.has_focus(id));
        if !focused && self.controller.state.filter != self.filter_value {
            self.filter_value = self.controller.state.filter.clone();
        }
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.filter_value)
                .id(id)
                .hint_text(format!("{}  {}", icon::MAGNIFYING_GLASS, s.find_word))
                .desired_width(if self.width < 900.0 { 150.0 } else { 220.0 }),
        );
        if self.focus_filter {
            response.request_focus();
            self.focus_filter = false;
        }
        let cleared = response.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
            && !self.filter_value.is_empty();
        if cleared {
            self.filter_value.clear();
        }
        if response.changed() || cleared {
            self.controller
                .dispatch(AppAction::SetFilter(self.filter_value.clone()));
        }
    }

    fn breadcrumbs(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let crumbs = self.crumbs();
        let (shown, hidden) = breadcrumb_indices(crumbs.len(), self.width);
        let p = palette_of(ui.ctx());
        let last = crumbs.len().saturating_sub(1);
        let mut go = None;
        let mut dropped = None;
        for (position, index) in shown.iter().enumerate() {
            if position > 0 {
                ui.label(super::theme::icon_text(icon::CARET_RIGHT).color(hex(p.muted)));
            }
            if position == 1 && !hidden.is_empty() {
                ui.menu_button("…", |ui| {
                    for index in &hidden {
                        if ui.button(&crumbs[*index].0).clicked() {
                            go = Some(crumbs[*index].1.clone());
                        }
                    }
                })
                .response
                .on_hover_text(s.hidden_folders);
                ui.label(super::theme::icon_text(icon::CARET_RIGHT).color(hex(p.muted)));
            }
            let (label, path) = &crumbs[*index];
            let text = if *index == last {
                RichText::new(label).strong()
            } else {
                RichText::new(label).color(hex(p.muted))
            };
            let response = ui
                .add(egui::Button::new(text).frame(false))
                .on_hover_text(fill(s.open_folder, &[("name", label)]));
            if response.clicked() && *index != last {
                go = Some(path.clone());
            }
            if let Some(payload) = response.dnd_release_payload::<DraggedRows>() {
                dropped = Some((payload.0.clone(), path.clone()));
            }
        }
        if let Some(path) = go {
            self.controller.dispatch(AppAction::Navigate(path));
        }
        if let Some((roots, target)) = dropped {
            if self.controller.writable() && self.background_idle() {
                self.controller.move_into(&roots, &target);
            }
        }
    }

    fn overflow_menu(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let idle = self.background_idle();
        let archive = self.controller.state.archive.is_some();
        let writable = self.controller.writable();
        let on_disk = self.controller.on_disk();
        ui.menu_button(super::theme::icon_text(icon::DOTS_THREE).size(16.0), |ui| {
            if let Some(release) = &self.controller.state.update {
                let label = fill(s.update_ready, &[("version", &release.tag)]);
                if ui.button(label).clicked() {
                    self.overflow_action(OverflowAction::Release);
                }
                ui.separator();
            }
            group_label(ui, s.archive_group);
            let can_add = idle && archive && writable;
            if ui
                .add_enabled(can_add, egui::Button::new(s.add_files_word))
                .clicked()
            {
                self.overflow_action(OverflowAction::AddFiles);
            }
            if ui
                .add_enabled(can_add, egui::Button::new(s.add_folders_word))
                .clicked()
            {
                self.overflow_action(OverflowAction::AddFolders);
            }
            if ui
                .add_enabled(can_add, egui::Button::new(s.new_folder))
                .clicked()
            {
                self.overflow_action(OverflowAction::NewFolder);
            }
            let undo = idle && self.controller.state.undo.is_some();
            if ui
                .add_enabled(undo, egui::Button::new(s.undo_word).shortcut_text("Ctrl+Z"))
                .clicked()
            {
                self.overflow_action(OverflowAction::Undo);
            }
            if ui
                .add_enabled(idle && archive, egui::Button::new(s.save_copy))
                .clicked()
            {
                self.overflow_action(OverflowAction::SaveCopy);
            }
            if ui
                .add_enabled(
                    idle && archive && writable,
                    egui::Button::new(s.password_action),
                )
                .clicked()
            {
                self.controller.dispatch(AppAction::BeginPasswordChange);
            }
            if self.controller.can_close_archive()
                && ui
                    .add_enabled(idle, egui::Button::new(s.close_archive))
                    .clicked()
            {
                self.controller.dispatch(AppAction::CloseArchive);
            }
            ui.separator();
            group_label(ui, s.selection_group);
            let browsing = archive || on_disk;
            if ui
                .add_enabled(
                    idle && browsing,
                    egui::Button::new(s.select_all).shortcut_text("Ctrl+A"),
                )
                .clicked()
            {
                self.controller.dispatch(AppAction::SelectAllVisible);
            }
            if ui
                .add_enabled(
                    idle && browsing,
                    egui::Button::new(s.invert_selection).shortcut_text("Ctrl+I"),
                )
                .clicked()
            {
                self.controller.dispatch(AppAction::InvertVisible);
            }
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(s.clear_selection).shortcut_text("Esc"),
                )
                .clicked()
            {
                self.controller.dispatch(AppAction::ClearSelection);
            }
            if clipboard::AVAILABLE {
                ui.separator();
                group_label(ui, s.clipboard_group);
                if ui
                    .add_enabled(self.can_copy_files(), egui::Button::new(s.copy_word))
                    .clicked()
                {
                    self.dispatch_clipboard(false);
                }
                if ui
                    .add_enabled(
                        self.can_copy_files() && writable,
                        egui::Button::new(s.cut_word),
                    )
                    .clicked()
                {
                    self.dispatch_clipboard(true);
                }
                if ui
                    .add_enabled(
                        self.can_paste_files() && writable,
                        egui::Button::new(s.paste_word),
                    )
                    .clicked()
                {
                    self.dispatch_paste();
                }
            }
            ui.separator();
            group_label(ui, s.view_group);
            let mut hidden = self.controller.state.settings.show_hidden;
            if ui.checkbox(&mut hidden, s.show_hidden).changed() {
                self.controller.dispatch(AppAction::SetShowHidden(hidden));
            }
            let recent = self.controller.state.settings.recent.clone();
            ui.menu_button(s.recent_group, |ui| {
                for path in recent.iter().take(RECENT_MAX) {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone());
                    if ui
                        .add_enabled(idle, egui::Button::new(name))
                        .on_hover_text(path)
                        .clicked()
                    {
                        self.controller
                            .dispatch(AppAction::Open(PathBuf::from(path)));
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(!recent.is_empty(), egui::Button::new(s.clear_history))
                    .clicked()
                {
                    self.controller.state.settings.recent.clear();
                    self.controller.state.settings.save();
                }
            });
            ui.separator();
            group_label(ui, s.application_group);
            if ui
                .add_enabled(idle, egui::Button::new(s.settings))
                .clicked()
            {
                self.controller.state.show_settings = true;
            }
            if ui
                .add(egui::Button::new(s.shortcuts_title).shortcut_text("F1"))
                .clicked()
            {
                self.controller.state.show_shortcuts = true;
            }
        })
        .response
        .on_hover_text(s.more_word);
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        ui.horizontal(|ui| {
            let color = if self.controller.state.error {
                p.danger
            } else {
                p.muted
            };
            let status = self.status();
            let tasks = self.shown_tasks().len();
            let running = self.controller.active_tasks().filter(|t| !t.quiet).count();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tasks > 0 && !self.controller.state.one_shot {
                    let label = if running > 0 {
                        fill(s.running_count, &[("n", &running.to_string())])
                    } else {
                        s.operations_title.to_string()
                    };
                    let jobs_open = self.jobs_open;
                    if ui
                        .add(
                            egui::Button::new(format!("{}  {label}", icon::TRAY))
                                .selected(jobs_open)
                                .frame_when_inactive(false),
                        )
                        .on_hover_text(s.operations_title)
                        .clicked()
                    {
                        self.jobs_open = !jobs_open;
                    }
                }
                if self.controller.state.archive.is_some() || self.controller.on_disk() {
                    let visible = self.controller.visible_rows().len();
                    let counts = format!(
                        "{visible} {} {} \u{b7} {} {}",
                        s.visible_of,
                        self.controller.folder_total(),
                        self.selected_count(),
                        s.checked
                    );
                    ui.label(RichText::new(counts).small().color(hex(p.muted)));
                }
                if self.width >= 1000.0 {
                    for (keys, what) in [
                        ("F1", s.shortcuts_title),
                        ("Ctrl+F", s.find_word),
                        ("F3", s.view_word),
                    ] {
                        ui.label(RichText::new(what).small().color(hex(p.muted)));
                        kbd(ui, keys);
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(status).color(hex(color))).truncate());
                });
            });
        });
    }
}
