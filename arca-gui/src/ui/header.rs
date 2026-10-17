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
        // The width itself is animated, so that open, rail and hidden all
        // slide into one another rather than only hidden and shown.
        let target = if !(layout.sidebar && !one_shot) {
            0.0
        } else if layout.rail {
            44.0
        } else {
            self.controller.state.settings.sidebar
        };
        let time = ui.ctx().global_style().animation_time;
        let width =
            ui.ctx()
                .animate_value_with_time(egui::Id::new("arca-sidebar-width"), target, time);
        if width > 0.5 {
            let panel = egui::Panel::left("arca-sidebar").frame(
                egui::Frame::new()
                    .fill(hex(p.surface))
                    .inner_margin(egui::Margin::symmetric(6, 8)),
            );
            let settled = (width - target).abs() < 0.5;
            let panel = if layout.rail || !settled {
                panel.exact_size(width).resizable(false)
            } else {
                panel
                    .default_size(target)
                    .size_range(SIDEBAR_LEAST..=SIDEBAR_MOST)
                    .resizable(true)
            };
            let rail = layout.rail;
            let shown = panel.show(ui, |ui| {
                if settled {
                    self.sidebar(ui, rail);
                } else {
                    // Draw at the width it is heading for under a clip, so
                    // that the rows do not re-wrap while the panel moves.
                    let rect = ui.max_rect();
                    let full = target.max(width) - 12.0;
                    let wide = egui::Rect::from_min_size(
                        rect.min,
                        egui::vec2(full.max(1.0), rect.height()),
                    );
                    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(wide));
                    child.multiply_opacity((width / 44.0).clamp(0.0, 1.0));
                    self.sidebar(&mut child, rail);
                }
            });
            if !layout.rail && settled {
                let width = shown.response.rect.width();
                self.remember_width(width, |settings| &mut settings.sidebar);
            }
        }
        let preview_open = layout.preview && browsing && !one_shot;
        if preview_open {
            self.preview_for_cursor();
        } else if self.controller.state.preview.index.is_some() {
            self.close_preview();
        }
        let mut expanded = preview_open;
        let shown = egui::Panel::right("arca-preview")
            .frame(
                egui::Frame::new()
                    .fill(hex(p.surface))
                    .inner_margin(egui::Margin::same(10)),
            )
            .default_size(self.controller.state.settings.preview_width)
            .size_range(280.0..=800.0)
            .resizable(true)
            .show_collapsible(ui, &mut expanded, |ui| self.preview_panel(ui));
        let settled = ui
            .ctx()
            .animate_bool(egui::Id::new("arca-preview-settled"), preview_open)
            >= 1.0;
        if let Some(shown) = shown.filter(|_| preview_open && settled) {
            let width = shown.response.rect.width();
            self.remember_width(width, |settings| &mut settings.preview_width);
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
                        // No gap between the row above and the list's
                        // header: they read as one bar.
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.nav_row(ui);
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

    /// The toolbar doubles as the title bar when Arca draws its own: the
    /// empty stretch moves the window, and a double click fills the screen.
    /// It is laid down before the buttons so that they stay on top of it.
    fn title_grip(&mut self, ui: &mut egui::Ui) {
        let height = ui.spacing().interact_size.y + 6.0;
        let rect =
            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), height));
        let response = ui.interact(
            rect,
            egui::Id::new("arca-title-grip"),
            egui::Sense::click_and_drag(),
        );
        if response.double_clicked() {
            let full = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!full));
        } else if response.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    fn window_buttons(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let full = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        if icon_button(ui, icon::X, s.close_window, true).clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let (glyph, tip) = if full {
            (icon::CORNERS_IN, s.restore)
        } else {
            (icon::CORNERS_OUT, s.maximize)
        };
        if icon_button(ui, glyph, tip, true).clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!full));
        }
        if icon_button(ui, icon::MINUS, s.minimize, true).clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// One menu for how the list looks: the view, the preview panel, how
    /// tall the rows are, the text size and hidden files.
    fn view_menu(&mut self, ui: &mut egui::Ui) {
        use crate::settings::{BrowserView, Density, TEXT_LEAST, TEXT_MOST};
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let columns = self.controller.state.browser.columns;
        let flat = self.controller.state.settings.flat;
        ui.menu_button(super::theme::icon_text(icon::LIST).size(16.0), |ui| {
            // A right-to-left row inside a menu would otherwise stretch the
            // menu across the whole window.
            ui.set_min_width(240.0);
            ui.set_max_width(240.0);
            group_label(ui, s.view_group);
            let view = self.controller.state.settings.browser_view;
            for (glyph, label, want, want_flat) in [
                (icon::COLUMNS, s.panes_view, BrowserView::Panes, false),
                (
                    icon::LIST_BULLETS,
                    s.details_view,
                    BrowserView::Details,
                    false,
                ),
                (icon::ROWS, s.flat_view, BrowserView::Details, true),
            ] {
                let on = if want == BrowserView::Details {
                    !columns && flat == want_flat
                } else {
                    columns && view == want
                };
                if pick_entry(ui, glyph, label, on).clicked() && !on {
                    self.switch_view(want, want_flat);
                }
            }
            let preview = self.controller.state.settings.preview_visible;
            if pick_entry(ui, icon::EYE, s.preview_panel, preview).clicked() {
                self.controller.state.settings.preview_visible = !preview;
                self.controller.state.settings.save();
            }
            ui.separator();
            group_label(ui, s.panes_view);
            let next_pane = self.controller.state.settings.next_pane;
            if pick_entry(ui, icon::COLUMNS, s.next_pane, next_pane).clicked() {
                self.set_next_pane(!next_pane);
            }
            let folder_peek = self.controller.state.settings.folder_peek;
            if pick_entry(ui, icon::FOLDER_OPEN, s.folder_peek, folder_peek).clicked() {
                self.controller.state.settings.folder_peek = !folder_peek;
                self.controller.state.settings.save();
            }
            let pointer_focus = self.controller.state.settings.pointer_focus;
            if pick_entry(ui, icon::CURSOR, s.pointer_focus, pointer_focus).clicked() {
                self.controller.state.settings.pointer_focus = !pointer_focus;
                self.controller.state.settings.save();
            }
            ui.separator();
            group_label(ui, s.density_title);
            let density = self.controller.state.settings.density;
            for (glyph, label, want) in [
                (
                    icon::ARROWS_IN_LINE_VERTICAL,
                    s.compact_word,
                    Density::Compact,
                ),
                (icon::ARROWS_OUT_LINE_VERTICAL, s.airy_word, Density::Airy),
            ] {
                if pick_entry(ui, glyph, label, density == want).clicked() && density != want {
                    self.controller.state.settings.density = want;
                    self.controller.state.settings.save();
                }
            }
            ui.separator();
            group_label(ui, s.text_size_title);
            let size = self.controller.state.settings.text_size;
            let mut step = None;
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(
                    super::theme::icon_text(icon::TEXT_AA)
                        .size(15.0)
                        .color(hex(p.focus)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    step = stepper(
                        ui,
                        &format!("{size:.0}"),
                        size > TEXT_LEAST,
                        size < TEXT_MOST,
                    );
                });
            });
            if let Some(step) = step {
                self.step_text_size(f32::from(step));
            }
            ui.separator();
            let hidden = self.controller.state.settings.show_hidden;
            if pick_entry(ui, icon::EYE_SLASH, s.hidden_files, hidden).clicked() {
                self.controller.dispatch(AppAction::SetShowHidden(!hidden));
            }
        })
        .response
        .on_hover_text(s.view_group);
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
        let own_bar = self.own_title_bar();
        if own_bar {
            self.title_grip(ui);
        }
        ui.horizontal(|ui| {
            if icon_button(ui, icon::SIDEBAR_SIMPLE, s.folder_tree, true).clicked() {
                cycle_sidebar(&mut self.controller.state.settings);
                self.controller.state.settings.save();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if own_bar {
                    self.window_buttons(ui);
                    ui.separator();
                }
                self.overflow_menu(ui);
                if icon_button(ui, icon::GEAR_SIX, s.settings, idle).clicked() {
                    self.controller.state.show_settings = true;
                }
                let browsing = archive || self.controller.on_disk();
                let search_tip = format!("{}  (Ctrl+K)", s.search_word);
                if icon_button(ui, icon::MAGNIFYING_GLASS, &search_tip, idle && browsing).clicked()
                {
                    self.toggle_search();
                }
                self.view_menu(ui);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.breadcrumbs(ui);
                });
            });
        });
    }

    /// Above the list, as Strata has it: back, forward, up, the folder's
    /// name, and the funnel; the filter row slides out underneath.
    fn nav_row(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let idle = self.background_idle();
        egui::Frame::new()
            .fill(hex(p.surface))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(ui, icon::ARROW_LEFT, s.back, self.controller.can_go_back())
                        .clicked()
                    {
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
                    ui.add_space(6.0);
                    let here = self
                        .crumbs()
                        .last()
                        .map(|crumb| crumb.0.clone())
                        .unwrap_or_default();
                    ui.label(RichText::new(here).strong().color(hex(p.link)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let filtering = !self.controller.state.filter.is_empty();
                        let panes = self.panes_view();
                        let active = self.controller.state.browser.active;
                        let on = if panes {
                            filtering
                                || self.pane_filter.as_ref().is_some_and(|(o, _)| *o == active)
                        } else {
                            self.filter_open || filtering
                        };
                        let find_tip = format!("{}  (Ctrl+F)", s.find_word);
                        if icon_toggle(ui, icon::FUNNEL, &find_tip, true, on).clicked() {
                            if panes {
                                self.toggle_pane_filter(active);
                            } else {
                                self.toggle_filter();
                            }
                        }
                    });
                });
                let wanted = !self.panes_view()
                    && (self.filter_open || !self.controller.state.filter.is_empty());
                let open = ui
                    .ctx()
                    .animate_bool(egui::Id::new("arca-filter-open"), wanted);
                if open > 0.0 {
                    let full = ui.spacing().interact_size.y + 6.0;
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), full * open),
                        egui::Sense::hover(),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(rect.width(), full),
                            ))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    child.set_clip_rect(rect.intersect(ui.clip_rect()));
                    child.multiply_opacity(open);
                    self.filter(&mut child);
                }
            });
    }

    /// The funnel button: opens the filter row with the field focused, or
    /// closes it and lets everything show again.
    pub(super) fn toggle_filter(&mut self) {
        if self.filter_open || !self.controller.state.filter.is_empty() {
            self.close_filter();
        } else {
            self.filter_open = true;
            self.focus_filter = true;
        }
    }

    fn close_filter(&mut self) {
        self.filter_open = false;
        self.filter_value.clear();
        if !self.controller.state.filter.is_empty() {
            self.controller
                .dispatch(AppAction::SetFilter(String::new()));
        }
        self.release_focus();
    }

    fn filter(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let id = egui::Id::new(FILTER_ID);
        let focused = ui.memory(|m| m.has_focus(id));
        if !focused && self.controller.state.filter != self.filter_value {
            self.filter_value = self.controller.state.filter.clone();
        }
        let p = palette_of(ui.ctx());
        ui.add_space(4.0);
        ui.label(
            super::theme::icon_text(icon::FUNNEL)
                .size(15.0)
                .color(hex(p.focus)),
        );
        let total = self.controller.folder_total();
        let hint = fill(s.filter_items, &[("count", &total.to_string())]);
        let response = ui.add(
            super::widgets::field(ui, &mut self.filter_value)
                .id(id)
                .hint_text(hint)
                .frame(egui::Frame::NONE)
                .desired_width(ui.available_width()),
        );
        if self.focus_filter {
            response.request_focus();
            self.focus_filter = false;
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_filter();
            return;
        }
        if response.changed() {
            self.controller
                .dispatch(AppAction::SetFilter(self.filter_value.clone()));
        }
    }

    fn breadcrumbs(&mut self, ui: &mut egui::Ui) {
        if self.path_edit.is_some() {
            self.path_field(ui);
            return;
        }
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
        let mut edit = false;
        if !crumbs.is_empty() {
            // Copy and edit sit right after the path and only show while the
            // pointer is over the bar, so the row stays quiet otherwise.
            if ui.rect_contains_pointer(ui.max_rect()) {
                ui.add_space(2.0);
                if icon_button(ui, icon::COPY, s.copy_path, true).clicked() {
                    self.copy_current_path();
                }
                edit |= icon_button(ui, icon::PENCIL_SIMPLE, s.edit_path, true).clicked();
            }
            let rest = ui.available_width();
            if rest > 8.0 {
                let (_, response) =
                    ui.allocate_exact_size(egui::vec2(rest, 26.0), egui::Sense::click());
                edit |= response
                    .on_hover_cursor(egui::CursorIcon::Text)
                    .on_hover_text(format!("{}  (Ctrl+L)", s.edit_path))
                    .clicked();
            }
        }
        if edit {
            self.start_path_edit();
        }
        if let Some(path) = go {
            self.controller.dispatch(AppAction::Navigate(path));
        }
        if let Some((roots, target)) = dropped {
            self.drop_rows(roots, target);
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

    /// The new-version notice, in the status bar where it does not get in
    /// the way: the download glyph and the whole sentence, in green.
    fn update_notice(&mut self, ui: &mut egui::Ui) {
        let Some(release) = self.controller.state.update.clone() else {
            return;
        };
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let label = fill(s.update_ready, &[("version", &release.tag)]);
        let mut job = egui::text::LayoutJob::default();
        job.append(
            icon::DOWNLOAD_SIMPLE,
            0.0,
            egui::TextFormat {
                font_id: super::theme::icon_font(14.0),
                color: hex(p.success),
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        job.append(
            &label,
            6.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.0),
                color: hex(p.success),
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        if ui
            .add(egui::Button::new(job).frame(false))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.controller.state.show_update = true;
        }
        ui.separator();
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
                self.update_notice(ui);
                if self.dialog.is_some() {
                    let p = palette_of(ui.ctx());
                    ui.label(
                        RichText::new(s.picker_waiting)
                            .size(12.0)
                            .color(hex(p.warning)),
                    );
                    ui.separator();
                }
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
