use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};

impl Shell {
    pub(super) fn preview_for_cursor(&mut self) {
        let index = self.controller.state.cursor.and_then(|cursor| {
            self.controller
                .visible_rows()
                .get(cursor)
                .and_then(|row| (!row.is_dir && !row.up).then_some(row.entry).flatten())
        });
        if index != self.controller.state.preview.index {
            if let Some(index) = index {
                self.controller.request_preview(index);
            } else {
                self.controller.cancel_preview();
            }
        }
        let generation = self.controller.state.preview.generation;
        if self
            .texture
            .as_ref()
            .is_some_and(|(cached, _)| *cached != generation)
        {
            self.texture = None;
        }
        if self.texture.is_none() {
            if let Some(decoded) = &self.controller.state.preview.image {
                let size = [decoded.width() as usize, decoded.height() as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
                let handle =
                    self.ctx
                        .load_texture("arca-preview", image, egui::TextureOptions::LINEAR);
                self.texture = Some((generation, handle));
            }
        }
    }

    pub(super) fn preview_panel(&mut self, ui: &mut egui::Ui) {
        let s = self.controller.s();
        let p = palette_of(ui.ctx());
        let entry = self.controller.state.preview.entry.clone();
        let viewed = self
            .controller
            .state
            .viewing
            .as_ref()
            .map(|view| view.name.clone());
        let name = viewed
            .or_else(|| entry.as_ref().map(|entry| entry.name.clone()))
            .map(|name| {
                let trimmed = name.trim_end_matches('/');
                trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
            })
            .unwrap_or_else(|| s.view_word.to_string());
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(&name).strong()).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, egui_phosphor::regular::X, s.close_preview, true).clicked() {
                    self.controller.state.settings.preview_visible = false;
                    self.controller.state.settings.save();
                    self.close_preview();
                }
            });
        });
        if let Some(entry) = &entry {
            let kind = std::path::Path::new(&name)
                .extension()
                .map(|ext| ext.to_string_lossy().to_uppercase())
                .unwrap_or_else(|| s.unavailable.to_string());
            egui::Grid::new("preview-facts")
                .num_columns(2)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        (s.col_size, human(entry.size)),
                        (s.col_modified, when(entry.mtime)),
                        (s.col_type, kind),
                    ] {
                        muted(ui, label);
                        ui.label(value);
                        ui.end_row();
                    }
                });
        }
        ui.separator();
        let message = match &self.controller.state.preview.status {
            PreviewStatus::Hidden => Some(s.preview_empty.to_string()),
            PreviewStatus::Empty => Some(s.preview_empty_file.to_string()),
            PreviewStatus::Loading => Some(s.preview_loading.to_string()),
            PreviewStatus::PasswordRequired { wrong } => Some(
                if *wrong {
                    s.password_wrong
                } else {
                    s.password_needed
                }
                .to_string(),
            ),
            PreviewStatus::Unsupported(reason) => {
                Some(format!("{}: {reason}", s.preview_unsupported))
            }
            PreviewStatus::Oversized { limit } => {
                Some(fill(s.too_big_to_view, &[("size", &human(*limit))]))
            }
            PreviewStatus::Error(reason) => Some(format!("{}: {reason}", s.preview_error)),
            PreviewStatus::Ready => None,
        };
        if let Some(message) = message {
            ui.horizontal(|ui| {
                if matches!(self.controller.state.preview.status, PreviewStatus::Loading) {
                    ui.spinner();
                }
                ui.label(RichText::new(message).color(hex(p.muted)));
            });
        }
        if matches!(
            self.controller.state.preview.status,
            PreviewStatus::PasswordRequired { .. }
        ) {
            let index = self.controller.state.preview.index;
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.preview_password)
                    .password(true)
                    .hint_text(s.password_needed)
                    .desired_width(f32::INFINITY),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if primary_button(ui, s.continue_word, true).clicked() || entered {
                let password = std::mem::take(&mut self.preview_password);
                if let Some(index) = index {
                    self.controller
                        .request_preview_with_password(index, Some(password));
                }
            }
        }
        let Some(view) = &self.controller.state.viewing else {
            return;
        };
        let mut look = view.look;
        ui.horizontal(|ui| {
            for (candidate, label) in [
                (Look::Text, s.as_text),
                (Look::Hex, s.as_hex),
                (Look::Picture, s.as_picture),
            ] {
                if candidate == Look::Picture && !view.picture {
                    continue;
                }
                if ui.selectable_label(look == candidate, label).clicked() {
                    look = candidate;
                }
            }
        });
        let line = ui.text_style_height(&egui::TextStyle::Monospace);
        match look {
            Look::Picture => {
                if let Some((_, texture)) = &self.texture {
                    ui.centered_and_justified(|ui| {
                        ui.add(
                            egui::Image::new(egui::load::SizedTexture::from_handle(texture))
                                .shrink_to_fit(),
                        );
                    });
                }
            }
            Look::Text => {
                egui::ScrollArea::both().auto_shrink(false).show_rows(
                    ui,
                    line,
                    view.lines.len(),
                    |ui, range| {
                        for index in range {
                            ui.label(RichText::new(&view.lines[index]).monospace());
                        }
                    },
                );
            }
            Look::Hex => {
                let bytes = view.bytes.clone();
                egui::ScrollArea::both().auto_shrink(false).show_rows(
                    ui,
                    line,
                    bytes.len().div_ceil(16),
                    |ui, range| {
                        for row in range {
                            let start = row * 16;
                            let end = (start + 16).min(bytes.len());
                            ui.label(
                                RichText::new(hex_line(start, &bytes[start..end])).monospace(),
                            );
                        }
                    },
                );
            }
        }
        if let Some(view) = &mut self.controller.state.viewing {
            view.look = look;
        }
    }
}
