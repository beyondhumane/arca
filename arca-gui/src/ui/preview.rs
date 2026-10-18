use super::theme::{hex, palette_of};
use super::widgets::*;
use super::*;
use eframe::egui::{self, RichText};

impl Shell {
    pub(super) fn preview_for_cursor(&mut self) {
        let index = self
            .controller
            .state
            .active_pane()
            .cursor
            .and_then(|cursor| {
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
        let on_disk = self.controller.on_disk();
        let look = self.controller.state.viewing.as_ref().map(|view| view.look);
        let picture = self
            .controller
            .state
            .viewing
            .as_ref()
            .is_some_and(|view| view.picture);
        let mut close = false;
        let mut open_outside = false;
        let mut chosen = look;
        let glyph = match &entry {
            Some(entry) => super::browser::kind_glyph(crate::tree::kind_of(&entry.name, false)),
            None => egui_phosphor::regular::EYE,
        };
        ui.horizontal(|ui| {
            ui.label(theme::icon_text(glyph).size(24.0).color(hex(p.focus)));
            ui.add_space(2.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = icon_button(ui, egui_phosphor::regular::X, s.close_preview, true).clicked();
                if let Some(look) = look {
                    if icon_toggle(
                        ui,
                        egui_phosphor::regular::ARROW_ELBOW_DOWN_LEFT,
                        s.wrap_lines,
                        look == Look::Text,
                        self.wrap_lines,
                    )
                    .clicked()
                    {
                        self.wrap_lines = !self.wrap_lines;
                    }
                    for (candidate, glyph, tip) in [
                        (Look::Picture, egui_phosphor::regular::IMAGE, s.as_picture),
                        (Look::Hex, egui_phosphor::regular::BINARY, s.as_hex),
                        (Look::Text, egui_phosphor::regular::TEXT_T, s.as_text),
                    ] {
                        if candidate == Look::Picture && !picture {
                            continue;
                        }
                        if icon_toggle(ui, glyph, tip, true, look == candidate).clicked() {
                            chosen = Some(candidate);
                        }
                    }
                }
                if entry.is_some() && on_disk {
                    open_outside = icon_button(
                        ui,
                        egui_phosphor::regular::ARROW_SQUARE_OUT,
                        s.open_with_system,
                        true,
                    )
                    .clicked();
                }
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(egui::Label::new(RichText::new(&name).strong()).truncate());
                    if let Some(entry) = &entry {
                        let facts = format!(
                            "{} \u{b7} {} \u{b7} {}",
                            human(entry.size),
                            when(entry.mtime),
                            mime_of(&name)
                        );
                        ui.add(
                            egui::Label::new(RichText::new(facts).size(12.0).color(hex(p.muted)))
                                .truncate(),
                        );
                    }
                });
            });
        });
        if close {
            self.controller
                .update_settings(|settings| settings.preview_visible = false);
            self.close_preview();
            return;
        }
        if open_outside {
            self.controller.open_preview_with_system();
        }
        if let (Some(view), Some(chosen)) = (&mut self.controller.state.viewing, chosen) {
            view.look = chosen;
        }
        ui.add_space(6.0);
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
            ui.add_space(6.0);
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
                super::widgets::field(ui, &mut self.preview_password)
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
        match view.look {
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
                let family = super::highlight::family_of(&name);
                let lines = &view.lines;
                let ink = super::highlight::Ink::from_palette(&p);
                let font = egui::TextStyle::Monospace.resolve(ui.style());
                code_rows(
                    ui,
                    &p,
                    lines.len(),
                    self.wrap_lines,
                    |index| index + 1,
                    |index, width| {
                        super::highlight::line_job(&lines[index], family, &ink, font.clone(), width)
                    },
                );
            }
            Look::Hex => {
                let bytes = view.bytes.clone();
                let ink = super::highlight::Ink::from_palette(&p);
                let font = egui::TextStyle::Monospace.resolve(ui.style());
                code_rows(
                    ui,
                    &p,
                    bytes.len().div_ceil(16),
                    false,
                    |row| row * 16,
                    |row, _| {
                        let start = row * 16;
                        let end = (start + 16).min(bytes.len());
                        let line = hex_line(start, &bytes[start..end]);
                        let (digits, letters) = line[10..].split_at(50.min(line.len() - 10));
                        let mut job = egui::text::LayoutJob::default();
                        job.append(digits, 0.0, mono(&font, ink.text));
                        job.append(letters, 0.0, mono(&font, ink.string));
                        job
                    },
                );
            }
        }
    }
}

fn mono(font: &egui::FontId, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    }
}

/// Rows of code with a gutter: line numbers for text, offsets for a dump.
///
/// Unwrapped, the rows are all one line tall and only the visible ones are
/// laid out, which is what keeps a thirty-megabyte file scrolling. Wrapped
/// rows vary in height, so they are all laid out; that is the trade the
/// wrap button makes, and it is off by default.
fn code_rows(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    count: usize,
    wrap: bool,
    number: impl Fn(usize) -> usize,
    job: impl Fn(usize, Option<f32>) -> egui::text::LayoutJob,
) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let line = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
    let digits = format!("{}", number(count.saturating_sub(1))).len().max(2);
    let hex_digits = count > 0 && number(1) == 16;
    let glyph = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let width = if hex_digits { 8 } else { digits };
    let gutter = glyph * width as f32 + 20.0;
    let muted = hex(p.muted);
    let gutter_fill = theme::alpha(p.surface, 0.6);
    ui.spacing_mut().item_spacing.y = 0.0;
    let row = |ui: &mut egui::Ui, index: usize, text_width: Option<f32>| {
        let label = if hex_digits {
            format!("{:08X}", number(index))
        } else {
            format!("{:>width$}", number(index), width = digits)
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(gutter, line), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, gutter_fill);
            // Wrapped rows grow downwards, so the number stays with the
            // first line rather than the middle of the block.
            ui.painter().text(
                egui::pos2(rect.right() - 10.0, rect.top() + 1.0),
                egui::Align2::RIGHT_TOP,
                label,
                font.clone(),
                muted,
            );
            ui.add_space(10.0);
            ui.add(egui::Label::new(job(index, text_width)).wrap_mode(if wrap {
                egui::TextWrapMode::Wrap
            } else {
                egui::TextWrapMode::Extend
            }));
        });
    };
    if wrap {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let text_width = (ui.available_width() - gutter - 10.0).max(80.0);
                let tall = ui.available_rect_before_wrap();
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        tall.min,
                        egui::vec2(gutter, tall.height().max(ui.available_height())),
                    ),
                    0.0,
                    gutter_fill,
                );
                for index in 0..count {
                    row(ui, index, Some(text_width));
                }
            });
    } else {
        let top = ui.cursor().min;
        let tall = ui.available_height();
        ui.painter().rect_filled(
            egui::Rect::from_min_size(top, egui::vec2(gutter, tall)),
            0.0,
            gutter_fill,
        );
        egui::ScrollArea::both()
            .auto_shrink(false)
            .show_rows(ui, line, count, |ui, range| {
                for index in range {
                    row(ui, index, None);
                }
            });
    }
}

/// A MIME type for the name, the way a browser would guess it. Not a
/// registry: the kinds people see in a file manager, and the extension
/// itself in capitals for the rest.
pub(crate) fn mime_of(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let known = match ext {
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "xml" | "xsl" | "xslt" | "plist" => "application/xml",
        "svg" => "image/svg+xml",
        "json" | "jsonc" => "application/json",
        "yml" | "yaml" => "application/yaml",
        "toml" => "application/toml",
        "ini" | "cfg" | "conf" => "text/plain",
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "fish" => "text/x-fish",
        "ps1" => "text/x-powershell",
        "bat" | "cmd" => "application/x-bat",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "rb" => "text/x-ruby",
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" | "tsx" | "mts" => "text/typescript",
        "jsx" => "text/jsx",
        "c" => "text/x-c",
        "h" => "text/x-c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "text/x-c++",
        "cs" => "text/x-csharp",
        "java" => "text/x-java",
        "go" => "text/x-go",
        "swift" => "text/x-swift",
        "kt" | "kts" => "text/x-kotlin",
        "php" => "application/x-php",
        "sql" => "application/sql",
        "nix" => "text/x-nix",
        "lock" => "text/plain",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/vnd.microsoft.icon",
        "tif" | "tiff" => "image/tiff",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "m4a" | "aac" => "audio/mp4",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "xz" | "txz" => "application/x-xz",
        "bz2" => "application/x-bzip2",
        "zst" => "application/zstd",
        "iso" => "application/x-iso9660-image",
        "exe" | "dll" => "application/vnd.microsoft.portable-executable",
        "deb" => "application/vnd.debian.binary-package",
        "rpm" => "application/x-rpm",
        "appimage" => "application/x-iso9660-appimage",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    if ext.is_empty() || ext == lower {
        return "application/octet-stream".to_string();
    }
    ext.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::mime_of;

    #[test]
    fn mime_types_for_the_usual_names() {
        assert_eq!(mime_of("repo-backup.sh"), "text/x-shellscript");
        assert_eq!(mime_of("a/b/Cargo.toml"), "application/toml");
        assert_eq!(mime_of("photo.JPG"), "image/jpeg");
        assert_eq!(mime_of("Makefile"), "application/octet-stream");
        assert_eq!(mime_of("x.weird"), "WEIRD");
    }
}
