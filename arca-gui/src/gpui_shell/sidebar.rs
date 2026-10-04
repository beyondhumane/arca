use super::*;

fn place_icon(kind: PlaceKind) -> Icon {
    match kind {
        PlaceKind::Home => Icon::empty().path("icons/house.svg"),
        PlaceKind::Desktop => Icon::empty().path("icons/monitor.svg"),
        PlaceKind::Documents => Icon::new(IconName::FileText),
        PlaceKind::Downloads => Icon::empty().path("icons/download.svg"),
        PlaceKind::Pictures => Icon::empty().path("icons/image.svg"),
        PlaceKind::Music => Icon::empty().path("icons/music.svg"),
        PlaceKind::Videos => Icon::empty().path("icons/film.svg"),
        PlaceKind::Computer | PlaceKind::Drive => Icon::new(IconName::HardDrive),
        PlaceKind::Pinned => Icon::empty().path("icons/pin.svg"),
    }
}

impl GpuiShell {
    pub(super) fn workspace_sidebar(
        &mut self,
        rail: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let s = self.controller.s();
        let idle = self.background_idle();
        let mut sidebar = div()
            .id("workspace-sidebar")
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .when(rail, |sidebar| sidebar.w(px(44.)))
            .when(!rail, |sidebar| sidebar.w_full());
        let mut actions = div()
            .id("sidebar-actions")
            .flex_none()
            .flex()
            .flex_col()
            .p_1()
            .gap_1();
        actions = actions
            .child(
                Self::icon_button(
                    cx,
                    "sidebar-open",
                    IconName::FolderOpen,
                    s.open.into(),
                    idle,
                )
                .when(!rail, |button| button.label(s.open))
                .on_click(cx.listener(|this, _, _, cx| this.begin_dialog(DialogKind::Open, cx))),
            )
            .child(
                Self::icon_button(
                    cx,
                    "sidebar-create",
                    IconName::Plus,
                    s.compress.into(),
                    idle,
                )
                .when(!rail, |button| button.label(s.compress))
                .on_click(cx.listener(|this, _, _, cx| {
                    let inputs = this.controller.selected_disk_paths();
                    this.controller.dispatch(AppAction::PrepareCompress(inputs));
                    cx.notify();
                })),
            );
        sidebar = sidebar.child(actions);
        if !rail {
            let has_archive = self.controller.state.archive.is_some();
            sidebar = sidebar.child(self.places_panel(has_archive, cx));
            if has_archive {
                sidebar = sidebar.child(self.recent_panel(true, cx));
                sidebar = sidebar.child(div().flex_1().min_h_0().child(self.sidebar(cx)));
            }
        }
        sidebar
    }

    fn recent_panel(&mut self, standalone: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.controller.s();
        let idle = self.background_idle();
        {
            let mut recent = div()
                .id("sidebar-recents")
                .flex_none()
                .when(standalone, |recent| recent.max_h(px(160.)).p_2())
                .when(!standalone, |recent| recent.p_1())
                .text_xs()
                .child(
                    div()
                        .when(!standalone, |label| label.pt_1())
                        .mb_1()
                        .text_color(cx.theme().muted_foreground)
                        .child(s.recent_group),
                );
            for (index, path) in self
                .controller
                .state
                .settings
                .recent
                .iter()
                .take(RECENT_MAX)
                .cloned()
                .enumerate()
            {
                let path = PathBuf::from(path);
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let label = path.display().to_string();
                recent = recent.child(
                    Self::button(("sidebar-recent", index), name, label, idle)
                        .w_full()
                        .overflow_hidden()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.controller.dispatch(AppAction::Open(path.clone()));
                            cx.notify();
                        })),
                );
            }
            if standalone {
                recent.overflow_y_scrollbar().into_any_element()
            } else {
                recent.into_any_element()
            }
        }
    }

    /// Where the disk can be entered from: the user's folders, what was pinned
    /// by hand, and the volumes that are mounted. Each is a jump, and the one
    /// being shown is marked so the list and the panel agree about where you
    /// are.
    pub(super) fn places_panel(
        &mut self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let s = self.controller.s();
        let idle = self.background_idle();
        let current = self
            .controller
            .on_disk()
            .then(|| self.controller.state.current_dir.clone());
        let mut panel = div()
            .id("sidebar-places")
            .when(compact, |panel| panel.flex_none().max_h(px(220.)))
            .when(!compact, |panel| panel.flex_1().min_h_0())
            .overflow_y_scrollbar()
            .p_1()
            .text_xs();
        let groups: [(&'static str, &'static str, Vec<Place>); 3] = [
            ("sidebar-place", s.places_group, self.controller.places()),
            (
                "sidebar-pinned",
                s.pinned_group,
                self.controller.pinned_places(),
            ),
            ("sidebar-device", s.devices_group, self.controller.devices()),
        ];
        for (group, label, places) in groups {
            if places.is_empty() {
                continue;
            }
            panel = panel.child(
                div()
                    .px_1()
                    .pt_1()
                    .mb_1()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            );
            for (index, place) in places.into_iter().enumerate() {
                let selected = current.as_deref() == Some(disk_dir(&place.path).as_str());
                let path = place.path.clone();
                let unpin_path = place.path.clone();
                let pinned = place.kind == PlaceKind::Pinned;
                let owner = cx.weak_entity();
                let button = Self::icon_button(
                    cx,
                    (group, index),
                    place_icon(place.kind),
                    place.path.display().to_string(),
                    idle,
                )
                .label(place.label.clone())
                .w_full()
                .selected(selected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.controller.dispatch(AppAction::Browse(path.clone()));
                    this.route_changed(cx);
                }));
                let item = div()
                    .id(gpui::SharedString::from(format!("{group}-item-{index}")))
                    .w_full()
                    .child(button);
                if pinned {
                    let unpin = s.unpin_word;
                    panel = panel.child(item.context_menu(move |menu, _, _| {
                        let owner = owner.clone();
                        let unpin_path = unpin_path.clone();
                        menu.item(PopupMenuItem::new(unpin).on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.controller
                                    .dispatch(AppAction::TogglePinned(unpin_path.clone()));
                                cx.notify();
                            });
                        }))
                    }));
                } else {
                    panel = panel.child(item);
                }
            }
        }
        if !compact && !self.controller.state.settings.recent.is_empty() {
            panel = panel.child(self.recent_panel(false, cx));
        }
        panel
    }

    /// Grow the kit's tree from the archive's folders, and put the mark on the
    /// folder the list is showing.
    ///
    /// The items are only rebuilt when the archive contents change: they carry
    /// which branches are open, and handing the tree a fresh set every frame
    /// would shut the lot on every repaint. Revealing the current folder opens
    /// the branch that leads to it, so an archive opened three levels down does
    /// not present a closed tree to re-walk by hand.
    pub(super) fn sync_folders(&mut self, cx: &mut Context<Self>) {
        let mut folder_hash = std::collections::hash_map::DefaultHasher::new();
        for entry in &self.controller.state.entries {
            entry.name.hash(&mut folder_hash);
            entry.is_dir.hash(&mut folder_hash);
        }
        let key = (
            self.controller.state.archive.clone(),
            self.controller.state.entries.len(),
            folder_hash.finish(),
        );
        if self.folders_key.as_ref() != Some(&key) {
            self.folders_key = Some(key);
            let root = TreeItem::new("", self.controller.s().archive_root)
                .expanded(true)
                .children(folder_items(&self.controller.state.folders, ""));
            self.folders
                .update(cx, |state, cx| state.set_items(vec![root], cx));
        }
        let current: gpui::SharedString = self.controller.state.current_dir.clone().into();
        self.folders.update(cx, |state, cx| {
            if state.selected_item().map(|item| item.id.clone()) == Some(current.clone()) {
                return;
            }
            state.reveal_item(&current, ScrollStrategy::Top, cx);
            let at = state.index_of(&current);
            state.set_selected_index(at, cx);
        });
    }

    /// The archive's folders, down the left edge.
    ///
    /// Breadcrumbs say where you are; this says what else there is. A deep
    /// archive was previously only navigable by descending one double-click at
    /// a time and reversing back out.
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        self.sync_folders(cx);
        let folders = self.controller.s().archive_folders;
        let widest = self.widest_folder_row(cx);
        let dropping = cx.weak_entity();
        let menu_owner = cx.weak_entity();
        let writable = self.controller.writable();
        let strings = self.controller.s();
        // The branch being renamed, and only while the field is the tree's.
        let renaming = self
            .controller
            .state
            .renaming
            .as_ref()
            .filter(|_| self.renaming_in_tree)
            .map(|(path, _)| path.clone());
        let rename_input = self.rename_input.clone();
        div()
            .id("archive-folders")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().border)
            .p_2()
            .role(Role::Tree)
            .aria_label(folders)
            .child(
                heading(folders)
                    .px_2()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .id("archive-folders-scroll")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scrollbar()
                    .child(
                        tree(&self.folders, move |_, entry, selected, _, cx| {
                            let open = entry.is_expanded();
                            let icon = if entry.is_root() {
                                IconName::Inbox
                            } else if open {
                                IconName::FolderOpen
                            } else {
                                IconName::Folder
                            };
                            // The kit's tree draws no disclosure mark of its own, and a
                            // branch with nothing to say it has one is a branch nobody
                            // opens.
                            let chevron = if entry.is_folder() {
                                Some(Icon::new(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                }))
                            } else {
                                None
                            };
                            // A folder here takes what is dropped on it, the
                            // same move a folder row in the list takes: the
                            // tree is the only way to reach a folder that is
                            // not a child of the one being shown.
                            let target = entry.item().id.to_string();
                            let shell = dropping.clone();
                            let pane_target = target.clone();
                            let pane_shell = dropping.clone();
                            let external_target = target.clone();
                            let external_shell = dropping.clone();
                            // The same pull a folder row of the list takes:
                            // into another folder it is a move, out of the
                            // window it is arca-drag's lazy extraction. The
                            // root is the archive itself and has nowhere to go.
                            let carrier = dropping.clone();
                            let from = entry.item().id.to_string();
                            let carried = entry.item().label.clone();
                            ListItem::new(entry.item().id.clone())
                                .when(!from.is_empty(), |item| {
                                    item.on_drag(DraggedRows, move |_, offset, _, app| {
                                        let preview = DragPreview {
                                            label: carried.to_string(),
                                            extra: 0,
                                            offset,
                                        };
                                        let _ = carrier.update(app, |shell, _| {
                                            shell.controller.pick_folder(&from);
                                            shell.carrying = true;
                                        });
                                        app.new(|_| preview)
                                    })
                                })
                                // What is under the pointer is what it lands in,
                                // and without saying so the whole gesture is a
                                // guess until the archive has been rewritten.
                                .drag_over::<DraggedRows>(|style, _, _, cx| {
                                    style
                                        .bg(cx.theme().drop_target)
                                        .border_color(cx.theme().drag_border)
                                })
                                .border_1()
                                .border_color(gpui::transparent_black())
                                .on_drop(move |_: &DraggedRows, _, cx| {
                                    let _ = shell.update(cx, |shell, cx| {
                                        let carried = shell.controller.selected_roots();
                                        if !carried.is_empty() {
                                            shell.controller.move_into(&carried, &target);
                                        }
                                        shell.carrying = false;
                                        cx.notify();
                                    });
                                })
                                .drag_over::<columns::PaneDrag>(|style, _, _, cx| {
                                    style.bg(cx.theme().drop_target)
                                })
                                .on_drop(move |dragged: &columns::PaneDrag, _, cx| {
                                    let _ = pane_shell.update(cx, |this, cx| {
                                        this.controller.move_into(&dragged.0, &pane_target);
                                        this.carrying = false;
                                        cx.stop_propagation();
                                        cx.notify();
                                    });
                                })
                                .drag_over::<ExternalPaths>(|style, _, _, cx| {
                                    style.bg(cx.theme().drop_target)
                                })
                                .on_drop(move |paths: &ExternalPaths, window, cx| {
                                    let _ = external_shell.update(cx, |this, cx| {
                                        this.controller.go_to(external_target.clone());
                                        this.drop_external(paths.paths().to_vec(), window, cx);
                                        cx.stop_propagation();
                                    });
                                })
                                .pr_1()
                                .pl(px(4. + 12. * entry.depth() as f32))
                                .selected(selected)
                                .role(Role::TreeItem)
                                .aria_label(entry.item().label.clone())
                                .aria_level(entry.depth() + 1)
                                .aria_selected(selected)
                                .aria_expanded(open)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .w(px(14.))
                                                .flex_none()
                                                .children(chevron.map(|icon| icon.small())),
                                        )
                                        .child(Icon::new(icon).small().text_color(cx.theme().link))
                                        // The name is typed over where it is
                                        // read, so the branch does not move
                                        // under the hand mid-rename.
                                        .map(|row| {
                                            if renaming.as_deref() == Some(entry.item().id.as_ref())
                                            {
                                                row.child(
                                                    div()
                                                        .flex_1()
                                                        .h(px(22.))
                                                        .border_1()
                                                        .border_color(cx.theme().border)
                                                        .bg(cx.theme().background)
                                                        .px_1()
                                                        .child(
                                                            Input::new(&rename_input)
                                                                .small()
                                                                .appearance(false),
                                                        ),
                                                )
                                            } else {
                                                row.child(
                                                    div()
                                                        .flex_1()
                                                        .truncate()
                                                        .child(entry.item().label.clone()),
                                                )
                                            }
                                        }),
                                )
                        })
                        // The same menu a folder row of the list carries, plus
                        // the folder being made inside it: a folder is the same
                        // thing in both panels, and a menu that offered less
                        // here would be a second set of rules to learn.
                        .context_menu(move |_, entry, menu, _, _| {
                            let path = entry.item().id.to_string();
                            // The archive itself, which is a place and nothing
                            // else: it cannot be renamed, copied or deleted
                            // from inside the window that has it open.
                            let root = path.is_empty();
                            let mut menu = menu;
                            let mut drawn = false;
                            for action in RowAction::ALL.into_iter().filter(|action| {
                                action.offered()
                                    && (!action.writable_only() || writable)
                                    // The list is where an entry is looked at,
                                    // and what "all" is counted against. The
                                    // tree has no bytes to show and no rows.
                                    && !matches!(
                                        action,
                                        RowAction::View | RowAction::SelectAll
                                    )
                                    && (!root || action.about_the_place())
                                    && (!matches!(action, RowAction::Rename | RowAction::NewFolder)
                                        || writable)
                            }) {
                                if action.starts_group() && drawn {
                                    menu = menu.separator();
                                }
                                drawn = true;
                                let (label, _) = action.label(strings);
                                let shell = menu_owner.clone();
                                let path = path.clone();
                                let item =
                                    PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                        let _ = shell.update(cx, |shell, cx| {
                                            shell.folder_action(action, &path, window, cx);
                                        });
                                    });
                                menu = menu.item(match row_action_icon(action) {
                                    Some(icon) => item.icon(icon),
                                    None => item,
                                });
                            }
                            menu
                        })
                        .h_full()
                        .w(px(widest))
                        .flex_none(),
                    ),
            )
    }

    /// How wide the sidebar is right now: what the pull left it at, and the
    /// remembered width until the first frame has laid the panels out.
    pub(super) fn sidebar_width(&self, cx: &App) -> f32 {
        self.sidebar_state
            .read(cx)
            .sizes()
            .first()
            .map(|width| f32::from(*width))
            .filter(|width| *width > 0.0)
            .unwrap_or(self.controller.state.settings.sidebar)
    }

    /// The sidebar width the pull ended on, kept where the column widths are
    /// kept so it survives the window.
    pub(super) fn remember_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        self.controller.state.settings.sidebar = width.clamp(SIDEBAR_LEAST, SIDEBAR_MOST);
        self.controller.state.settings.save();
        cx.notify();
    }

    /// Lo ancha que se dibuja una fila del arbol: lo que se ve del panel, y no
    /// mas.
    ///
    /// Antes se medía el nombre mas largo -- a siete pixeles por letra -- y se
    /// dibujaba el arbol de ese ancho, de modo que un nombre profundo se leia
    /// desplazandose a lo ancho en vez de quedar cortado. El precio era que la
    /// fila señalada se pintaba mas ancha que el panel y su recuadro se cerraba
    /// fuera de lo que se ve: por la derecha parecia abierto siempre, con el
    /// panel estrecho, porque el borde estaba a 240 con el panel en 160.
    ///
    /// Encajandolas, el nombre se recorta con puntos suspensivos y el recuadro
    /// se cierra. Un nombre que no cabe se lee ensanchando el panel, que se
    /// arrastra entre 160 y 520.
    pub(super) fn widest_folder_row(&self, cx: &App) -> f32 {
        sidebar_inner(self.sidebar_width(cx))
    }
}
