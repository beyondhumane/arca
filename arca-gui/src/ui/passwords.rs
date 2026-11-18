//! Settings > Passwords, and the rows the password box gains from it.

use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use crate::vault::{Saved, LOCK_AFTER};
use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use zeroize::Zeroizing;

#[derive(Default)]
pub(super) struct PasswordsPage {
    master: Zeroizing<String>,
    again: Zeroizing<String>,
    changing: bool,
    setting_aside: bool,
    message: Option<(String, bool)>,
    draft: Option<Draft>,
    shown: Option<usize>,
    prompt_master: Zeroizing<String>,
    prompt_error: Option<String>,
    was_unlocked: bool,
}

#[derive(Default)]
struct Draft {
    editing: Option<usize>,
    label: String,
    mask: String,
    password: Zeroizing<String>,
    auto: bool,
    session: bool,
    show: bool,
}

pub(super) const MASTER_FIELD: &str = "arca-vault-master";
pub(super) const AGAIN_FIELD: &str = "arca-vault-again";
pub(super) const PROMPT_MASTER_FIELD: &str = "arca-vault-prompt-master";
pub(super) const SAVED_PASSWORD_FIELD: &str = "arca-saved-password";

fn secret_field<'t>(
    ui: &egui::Ui,
    text: &'t mut String,
    hint: &str,
    id: &str,
) -> egui::TextEdit<'t> {
    field(ui, text)
        .id(egui::Id::new(id))
        .password(true)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
}

impl PasswordsPage {
    fn clear_master(&mut self) {
        self.master = Zeroizing::default();
        self.again = Zeroizing::default();
    }

    fn follow_lock(&mut self, unlocked: bool) {
        if unlocked == self.was_unlocked {
            return;
        }
        self.was_unlocked = unlocked;
        self.shown = None;
        self.clear_master();
        self.prompt_master = Zeroizing::default();
        if !unlocked {
            self.changing = false;
        }
        if !unlocked || self.draft.as_ref().is_some_and(|d| d.editing.is_some()) {
            self.draft = None;
        }
    }

    fn said(&mut self, result: Result<(), String>, done: Option<&str>) -> bool {
        match result {
            Ok(()) => {
                self.message = done.map(|text| (text.to_string(), false));
                true
            }
            Err(error) => {
                self.message = Some((error, true));
                false
            }
        }
    }
}

impl Shell {
    /// Lets go of whatever the page holds for the vault once it locks or
    /// unlocks, whether or not the page is on screen when it happens.
    pub(super) fn follow_vault(&mut self) {
        let unlocked = self.controller.state.saved.keyring.unlocked();
        self.passwords.follow_lock(unlocked);
    }

    pub(super) fn settings_passwords(&mut self, ui: &mut egui::Ui) {
        self.follow_vault();
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let keyring = &self.controller.state.saved.keyring;
        let (unlocked, has_file, can_keep) =
            (keyring.unlocked(), keyring.has_file(), keyring.can_keep());
        let minutes = (LOCK_AFTER.as_secs() / 60).to_string();
        let status = if unlocked {
            fill(s.vault_state_open, &[("minutes", &minutes)])
        } else if has_file {
            s.vault_state_locked.to_string()
        } else {
            s.vault_state_none.to_string()
        };
        section_title(ui, s.vault_title);
        card(ui, s.vault_title, Some(s.vault_about), false, |ui| {
            ui.label(RichText::new(&status).color(hex(p.muted)));
            ui.add_space(6.0);
            if unlocked {
                self.vault_open(ui);
            } else if has_file {
                self.vault_locked(ui);
            } else if can_keep {
                self.vault_new(ui);
            }
        });
        if let Some((text, error)) = &self.passwords.message {
            let color = if *error { p.danger } else { p.muted };
            ui.label(RichText::new(text).color(hex(color)));
            ui.add_space(6.0);
        }
        ui.add_space(12.0);
        section_title(ui, s.saved_title);
        self.saved_list(ui);
        if self.passwords.draft.is_some() {
            self.draft_card(ui);
        } else if secondary_button(ui, s.saved_add, true).clicked() {
            self.passwords.draft = Some(Draft {
                session: !self.controller.state.saved.keyring.unlocked(),
                ..Default::default()
            });
        }
    }

    fn vault_new(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let page = &mut self.passwords;
        let edit = secret_field(ui, &mut page.master, s.master_password, MASTER_FIELD);
        let first = secret_edit(ui, edit);
        named(&first, s.master_password);
        let edit = secret_field(ui, &mut page.again, s.master_again, AGAIN_FIELD);
        let second = secret_edit(ui, edit);
        named(&second, s.master_again);
        ui.add_space(6.0);
        let ready = !page.master.is_empty();
        if primary_button(ui, s.master_set, ready).clicked() {
            if *page.master != *page.again {
                page.message = Some((s.master_mismatch.to_string(), true));
                return;
            }
            let master = std::mem::take(&mut page.master);
            page.clear_master();
            let result = self.controller.create_vault(&master);
            self.passwords.said(result, None);
        }
    }

    fn vault_locked(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let page = &mut self.passwords;
        let edit = secret_field(ui, &mut page.master, s.master_password, MASTER_FIELD);
        let typed = secret_edit(ui, edit);
        named(&typed, s.master_password);
        let entered = typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        let mut unlock = entered;
        let mut aside = false;
        ui.horizontal(|ui| {
            unlock |= primary_button(ui, s.unlock_word, !page.master.is_empty()).clicked();
            if !page.setting_aside {
                page.setting_aside = secondary_button(ui, s.vault_set_aside, true).clicked();
            }
        });
        if page.setting_aside {
            ui.add_space(6.0);
            muted(ui, s.vault_set_aside_hint);
            ui.label(s.vault_set_aside_confirm);
            ui.horizontal(|ui| {
                aside = danger_button(ui, s.vault_set_aside.trim_end_matches('…')).clicked();
                if secondary_button(ui, s.cancel, true).clicked() {
                    page.setting_aside = false;
                }
            });
        }
        if unlock && !page.master.is_empty() {
            let master = std::mem::take(&mut page.master);
            let result = self.controller.unlock_vault(&master);
            self.passwords.said(result, None);
        } else if aside {
            self.passwords.setting_aside = false;
            let result = self.controller.state.saved.keyring.set_aside();
            let s = self.controller.s();
            self.passwords.message = Some(match result {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    (fill(s.vault_set_aside_done, &[("name", &name)]), false)
                }
                Err(error) => (crate::controller::vault_error(s, &error), true),
            });
        }
    }

    fn vault_open(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let mut lock = false;
        ui.horizontal(|ui| {
            lock = secondary_button(ui, s.lock_word, true).clicked();
            if !self.passwords.changing {
                self.passwords.changing = secondary_button(ui, s.master_change, true).clicked();
            }
        });
        if lock {
            self.controller.lock_vault();
            self.passwords.changing = false;
            self.passwords.draft = None;
            self.passwords.shown = None;
            return;
        }
        if !self.passwords.changing {
            return;
        }
        ui.add_space(6.0);
        let page = &mut self.passwords;
        let edit = secret_field(ui, &mut page.master, s.master_password, MASTER_FIELD);
        let first = secret_edit(ui, edit);
        named(&first, s.master_password);
        let edit = secret_field(ui, &mut page.again, s.master_again, AGAIN_FIELD);
        let second = secret_edit(ui, edit);
        named(&second, s.master_again);
        let mut save = false;
        ui.horizontal(|ui| {
            save = primary_button(ui, s.save_word, !page.master.is_empty()).clicked();
            if secondary_button(ui, s.cancel, true).clicked() {
                page.changing = false;
                page.clear_master();
            }
        });
        if save {
            if *page.master != *page.again {
                page.message = Some((s.master_mismatch.to_string(), true));
                return;
            }
            let master = std::mem::take(&mut page.master);
            page.clear_master();
            page.changing = false;
            let result = self
                .controller
                .vault_do(|keyring| keyring.change_master(&master));
            self.passwords.said(result, None);
        }
    }

    fn saved_list(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let unlocked = self.controller.state.saved.keyring.unlocked();
        let entries = &self.controller.state.saved.keyring.entries;
        if entries.is_empty() {
            muted(ui, s.saved_empty);
            ui.add_space(8.0);
            return;
        }
        let mut edit = None;
        let mut remove = None;
        let mut toggle = None;
        for (i, entry) in entries.iter().enumerate() {
            let title = if entry.label.is_empty() {
                entry.mask.as_str()
            } else {
                entry.label.as_str()
            };
            let mut about = vec![if entry.mask.is_empty() {
                s.every_archive.to_string()
            } else {
                entry.mask.clone()
            }];
            if entry.auto {
                about.push(s.try_without_asking.to_string());
            }
            if entry.session {
                about.push(s.session_only.to_string());
            }
            let about = about.join(" \u{b7} ");
            let shown = self.passwords.shown == Some(i);
            let can_change = unlocked || entry.session;
            card(ui, title, Some(&about), true, |ui| {
                if can_change {
                    if icon_button(ui, icon::TRASH, s.delete_word, true).clicked() {
                        remove = Some(i);
                    }
                    if icon_button(ui, icon::PENCIL_SIMPLE, s.edit_word, true).clicked() {
                        edit = Some(i);
                    }
                }
                let (glyph, tip) = if shown {
                    (icon::EYE_SLASH, s.hide_word)
                } else {
                    (icon::EYE, s.show_password_word)
                };
                if icon_button(ui, glyph, tip, true).clicked() {
                    toggle = Some(i);
                }
                if shown {
                    ui.add(
                        egui::Label::new(
                            RichText::new(entry.password.as_str())
                                .font(super::theme::mono_font(ui.style(), 0.0))
                                .color(hex(p.text)),
                        )
                        .selectable(false),
                    );
                }
            });
        }
        if let Some(i) = toggle {
            self.passwords.shown = (self.passwords.shown != Some(i)).then_some(i);
        }
        if let Some(i) = edit {
            self.edit_saved(i);
        }
        if let Some(i) = remove {
            self.passwords.shown = None;
            self.passwords.draft = None;
            let result = self.controller.vault_do(|keyring| keyring.remove(i));
            self.passwords.said(result, None);
        }
    }

    fn draft_card(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let unlocked = self.controller.state.saved.keyring.unlocked();
        let Some(draft) = self.passwords.draft.as_mut() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let title = if draft.editing.is_some() {
            s.edit_word
        } else {
            s.saved_add
        };
        card(ui, title, None, false, |ui| {
            let label = ui.add(
                field(ui, &mut draft.label)
                    .hint_text(s.entry_label)
                    .desired_width(f32::INFINITY),
            );
            named(&label, s.entry_label);
            ui.add_space(4.0);
            let mask = ui.add(
                field(ui, &mut draft.mask)
                    .hint_text(s.entry_mask)
                    .desired_width(f32::INFINITY),
            );
            named(&mask, s.entry_mask);
            muted(ui, s.entry_mask_hint);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (glyph, tip) = if draft.show {
                    (icon::EYE_SLASH, s.hide_word)
                } else {
                    (icon::EYE, s.show_password_word)
                };
                if icon_button(ui, glyph, tip, true).clicked() {
                    draft.show = !draft.show;
                }
                let edit = secret_field(
                    ui,
                    &mut draft.password,
                    s.password_word,
                    SAVED_PASSWORD_FIELD,
                )
                .password(!draft.show);
                let secret = secret_edit(ui, edit);
                named(&secret, s.password_word);
            });
            ui.add_space(6.0);
            switch(ui, &mut draft.auto, s.try_without_asking);
            muted(ui, s.try_without_asking_hint);
            ui.add_space(4.0);
            if !unlocked {
                draft.session = true;
            }
            ui.add_enabled_ui(unlocked, |ui| {
                switch(ui, &mut draft.session, s.session_only);
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                save = primary_button(ui, s.save_word, !draft.password.is_empty()).clicked();
                cancel = secondary_button(ui, s.cancel, true).clicked();
            });
        });
        if cancel {
            self.passwords.draft = None;
        } else if save {
            self.save_draft();
        }
    }

    fn edit_saved(&mut self, i: usize) {
        let Some(entry) = self.controller.state.saved.keyring.entries.get(i) else {
            return;
        };
        self.passwords.draft = Some(Draft {
            editing: Some(i),
            label: entry.label.clone(),
            mask: entry.mask.clone(),
            password: entry.password.clone(),
            auto: entry.auto,
            session: entry.session,
            show: false,
        });
    }

    fn save_draft(&mut self) {
        let Some(draft) = self.passwords.draft.take() else {
            return;
        };
        let entry = Saved {
            label: draft.label.trim().to_string(),
            mask: draft.mask.trim().to_string(),
            password: draft.password,
            auto: draft.auto,
            session: draft.session,
        };
        let result = self.controller.vault_do(|keyring| match draft.editing {
            Some(i) => keyring.replace(i, entry),
            None => keyring.add(entry),
        });
        self.passwords.shown = None;
        self.passwords.said(result, None);
    }

    /// What the password box shows beyond its own field when it asks for
    /// a password an archive already has: saved passwords waiting to be
    /// picked, a way to unlock the vault, and the box to save what is typed.
    /// Gives back the saved password picked, if one was.
    pub(super) fn saved_password_rows(&mut self, ui: &mut egui::Ui) -> Option<usize> {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let mut picked = None;
        let offered = self.controller.offered_passwords();
        if !offered.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (i, label) in &offered {
                    let text = fill(s.use_saved, &[("label", label)]);
                    if secondary_button(ui, &text, true).clicked() {
                        picked = Some(*i);
                    }
                }
            });
        }
        let keyring = &self.controller.state.saved.keyring;
        if keyring.has_file() && !keyring.unlocked() {
            ui.add_space(8.0);
            let mut unlock = false;
            ui.horizontal(|ui| {
                let page = &mut self.passwords;
                unlock =
                    secondary_button(ui, s.unlock_word, !page.prompt_master.is_empty()).clicked();
                let edit = secret_field(
                    ui,
                    &mut page.prompt_master,
                    s.unlock_saved,
                    PROMPT_MASTER_FIELD,
                );
                let typed = secret_edit(ui, edit);
                named(&typed, s.unlock_saved);
                unlock |= typed.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !page.prompt_master.is_empty();
            });
            if let Some(error) = &self.passwords.prompt_error {
                ui.label(RichText::new(error).color(hex(p.danger)));
            }
            if unlock {
                let master = std::mem::take(&mut self.passwords.prompt_master);
                self.passwords.prompt_error = self.controller.unlock_vault(&master).err();
            }
        }
        ui.add_space(8.0);
        let mut save = self.controller.state.saved.save_typed;
        if switch(ui, &mut save, s.save_typed).changed() {
            self.controller.state.saved.save_typed = save;
        }
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Keyring;

    fn draw(shell: &mut Shell, ctx: &egui::Context) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            shell.settings_passwords(ui);
            let _ = shell.saved_password_rows(ui);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn the_page_draws_in_every_state_of_the_vault() {
        let room = crate::test_support::Room::new();
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, crate::Startup::Browse(None));
        shell.controller =
            crate::controller::AppController::new(crate::settings::Settings::default());
        let keyring = &mut shell.controller.state.saved.keyring;
        *keyring = Keyring::cheap(room.path("passwords.vault"));
        keyring
            .add(Saved {
                label: String::new(),
                mask: "*.zip".into(),
                password: Zeroizing::new("first".into()),
                auto: false,
                session: true,
            })
            .unwrap();
        draw(&mut shell, &ctx);
        shell.passwords.shown = Some(0);
        shell.passwords.draft = Some(Draft::default());
        draw(&mut shell, &ctx);
        shell.controller.create_vault("master").unwrap();
        shell.passwords.changing = true;
        draw(&mut shell, &ctx);
        shell.controller.state.saved.keyring.lock();
        shell.passwords.setting_aside = true;
        draw(&mut shell, &ctx);
        assert!(shell.controller.state.saved.keyring.entries.is_empty());
    }

    #[test]
    fn editing_one_of_two_alike_saved_passwords_changes_that_one() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, crate::Startup::Browse(None));
        shell.controller =
            crate::controller::AppController::new(crate::settings::Settings::default());
        for label in ["first", "second"] {
            shell
                .controller
                .state
                .saved
                .keyring
                .add(Saved {
                    label: label.into(),
                    mask: "*.zip".into(),
                    password: Zeroizing::new("same".into()),
                    auto: false,
                    session: true,
                })
                .unwrap();
        }
        shell.edit_saved(1);
        shell.passwords.draft.as_mut().unwrap().label = "renamed".into();
        shell.save_draft();
        let entries = &shell.controller.state.saved.keyring.entries;
        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["first", "renamed"]);
        assert!(shell.passwords.message.is_none());
    }

    struct Frames {
        ctx: egui::Context,
        time: f64,
    }

    impl Frames {
        fn new() -> Self {
            Frames {
                ctx: egui::Context::default(),
                time: 0.0,
            }
        }

        fn run(&mut self, events: Vec<egui::Event>, mut draw: impl FnMut(&mut egui::Ui)) {
            self.time += 0.1;
            let input = egui::RawInput {
                time: Some(self.time),
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            };
            let mut output = self.ctx.run_ui(input, |ui| draw(ui));
            output.textures_delta.clear();
        }

        fn idle(&mut self, frames: usize, mut draw: impl FnMut(&mut egui::Ui)) {
            for _ in 0..frames {
                self.run(Vec::new(), &mut draw);
            }
        }

        fn focus(&mut self, id: &str) {
            self.ctx
                .memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
        }
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn undo() -> egui::Event {
        key(egui::Key::Z, egui::Modifiers::COMMAND)
    }

    fn enter() -> egui::Event {
        key(egui::Key::Enter, egui::Modifiers::NONE)
    }

    fn typed(text: &str) -> egui::Event {
        egui::Event::Text(text.into())
    }

    fn shell_with_vault(room: &crate::test_support::Room, frames: &Frames) -> Shell {
        let mut shell = Shell::new(&frames.ctx, crate::Startup::Browse(None));
        shell.controller =
            crate::controller::AppController::new(crate::settings::Settings::default());
        shell.controller.state.saved.keyring = Keyring::cheap(room.path("passwords.vault"));
        shell.controller.create_vault("hunter2-master").unwrap();
        shell.controller.lock_vault();
        shell
    }

    #[test]
    fn undo_in_a_secret_field_never_brings_back_what_was_taken_out() {
        let mut frames = Frames::new();
        let mut text = String::new();
        let draw = |ui: &mut egui::Ui, text: &mut String| {
            let edit = field(ui, text).id(egui::Id::new("secret")).password(true);
            secret_edit(ui, edit);
        };
        frames.focus("secret");
        frames.run(Vec::new(), |ui| draw(ui, &mut text));
        frames.run(vec![typed("hunter2-master")], |ui| draw(ui, &mut text));
        frames.idle(30, |ui| draw(ui, &mut text));
        assert_eq!(text, "hunter2-master");
        drop(std::mem::take(&mut text));
        frames.idle(30, |ui| draw(ui, &mut text));
        frames.run(vec![undo()], |ui| draw(ui, &mut text));
        frames.idle(2, |ui| draw(ui, &mut text));
        assert_eq!(text, "");
    }

    #[test]
    fn the_master_password_is_gone_from_its_field_after_unlocking_and_locking() {
        let room = crate::test_support::Room::new();
        let mut frames = Frames::new();
        let mut shell = shell_with_vault(&room, &frames);
        frames.focus(MASTER_FIELD);
        frames.run(Vec::new(), |ui| shell.settings_passwords(ui));
        frames.run(vec![typed("hunter2-master")], |ui| {
            shell.settings_passwords(ui)
        });
        frames.idle(30, |ui| shell.settings_passwords(ui));
        frames.run(vec![enter()], |ui| shell.settings_passwords(ui));
        assert!(shell.controller.state.saved.keyring.unlocked());
        assert!(shell.passwords.master.is_empty());
        frames.idle(30, |ui| shell.settings_passwords(ui));
        shell.controller.lock_vault();
        frames.idle(3, |ui| shell.settings_passwords(ui));
        frames.focus(MASTER_FIELD);
        frames.run(Vec::new(), |ui| shell.settings_passwords(ui));
        frames.run(vec![undo()], |ui| shell.settings_passwords(ui));
        frames.idle(2, |ui| shell.settings_passwords(ui));
        assert!(shell.passwords.master.is_empty());
    }

    #[test]
    fn the_unlock_box_beside_the_archive_password_forgets_the_master() {
        let room = crate::test_support::Room::new();
        let mut frames = Frames::new();
        let mut shell = shell_with_vault(&room, &frames);
        shell.controller.state.waiting_on_password = Some(Pending::OpenArchive);
        frames.focus(PROMPT_MASTER_FIELD);
        frames.run(Vec::new(), |ui| {
            shell.saved_password_rows(ui);
        });
        frames.run(vec![typed("hunter2-master")], |ui| {
            shell.saved_password_rows(ui);
        });
        frames.idle(30, |ui| {
            shell.saved_password_rows(ui);
        });
        frames.run(vec![enter()], |ui| {
            shell.saved_password_rows(ui);
        });
        assert!(shell.controller.state.saved.keyring.unlocked());
        shell.controller.lock_vault();
        frames.idle(3, |ui| {
            shell.saved_password_rows(ui);
        });
        frames.focus(PROMPT_MASTER_FIELD);
        frames.run(vec![undo()], |ui| {
            shell.saved_password_rows(ui);
        });
        frames.idle(2, |ui| {
            shell.saved_password_rows(ui);
        });
        assert!(shell.passwords.prompt_master.is_empty());
    }

    #[test]
    fn the_archive_password_box_forgets_what_was_typed_in_it() {
        let mut frames = Frames::new();
        let mut shell = Shell::new(&frames.ctx, crate::Startup::Browse(None));
        shell.controller =
            crate::controller::AppController::new(crate::settings::Settings::default());
        shell.controller.state.waiting_on_password = Some(Pending::OpenArchive);
        let field = super::super::dialogs::ARCHIVE_PASSWORD_FIELD;
        frames.focus(field);
        frames.run(Vec::new(), |ui| shell.password_dialog(ui));
        frames.run(vec![typed("archive-secret")], |ui| {
            shell.password_dialog(ui)
        });
        frames.idle(30, |ui| shell.password_dialog(ui));
        assert_eq!(shell.controller.state.password_input, "archive-secret");
        frames.run(vec![enter()], |ui| shell.password_dialog(ui));
        assert!(shell.controller.state.waiting_on_password.is_none());
        assert!(shell.controller.state.password_input.is_empty());
        shell.controller.state.waiting_on_password = Some(Pending::OpenArchive);
        frames.idle(3, |ui| shell.password_dialog(ui));
        frames.focus(field);
        frames.run(vec![undo()], |ui| shell.password_dialog(ui));
        frames.idle(2, |ui| shell.password_dialog(ui));
        assert!(shell.controller.state.password_input.is_empty());
    }
}
