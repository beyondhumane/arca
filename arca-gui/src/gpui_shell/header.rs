use super::*;

impl GpuiShell {
    pub(super) fn header(&mut self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let s = self.controller.s();
        let has_archive = self.controller.state.archive.is_some();
        let idle = self.background_idle();
        let browsing = !self.background_blocked();
        let owner = cx.entity().downgrade();
        let recent = self
            .controller
            .state
            .settings
            .recent
            .iter()
            .take(RECENT_MAX)
            .cloned()
            .collect::<Vec<_>>();
        let writable = self.controller.writable();
        let can_undo = has_archive && self.controller.state.undo.is_some();
        let can_copy = self.can_copy_files();
        let can_paste = self.can_paste_files();
        let release = self
            .controller
            .state
            .update
            .as_ref()
            .map(|release| fill(s.update_ready, &[("version", &release.tag)]));
        let destination = format!("{}: /{}", s.paste_word, self.controller.state.current_dir);
        let flat_view = self.controller.state.settings.flat;
        let show_hidden = self.controller.state.settings.show_hidden;
        let on_disk = self.controller.on_disk();
        let folders_on = self.controller.state.settings.folders;
        let visible_columns = Columns::ALL
            .iter()
            .map(|(column, _)| (*column, self.controller.state.settings.columns.on(*column)))
            .collect::<Vec<_>>();

        let overflow = brand_button("overflow")
            .icon(IconName::Ellipsis)
            .accessibility_label(s.more_word)
            .tooltip(s.more_word)
            .custom(
                ButtonCustomVariant::new(cx)
                    .hover(cx.theme().button_hover)
                    .active(cx.theme().button_active),
            )
            .compact()
            .disabled(!idle)
            .dropdown_menu(move |menu, window, popup_cx| {
                let mut menu = menu;
                if let Some(label) = release.clone() {
                    menu = menu
                        .item(Self::popup_action(
                            owner.clone(),
                            label,
                            OverflowAction::Release,
                        ))
                        .separator();
                }
                let password_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(s.password_action)
                        .disabled(!writable)
                        .on_click(move |_, _, cx| {
                            let _ = password_owner.update(cx, |this, cx| {
                                this.controller.dispatch(AppAction::BeginPasswordChange);
                                cx.notify();
                            });
                        }),
                );
                let archive_owner = owner.clone();
                menu = menu.submenu_with_icon(
                    Some(Icon::new(IconName::Inbox)),
                    s.archive_group,
                    window,
                    popup_cx,
                    move |submenu, window, popup_cx| {
                        let test_owner = archive_owner.clone();
                        let add_owner = archive_owner.clone();
                        let add_folders_owner = archive_owner.clone();
                        let folder_owner = archive_owner.clone();
                        let undo_owner = archive_owner.clone();
                        let save_owner = archive_owner.clone();
                        submenu
                            .item(
                                PopupMenuItem::new(s.test_word)
                                    .disabled(!has_archive)
                                    .on_click(move |_, _, cx| {
                                        let _ = test_owner.update(cx, |this, cx| {
                                            if let Some(archive) =
                                                this.controller.state.archive.clone()
                                            {
                                                this.controller.dispatch(AppAction::Run(
                                                    Job::Test {
                                                        archive,
                                                        only: None,
                                                        password: this
                                                            .controller
                                                            .state
                                                            .archive_password
                                                            .clone(),
                                                    },
                                                ));
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .separator()
                            // Two entries and not one, for the same reason the
                            // add box has two: the native dialog picks files or
                            // folders, never both.
                            .submenu(s.add_to_archive, window, popup_cx, move |inner, _, _| {
                                inner
                                    .item(
                                        Self::popup_action(
                                            add_owner.clone(),
                                            s.add_files_word,
                                            OverflowAction::AddFiles,
                                        )
                                        .disabled(!writable),
                                    )
                                    .item(
                                        Self::popup_action(
                                            add_folders_owner.clone(),
                                            s.add_folders_word,
                                            OverflowAction::AddFolders,
                                        )
                                        .disabled(!writable),
                                    )
                            })
                            .item(
                                Self::popup_action(
                                    folder_owner,
                                    s.new_folder,
                                    OverflowAction::NewFolder,
                                )
                                .disabled(!writable),
                            )
                            .item(
                                Self::popup_action(undo_owner, s.undo_word, OverflowAction::Undo)
                                    .disabled(!can_undo),
                            )
                            .item(
                                Self::popup_action(
                                    save_owner,
                                    s.save_copy,
                                    OverflowAction::SaveCopy,
                                )
                                .disabled(!has_archive),
                            )
                    },
                );

                let selection_owner = owner.clone();
                menu = menu.submenu_with_icon(
                    Some(Icon::new(IconName::Check)),
                    s.selection_group,
                    window,
                    popup_cx,
                    move |submenu, _, _| {
                        let select_owner = selection_owner.clone();
                        let invert_owner = selection_owner.clone();
                        let clear_owner = selection_owner.clone();
                        submenu
                            .item(
                                PopupMenuItem::new(s.select_all)
                                    .disabled(!has_archive)
                                    .on_click(move |_, _, cx| {
                                        let _ = select_owner.update(cx, |this, cx| {
                                            this.controller.dispatch(AppAction::SelectAllVisible);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(s.invert_selection)
                                    .disabled(!has_archive)
                                    .on_click(move |_, _, cx| {
                                        let _ = invert_owner.update(cx, |this, cx| {
                                            this.controller.dispatch(AppAction::InvertVisible);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(s.clear_selection)
                                    .disabled(!has_archive)
                                    .on_click(move |_, _, cx| {
                                        let _ = clear_owner.update(cx, |this, cx| {
                                            this.controller.dispatch(AppAction::ClearSelection);
                                            cx.notify();
                                        });
                                    }),
                            )
                    },
                );

                let clipboard_owner = owner.clone();
                let destination = destination.clone();
                menu = menu.submenu_with_icon(
                    Some(Icon::new(IconName::Copy)),
                    s.clipboard_group,
                    window,
                    popup_cx,
                    move |submenu, _, _| {
                        let copy_owner = clipboard_owner.clone();
                        let cut_owner = clipboard_owner.clone();
                        let paste_owner = clipboard_owner.clone();
                        submenu
                            .item(
                                PopupMenuItem::new(s.copy_word)
                                    .disabled(!can_copy)
                                    .on_click(move |_, _, cx| {
                                        let _ = copy_owner.update(cx, |this, cx| {
                                            this.controller
                                                .dispatch(AppAction::Copy { cut: false });
                                            cx.notify();
                                        });
                                    }),
                            )
                            .item(PopupMenuItem::new(s.cut_word).disabled(!can_copy).on_click(
                                move |_, _, cx| {
                                    let _ = cut_owner.update(cx, |this, cx| {
                                        this.controller.dispatch(AppAction::Copy { cut: true });
                                        cx.notify();
                                    });
                                },
                            ))
                            .item(
                                PopupMenuItem::new(destination.clone())
                                    .disabled(!can_paste)
                                    .on_click(move |_, _, cx| {
                                        let _ = paste_owner.update(cx, |this, cx| {
                                            this.controller.dispatch(AppAction::Paste);
                                            cx.notify();
                                        });
                                    }),
                            )
                    },
                );

                let recent_owner = owner.clone();
                let recent_menu = recent.clone();
                menu = menu.submenu(s.recent_group, window, popup_cx, move |submenu, _, _| {
                    let mut submenu = submenu;
                    for path in recent_menu.iter() {
                        let open_owner = recent_owner.clone();
                        let target = PathBuf::from(path);
                        let leaf = target
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or(path.clone());
                        submenu =
                            submenu.item(PopupMenuItem::new(leaf).on_click(move |_, _, cx| {
                                let _ = open_owner.update(cx, |this, cx| {
                                    this.controller.dispatch(AppAction::Open(target.clone()));
                                    cx.notify();
                                });
                            }));
                    }
                    submenu.item(
                        PopupMenuItem::new(s.clear_history)
                            .disabled(recent_menu.is_empty())
                            .on_click({
                                let history_owner = recent_owner.clone();
                                move |_, _, cx| {
                                    let _ = history_owner.update(cx, |this, cx| {
                                        this.controller.state.settings.recent.clear();
                                        this.controller.state.settings.save();
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                });

                let view_owner = owner.clone();
                let columns_menu = visible_columns.clone();
                menu = menu.submenu(
                    s.view_group,
                    window,
                    popup_cx,
                    move |submenu, window, popup_cx| {
                        let flat_owner = view_owner.clone();
                        let column_owner = view_owner.clone();
                        let folders_owner = view_owner.clone();
                        // Esconder el panel de carpetas. El ajuste seguia
                        // guardandose y leyendose, pero nadie lo miraba al dibujar:
                        // el panel salia siempre y no habia donde quitarlo.
                        let mut submenu = submenu.item(
                            PopupMenuItem::new(s.folder_tree)
                                .disabled(!has_archive)
                                .checked(folders_on)
                                .on_click(move |_, _, cx| {
                                    let _ = folders_owner.update(cx, |this, cx| {
                                        let settings = &mut this.controller.state.settings;
                                        settings.folders = !settings.folders;
                                        settings.save();
                                        cx.notify();
                                    });
                                }),
                        );
                        submenu = submenu.item(
                            PopupMenuItem::new(s.flat_view)
                                .disabled(!has_archive)
                                .checked(flat_view)
                                .on_click(move |_, _, cx| {
                                    let _ = flat_owner.update(cx, |this, cx| {
                                        let flat = !this.controller.state.settings.flat;
                                        this.switch_view(false, flat, cx);
                                        cx.notify();
                                    });
                                }),
                        );
                        let hidden_owner = view_owner.clone();
                        submenu = submenu.item(
                            PopupMenuItem::new(s.show_hidden)
                                .disabled(!on_disk)
                                .checked(show_hidden)
                                .on_click(move |_, _, cx| {
                                    let _ = hidden_owner.update(cx, |this, cx| {
                                        this.controller
                                            .dispatch(AppAction::SetShowHidden(!show_hidden));
                                        cx.notify();
                                    });
                                }),
                        );
                        // Las columnas, en su propio sitio. Estaban sueltas debajo
                        // de estas dos, y son otra cosa: una dice como se dispone
                        // la ventana y la otra que datos se ensenan de cada fila.
                        // Trece entradas seguidas sin separar no son un menu, son
                        // una lista.
                        let columns_menu = columns_menu.clone();
                        submenu.submenu(s.columns_word, window, popup_cx, move |mut cols, _, _| {
                            for (column, shown) in columns_menu.iter().copied() {
                                let label = Columns::label(column, s);
                                let column_owner = column_owner.clone();
                                cols =
                                    cols.item(PopupMenuItem::new(label).checked(shown).on_click(
                                        move |_, _, cx| {
                                            let _ = column_owner.update(cx, |this, cx| {
                                                this.controller
                                                    .dispatch(AppAction::ToggleColumn(column));
                                                cx.notify();
                                            });
                                        },
                                    ));
                            }
                            cols
                        })
                    },
                );

                let app_owner = owner.clone();
                menu.submenu(
                    s.application_group,
                    window,
                    popup_cx,
                    move |submenu, _, _| {
                        let shortcuts_owner = app_owner.clone();
                        let settings_owner = app_owner.clone();
                        submenu
                            .item(PopupMenuItem::new(s.shortcuts_title).on_click(
                                move |_, _, cx| {
                                    let _ = shortcuts_owner.update(cx, |this, cx| {
                                        this.controller.state.show_shortcuts = true;
                                        cx.notify();
                                    });
                                },
                            ))
                            .item(PopupMenuItem::new(s.settings).on_click(move |_, _, cx| {
                                let _ = settings_owner.update(cx, |this, cx| {
                                    this.controller.state.show_settings = true;
                                    cx.notify();
                                });
                            }))
                    },
                )
            });

        let mut nav = div()
            .id("navigation")
            .role(Role::Toolbar)
            .aria_label(s.toolbar_region)
            .h(px(44.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .text_xs()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().border);
        nav = nav.child(
            Self::icon_button(
                cx,
                "sidebar-toggle",
                IconName::PanelLeft,
                s.folder_tree.into(),
                browsing,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                let settings = &mut this.controller.state.settings;
                layout::cycle_sidebar(settings);
                settings.save();
                cx.notify();
            })),
        );
        for (id, icon, label, enabled, action) in [
            (
                "back",
                IconName::ArrowLeft,
                s.back,
                self.controller.can_go_back(),
                0,
            ),
            (
                "forward",
                IconName::ArrowRight,
                s.forward,
                self.controller.can_go_forward(),
                1,
            ),
            (
                "up",
                IconName::ArrowUp,
                s.up,
                self.controller.can_go_up(),
                2,
            ),
        ] {
            nav = nav.child(
                Self::icon_button(cx, id, icon, label.into(), browsing && enabled).on_click(
                    cx.listener(move |this, _, _, cx| {
                        match action {
                            0 => this.controller.go_back(),
                            1 => this.controller.go_forward(),
                            _ => this.controller.go_up(),
                        }
                        this.route_changed(cx);
                    }),
                ),
            );
        }
        if self.controller.can_close_archive() {
            nav = nav.child(
                Self::icon_button(
                    cx,
                    "close-archive",
                    IconName::CircleX,
                    s.close_archive.into(),
                    browsing,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.controller.dispatch(AppAction::CloseArchive);
                    this.route_changed(cx);
                })),
            );
        }
        let crumbs = self.crumbs();
        let (shown, hidden) = layout::breadcrumb_indices(crumbs.len(), self.workspace_width);
        let mut path = div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap_1();
        for (position, index) in shown.into_iter().enumerate() {
            if position > 0 {
                path = path.child(div().child("/"));
            }
            if position == 1 && !hidden.is_empty() {
                let omitted: Vec<_> = hidden.iter().map(|i| crumbs[*i].clone()).collect();
                let owner = cx.weak_entity();
                path = path.child(
                    brand_button("crumb-more")
                        .label("...")
                        .ghost()
                        .compact()
                        .accessibility_label(s.hidden_folders)
                        .disabled(!browsing)
                        .dropdown_menu(move |mut menu, _, _| {
                            for (name, target) in &omitted {
                                let target = target.clone();
                                let owner = owner.clone();
                                menu = menu.item(PopupMenuItem::new(name.clone()).on_click(
                                    move |_, _, cx| {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.controller.go_to(target.clone());
                                            this.route_changed(cx);
                                        });
                                    },
                                ));
                            }
                            menu
                        }),
                );
            }
            let (name, target) = crumbs[index].clone();
            path = path.child(
                Self::button(
                    ("crumb", index),
                    name.clone(),
                    fill(s.open_folder, &[("name", &name)]),
                    browsing,
                )
                .max_w(px(if index == 0 { 150. } else { 120. }))
                .overflow_hidden()
                .when(index == 0, |button| button.icon(IconName::Inbox))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.controller.go_to(target.clone());
                    this.route_changed(cx);
                })),
            );
        }
        nav = nav.child(path);
        if self.filter_open {
            nav = nav.child(
                div().w(px(150.)).flex_none().child(
                    Input::new(&self.filter)
                        .small()
                        .cleanable(true)
                        .aria_label(s.find_word),
                ),
            );
        } else {
            nav = nav.child(
                Self::icon_button(
                    cx,
                    "filter-toggle",
                    IconName::Search,
                    s.find_word.into(),
                    browsing,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.focus_filter(&FocusFilter, window, cx);
                })),
            );
        }
        let owner = cx.weak_entity();
        let columns = self.controller.state.browser.columns;
        nav = nav.child(
            Self::button(
                "browser-view",
                if columns {
                    s.column_view
                } else {
                    s.details_view
                },
                s.view_group.into(),
                browsing,
            )
            .dropdown_menu(move |menu, _, _| {
                let details = owner.clone();
                menu.item(
                    PopupMenuItem::new(s.column_view)
                        .checked(columns)
                        .on_click({
                            let owner = owner.clone();
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.switch_view(true, false, cx);
                                });
                            }
                        }),
                )
                .item(
                    PopupMenuItem::new(s.details_view)
                        .checked(!columns)
                        .on_click(move |_, _, cx| {
                            let _ = details.update(cx, |this, cx| {
                                this.switch_view(false, false, cx);
                            });
                        }),
                )
            }),
        );
        nav = nav.child(
            Self::icon_button(
                cx,
                "extract-all",
                IconName::PanelBottomOpen,
                s.extract_all.into(),
                idle && has_archive,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.begin_dialog(
                    DialogKind::Extract {
                        only_checked: false,
                    },
                    cx,
                );
            })),
        );
        nav = nav.child(
            Self::icon_button(
                cx,
                "test",
                IconName::Check,
                s.test_word.into(),
                idle && has_archive,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(archive) = this.controller.state.archive.clone() {
                    this.controller.dispatch(AppAction::Run(Job::Test {
                        archive,
                        only: None,
                        password: this.controller.state.archive_password.clone(),
                    }));
                    cx.notify();
                }
            })),
        );
        nav.child(overflow)
    }

    pub(super) fn switch_view(&mut self, columns: bool, flat: bool, cx: &mut Context<Self>) {
        self.controller.transition_browser_view(columns, flat);
        if !columns {
            self.list_scroll = self.table.read(cx).vertical_scroll_handle.clone();
        }
        self.controller.state.settings.browser_view = if columns {
            crate::settings::BrowserView::Columns
        } else {
            crate::settings::BrowserView::Details
        };
        if flat {
            self.controller
                .state
                .settings
                .columns
                .set(SortColumn::Path, true);
        }
        self.controller.state.settings.save();
        self.route_changed(cx);
    }
}
