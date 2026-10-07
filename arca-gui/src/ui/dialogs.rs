use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

pub(crate) fn dialog_width(kind: ModalKind, window: f32) -> f32 {
    let wanted: f32 = match kind {
        ModalKind::Settings => 860.0,
        ModalKind::Shortcuts => 720.0,
        ModalKind::Add => 600.0,
        ModalKind::Conflict => 560.0,
        _ => 448.0,
    };
    wanted.min(window - 32.0).max(240.0)
}

pub(crate) fn dialog_max_height(window: f32) -> f32 {
    (window - 96.0).max(160.0)
}

type ShortcutRow = (&'static [&'static str], &'static str);

pub(crate) fn shortcut_rows(s: &'static Strings) -> [Vec<ShortcutRow>; 2] {
    [
        vec![
            (&["ctrl-o"], s.open),
            (&["ctrl-n"], s.compress),
            (&["ctrl-e"], s.extract_all),
            (&["alt-w"], s.extract_here),
            (&["f3"], s.view_word),
            (&["ctrl-t"], s.test_word),
            (&["f5"], s.refresh_word),
            (&["ctrl-f"], s.find_word),
            (&["ctrl-z"], s.undo_word),
            (&["ctrl-a"], s.select_all),
            (&["ctrl-i"], s.invert_selection),
            (&["escape"], s.clear_selection),
            (&["space"], s.toggle_word),
            (&["+", "-"], s.select_group),
            (&["f2"], s.rename_word),
            (&["delete"], s.delete_word),
        ],
        vec![
            (&["f1"], s.shortcuts_title),
            (&["ctrl-c"], s.copy_word),
            (&["ctrl-x"], s.cut_word),
            (&["ctrl-v"], s.paste_word),
            (&["ctrl-shift-c"], s.copy_names),
            (&["enter"], s.open_word),
            (&["backspace", "left"], s.up),
            (&["right"], s.enter_folder),
            (&["alt-left"], s.back),
            (&["alt-right"], s.forward),
            (&["up", "down"], s.move_word),
            (&["home", "end"], s.move_word),
            (&["pageup", "pagedown"], s.move_word),
            (&["tab"], s.focus_word),
            (&["a"], s.jump_word),
        ],
    ]
}

pub(crate) fn keystroke(spec: &str, mac: bool) -> String {
    if spec.len() == 1 {
        return spec.to_uppercase();
    }
    let parts: Vec<String> = spec
        .split('-')
        .map(|part| match part {
            "ctrl" if mac => "⌘".into(),
            "ctrl" => "Ctrl".into(),
            "alt" if mac => "⌥".into(),
            "alt" => "Alt".into(),
            "shift" if mac => "⇧".into(),
            "shift" => "Shift".into(),
            "escape" => "Esc".into(),
            "delete" => "Delete".into(),
            "backspace" => "Backspace".into(),
            "enter" => "Enter".into(),
            "space" => "Space".into(),
            "tab" => "Tab".into(),
            "left" => "←".into(),
            "right" => "→".into(),
            "up" => "↑".into(),
            "down" => "↓".into(),
            "home" => "Home".into(),
            "end" => "End".into(),
            "pageup" => "PgUp".into(),
            "pagedown" => "PgDn".into(),
            other => other.to_uppercase(),
        })
        .collect();
    parts.join(if mac { "" } else { "+" })
}

fn shortcuts_table(ui: &mut egui::Ui, s: &'static Strings) {
    let mac = cfg!(target_os = "macos");
    ui.columns(2, |columns| {
        for (column, rows) in columns.iter_mut().zip(shortcut_rows(s)) {
            egui::Grid::new(ui_id(column, "keys"))
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(column, |ui| {
                    for (keys, what) in rows {
                        ui.horizontal(|ui| {
                            for key in keys {
                                kbd(ui, &keystroke(key, mac));
                            }
                        });
                        ui.label(what);
                        ui.end_row();
                    }
                });
        }
    });
}

fn ui_id(ui: &egui::Ui, salt: &str) -> egui::Id {
    ui.id().with(salt)
}

impl Shell {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.modal_kind() else {
            return;
        };
        let width = dialog_width(kind, ctx.content_rect().width());
        let p = palette_of(ctx);
        let modal = egui::Modal::new(egui::Id::new(("arca-modal", kind as u8))).frame(
            egui::Frame::popup(&ctx.global_style())
                .fill(hex(p.surface))
                .inner_margin(18),
        );
        let tallest = dialog_max_height(ctx.content_rect().height());
        let response = modal.show(ctx, |ui| {
            ui.set_width(width);
            egui::ScrollArea::vertical()
                .max_height(tallest)
                .auto_shrink([false, true])
                .show(ui, |ui| match kind {
                    ModalKind::Password => self.password_dialog(ui),
                    ModalKind::Conflict => self.conflict_dialog(ui),
                    ModalKind::Delete => self.delete_dialog(ui),
                    ModalKind::Drop => self.drop_dialog(ui),
                    ModalKind::Add => self.add_dialog(ui),
                    ModalKind::Settings => self.settings_dialog(ui),
                    ModalKind::Shortcuts => {
                        let s = self.controller.s();
                        heading(ui, s.shortcuts_title);
                        ui.add_space(8.0);
                        shortcuts_table(ui, s);
                    }
                    ModalKind::NewFolder | ModalKind::Mask => self.name_dialog(ui, kind),
                });
        });
        if response.should_close() && self.modal_kind() == Some(kind) {
            self.cancel_modal(kind);
        }
    }

    fn footer_buttons(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    }

    fn password_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let state = &self.controller.state;
        let setting = matches!(state.waiting_on_password, Some(Pending::NewPassword(_)));
        let opening = matches!(
            state.waiting_on_password,
            Some(Pending::Extract(_) | Pending::OpenArchive | Pending::Read(_))
        );
        let removable = setting && state.entries.iter().any(|entry| entry.encrypted);
        heading(
            ui,
            if setting {
                s.set_password
            } else {
                s.password_needed
            },
        );
        let (hint, warn) = if setting {
            (s.new_password, false)
        } else if state.password_wrong {
            if state.notice == s.password_or_corrupt {
                (s.password_or_corrupt, true)
            } else {
                (s.password_wrong, true)
            }
        } else {
            (s.password_hint, false)
        };
        ui.label(RichText::new(hint).color(hex(if warn { p.danger } else { p.muted })));
        ui.add_space(8.0);
        let mut value = self.controller.state.password_input.clone();
        let field = ui.add(
            egui::TextEdit::singleline(&mut value)
                .password(true)
                .desired_width(f32::INFINITY),
        );
        if ui.memory(|m| m.focused().is_none()) {
            field.request_focus();
        }
        if field.changed() {
            self.controller
                .dispatch(AppAction::SetPasswordInput(value.clone()));
        }
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let mut submit = entered;
        let mut cancel = false;
        let mut remove = false;
        Self::footer_buttons(ui, |ui| {
            let label = if setting {
                s.set_password
            } else if opening {
                s.open_word
            } else {
                s.continue_word
            };
            submit |= primary_button(ui, label, true).clicked();
            if removable {
                remove = secondary_button(ui, s.remove_password, true).clicked();
            }
            cancel = secondary_button(ui, s.cancel, true).clicked();
        });
        if remove {
            self.controller
                .dispatch(AppAction::SubmitPassword(String::new()));
        } else if submit {
            self.controller.dispatch(AppAction::SubmitPassword(value));
        } else if cancel {
            self.cancel_modal(ModalKind::Password);
        }
    }

    fn conflict_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let Some((_, path)) = self.controller.conflict() else {
            return;
        };
        let path = path.to_string();
        heading(ui, s.conflict_title);
        ui.label(format!("{} {path}", s.already_there));
        let mut answer = None;
        Self::footer_buttons(ui, |ui| {
            for (label, choice, primary) in [
                (s.cancel, Answer::Cancel, false),
                (s.rename_all, Answer::RenameAll, false),
                (s.rename, Answer::Rename, false),
                (s.no_all, Answer::SkipAll, false),
                (s.no, Answer::Skip, false),
                (s.yes_all, Answer::ReplaceAll, false),
                (s.yes, Answer::Replace, true),
            ]
            .into_iter()
            .rev()
            {
                let clicked = if primary {
                    primary_button(ui, label, true).clicked()
                } else {
                    secondary_button(ui, label, true).clicked()
                };
                if clicked {
                    answer = Some(choice);
                }
            }
        });
        if let Some(answer) = answer {
            self.answer_conflict(answer);
        }
    }

    fn delete_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let names = self
            .controller
            .state
            .confirm_delete
            .clone()
            .unwrap_or_default();
        heading(ui, s.delete_word);
        ui.label(fill(s.confirm_delete, &[("n", &names.len().to_string())]));
        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for name in names.iter().take(50) {
                    ui.label(RichText::new(name).color(hex(p.muted)));
                }
            });
        let mut choice = None;
        Self::footer_buttons(ui, |ui| {
            if danger_button(ui, s.delete_word).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                choice = Some(true);
            }
            if secondary_button(ui, s.cancel, true).clicked() {
                choice = Some(false);
            }
        });
        if let Some(choice) = choice {
            self.controller.dispatch(AppAction::ConfirmDelete(choice));
        }
    }

    fn drop_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let names: Vec<String> = self
            .controller
            .state
            .confirm_drop
            .iter()
            .flatten()
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            })
            .collect();
        heading(ui, s.drop_title);
        ui.label(
            RichText::new(format!("{} {}", s.dropped_word, names.join(", "))).color(hex(p.muted)),
        );
        let mut choice = None;
        Self::footer_buttons(ui, |ui| {
            if primary_button(ui, s.open_word, true).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                choice = Some(DropChoice::Open);
            }
            if secondary_button(ui, s.add_to_archive, true).clicked() {
                choice = Some(DropChoice::Add);
            }
            if secondary_button(ui, s.cancel, true).clicked() {
                choice = Some(DropChoice::Cancel);
            }
        });
        if let Some(choice) = choice {
            self.controller.dispatch(AppAction::AnswerDrop(choice));
        }
    }

    fn pickers(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let format = self.controller.state.format;
        let is_zip = format == Format::Zip;
        let is_lzma2 = matches!(format, Format::SevenZ | Format::TarXz | Format::Xz);
        egui::Grid::new(ui.id().with("pickers"))
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(s.format);
                let mut chosen = format;
                egui::ComboBox::from_id_salt("format")
                    .selected_text(format.label())
                    .show_ui(ui, |ui| {
                        for candidate in AppController::create_formats() {
                            ui.selectable_value(&mut chosen, candidate, candidate.label());
                        }
                    });
                if chosen != format {
                    self.controller.set_create_format(chosen);
                }
                ui.end_row();
                ui.label(s.compressor);
                if is_lzma2 {
                    ui.label("LZMA2");
                } else if format == Format::Rar {
                    ui.label("RAR");
                } else {
                    let codec = self.controller.state.codec;
                    let mut chosen = codec;
                    ui.add_enabled_ui(is_zip, |ui| {
                        egui::ComboBox::from_id_salt("codec")
                            .selected_text(self.controller.codec_name(codec))
                            .show_ui(ui, |ui| {
                                for candidate in [Codec::Store, Codec::Deflate, Codec::Zstd] {
                                    ui.selectable_value(
                                        &mut chosen,
                                        candidate,
                                        self.controller.codec_name(candidate),
                                    );
                                }
                            });
                    });
                    self.controller.state.codec = chosen;
                }
                ui.end_row();
                ui.label(s.level);
                let level = self.controller.state.level;
                let mut chosen = level;
                egui::ComboBox::from_id_salt("level")
                    .selected_text(self.controller.level_name(level))
                    .show_ui(ui, |ui| {
                        for candidate in [Level::Store, Level::Fast, Level::Normal, Level::Best] {
                            ui.selectable_value(
                                &mut chosen,
                                candidate,
                                self.controller.level_name(candidate),
                            );
                        }
                    });
                self.controller.state.level = chosen;
                ui.end_row();
            });
    }

    fn add_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        heading(ui, s.add_to_archive);
        ui.add_space(6.0);
        ui.label(s.output_name);
        ui.add(
            egui::TextEdit::singleline(&mut self.controller.state.output_name)
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        self.pickers(ui);
        let format = self.controller.state.format;
        if matches!(format, Format::Zip | Format::SevenZ) {
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.controller.state.add_password)
                    .password(true)
                    .hint_text(s.password_optional)
                    .desired_width(f32::INFINITY),
            );
            if format == Format::SevenZ {
                ui.checkbox(&mut self.controller.state.hide_names, s.hide_names);
            }
        }
        if let Some(note) = self.controller.create_policy_note() {
            ui.label(RichText::new(note).small().color(hex(p.warning)));
        }
        ui.add_space(8.0);
        let count = self.controller.state.pending_inputs.len();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{count} {}", s.items_word)).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(count > 0, egui::Button::new(s.remove_all))
                    .clicked()
                {
                    self.controller.state.pending_inputs.clear();
                }
                ui.menu_button(format!("{}  {}", icon::PLUS, s.add_word), |ui| {
                    if ui.button(s.add_files_word).clicked() {
                        self.begin_dialog(DialogKind::PickInputs { folders: false });
                    }
                    if ui.button(s.add_folders_word).clicked() {
                        self.begin_dialog(DialogKind::PickInputs { folders: true });
                    }
                });
            });
        });
        let mut remove = None;
        egui::Frame::new()
            .fill(hex(p.background))
            .corner_radius(6)
            .inner_margin(6)
            .show(ui, |ui| {
                ui.set_min_height(80.0);
                ui.set_width(ui.available_width());
                if count == 0 {
                    ui.label(RichText::new(s.drop_inputs_here).color(hex(p.muted)));
                }
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for (index, path) in self.controller.state.pending_inputs.iter().enumerate()
                        {
                            ui.horizontal(|ui| {
                                if icon_button(ui, icon::X, s.remove_input, true).clicked() {
                                    remove = Some(index);
                                }
                                ui.add(egui::Label::new(path.display().to_string()).truncate());
                            });
                        }
                    });
            });
        if let Some(index) = remove {
            self.controller.state.pending_inputs.remove(index);
        }
        let mut start = false;
        let mut cancel = false;
        Self::footer_buttons(ui, |ui| {
            start = primary_button(ui, s.start, count > 0).clicked();
            cancel = secondary_button(ui, s.cancel, true).clicked();
        });
        if start {
            self.start_add();
        } else if cancel {
            self.cancel_modal(ModalKind::Add);
        }
    }

    fn name_dialog(&mut self, ui: &mut egui::Ui, kind: ModalKind) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let (title, hint, confirm) = if kind == ModalKind::NewFolder {
            (s.new_folder, s.folder_name, s.new_folder)
        } else if self.controller.state.picking_group.unwrap_or(true) {
            (s.select_group, s.mask_hint, s.start)
        } else {
            (s.deselect_group, s.mask_hint, s.start)
        };
        heading(ui, title);
        let description = if kind == ModalKind::NewFolder {
            format!("{hint}: /{}", self.controller.state.current_dir)
        } else {
            hint.to_string()
        };
        ui.label(RichText::new(description).color(hex(p.muted)));
        ui.add_space(6.0);
        let field =
            ui.add(egui::TextEdit::singleline(&mut self.name_value).desired_width(f32::INFINITY));
        if self.focus_name {
            field.request_focus();
            self.focus_name = false;
        }
        let mut ok = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let mut cancel = false;
        Self::footer_buttons(ui, |ui| {
            ok |= primary_button(ui, confirm, true).clicked();
            cancel = secondary_button(ui, s.cancel, true).clicked();
        });
        if ok {
            self.confirm_name(kind);
        } else if cancel {
            self.cancel_modal(kind);
        }
    }

    fn settings_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        ui.horizontal(|ui| {
            heading(ui, s.settings);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, icon::X, s.cancel, true).clicked() {
                    self.cancel_modal(ModalKind::Settings);
                }
            });
        });
        ui.add_space(8.0);
        let sections = [
            (SettingsSection::General, s.settings_general, icon::GEAR_SIX),
            (
                SettingsSection::Archives,
                s.settings_archives,
                icon::ARCHIVE,
            ),
            (
                SettingsSection::Appearance,
                s.settings_appearance,
                icon::PALETTE,
            ),
            (SettingsSection::Views, s.settings_views, icon::SIDEBAR),
            (
                SettingsSection::Keybindings,
                s.shortcuts_title,
                icon::KEYBOARD,
            ),
            (
                SettingsSection::Updates,
                s.settings_updates,
                icon::ARROW_CIRCLE_DOWN,
            ),
            (SettingsSection::About, s.settings_about, icon::INFO),
        ];
        let query = self.settings_query.trim().to_lowercase();
        let matching: Vec<_> = sections
            .iter()
            .filter(|(_, label, _)| query.is_empty() || label.to_lowercase().contains(&query))
            .copied()
            .collect();
        if let Some((first, _, _)) = matching.first() {
            if !matching
                .iter()
                .any(|(section, _, _)| *section == self.settings_section)
            {
                self.settings_section = *first;
            }
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(180.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_query)
                        .hint_text(format!("{}  {}", icon::MAGNIFYING_GLASS, s.settings_search))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                if matching.is_empty() {
                    ui.label(RichText::new(s.settings_no_match).color(hex(p.muted)));
                }
                for (section, label, glyph) in matching {
                    if nav_item(ui, glyph, label, self.settings_section == section, false).clicked()
                    {
                        self.settings_section = section;
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_min_height(420.0);
                ui.set_width(ui.available_width());
                let title = sections
                    .iter()
                    .find(|(section, _, _)| *section == self.settings_section)
                    .map_or(s.settings, |(_, label, _)| *label);
                ui.label(RichText::new(title).font(theme::display(16.0)));
                ui.add_space(8.0);
                match self.settings_section {
                    SettingsSection::General => self.settings_general(ui),
                    SettingsSection::Archives => {
                        ui.label(RichText::new(s.defaults_title).color(hex(p.muted)));
                        self.pickers(ui);
                        ui.add_space(6.0);
                        let mut on = self.controller.state.into_subfolder;
                        if ui.checkbox(&mut on, s.into_subfolder).changed() {
                            self.settings_activate(SettingsControl::Subfolder);
                        }
                    }
                    SettingsSection::Appearance => {
                        ui.label(s.theme);
                        self.choices(ui, SettingsSection::Appearance);
                    }
                    SettingsSection::Views => self.settings_views(ui),
                    SettingsSection::Keybindings => shortcuts_table(ui, s),
                    SettingsSection::Updates => {
                        let mut on = self.controller.state.settings.updates;
                        if ui.checkbox(&mut on, s.check_updates).changed() {
                            self.settings_activate(SettingsControl::Updates);
                        }
                        let mut version = format!("Arca {}", env!("CARGO_PKG_VERSION"));
                        if let Some(release) = &self.controller.state.update {
                            version.push_str(&format!(" · {}", release.tag));
                        }
                        ui.label(RichText::new(version).color(hex(p.muted)));
                        if let Some(release) = &self.controller.state.update {
                            let label = fill(s.update_ready, &[("version", &release.tag)]);
                            if primary_button(ui, &label, true).clicked() {
                                self.controller.start_update();
                            }
                        }
                    }
                    SettingsSection::About => {
                        ui.add(
                            egui::Image::from_bytes("bytes://arca-mark.svg", crate::assets::MARK)
                                .fit_to_exact_size(egui::Vec2::splat(56.0)),
                        );
                        ui.label(
                            RichText::new(format!("Arca {}", env!("CARGO_PKG_VERSION")))
                                .font(theme::display(18.0)),
                        );
                        ui.label(RichText::new(s.about_blurb).color(hex(p.muted)));
                    }
                }
            });
        });
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let lang = self.controller.state.settings.lang;
        let _ = lang;
        ui.label(s.language);
        self.choices(ui, SettingsSection::General);
        ui.add_space(8.0);
        ui.label(s.name_encoding);
        let page = self.controller.state.settings.page;
        let current = arca_zip::pages::Page::ALL
            .iter()
            .find(|(candidate, _, _)| *candidate == page)
            .map_or("", |(_, _, label)| *label);
        let mut chosen = page;
        egui::ComboBox::from_id_salt("settings-page")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (candidate, _, label) in arca_zip::pages::Page::ALL {
                    ui.selectable_value(&mut chosen, candidate, label);
                }
            });
        if chosen != page {
            self.controller.reread_names(chosen);
        }
        ui.add_space(8.0);
        let mut hidden = self.controller.state.settings.show_hidden;
        if ui.checkbox(&mut hidden, s.show_hidden).changed() {
            self.controller.dispatch(AppAction::SetShowHidden(hidden));
        }
    }

    fn settings_views(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let columns =
            self.controller.state.settings.browser_view == crate::settings::BrowserView::Columns;
        ui.label(s.default_view);
        if ui.radio(columns, s.column_view).clicked() && !columns {
            self.switch_view(true, false);
        }
        if ui.radio(!columns, s.details_view).clicked() && columns {
            self.switch_view(false, self.controller.state.settings.flat);
        }
        ui.add_space(8.0);
        let settings = &mut self.controller.state.settings;
        let mut changed = false;
        changed |= ui
            .checkbox(&mut settings.preview_visible, s.show_preview_panel)
            .changed();
        changed |= ui
            .checkbox(&mut settings.folders, s.show_folder_tree)
            .changed();
        changed |= ui
            .checkbox(&mut settings.sidebar_collapsed, s.icon_sidebar)
            .changed();
        if changed {
            settings.save();
        }
    }

    fn choices(&mut self, ui: &mut egui::Ui, section: SettingsSection) {
        let s = self.controller.s();
        let lang = self.controller.state.settings.lang;
        let theme = self.controller.state.settings.theme;
        for control in SettingsControl::ALL {
            let (owner, on, label) = match control {
                SettingsControl::LangSystem => {
                    (SettingsSection::General, lang.is_none(), s.theme_system)
                }
                SettingsControl::LangEn => (
                    SettingsSection::General,
                    lang == Some(Lang::En),
                    Lang::En.label(),
                ),
                SettingsControl::LangEs => (
                    SettingsSection::General,
                    lang == Some(Lang::Es),
                    Lang::Es.label(),
                ),
                SettingsControl::ThemeSystem => (
                    SettingsSection::Appearance,
                    theme == ThemePreference::System,
                    s.theme_system,
                ),
                SettingsControl::ThemeLight => (
                    SettingsSection::Appearance,
                    theme == ThemePreference::Light,
                    s.theme_light,
                ),
                SettingsControl::ThemeDark => (
                    SettingsSection::Appearance,
                    theme == ThemePreference::Dark,
                    s.theme_dark,
                ),
                SettingsControl::Updates | SettingsControl::Subfolder => continue,
            };
            if owner == section && ui.radio(on, label).clicked() && !on {
                self.settings_activate(control);
            }
        }
    }

    fn settings_activate(&mut self, control: SettingsControl) {
        match control {
            SettingsControl::LangSystem => self.controller.dispatch(AppAction::SetLanguage(None)),
            SettingsControl::LangEn => self
                .controller
                .dispatch(AppAction::SetLanguage(Some(Lang::En))),
            SettingsControl::LangEs => self
                .controller
                .dispatch(AppAction::SetLanguage(Some(Lang::Es))),
            SettingsControl::ThemeSystem => self.set_theme(ThemePreference::System),
            SettingsControl::ThemeLight => self.set_theme(ThemePreference::Light),
            SettingsControl::ThemeDark => self.set_theme(ThemePreference::Dark),
            SettingsControl::Updates => {
                let settings = &mut self.controller.state.settings;
                settings.updates = !settings.updates;
                settings.save();
                if settings.updates {
                    self.controller.ask_about_updates();
                }
            }
            SettingsControl::Subfolder => {
                self.controller.state.into_subfolder = !self.controller.state.into_subfolder;
            }
        }
    }
}
