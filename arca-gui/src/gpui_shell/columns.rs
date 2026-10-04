use super::*;

pub(super) struct PaneSurface {
    pub directory: String,
    pub scroll: UniformListScrollHandle,
    pub focus: FocusHandle,
    pub width: f32,
}

#[derive(Clone)]
pub(super) struct PaneDrag(pub Vec<String>);

impl GpuiShell {
    pub(super) fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let browser_focused = self.list_focus.is_focused(window);
        let common = self
            .pane_surfaces
            .iter()
            .zip(&self.controller.state.browser.panes)
            .take_while(|(surface, pane)| surface.directory == pane.directory)
            .count();
        if common != self.pane_surfaces.len() || common != self.controller.state.browser.panes.len()
        {
            self.pane_surfaces.truncate(common);
            for (index, pane) in self
                .controller
                .state
                .browser
                .panes
                .iter()
                .enumerate()
                .skip(common)
            {
                let scroll = UniformListScrollHandle::new();
                scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(-pane.scroll_offset)));
                let focus = cx.focus_handle().tab_stop(true);
                cx.on_focus(&focus, window, move |this, window, cx| {
                    if !this.background_blocked() {
                        this.activate_column(index, window, cx);
                    }
                })
                .detach();
                self.pane_surfaces.push(PaneSurface {
                    directory: pane.directory.clone(),
                    scroll,
                    focus,
                    width: self.controller.state.settings.directory_width,
                });
            }
            self.columns_state = cx.new(|_| ResizableState::default());
        }
        for (index, surface) in self.pane_surfaces.iter().enumerate() {
            self.controller.set_pane_scroll(
                index,
                -f32::from(surface.scroll.0.borrow().base_handle.offset().y),
            );
        }
        if self.controller.state.browser.columns {
            let active = self.controller.state.browser.active;
            if self.revealed_pane != Some(active) {
                let left: f32 = self.pane_surfaces[..active]
                    .iter()
                    .map(|surface| surface.width)
                    .sum();
                self.columns_scroll.set_offset(point(px(-left), px(0.)));
                self.revealed_pane = Some(active);
            }
            if let Some(surface) = self.pane_surfaces.get(active) {
                self.list_scroll = surface.scroll.clone();
                self.list_focus = surface.focus.clone();
                if browser_focused {
                    window.focus(&self.list_focus, cx);
                }
            }
        }
    }

    pub(super) fn activate_column(
        &mut self,
        pane: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pane != self.controller.state.browser.active && self.controller.activate_pane(pane) {
            if let Some(surface) = self.pane_surfaces.get(pane) {
                self.list_focus = surface.focus.clone();
                self.list_scroll = surface.scroll.clone();
                window.focus(&self.list_focus, cx);
            }
            self.route_changed(cx);
        }
    }

    pub(super) fn directory_columns(&mut self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let mut panels = Vec::new();
        let total: f32 = self
            .pane_surfaces
            .iter()
            .map(|surface| surface.width)
            .sum::<f32>()
            + 48.;
        for pane in 0..self.controller.state.browser.panes.len() {
            let content = self.directory_pane(pane, cx);
            panels.push(
                resizable_panel()
                    .size(px(self.pane_surfaces[pane].width))
                    .flex_none()
                    .size_range(px(220.)..px(560.))
                    .child(content),
            );
        }
        panels.push(
            resizable_panel()
                .size(px(48.))
                .size_range(px(0.)..px(10000.))
                .child(div().size_full()),
        );
        let owner = cx.weak_entity();
        div()
            .id("directory-columns")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_x_scroll()
            .track_scroll(&self.columns_scroll)
            .horizontal_scrollbar(&self.columns_scroll)
            .child(
                div().h_full().w(px(total)).child(
                    h_resizable("directory-panes")
                        .with_state(&self.columns_state)
                        .children(panels)
                        .on_resize(move |state, _, cx| {
                            let sizes: Vec<f32> = state
                                .read(cx)
                                .sizes()
                                .iter()
                                .map(|width| f32::from(*width))
                                .collect();
                            let _ = owner.update(cx, |this, cx| {
                                for (surface, width) in this.pane_surfaces.iter_mut().zip(sizes) {
                                    if (surface.width - width).abs() > 0.5 {
                                        this.controller.state.settings.directory_width =
                                            width.clamp(220., 560.);
                                    }
                                    surface.width = width.clamp(220., 560.);
                                }
                                this.controller.state.settings.save();
                                cx.notify();
                            });
                        }),
                ),
            )
    }

    fn directory_pane(&mut self, pane: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let s = self.controller.s();
        let active = pane == self.controller.state.browser.active;
        let state: &crate::controller::DirectoryPane = &self.controller.state.browser.panes[pane];
        let name = if state.directory.is_empty() {
            self.controller.root_label().to_string()
        } else {
            state
                .directory
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string()
        };
        let path = state.directory.clone();
        let filtered = !state.filter.is_empty();
        let rows = self.controller.pane_rows(pane);
        let count = rows.len();
        let focus = self.pane_surfaces[pane].focus.clone();
        let scroll = self.pane_surfaces[pane].scroll.clone();
        let owner = cx.weak_entity();
        let sort_owner = owner.clone();
        let order = if active {
            self.controller.state.order
        } else {
            state.order
        };
        let title = div()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                Self::icon_button(
                    cx,
                    ("pane-filter", pane),
                    IconName::Search,
                    s.find_word.into(),
                    !self.background_blocked(),
                )
                .selected(filtered)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_column(pane, window, cx);
                    this.focus_filter(&FocusFilter, window, cx);
                })),
            )
            .child(
                Self::icon_button(
                    cx,
                    ("pane-sort", pane),
                    IconName::ArrowDown,
                    s.sort_word.into(),
                    !self.background_blocked(),
                )
                .dropdown_menu(move |mut menu, _, _| {
                    for column in [
                        SortColumn::Name,
                        SortColumn::Size,
                        SortColumn::Modified,
                        SortColumn::Type,
                    ] {
                        let owner = sort_owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(Columns::label(column, s))
                                .checked(order.0 == column)
                                .on_click(move |_, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.activate_column(pane, window, cx);
                                        this.controller.set_pane_order(
                                            pane,
                                            (column, order.0 != column || !order.1),
                                        );
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }),
            );
        let mut panel = div()
            .id(("directory-pane", pane))
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .when(!self.background_blocked(), |panel| panel.role(Role::List))
            .aria_label(if path.is_empty() {
                self.controller.root_label().to_string()
            } else {
                path.clone()
            })
            .track_focus(&focus)
            .tab_stop(!self.background_blocked())
            .focus_visible(focus_ring(cx))
            .bg(cx.theme().table)
            .border_r_1()
            .border_color(cx.theme().border)
            .when(active, |panel| {
                panel.border_t_2().border_color(cx.theme().ring)
            })
            .on_key_down(cx.listener(move |this, event, window, cx| {
                if this.typing(window, cx) {
                    return;
                }
                this.activate_column(pane, window, cx);
                this.list_key_down(event, window, cx);
            }))
            .child(title);
        if self.background_idle() && self.controller.writable() {
            let target = path.clone();
            let legacy = target.clone();
            panel = panel
                .drag_over::<PaneDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                .on_drop(cx.listener(move |this, dragged: &PaneDrag, _, cx| {
                    this.controller.move_into(&dragged.0, &target);
                    this.carrying = false;
                    cx.stop_propagation();
                    cx.notify();
                }))
                .drag_over::<DraggedRows>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                .on_drop(cx.listener(move |this, _: &DraggedRows, _, cx| {
                    let roots = this.controller.selected_roots();
                    this.controller.move_into(&roots, &legacy);
                    this.carrying = false;
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                    this.activate_column(pane, window, cx);
                    this.drop_external(paths.paths().to_vec(), window, cx);
                    cx.stop_propagation();
                }));
        }
        if count == 0 {
            let filtered = if active {
                !self.controller.state.filter.is_empty()
            } else {
                !state.filter.is_empty()
            };
            panel = panel.child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if filtered {
                        s.no_matches
                    } else {
                        s.empty_folder
                    }),
            );
        } else {
            panel = panel.child(
                uniform_list(
                    ("pane-list", pane),
                    count,
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                rows.get(index)
                                    .map(|row| this.directory_row(pane, index, row, cx))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&scroll)
                .h(px(count as f32 * 28.))
                .min_h_0()
                .w_full(),
            );
        }
        let menu_owner = owner.clone();
        let writable = self.controller.writable();
        let idle = self.background_idle();
        // The empty-space menu lives on the filler below the rows, not on the
        // whole pane: the menu element only checks hover, so a row and the pane
        // around it would otherwise both open theirs on the same right click.
        let filler = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .context_menu(move |menu, _, _| {
                let mut menu = menu;
                for action in RowAction::EMPTY_SPACE {
                    if !action.offered() || (action.writable_only() && !writable) {
                        continue;
                    }
                    let owner = menu_owner.clone();
                    let path = path.clone();
                    let (label, _) = action.label(s);
                    menu = menu.item(
                        PopupMenuItem::new(format!("{label}: /{path}"))
                            .disabled(!idle)
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.activate_column(pane, window, cx);
                                    this.row_action(action, usize::MAX, window, cx);
                                });
                            }),
                    );
                }
                menu
            });
        panel.child(filler).into_any_element()
    }

    fn directory_row(
        &mut self,
        pane: usize,
        index: usize,
        row: &super::Row,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let s = self.controller.s();
        let active = pane == self.controller.state.browser.active;
        let checked = self.controller.pane_is_checked(pane, row);
        let cursor = if active {
            self.controller.state.cursor
        } else {
            self.controller.state.browser.panes[pane].cursor
        };
        let opened = self.controller.opened_descendant(pane) == Some(row.path.as_str());
        let enabled = !self.background_blocked();
        let target = row.clone();
        let mut item = div()
            .id(ElementId::Name(format!("pane-row-{pane}-{index}").into()))
            .h(px(28.))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .text_sm()
            .when(!self.background_blocked(), |item| item.role(Role::ListItem))
            .aria_selected(checked)
            .aria_label(format!(
                "{}; {}",
                row.label,
                if checked { s.checked } else { s.not_checked }
            ))
            .border_1()
            .border_color(gpui::transparent_black())
            .when(checked, |item| item.bg(cx.theme().table_active))
            .when(cursor == Some(index), |item| {
                item.border_color(cx.theme().ring)
            })
            .when(opened, |item| {
                item.border_l_2().font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .hover(|style| style.bg(cx.theme().table_hover))
            .child(
                Icon::new(if checked {
                    IconName::Check
                } else {
                    Self::kind_icon(row.kind, cx).0
                })
                .size_4(),
            );
        if active
            && !self.renaming_in_tree
            && self
                .controller
                .state
                .renaming
                .as_ref()
                .is_some_and(|(path, _)| path == &row.path)
        {
            item = item.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.rename_input).small()),
            );
        } else {
            item = item.child(div().flex_1().truncate().child(row.label.clone()));
        }
        if row.is_dir {
            item = item.child(Icon::new(IconName::ChevronRight).size_3());
        } else {
            item = item.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        row.entry
                            .map(|i| human(self.controller.state.entries[i].size))
                            .unwrap_or_default(),
                    ),
            );
        }
        if enabled {
            item = item.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.activate_column(pane, window, cx);
                if target.is_dir && event.standard_click() && !event.modifiers().modified() {
                    this.controller.clear_picked();
                    this.controller.set_pane_cursor(pane, Some(index));
                    window.focus(&this.list_focus, cx);
                    this.controller.enter_folder_from_pane(pane, &target.path);
                    this.route_changed(cx);
                } else {
                    this.select_row(index, event, window, cx);
                }
            }));
        }
        let owner = cx.weak_entity();
        let context_row = row.clone();
        let context_path = self.controller.state.browser.panes[pane].directory.clone();
        let writable = self.controller.writable();
        let on_disk = self.controller.on_disk();
        let idle = self.background_idle();
        let menu = move |mut menu: PopupMenu, _: &mut Window, _: &mut Context<PopupMenu>| {
            for action in RowAction::ALL {
                if !action.offered()
                    || !action.shown(on_disk)
                    || (context_row.is_dir && action == RowAction::View)
                    || (!context_row.is_dir && action == RowAction::Pin)
                    || (action.writable_only() && !writable)
                {
                    continue;
                }
                let owner = owner.clone();
                let row = context_row.clone();
                let (label, _) = action.label(s);
                let label = action.destination_label(label, Some(&row), &context_path);
                menu = menu.item(PopupMenuItem::new(label).disabled(!idle).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.activate_column(pane, window, cx);
                            if !action.about_the_place() && !this.controller.is_checked(&row) {
                                this.controller.state.checked.fill(false);
                                this.controller.set_checked(&row, true);
                            }
                            this.controller.set_pane_cursor(pane, Some(index));
                            this.row_action(action, index, window, cx);
                        });
                    },
                ));
            }
            menu
        };
        if self.background_idle() {
            let dragged = row.clone();
            let owner = cx.weak_entity();
            let roots = if checked {
                self.controller.pane_selected_roots(pane)
            } else {
                vec![row.path.clone()]
            };
            item = item.on_drag(PaneDrag(roots), move |_, offset, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.activate_column(pane, window, cx);
                    if !this.controller.is_checked(&dragged) {
                        this.controller.state.checked.fill(false);
                        this.controller.set_checked(&dragged, true);
                    }
                    this.controller.cancel_preview();
                    this.carrying = true;
                });
                cx.new(|_| DragPreview {
                    label: dragged.label.clone(),
                    extra: 0,
                    offset,
                })
            });
            if row.is_dir && self.controller.writable() {
                let target = row.path.clone();
                let external = target.clone();
                let legacy = target.clone();
                item = item
                    .drag_over::<ExternalPaths>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                    .drag_over::<DraggedRows>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                    .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                        this.controller.go_to(external.clone());
                        this.drop_external(paths.paths().to_vec(), window, cx);
                        cx.stop_propagation();
                    }))
                    .on_drop(cx.listener(move |this, _: &DraggedRows, _, cx| {
                        let roots = this.controller.selected_roots();
                        this.controller.move_into(&roots, &legacy);
                        this.carrying = false;
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .drag_over::<PaneDrag>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                    .on_drop(cx.listener(move |this, dragged: &PaneDrag, _, cx| {
                        if !dragged.0.iter().any(|path| target.starts_with(path)) {
                            this.controller.move_into(&dragged.0, &target);
                        }
                        this.carrying = false;
                        cx.stop_propagation();
                        cx.notify();
                    }));
            }
        }
        item.context_menu(menu).into_any_element()
    }
}
