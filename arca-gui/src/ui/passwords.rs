//! Settings > Passwords, and the rows the password box gains from it.

use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use crate::vault::{Saved, VaultError, LOCK_AFTER};
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
    before: Option<(String, Zeroizing<String>)>,
    label: String,
    mask: String,
    password: Zeroizing<String>,
    auto: bool,
    session: bool,
    show: bool,
}

fn secret_field<'t>(ui: &egui::Ui, text: &'t mut String, hint: &str) -> egui::TextEdit<'t> {
    field(ui, text)
        .password(true)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
}

impl PasswordsPage {
    fn clear_master(&mut self) {
        self.master = Zeroizing::default();
        self.again = Zeroizing::default();
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
    pub(super) fn settings_passwords(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let keyring = &self.controller.state.saved.keyring;
        let (unlocked, has_file, can_keep) =
            (keyring.unlocked(), keyring.has_file(), keyring.can_keep());
        if unlocked != self.passwords.was_unlocked {
            self.passwords.was_unlocked = unlocked;
            self.passwords.shown = None;
            if !unlocked {
                self.passwords.draft = None;
            }
        }
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
        let first = ui.add(secret_field(ui, &mut page.master, s.master_password));
        named(&first, s.master_password);
        let second = ui.add(secret_field(ui, &mut page.again, s.master_again));
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
        let typed = ui.add(secret_field(ui, &mut page.master, s.master_password));
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
        let first = ui.add(secret_field(ui, &mut page.master, s.master_password));
        named(&first, s.master_password);
        let second = ui.add(secret_field(ui, &mut page.again, s.master_again));
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
            let entry = &self.controller.state.saved.keyring.entries[i];
            self.passwords.draft = Some(Draft {
                before: Some((entry.mask.clone(), entry.password.clone())),
                label: entry.label.clone(),
                mask: entry.mask.clone(),
                password: entry.password.clone(),
                auto: entry.auto,
                session: entry.session,
                show: false,
            });
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
        let title = if draft.before.is_some() {
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
                let secret = ui.add(
                    field(ui, &mut *draft.password)
                        .password(!draft.show)
                        .hint_text(s.password_word)
                        .desired_width(f32::INFINITY),
                );
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
            let result = self.controller.vault_do(|keyring| match draft.before {
                Some((mask, password)) => keyring
                    .entries
                    .iter()
                    .position(|e| e.mask == mask && e.password == password)
                    .map_or(Err(VaultError::Locked), |i| keyring.replace(i, entry)),
                None => keyring.add(entry),
            });
            self.passwords.shown = None;
            self.passwords.said(result, None);
        }
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
                let typed = ui.add(secret_field(ui, &mut page.prompt_master, s.unlock_saved));
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
}
