use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use crate::settings::{TitleBar, TEXT_LEAST, TEXT_MOST};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

pub(crate) fn dialog_width(kind: ModalKind, window: f32) -> f32 {
    let wanted: f32 = match kind {
        ModalKind::Settings => 1040.0,
        ModalKind::Shortcuts => 720.0,
        ModalKind::Search => 640.0,
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
            (&["ctrl-l"], s.edit_path),
            (&["ctrl-k"], s.search_word),
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
                .min_col_width(120.0)
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

pub(crate) fn section_words(section: SettingsSection, s: &'static Strings) -> Vec<&'static str> {
    match section {
        SettingsSection::General => vec![s.language, s.name_encoding, s.show_hidden],
        SettingsSection::Archives => vec![s.defaults_title, s.into_subfolder],
        SettingsSection::Appearance => vec![
            s.theme,
            s.current_theme,
            s.library_section,
            s.add_theme,
            s.font_title,
            s.text_size_title,
            s.title_bar_title,
            s.motion_title,
        ],
        SettingsSection::Views => vec![
            s.default_view,
            s.show_preview_panel,
            s.show_folder_tree,
            s.icon_sidebar,
        ],
        SettingsSection::Updates => vec![
            s.check_updates,
            s.update_status,
            s.check_now,
            s.channel_title,
            s.channel_preview,
        ],
        SettingsSection::Keybindings | SettingsSection::About => Vec::new(),
    }
}

fn ui_id(ui: &egui::Ui, salt: &str) -> egui::Id {
    ui.id().with(salt)
}

impl Shell {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        let fade = egui::Id::new("arca-modal-fade");
        let Some(kind) = self.modal_kind() else {
            theme::forget(ctx, fade);
            return;
        };
        let width = dialog_width(kind, ctx.content_rect().width());
        let p = palette_of(ctx);
        let modal = egui::Modal::new(egui::Id::new(("arca-modal", kind as u8))).frame(
            egui::Frame::popup(&ctx.global_style())
                .fill(hex(p.surface))
                .stroke(egui::Stroke::new(1.0, theme::alpha(p.border, 0.8)))
                .corner_radius(9)
                .inner_margin(if matches!(kind, ModalKind::Settings | ModalKind::Search) {
                    0
                } else {
                    18
                }),
        );
        let tallest = dialog_max_height(ctx.content_rect().height());
        let measured = egui::Id::new(("arca-modal-content", kind as u8));
        let wanted = ctx
            .data(|data| data.get_temp::<f32>(measured))
            .unwrap_or(0.0)
            .min(tallest);
        let response = modal.show(ctx, |ui| {
            ui.multiply_opacity(theme::appear(ui.ctx(), fade));
            ui.set_width(width);
            if kind == ModalKind::Settings {
                ui.set_height((ctx.content_rect().height() - 64.0).clamp(320.0, 760.0));
                self.settings_dialog(ui);
                return;
            }
            if kind == ModalKind::Search {
                self.search_dialog(ui);
                return;
            }
            let shown = egui::ScrollArea::vertical()
                .max_height(tallest)
                .min_scrolled_height(wanted.max(64.0))
                .auto_shrink([false, true])
                .show(ui, |ui| match kind {
                    ModalKind::Password => self.password_dialog(ui),
                    ModalKind::Conflict => self.conflict_dialog(ui),
                    ModalKind::Delete => self.delete_dialog(ui),
                    ModalKind::Drop => self.drop_dialog(ui),
                    ModalKind::Add => self.add_dialog(ui),
                    ModalKind::Settings | ModalKind::Search => {}
                    ModalKind::Shortcuts => {
                        let s = self.controller.s();
                        heading(ui, s.shortcuts_title);
                        ui.add_space(8.0);
                        shortcuts_table(ui, s);
                    }
                    ModalKind::NewFolder | ModalKind::Mask => self.name_dialog(ui, kind),
                    ModalKind::Properties => self.properties_dialog(ui),
                });
            ui.ctx()
                .data_mut(|data| data.insert_temp(measured, shown.content_size.y));
        });
        if response.should_close() && self.modal_kind() == Some(kind) {
            self.cancel_modal(kind);
        }
    }

    fn footer_buttons(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
        ui.add_space(12.0);
        let height = ui.spacing().interact_size.y + 8.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::right_to_left(egui::Align::Center),
            add,
        );
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
            super::widgets::field(ui, &mut value)
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
        let rows = [
            vec![
                (s.rename, Answer::Rename, false),
                (s.no, Answer::Skip, false),
                (s.yes, Answer::Replace, true),
            ],
            vec![
                (s.cancel, Answer::Cancel, false),
                (s.rename_all, Answer::RenameAll, false),
                (s.no_all, Answer::SkipAll, false),
                (s.yes_all, Answer::ReplaceAll, false),
            ],
        ];
        for row in rows {
            Self::footer_buttons(ui, |ui| {
                for (label, choice, primary) in row.into_iter().rev() {
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
        }
        if let Some(answer) = answer {
            self.answer_conflict(answer);
        }
    }

    fn properties_dialog(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let Some(props) = &self.properties else {
            return;
        };
        heading(ui, &props.name);
        ui.add_space(6.0);
        let kind = if props.is_dir {
            s.folder_word
        } else {
            s.file_word
        };
        let size = if props.capped {
            format!("≥ {}", pretty_bytes(props.size))
        } else {
            pretty_bytes(props.size)
        };
        let items = if props.capped {
            format!("≥ {}", props.items)
        } else {
            props.items.to_string()
        };
        let location = props
            .path
            .parent()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        let mut rows: Vec<(&str, String)> = vec![
            (s.col_type, kind.to_string()),
            (s.location_word, location),
            (s.col_size, size),
        ];
        // The footer's "items" is lower case; a row label leads with a capital.
        let items_label = capitalised(s.items_word);
        if props.is_dir {
            rows.push((&items_label, items));
        }
        if let Some(when) = props.modified {
            rows.push((s.col_modified, stamp(when)));
        }
        egui::Grid::new("arca-properties")
            .num_columns(2)
            .spacing(egui::vec2(18.0, 8.0))
            .show(ui, |ui| {
                for (label, value) in rows {
                    ui.label(RichText::new(label).color(hex(p.muted)));
                    ui.add(egui::Label::new(value).wrap());
                    ui.end_row();
                }
            });
        let mut close = false;
        Self::footer_buttons(ui, |ui| {
            close = secondary_button(ui, s.close_word, true).clicked();
        });
        if close {
            self.properties = None;
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
            super::widgets::field(ui, &mut self.controller.state.output_name)
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        self.pickers(ui);
        let format = self.controller.state.format;
        if matches!(format, Format::Zip | Format::SevenZ) {
            ui.add_space(6.0);
            ui.add(
                super::widgets::field(ui, &mut self.controller.state.add_password)
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
            ui.add(super::widgets::field(ui, &mut self.name_value).desired_width(f32::INFINITY));
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
            .filter(|(section, label, _)| {
                query.is_empty()
                    || std::iter::once(*label)
                        .chain(section_words(*section, s))
                        .any(|word| word.to_lowercase().contains(&query))
            })
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
        let size = egui::vec2(ui.available_width(), ui.available_height());
        let (whole, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let compact = whole.width() < 700.0;
        let nav_width = if compact { 52.0 } else { 220.0 };
        let nav = egui::Rect::from_min_size(whole.min, egui::vec2(nav_width, whole.height()));
        ui.painter().rect_filled(
            nav,
            egui::CornerRadius {
                nw: 9,
                sw: 9,
                ne: 0,
                se: 0,
            },
            theme::alpha(p.background, 0.35),
        );
        ui.painter().vline(
            nav.right(),
            nav.y_range(),
            egui::Stroke::new(1.0, theme::alpha(p.border, 0.6)),
        );
        let inset = if compact {
            egui::vec2(8.0, 12.0)
        } else {
            egui::vec2(12.0, 18.0)
        };
        let mut rail = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(nav.shrink2(inset))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        rail.spacing_mut().item_spacing.y = 2.0;
        if compact {
            rail.menu_button(theme::icon_text(icon::MAGNIFYING_GLASS).size(16.0), |ui| {
                ui.add(
                    super::widgets::field(ui, &mut self.settings_query)
                        .hint_text(s.settings_search)
                        .desired_width(200.0),
                );
            })
            .response
            .on_hover_text(s.settings_search);
        } else {
            rail.add(
                super::widgets::field(ui, &mut self.settings_query)
                    .hint_text(format!("{}  {}", icon::MAGNIFYING_GLASS, s.settings_search))
                    .margin(egui::vec2(8.0, 5.0))
                    .desired_width(f32::INFINITY),
            );
        }
        rail.add_space(10.0);
        if matching.is_empty() && !compact {
            rail.label(RichText::new(s.settings_no_match).color(hex(p.muted)));
        }
        for (section, label, glyph) in &matching {
            let on = self.settings_section == *section;
            if nav_item(&mut rail, glyph, label, on, compact).clicked() {
                self.settings_section = *section;
            }
        }
        let page = egui::Rect::from_min_max(egui::pos2(nav.right() + 1.0, whole.top()), whole.max);
        let mut body = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(page)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let title = sections
            .iter()
            .find(|(section, _, _)| *section == self.settings_section)
            .map_or(s.settings, |(_, label, _)| *label);
        let bar = body
            .allocate_ui_with_layout(
                egui::vec2(page.width(), 44.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new(title).font(theme::display(15.0)).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if icon_button(ui, icon::X, s.cancel, true).clicked() {
                            self.cancel_modal(ModalKind::Settings);
                        }
                    });
                },
            )
            .response
            .rect;
        body.painter().hline(
            page.x_range(),
            bar.bottom(),
            egui::Stroke::new(1.0, theme::alpha(p.border, 0.5)),
        );
        egui::ScrollArea::vertical()
            .id_salt("settings-page")
            .auto_shrink(false)
            .show(&mut body, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 24,
                        right: 24,
                        top: 18,
                        bottom: 26,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.settings_page(ui);
                    });
            });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        match self.settings_section {
            SettingsSection::General => self.settings_general(ui),
            SettingsSection::Archives => {
                section_title(ui, s.defaults_title);
                card(ui, s.defaults_title, None, false, |ui| self.pickers(ui));
                let mut on = self.controller.state.into_subfolder;
                card(ui, s.into_subfolder, Some(s.subfolder_hint), true, |ui| {
                    if switch(ui, &mut on, s.into_subfolder).changed() {
                        self.settings_activate(SettingsControl::Subfolder);
                    }
                });
            }
            SettingsSection::Appearance => self.settings_appearance(ui),
            SettingsSection::Views => self.settings_views(ui),
            SettingsSection::Keybindings => shortcuts_table(ui, s),
            SettingsSection::Updates => self.settings_updates(ui),
            SettingsSection::About => {
                egui::Frame::new()
                    .fill(theme::alpha(p.background, 0.55))
                    .stroke(egui::Stroke::new(1.0, theme::alpha(p.border, 0.6)))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Image::from_bytes(
                                    "bytes://arca-mark.svg",
                                    crate::assets::MARK,
                                )
                                .fit_to_exact_size(egui::Vec2::splat(56.0)),
                            );
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!("Arca {}", crate::controller::VERSION))
                                        .font(theme::display(18.0)),
                                );
                                ui.label(RichText::new(s.about_blurb).color(hex(p.muted)));
                            });
                        });
                    });
            }
        }
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        card(ui, s.language, Some(s.language_hint), false, |ui| {
            self.choices(ui, SettingsSection::General);
        });
        let page = self.controller.state.settings.page;
        let current = arca_zip::pages::Page::ALL
            .iter()
            .find(|(candidate, _, _)| *candidate == page)
            .map_or("", |(_, _, label)| *label);
        let mut chosen = page;
        card(ui, s.name_encoding, Some(s.encoding_hint), true, |ui| {
            egui::ComboBox::from_id_salt("settings-page")
                .selected_text(current)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (candidate, _, label) in arca_zip::pages::Page::ALL {
                        ui.selectable_value(&mut chosen, candidate, label);
                    }
                });
        });
        if chosen != page {
            self.controller.reread_names(chosen);
        }
        let mut hidden = self.controller.state.settings.show_hidden;
        card(ui, s.show_hidden, Some(s.hidden_hint), true, |ui| {
            if switch(ui, &mut hidden, s.show_hidden).changed() {
                self.controller.dispatch(AppAction::SetShowHidden(hidden));
            }
        });
    }

    fn settings_views(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let columns =
            self.controller.state.settings.browser_view == crate::settings::BrowserView::Columns;
        card(
            ui,
            s.default_view,
            Some(s.view_hint),
            false,
            |ui| match segmented(ui, &[(s.column_view, columns), (s.details_view, !columns)]) {
                Some(0) if !columns => {
                    self.switch_view(crate::settings::BrowserView::Columns, false)
                }
                Some(1) if columns => self.switch_view(
                    crate::settings::BrowserView::Details,
                    self.controller.state.settings.flat,
                ),
                _ => {}
            },
        );
        for (label, hint, pick) in [
            (
                s.show_preview_panel,
                s.preview_hint,
                (|settings| &mut settings.preview_visible) as fn(&mut crate::Settings) -> &mut bool,
            ),
            (s.show_folder_tree, s.tree_hint, |settings| {
                &mut settings.folders
            }),
            (s.icon_sidebar, s.rail_hint, |settings| {
                &mut settings.sidebar_collapsed
            }),
        ] {
            card(ui, label, Some(hint), true, |ui| {
                let settings = &mut self.controller.state.settings;
                if switch(ui, pick(settings), label).changed() {
                    settings.save();
                }
            });
        }
    }

    fn choices(&mut self, ui: &mut egui::Ui, section: SettingsSection) {
        let s = self.controller.s();
        let lang = self.controller.state.settings.lang;
        let theme = self.controller.state.settings.theme;
        let mut shown = Vec::new();
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
            if owner == section {
                shown.push((control, label, on));
            }
        }
        let labels: Vec<_> = shown.iter().map(|(_, label, on)| (*label, *on)).collect();
        if let Some(index) = segmented(ui, &labels) {
            let (control, _, on) = shown[index];
            if !on {
                self.settings_activate(control);
            }
        }
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        use super::themes::{self, Theme};
        use crate::settings::FontChoice;
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let dark_now = ui.visuals().dark_mode;
        section_title(ui, s.theme_section);
        card(ui, s.theme, Some(s.theme_hint), false, |ui| {
            self.choices(ui, SettingsSection::Appearance);
        });
        let settings = &self.controller.state.settings;
        let current = if dark_now {
            settings.dark_theme.clone()
        } else {
            settings.light_theme.clone()
        };
        card(
            ui,
            s.current_theme,
            Some(s.current_theme_hint),
            true,
            |ui| {
                ui.label(egui::RichText::new(&current).strong());
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(66.0, 22.0), egui::Sense::hover());
                paint_swatch(ui, rect, p);
            },
        );

        ui.add_space(12.0);
        section_title(ui, s.library_section);
        ui.horizontal(|ui| {
            ui.add(
                super::widgets::field(ui, &mut self.theme_query)
                    .hint_text(s.search_themes)
                    .background_color(hex(p.raised))
                    .desired_width(240.0),
            );
            let f = self.theme_filter;
            if let Some(i) = segmented(
                ui,
                &[
                    (s.filter_all, f == 0),
                    (s.kind_light, f == 1),
                    (s.kind_dark, f == 2),
                ],
            ) {
                self.theme_filter = i;
            }
        });
        ui.add_space(6.0);
        let query = self.theme_query.to_lowercase();
        let filter = self.theme_filter;
        let shown: Vec<Theme> = self
            .themes
            .iter()
            .filter(|t| match filter {
                1 => !t.dark,
                2 => t.dark,
                _ => true,
            })
            .filter(|t| t.name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        let (dark_name, light_name) = (settings.dark_theme.clone(), settings.light_theme.clone());
        let mut pick = None;
        let mut remove = None;
        let mut add = false;
        let folder = themes::folder()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        egui::Frame::new()
            .fill(theme::alpha(p.background, 0.55))
            .stroke(egui::Stroke::new(1.0, theme::alpha(p.border, 0.6)))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("theme-library")
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for t in &shown {
                            let active = t.dark == dark_now
                                && &t.name == if t.dark { &dark_name } else { &light_name };
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 40.0),
                                egui::Sense::click(),
                            );
                            let lit = ui
                                .ctx()
                                .animate_bool(response.id, response.hovered() || active);
                            let painter = ui.painter();
                            painter.rect_filled(
                                rect.shrink(1.0),
                                4,
                                theme::alpha(p.focus, 0.03 + 0.11 * lit),
                            );
                            let swatch = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + 12.0, rect.center().y - 11.0),
                                egui::vec2(66.0, 22.0),
                            );
                            paint_swatch(ui, swatch, t.palette);
                            let painter = ui.painter();
                            painter.text(
                                egui::pos2(rect.left() + 92.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &t.name,
                                egui::FontId::proportional(13.0),
                                hex(if active { p.focus } else { p.text }),
                            );
                            let kind = if t.dark { s.kind_dark } else { s.kind_light };
                            painter.text(
                                egui::pos2(rect.right() - 16.0, rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                kind.to_uppercase(),
                                egui::FontId::proportional(10.5),
                                hex(p.muted),
                            );
                            if t.custom {
                                let bin = egui::Rect::from_center_size(
                                    egui::pos2(rect.right() - 76.0, rect.center().y),
                                    egui::vec2(24.0, 24.0),
                                );
                                let del = ui
                                    .interact(bin, response.id.with("delete"), egui::Sense::click())
                                    .on_hover_text(s.delete_word);
                                ui.painter().text(
                                    bin.center(),
                                    egui::Align2::CENTER_CENTER,
                                    egui_phosphor::regular::TRASH,
                                    theme::icon_font(14.0),
                                    hex(if del.hovered() { p.danger } else { p.muted }),
                                );
                                if del.clicked() {
                                    remove = Some(t.name.clone());
                                    continue;
                                }
                            }
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::RadioButton,
                                    true,
                                    active,
                                    &t.name,
                                )
                            });
                            if response.clicked() {
                                pick = Some(t.clone());
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(fill(s.custom_themes_in, &[("path", &folder)]))
                            .small()
                            .color(hex(p.muted)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        add = secondary_button(ui, &format!("+  {}", s.add_theme), true).clicked();
                    });
                });
            });
        if add && self.theme_draft.is_none() {
            self.theme_draft = Some(Theme {
                name: String::new(),
                dark: dark_now,
                palette: p,
                custom: true,
            });
        }
        if let Some(mut draft) = self.theme_draft.take() {
            let mut keep = true;
            let mut save = false;
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(theme::alpha(p.background, 0.55))
                .stroke(egui::Stroke::new(1.0, theme::alpha(p.border, 0.6)))
                .corner_radius(6)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(s.add_theme).strong());
                    ui.add_space(4.0);
                    ui.add(
                        super::widgets::field(ui, &mut draft.name)
                            .hint_text(s.theme_name)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(4.0);
                    if let Some(i) = segmented(
                        ui,
                        &[(s.kind_dark, draft.dark), (s.kind_light, !draft.dark)],
                    ) {
                        draft.dark = i == 0;
                    }
                    ui.add_space(6.0);
                    let per_column = theme::Palette::KEYS.len().div_ceil(3);
                    ui.columns(3, |columns| {
                        for (i, key) in theme::Palette::KEYS.iter().enumerate() {
                            let ui = &mut columns[i / per_column];
                            let Some(slot) = draft.palette.slot(key) else {
                                continue;
                            };
                            let mut rgb = [(*slot >> 16) as u8, (*slot >> 8) as u8, *slot as u8];
                            let line = egui::vec2(ui.available_width(), 28.0);
                            ui.allocate_ui_with_layout(
                                line,
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                                        *slot = (rgb[0] as u32) << 16
                                            | (rgb[1] as u32) << 8
                                            | rgb[2] as u32;
                                    }
                                    ui.label(s.palette_labels[i]);
                                },
                            );
                        }
                    });
                    let taken = themes::builtin_name(draft.name.trim());
                    if taken {
                        ui.add_space(4.0);
                        ui.colored_label(hex(p.danger), s.theme_name_taken);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let ready = !draft.name.trim().is_empty() && !taken;
                        save = primary_button(ui, s.save_theme, ready).clicked();
                        if secondary_button(ui, s.cancel, true).clicked() {
                            keep = false;
                        }
                    });
                });
            if save {
                draft.name = draft.name.trim().to_string();
                if themes::save(&draft).is_ok() {
                    self.themes = themes::all();
                    pick = Some(draft.clone());
                    self.look.clear();
                }
                keep = false;
            }
            if keep {
                self.theme_draft = Some(draft);
            }
        }
        if let Some(name) = remove {
            let _ = themes::delete(&name);
            self.themes = themes::all();
            self.look.clear();
            let settings = &mut self.controller.state.settings;
            let fresh = crate::settings::Settings::default();
            if settings.dark_theme == name {
                settings.dark_theme = fresh.dark_theme;
            }
            if settings.light_theme == name {
                settings.light_theme = fresh.light_theme;
            }
            settings.save();
        }
        if let Some(t) = pick {
            let settings = &mut self.controller.state.settings;
            if t.dark {
                settings.dark_theme = t.name.clone();
            } else {
                settings.light_theme = t.name.clone();
            }
            settings.save();
            let follow = settings.theme == ThemePreference::System && dark_now == t.dark;
            if !follow {
                self.set_theme(if t.dark {
                    ThemePreference::Dark
                } else {
                    ThemePreference::Light
                });
            }
        }

        ui.add_space(12.0);
        section_title(ui, s.text_section);
        let font = self.controller.state.settings.font.clone();
        let mut font_pick = None;
        let mut from_file = false;
        card(ui, s.font_title, Some(s.font_hint), false, |ui| {
            font_pick = segmented(
                ui,
                &[
                    ("Inter", font == FontChoice::Inter),
                    ("Sora", font == FontChoice::Sora),
                    ("Hack", font == FontChoice::Mono),
                    ("Ubuntu", font == FontChoice::Ubuntu),
                ],
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                from_file = secondary_button(ui, s.font_file, true).clicked();
                if let FontChoice::File(path) = &font {
                    ui.label(egui::RichText::new(path).small().color(hex(p.muted)));
                }
            });
        });
        let chosen = match font_pick {
            Some(0) => Some(FontChoice::Inter),
            Some(1) => Some(FontChoice::Sora),
            Some(2) => Some(FontChoice::Mono),
            Some(3) => Some(FontChoice::Ubuntu),
            _ if from_file => rfd::FileDialog::new()
                .add_filter("Fonts", &["ttf", "otf", "ttc"])
                .pick_file()
                .map(|path| FontChoice::File(path.to_string_lossy().into_owned())),
            _ => None,
        };
        let size = self.controller.state.settings.text_size;
        let mut size_step = None;
        let hint = fill(
            s.text_size_hint,
            &[
                ("least", &format!("{TEXT_LEAST:.0}")),
                ("most", &format!("{TEXT_MOST:.0}")),
            ],
        );
        card(ui, s.text_size_title, Some(&hint), true, |ui| {
            size_step = stepper(
                ui,
                &format!("{size:.0}"),
                size > TEXT_LEAST,
                size < TEXT_MOST,
            );
        });
        ui.add_space(12.0);
        section_title(ui, s.motion_section);
        let mut motion = self.controller.state.settings.motion;
        let mut motion_changed = false;
        card(ui, s.motion_title, Some(s.motion_hint), true, |ui| {
            motion_changed = switch(ui, &mut motion, s.motion_title).changed();
        });
        ui.add_space(12.0);
        section_title(ui, s.title_bar_title);
        let custom = self.controller.state.settings.title_bar == TitleBar::Custom;
        let mut bar_pick = None;
        card(ui, s.title_bar_title, Some(s.title_bar_hint), false, |ui| {
            bar_pick = segmented(
                ui,
                &[(s.title_bar_system, !custom), (s.title_bar_custom, custom)],
            );
        });
        let settings = &mut self.controller.state.settings;
        let mut dirty = false;
        if let Some(font) = chosen {
            settings.font = font;
            dirty = true;
        }
        if let Some(step) = size_step {
            settings.text_size = (size + f32::from(step)).clamp(TEXT_LEAST, TEXT_MOST);
            dirty = true;
        }
        if motion_changed {
            settings.motion = motion;
            dirty = true;
        }
        if let Some(i) = bar_pick {
            settings.title_bar = if i == 0 {
                TitleBar::System
            } else {
                TitleBar::Custom
            };
            dirty = true;
        }
        if dirty {
            settings.save();
        }
    }

    fn settings_updates(&mut self, ui: &mut egui::Ui) {
        use crate::controller::UpdateCheck;
        use crate::settings::Channel;
        let s = self.controller.s();
        let state = &self.controller.state;
        let found = state.update.clone();
        let status = match (state.update_check, &found) {
            (UpdateCheck::Checking, _) => s.checking.to_string(),
            (UpdateCheck::Failed, _) => s.check_failed.to_string(),
            (_, Some(release)) => fill(s.update_ready, &[("version", &release.tag)]),
            (UpdateCheck::Current, None) => s.up_to_date.to_string(),
            _ => s.updates_hint.to_string(),
        };
        let description = format!("Arca {} \u{b7} {status}", crate::controller::VERSION);
        let busy = state.update_check == UpdateCheck::Checking;
        section_title(ui, s.status_section);
        let mut check = false;
        let mut install = false;
        card(ui, s.update_status, Some(&description), true, |ui| {
            if found.is_some() {
                let label = fill(
                    s.update_ready,
                    &[("version", found.as_ref().map_or("", |r| &r.tag))],
                );
                install = primary_button(ui, &label, true).clicked();
            } else {
                let label = if busy { s.checking } else { s.check_now };
                check = secondary_button(ui, label, !busy).clicked();
            }
        });
        if check {
            self.controller.check_for_updates();
        }
        if install {
            self.controller.start_update();
        }
        ui.add_space(12.0);
        section_title(ui, s.preferences_section);
        let mut on = self.controller.state.settings.updates;
        let mut toggled = false;
        card(ui, s.check_updates, Some(s.updates_hint), true, |ui| {
            toggled = switch(ui, &mut on, s.check_updates).changed();
        });
        if toggled {
            self.settings_activate(SettingsControl::Updates);
        }
        let preview = self.controller.state.settings.channel == Channel::Preview;
        let hint = if preview {
            s.channel_preview_hint
        } else {
            s.channel_stable_hint
        };
        let mut pick = None;
        card(ui, s.channel_title, Some(hint), false, |ui| {
            pick = segmented(
                ui,
                &[(s.channel_stable, !preview), (s.channel_preview, preview)],
            );
        });
        let channel = match pick {
            Some(0) if preview => Some(Channel::Stable),
            Some(1) if !preview => Some(Channel::Preview),
            _ => None,
        };
        if let Some(channel) = channel {
            let controller = &mut self.controller;
            controller.state.settings.channel = channel;
            controller.state.settings.save();
            controller.state.update = None;
            controller.state.update_check = UpdateCheck::Idle;
            if controller.state.settings.updates {
                controller.check_for_updates();
            }
        }
        if let Some(release) = &found {
            ui.add_space(12.0);
            section_title(ui, s.notes_section);
            let title = fill(s.whats_new, &[("version", &release.tag)]);
            let mut open = false;
            card(ui, &title, None, true, |ui| {
                open = secondary_button(ui, s.open_notes, true).clicked();
            });
            if open {
                self.controller.open_release_page();
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

fn paint_swatch(ui: &egui::Ui, rect: egui::Rect, palette: theme::Palette) {
    let painter = ui.painter();
    painter.rect(
        rect,
        5,
        hex(palette.background),
        egui::Stroke::new(1.0, hex(palette.border)),
        egui::StrokeKind::Inside,
    );
    for (i, color) in [palette.focus, palette.danger, palette.muted, palette.text]
        .into_iter()
        .enumerate()
    {
        painter.circle_filled(
            egui::pos2(rect.left() + 12.0 + i as f32 * 14.0, rect.center().y),
            4.0,
            hex(color),
        );
    }
}

fn pretty_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Local-agnostic `YYYY-MM-DD HH:MM` from a system time, without a dates
/// crate: days to civil date after Howard Hinnant.
fn stamp(when: std::time::SystemTime) -> String {
    let secs = when
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        (rest % 3600) / 60
    )
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
