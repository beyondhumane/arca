use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub sidebar: bool,
    pub rail: bool,
    pub preview: bool,
}

pub(super) fn decide(width: f32, settings: &Settings) -> Layout {
    let rail = settings.sidebar_collapsed || width < 1100.;
    let sidebar = settings.folders;
    let side = if !sidebar {
        0.
    } else if rail {
        44.
    } else {
        settings.sidebar
    };
    Layout {
        sidebar,
        rail,
        preview: settings.preview_visible && width - side - settings.preview_width >= 440.,
    }
}

pub(super) fn cycle_sidebar(settings: &mut Settings) {
    if !settings.folders {
        settings.folders = true;
        settings.sidebar_collapsed = false;
    } else if !settings.sidebar_collapsed {
        settings.sidebar_collapsed = true;
    } else {
        settings.folders = false;
    }
}

pub(super) fn breadcrumb_indices(count: usize, width: f32) -> (Vec<usize>, Vec<usize>) {
    let capacity = if width < 900. {
        3
    } else if width < 1300. {
        5
    } else {
        7
    };
    if count <= capacity {
        return ((0..count).collect(), Vec::new());
    }
    let tail = count - capacity + 1;
    (
        std::iter::once(0).chain(tail..count).collect(),
        (1..tail).collect(),
    )
}

impl GpuiShell {
    pub(super) fn sync_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let width = f32::from(window.viewport_size().width);
        let resized = self.workspace_width != width;
        self.workspace_width = width;
        self.workspace_height = f32::from(window.viewport_size().height);
        let mut layout = decide(self.workspace_width, &self.controller.state.settings);
        layout.preview &= (self.controller.state.archive.is_some() || self.controller.on_disk())
            && self.cursor_on_file();
        if resized || self.effective_layout != Some(layout) {
            if !layout.preview {
                self.controller.cancel_preview();
                self.viewer_image = None;
            }
            self.sidebar_state = cx.new(|_| ResizableState::default());
            self.preview_state = cx.new(|_| ResizableState::default());
            self.effective_layout = Some(layout);
        }
        self.sync_panes(window, cx);
        if self.filter.read(cx).value().as_str() != self.controller.state.filter {
            let value = self.controller.state.filter.clone();
            self.filter
                .update(cx, |input, cx| input.set_value(value, window, cx));
        }
        if layout.preview {
            self.preview_for_cursor(cx);
        }
    }

    pub(super) fn workspace(
        &mut self,
        content: Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let layout = self
            .effective_layout
            .unwrap_or_else(|| decide(self.workspace_width, &self.controller.state.settings));
        let mut browser = div()
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(content)
            .into_any_element();
        if layout.preview {
            let width = self.controller.state.settings.preview_width;
            let panel = self.preview_panel(cx);
            let owner = cx.weak_entity();
            browser = h_resizable("browser-preview")
                .with_state(&self.preview_state)
                .child(
                    resizable_panel()
                        .size_range(px(440.)..px(10000.))
                        .child(browser),
                )
                .child(
                    resizable_panel()
                        .size(px(width))
                        .flex_none()
                        .size_range(px(280.)..px(800.))
                        .child(panel),
                )
                .on_resize(move |state, _, cx| {
                    if let Some(width) = state.read(cx).sizes().get(1).copied() {
                        let _ = owner.update(cx, |this, cx| {
                            this.controller.state.settings.preview_width =
                                f32::from(width).clamp(280., 800.);
                            this.controller.state.settings.save();
                            cx.notify();
                        });
                    }
                })
                .into_any_element();
        }
        let mut body = div()
            .id("workspace")
            .flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden();
        if layout.sidebar && !layout.rail {
            let sidebar = self.workspace_sidebar(false, cx);
            let owner = cx.weak_entity();
            body = body.child(
                h_resizable("sidebar-workspace")
                    .with_state(&self.sidebar_state)
                    .child(
                        resizable_panel()
                            .size(px(self.controller.state.settings.sidebar))
                            .flex_none()
                            .size_range(px(SIDEBAR_LEAST)..px(SIDEBAR_MOST))
                            .child(sidebar),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(440.)..px(10000.))
                            .child(browser),
                    )
                    .on_resize(move |state, _, cx| {
                        if let Some(width) = state.read(cx).sizes().first().copied() {
                            let _ = owner
                                .update(cx, |this, cx| this.remember_sidebar(f32::from(width), cx));
                        }
                    }),
            );
        } else {
            if layout.sidebar {
                body = body.child(self.workspace_sidebar(true, cx));
            }
            body = body.child(browser);
        }
        if !self.controller.state.one_shot {
            if let Some(panel) = self.jobs_panel(false, cx) {
                body = body.relative().child(panel);
            }
        }
        body
    }

    fn cursor_on_file(&self) -> bool {
        self.controller.state.cursor.is_some_and(|cursor| {
            self.controller
                .visible_rows()
                .get(cursor)
                .is_some_and(|row| !row.is_dir && !row.up && row.entry.is_some())
        })
    }

    pub(super) fn footer(
        &self,
        visible: usize,
        selected: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let s = self.controller.s();
        let muted = cx.theme().muted_foreground;
        let hints: [(&str, &str); 6] = [
            ("Enter", s.open_word),
            ("Ctrl+F", s.find_word),
            ("F2", s.rename_word),
            ("Ctrl+C / X", s.copy_word),
            ("Ctrl+V", s.paste_word),
            ("Del", s.delete_word),
        ];
        let shown = if self.workspace_width < 1000. {
            0
        } else {
            hints.len()
        };
        div()
            .id("workspace-status")
            .role(Role::Status)
            .aria_label(s.status_region)
            .h(px(26.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap_4()
            .px_3()
            .text_xs()
            .text_color(muted)
            .bg(cx.theme().status_bar)
            .border_t_1()
            .border_color(cx.theme().border)
            .children(hints.into_iter().take(shown).map(|(key, what)| {
                div()
                    .flex_none()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(key),
                    )
                    .child(what)
            }))
            .child(div().flex_1().min_w_0().truncate().child(self.status()))
            .child(div().flex_none().child(format!(
                "{visible} {} {} \u{b7} {selected} {}",
                s.visible_of,
                self.controller.folder_total(),
                s.checked
            )))
            .when(!self.shown_tasks().is_empty(), |footer| {
                let count = self.shown_tasks().len();
                footer.child(
                    Self::button(
                        "jobs-toggle",
                        fill(s.operations_count, &[("n", &count.to_string())]),
                        s.operations_title.to_string(),
                        !self.background_blocked(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.jobs_open = !this.jobs_open;
                        cx.notify();
                    })),
                )
            })
            .child(
                Self::button(
                    "footer-help",
                    "F1",
                    s.shortcuts_title.into(),
                    !self.background_blocked(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller.state.show_shortcuts = true;
                    cx.notify();
                })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_layout_preserves_preferences_and_restores_panels() {
        let settings = Settings::default();
        assert_eq!(
            decide(800., &settings),
            Layout {
                sidebar: true,
                rail: true,
                preview: false
            }
        );
        assert_eq!(
            decide(1280., &settings),
            Layout {
                sidebar: true,
                rail: false,
                preview: true
            }
        );
        assert_eq!(decide(1920., &settings), decide(1280., &settings));
        assert!(settings.preview_visible);
        assert!(!settings.sidebar_collapsed);
    }
    #[test]
    fn sidebar_toggle_cycles_expanded_rail_hidden_without_losing_width() {
        let mut settings = Settings::default();
        let width = settings.sidebar;
        cycle_sidebar(&mut settings);
        assert!(settings.folders && settings.sidebar_collapsed);
        cycle_sidebar(&mut settings);
        assert!(!settings.folders);
        cycle_sidebar(&mut settings);
        assert!(settings.folders && !settings.sidebar_collapsed);
        assert_eq!(settings.sidebar, width);
    }
    #[test]
    fn preview_space_accounts_for_hidden_sidebar_and_saved_width() {
        let settings = Settings {
            preview_width: 800.,
            ..Settings::default()
        };
        assert!(!decide(1280., &settings).preview);
        assert!(decide(1920., &settings).preview);
        let hidden = Settings {
            folders: false,
            ..settings
        };
        assert!(decide(1280., &hidden).preview);
    }
    #[test]
    fn hidden_panels_stay_hidden_on_widening() {
        let settings = Settings {
            folders: false,
            preview_visible: false,
            ..Settings::default()
        };
        assert!(!decide(1920., &settings).sidebar);
        assert!(!decide(1920., &settings).preview);
    }
    #[test]
    fn breadcrumbs_keep_root_and_destination_with_accessible_middle() {
        assert_eq!(breadcrumb_indices(6, 800.), (vec![0, 4, 5], vec![1, 2, 3]));
        assert_eq!(breadcrumb_indices(2, 800.), (vec![0, 1], vec![]));
        let (shown, hidden) = breadcrumb_indices(10, 1920.);
        assert_eq!(shown, vec![0, 4, 5, 6, 7, 8, 9]);
        assert_eq!(hidden, vec![1, 2, 3]);
    }
}
