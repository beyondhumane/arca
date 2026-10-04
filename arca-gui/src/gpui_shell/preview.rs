use super::*;
use crate::controller::PreviewStatus;

impl GpuiShell {
    pub(super) fn show_preview(
        &mut self,
        entry: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_return_focus = window.focused(cx);
        self.controller.state.settings.preview_visible = true;
        self.controller.state.settings.save();
        self.controller.view_entry(entry);
        cx.notify();
    }

    pub(super) fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.cancel_preview();
        self.viewer_image = None;
        if let Some(focus) = self.preview_return_focus.take() {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.list_focus, cx);
        }
    }

    pub(super) fn preview_for_cursor(&mut self, _: &mut Context<Self>) {
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
            .viewer_image
            .as_ref()
            .is_some_and(|(cached, _)| *cached != generation)
        {
            self.viewer_image = None;
        }
        if self.viewer_image.is_none() {
            if let Some(decoded) = &self.controller.state.preview.image {
                let mut bgra = decoded.as_ref().clone();
                for pixel in bgra.pixels_mut() {
                    pixel.0.swap(0, 2);
                }
                self.viewer_image = Some((
                    generation,
                    std::sync::Arc::new(gpui::RenderImage::new([image::Frame::new(bgra)])),
                ));
            }
        }
    }

    pub(super) fn preview_panel(&mut self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let s = self.controller.s();
        let entry = self.controller.state.preview.entry.clone();
        let name = self
            .controller
            .state
            .viewing
            .as_ref()
            .map(|view| view.name.clone())
            .or_else(|| {
                entry.as_ref().map(|entry| {
                    entry
                        .name
                        .rsplit('/')
                        .next()
                        .unwrap_or(&entry.name)
                        .to_string()
                })
            })
            .unwrap_or_else(|| s.view_word.to_string());
        let mut panel = div()
            .id("integrated-preview")
            .when(!self.background_blocked(), |panel| panel.role(Role::Region))
            .aria_label(s.view_word)
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(36.))
                    .px_2()
                    .flex_none()
                    .child(heading(name).flex_1().truncate().text_sm())
                    .child(
                        Self::icon_button(
                            cx,
                            "preview-close",
                            IconName::Close,
                            s.close_preview.into(),
                            !self.background_blocked(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.controller.state.settings.preview_visible = false;
                            this.controller.state.settings.save();
                            this.close_preview(window, cx);
                            cx.notify();
                        })),
                    ),
            );
        if let Some(entry) = &entry {
            let date = entry
                .mtime
                .map(|time| when(Some(time)))
                .unwrap_or_else(|| s.unavailable.to_string());
            let kind = std::path::Path::new(&entry.name)
                .extension()
                .and_then(|ext| ext.to_str())
                .filter(|ext| !ext.is_empty())
                .unwrap_or(s.unavailable);
            panel = panel.child(
                div()
                    .flex_none()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{}: {} | {}: {} | {}: {}",
                        s.col_size,
                        human(entry.size),
                        s.col_modified,
                        date,
                        s.col_type,
                        kind
                    )),
            );
        }
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
            panel = panel.child(div().p_4().text_sm().child(message));
        }
        if matches!(
            self.controller.state.preview.status,
            PreviewStatus::PasswordRequired { .. }
        ) {
            let index = self.controller.state.preview.index;
            panel = panel.child(
                div()
                    .p_3()
                    .child(
                        Input::new(&self.preview_password)
                            .mask_toggle()
                            .aria_label(s.password_needed),
                    )
                    .child(
                        Self::button(
                            "preview-unlock",
                            s.continue_word,
                            s.continue_word.into(),
                            true,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let password = this.preview_password.read(cx).value().to_string();
                                this.preview_password
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                if let Some(index) = index {
                                    this.controller
                                        .request_preview_with_password(index, Some(password));
                                }
                                cx.notify();
                            },
                        )),
                    ),
            );
        }
        if let Some(view) = &self.controller.state.viewing {
            let look = view.look;
            let mut tabs = div().flex().flex_none().gap_1().px_2();
            for (index, (candidate, label)) in [
                (super::Look::Text, s.as_text),
                (super::Look::Hex, s.as_hex),
                (super::Look::Picture, s.as_picture),
            ]
            .into_iter()
            .enumerate()
            {
                if candidate == super::Look::Picture && !view.picture {
                    continue;
                }
                tabs = tabs.child(
                    brand_button(("preview-tab", index))
                        .label(label)
                        .compact()
                        .ghost()
                        .selected(look == candidate)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(view) = &mut this.controller.state.viewing {
                                view.look = candidate;
                            }
                            cx.notify();
                        })),
                );
            }
            panel = panel.child(tabs);
            let scroll = self.viewer_scroll.clone();
            let content = match look {
                super::Look::Picture => div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .children(self.viewer_image.as_ref().map(|(_, image)| {
                        gpui::img(image.clone())
                            .size_full()
                            .object_fit(gpui::ObjectFit::Contain)
                    }))
                    .into_any_element(),
                super::Look::Text => {
                    let total = view.lines.len();
                    let owner = cx.weak_entity();
                    uniform_list("preview-text", total, move |range: Range<usize>, _, cx| {
                        let Some(shell) = owner.upgrade() else {
                            return Vec::new();
                        };
                        let Some(view) = shell.read(cx).controller.state.viewing.as_ref() else {
                            return Vec::new();
                        };
                        range
                            .filter_map(|index| {
                                view.lines.get(index).map(|line| {
                                    div()
                                        .h(px(20.))
                                        .flex()
                                        .font_family("monospace")
                                        .text_xs()
                                        .child(
                                            div()
                                                .w(px(48.))
                                                .flex_none()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!("{}", index + 1)),
                                        )
                                        .child(line.clone())
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .track_scroll(&scroll)
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .into_any_element()
                }
                super::Look::Hex => {
                    let bytes = view.bytes.clone();
                    uniform_list(
                        "preview-hex",
                        bytes.len().div_ceil(16),
                        move |range: Range<usize>, _, _| {
                            range
                                .map(|row| {
                                    let start = row * 16;
                                    div().h(px(20.)).font_family("monospace").text_xs().child(
                                        super::hex_line(
                                            start,
                                            &bytes[start..(start + 16).min(bytes.len())],
                                        ),
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll)
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .into_any_element()
                }
            };
            panel = panel.child(content);
        }
        panel
    }
}
