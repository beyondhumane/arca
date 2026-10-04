use super::*;

pub(super) fn pick<T: Copy + PartialEq + 'static>(
    id: &'static str,
    name: &'static str,
    options: Vec<(T, &'static str)>,
    current: T,
    enabled: bool,
    weak: WeakEntity<GpuiShell>,
    apply: fn(&mut GpuiShell, T),
) -> gpui::AnyElement {
    let label = options
        .iter()
        .find(|(candidate, _)| *candidate == current)
        .map(|(_, label)| *label)
        .unwrap_or_default();
    brand_button(id)
        .label(label)
        .accessibility_label(name)
        .dropdown_caret(true)
        .outline()
        .disabled(!enabled)
        .dropdown_menu(move |menu, _, _| {
            options.iter().fold(menu, |menu, (value, label)| {
                let (value, weak) = (*value, weak.clone());
                menu.item(
                    PopupMenuItem::new(*label)
                        .checked(value == current)
                        .on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                apply(this, value);
                                cx.notify();
                            });
                        }),
                )
            })
        })
        .into_any_element()
}

/// The three compression settings, shared by the settings dialog and the add
/// dialog so the two cannot drift apart.
pub(super) fn format_pick(
    id: &'static str,
    shell: &Entity<GpuiShell>,
    weak: &WeakEntity<GpuiShell>,
    cx: &App,
) -> gpui::AnyElement {
    let s = shell.read(cx).controller.s();
    let options = super::Format::WRITABLE
        .map(|format| (format, format.label()))
        .to_vec();
    pick(
        id,
        s.format,
        options,
        shell.read(cx).controller.state.format,
        true,
        weak.clone(),
        |this, format| this.controller.set_create_format(format),
    )
}

pub(super) fn codec_pick(
    id: &'static str,
    shell: &Entity<GpuiShell>,
    weak: &WeakEntity<GpuiShell>,
    enabled: bool,
    cx: &App,
) -> gpui::AnyElement {
    let this = shell.read(cx);
    let options = [
        super::Codec::Store,
        super::Codec::Deflate,
        super::Codec::Zstd,
    ]
    .map(|codec| (codec, this.controller.codec_name(codec)))
    .to_vec();
    pick(
        id,
        this.controller.s().compressor,
        options,
        this.controller.state.codec,
        enabled,
        weak.clone(),
        |this, codec| this.controller.state.codec = codec,
    )
}

pub(super) fn level_pick(
    id: &'static str,
    shell: &Entity<GpuiShell>,
    weak: &WeakEntity<GpuiShell>,
    cx: &App,
) -> gpui::AnyElement {
    let this = shell.read(cx);
    let options = [
        super::Level::Store,
        super::Level::Fast,
        super::Level::Normal,
        super::Level::Best,
    ]
    .map(|level| (level, this.controller.level_name(level)))
    .to_vec();
    pick(
        id,
        this.controller.s().level,
        options,
        this.controller.state.level,
        true,
        weak.clone(),
        |this, level| this.controller.state.level = level,
    )
}

pub(super) fn dialog_dimensions(
    kind: ModalKind,
    viewport: gpui::Size<gpui::Pixels>,
) -> (gpui::Size<gpui::Pixels>, gpui::Pixels) {
    let preferred_width = match kind {
        ModalKind::Conflict | ModalKind::Settings => 560.,
        ModalKind::Shortcuts => 720.,
        ModalKind::Add => 600.,
        _ => 448.,
    };
    let top = (viewport.height / 10.).min(px(48.));
    let width = px(preferred_width).min((viewport.width - px(32.)).max(px(1.)));
    let height = (viewport.height - top - px(16.)).max(px(1.));
    (size(width, height), top)
}

pub(super) fn build_dialog(
    kind: ModalKind,
    shell: &Entity<GpuiShell>,
    dialog: Dialog,
    window: &mut Window,
    cx: &mut App,
) -> Dialog {
    let s = shell.read(cx).controller.s();
    let weak = shell.downgrade();
    let (bounds, top) = dialog_dimensions(kind, window.viewport_size());
    let dialog = dialog.w(bounds.width).max_h(bounds.height).margin_top(top);
    // Escape, the backdrop and the close button all leave the controller still
    // asking the question, and the next frame would reopen the dialog.
    // Answering for the user keeps the two in step -- but only while the state
    // is still on this question, because `on_close` also runs right after ok or
    // cancel already answered it. Either way the kit has already dropped the
    // dialog, so forget it here too: if the state asks the same question again
    // (a wrong password) `sync_dialog` must open a new one instead of seeing
    // nothing to do.
    let dialog = dialog.on_close({
        let weak = weak.clone();
        move |_, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.open_modal = None;
                if this.modal_kind() == Some(kind) {
                    this.cancel_modal(kind);
                    cx.notify();
                }
            });
        }
    });
    // Same answer as the close button, for the people who reach for a labelled
    // button instead of an X.
    let cancel_button = |id: &'static str, label: &'static str| {
        let weak = weak.clone();
        brand_button(id)
            .label(label)
            .outline()
            .on_click(move |_, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.cancel_modal(kind);
                    cx.notify();
                });
            })
    };
    match kind {
        ModalKind::Delete => {
            let names = shell
                .read(cx)
                .controller
                .state
                .confirm_delete
                .clone()
                .unwrap_or_default();
            let confirm = weak.clone();
            dialog
                .title(heading(s.delete_word))
                .child(
                    DialogDescription::new()
                        .child(fill(s.confirm_delete, &[("n", &names.len().to_string())])),
                )
                // `button_props` alone draws nothing: the kit only renders
                // action buttons when a footer asks for them. `DialogClose`
                // and `DialogAction` are what route a press back into
                // `on_close` and `on_ok`.
                .footer(
                    DialogFooter::new()
                        .child(
                            DialogClose::new()
                                .child(brand_button("delete-cancel").label(s.cancel).outline()),
                        )
                        .child(
                            DialogAction::new().child(
                                brand_button("delete-confirm").label(s.delete_word).danger(),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let _ = confirm.update(cx, |this, cx| {
                        this.controller.dispatch(AppAction::ConfirmDelete(true));
                        cx.notify();
                    });
                    true
                })
        }
        ModalKind::Conflict => {
            let path = shell
                .read(cx)
                .controller
                .state
                .conflict
                .clone()
                .unwrap_or_default();
            // Every choice closes the dialog the same way: it answers, the
            // controller drops the question and the next reconcile takes the
            // dialog down. Nothing here has to close it by hand.
            let choice = |id: &'static str, label: &'static str, answer: Answer| {
                let weak = weak.clone();
                brand_button(id).label(label).on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.answer_conflict(answer);
                        cx.notify();
                    });
                })
            };
            let enter = weak.clone();
            dialog
                .title(heading(s.conflict_title))
                .child(DialogDescription::new().child(format!("{} {path}", s.already_there)))
                .footer(
                    DialogFooter::new()
                        .flex_wrap()
                        .child(choice("conflict-replace", s.yes, Answer::Replace).primary())
                        .child(choice(
                            "conflict-replace-all",
                            s.yes_all,
                            Answer::ReplaceAll,
                        ))
                        .child(choice("conflict-skip", s.no, Answer::Skip))
                        .child(choice("conflict-skip-all", s.no_all, Answer::SkipAll))
                        .child(choice("conflict-keep-both", s.rename, Answer::Rename))
                        .child(choice(
                            "conflict-keep-both-all",
                            s.rename_all,
                            Answer::RenameAll,
                        ))
                        .child(choice("conflict-cancel", s.cancel, Answer::Cancel).outline()),
                )
                // Enter keeps answering what it answered before the migration.
                .on_ok(move |_, _, cx| {
                    let _ = enter.update(cx, |this, cx| {
                        this.answer_conflict(Answer::Replace);
                        cx.notify();
                    });
                    true
                })
        }
        ModalKind::Drop => {
            let paths = shell
                .read(cx)
                .controller
                .state
                .confirm_drop
                .clone()
                .unwrap_or_default();
            let names = paths
                .iter()
                .take(8)
                .filter_map(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let choice = |id: &'static str, label: &'static str, choice: DropChoice| {
                let weak = weak.clone();
                brand_button(id).label(label).on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.controller.dispatch(AppAction::AnswerDrop(choice));
                        cx.notify();
                    });
                })
            };
            let enter = weak.clone();
            dialog
                .title(heading(s.drop_title))
                .child(DialogDescription::new().child(format!("{} {names}", s.dropped_word)))
                .footer(
                    DialogFooter::new()
                        .child(choice("drop-open", s.open_word, DropChoice::Open).primary())
                        .child(choice("drop-add", s.add_to_archive, DropChoice::Add))
                        .child(choice("drop-cancel", s.cancel, DropChoice::Cancel).outline()),
                )
                .on_ok(move |_, _, cx| {
                    let _ = enter.update(cx, |this, cx| {
                        this.controller
                            .dispatch(AppAction::AnswerDrop(DropChoice::Open));
                        cx.notify();
                    });
                    true
                })
        }
        ModalKind::Shortcuts => {
            // Keystrokes, not printed key names: the kit spells each one the
            // way the platform does, so the window stops claiming Supr on a
            // keyboard whose key says Delete.
            let left: [(&[&str], &str); 16] = [
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
            ];
            let right: [(&[&str], &str); 15] = [
                (&["f1"], s.shortcuts_title),
                (&["ctrl-c"], s.copy_word),
                (&["ctrl-x"], s.cut_word),
                (&["ctrl-v"], s.paste_word),
                (&["ctrl-shift-c"], s.copy_names),
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
            ];
            let column = |rows: &[(&[&str], &str)]| {
                rows.iter()
                    .fold(div().flex().flex_col().gap_1(), |column, (keys, what)| {
                        column.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .child(
                                    div()
                                        .w(px(130.))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .children(keys.iter().filter_map(|key| {
                                            gpui::Keystroke::parse(key).ok().map(Kbd::new)
                                        })),
                                )
                                .child(what.to_string()),
                        )
                    })
            };
            // No close button of its own: the kit's own close, escape and
            // backdrop are the way out of every dialog now.
            dialog.title(heading(s.shortcuts_title)).child(
                div()
                    .flex()
                    .gap_8()
                    .child(column(&left))
                    .child(column(&right)),
            )
        }
        ModalKind::Password => {
            // Three questions wear this one window: the password to open an
            // archive with, the one it already has, and the one it is about to
            // get. Only the last of them is a password being chosen.
            let setting = matches!(
                shell.read(cx).controller.state.waiting_on_password,
                Some(Pending::NewPassword(_))
            );
            let opening = matches!(
                shell.read(cx).controller.state.waiting_on_password,
                Some(Pending::Extract(_) | Pending::OpenArchive | Pending::Read(_))
            );
            // Only worth offering where there is a password to take off.
            let removable = setting
                && shell
                    .read(cx)
                    .controller
                    .state
                    .entries
                    .iter()
                    .any(|entry| entry.encrypted);
            let field = shell.read(cx).password.clone();
            let submit = weak.clone();
            let remove = weak.clone();
            let box_ = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Input::new(&field).mask_toggle());
            let mut footer = DialogFooter::new().child(cancel_button("password-cancel", s.cancel));
            if removable {
                footer = footer.child(
                    brand_button("password-remove")
                        .label(s.remove_password)
                        .on_click(move |_, _, cx| {
                            let _ = remove.update(cx, |this, cx| {
                                this.controller
                                    .dispatch(AppAction::SubmitPassword(String::new()));
                                cx.notify();
                            });
                        }),
                );
            }
            dialog
                .title(heading(if opening {
                    s.password_needed
                } else {
                    s.set_password
                }))
                .child(DialogDescription::new().child(if setting {
                    s.new_password
                } else if shell.read(cx).controller.state.password_wrong {
                    if shell.read(cx).controller.state.notice == s.password_or_corrupt {
                        s.password_or_corrupt
                    } else {
                        s.password_wrong
                    }
                } else {
                    s.password_hint
                }))
                .child(box_)
                .footer(
                    footer.child(
                        DialogAction::new().child(
                            // Three questions reach this box, and the button
                            // has to say which one it is answering. "Start"
                            // answered none of them: nothing starts, the
                            // archive opens -- and when the password being
                            // asked for is the one the archive already has,
                            // pressing this only brings up the second half of
                            // the question.
                            brand_button("password-submit")
                                .label(if setting {
                                    s.set_password
                                } else if opening {
                                    s.open_word
                                } else {
                                    s.continue_word
                                })
                                .primary(),
                        ),
                    ),
                )
                .on_ok(move |_, _, cx| {
                    let mut close = true;
                    let _ = submit.update(cx, |this, cx| {
                        let password = this.password.read(cx).value().to_string();
                        this.controller
                            .dispatch(AppAction::SubmitPassword(password));
                        // The current password is the first of two questions,
                        // and an empty box is no answer at all: closing on
                        // either leaves the window waiting on a window that is
                        // no longer there.
                        close = this.controller.state.waiting_on_password.is_none();
                        cx.notify();
                    });
                    close
                })
        }
        ModalKind::Add => {
            let is_zip = shell.read(cx).controller.state.format == super::Format::Zip;
            let is_sevenz = shell.read(cx).controller.state.format == super::Format::SevenZ;
            let count = shell.read(cx).controller.state.pending_inputs.len();
            let output_name = shell.read(cx).output_name.clone();
            let add_password = shell.read(cx).add_password.clone();
            let labelled = |label: &'static str, control: gpui::AnyElement| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(label)
                    .child(control)
            };
            let start = weak.clone();
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(s.output_name)
                        .child(Input::new(&output_name)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(labelled(
                            s.format,
                            format_pick("add-format", shell, &weak, cx),
                        ))
                        .child(labelled(
                            s.compressor,
                            if is_sevenz {
                                div().child("LZMA2").into_any_element()
                            } else {
                                codec_pick("add-codec", shell, &weak, is_zip, cx)
                            },
                        ))
                        .child(labelled(s.level, level_pick("add-level", shell, &weak, cx))),
                );
            if is_zip || is_sevenz {
                body = body.child(Input::new(&add_password).mask_toggle());
            }
            if is_sevenz {
                let toggle = weak.clone();
                body = body.child(
                    Switch::new("add-hide-names")
                        .label(s.hide_names)
                        .checked(shell.read(cx).controller.state.hide_names)
                        .on_click(move |_, _, cx| {
                            let _ = toggle.update(cx, |this, cx| {
                                this.controller.state.hide_names =
                                    !this.controller.state.hide_names;
                                cx.notify();
                            });
                        }),
                );
            }
            // One button with two entries under it, because Windows has two
            // dialogs: one picks files, the other picks folders, and neither
            // picks both. Each pick adds to the list instead of replacing it,
            // so a mixed selection is built up in as many passes as it takes --
            // or in one, by dropping it on the list below.
            let pick_files = weak.clone();
            let pick_folders = weak.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        brand_button("add-pick")
                            .label(s.add_word)
                            .icon(IconName::ChevronDown)
                            .dropdown_menu(move |menu, _, _| {
                                let files = pick_files.clone();
                                let folders = pick_folders.clone();
                                menu.item(PopupMenuItem::new(s.add_files_word).on_click(
                                    move |_, _, cx| {
                                        let _ = files.update(cx, |this, cx| {
                                            this.begin_dialog(
                                                DialogKind::PickInputs { folders: false },
                                                cx,
                                            );
                                        });
                                    },
                                ))
                                .item(
                                    PopupMenuItem::new(s.add_folders_word).on_click(
                                        move |_, _, cx| {
                                            let _ = folders.update(cx, |this, cx| {
                                                this.begin_dialog(
                                                    DialogKind::PickInputs { folders: true },
                                                    cx,
                                                );
                                            });
                                        },
                                    ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{count} {}", s.items_word)),
                            )
                            .child({
                                let empty = weak.clone();
                                brand_button("add-clear")
                                    .label(s.remove_all)
                                    .ghost()
                                    .disabled(count == 0)
                                    .on_click(move |_, _, cx| {
                                        let _ = empty.update(cx, |this, cx| {
                                            this.controller.state.pending_inputs.clear();
                                            cx.notify();
                                        });
                                    })
                            }),
                    ),
            );
            body = body.child(pending_list(shell, &weak, s, cx));
            dialog
                .title(heading(s.add_to_archive))
                .child(DialogDescription::new().child(s.defaults_title))
                .child(body)
                .footer(
                    DialogFooter::new()
                        .child(cancel_button("add-cancel", s.cancel))
                        .child(
                            DialogAction::new().child(
                                brand_button("add-start")
                                    .label(s.start)
                                    .primary()
                                    .disabled(count == 0),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let _ = start.update(cx, |this, cx| this.start_add(cx));
                    true
                })
        }
        ModalKind::Settings => {
            let lang = shell.read(cx).controller.state.settings.lang;
            let theme = shell.read(cx).controller.state.settings.theme;
            let is_zip = shell.read(cx).controller.state.format == super::Format::Zip;
            let page = shell.read(cx).controller.state.settings.page;
            let pages = arca_zip::pages::Page::ALL
                .iter()
                .map(|(page, _, label)| (*page, *label))
                .collect::<Vec<_>>();
            let subfolder_on = shell.read(cx).controller.state.into_subfolder;
            let updates_on = shell.read(cx).controller.state.settings.updates;
            let muted = cx.theme().muted_foreground;
            // Radios rather than a row of buttons where one looks pressed:
            // these are one-of-three choices, and the kit's radio says so to
            // the keyboard and to the screen reader without being told.
            let langs = [None, Some(super::Lang::En), Some(super::Lang::Es)];
            let language = {
                let weak = weak.clone();
                RadioGroup::horizontal("settings-language")
                    .selected_index(langs.iter().position(|candidate| *candidate == lang))
                    .children(vec![
                        s.theme_system,
                        super::Lang::En.label(),
                        super::Lang::Es.label(),
                    ])
                    .on_click(move |index, window, cx| {
                        let control = match index {
                            0 => SettingsControl::LangSystem,
                            1 => SettingsControl::LangEn,
                            _ => SettingsControl::LangEs,
                        };
                        let _ = weak.update(cx, |this, cx| {
                            this.settings_activate(control, window, cx);
                            cx.notify();
                        });
                    })
            };
            let themes = [
                super::ThemePreference::System,
                super::ThemePreference::Light,
                super::ThemePreference::Dark,
            ];
            let appearance = {
                let weak = weak.clone();
                RadioGroup::horizontal("settings-theme")
                    .selected_index(themes.iter().position(|candidate| *candidate == theme))
                    .children(vec![s.theme_system, s.theme_light, s.theme_dark])
                    .on_click(move |index, window, cx| {
                        let control = match index {
                            0 => SettingsControl::ThemeSystem,
                            1 => SettingsControl::ThemeLight,
                            _ => SettingsControl::ThemeDark,
                        };
                        let _ = weak.update(cx, |this, cx| {
                            this.settings_activate(control, window, cx);
                            cx.notify();
                        });
                    })
            };
            let row = |label: &'static str, control: gpui::AnyElement| {
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(div().w(px(110.)).flex_none().child(label))
                    .child(control)
            };
            let updates = {
                let weak = weak.clone();
                Switch::new("settings-updates")
                    .label(s.check_updates)
                    .checked(updates_on)
                    .on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.settings_activate(SettingsControl::Updates, window, cx);
                            cx.notify();
                        });
                    })
            };
            let subfolder = {
                let weak = weak.clone();
                Switch::new("settings-subfolder")
                    .label(s.into_subfolder)
                    .checked(subfolder_on)
                    .on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.settings_activate(SettingsControl::Subfolder, window, cx);
                            cx.notify();
                        });
                    })
            };
            dialog
                .title(heading(s.settings))
                .child(DialogDescription::new().child(s.defaults_title))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(row(s.language, language.into_any_element()))
                        .child(row(s.theme, appearance.into_any_element()))
                        .child(row(
                            s.name_encoding,
                            pick(
                                "settings-page",
                                s.name_encoding,
                                pages,
                                page,
                                true,
                                weak.clone(),
                                |this, page| this.controller.reread_names(page),
                            ),
                        ))
                        .child(Separator::horizontal())
                        .child(div().text_sm().text_color(muted).child(s.defaults_title))
                        .child(row(
                            s.format,
                            format_pick("settings-format", shell, &weak, cx),
                        ))
                        .child(row(
                            s.compressor,
                            codec_pick("settings-codec", shell, &weak, is_zip, cx),
                        ))
                        .child(row(s.level, level_pick("settings-level", shell, &weak, cx)))
                        .child(updates)
                        .child(subfolder)
                        .child(Separator::horizontal())
                        // Que version es esta. El numero se compila dentro del
                        // binario, asi que es el del programa abierto y no el
                        // de lo que haya instalado en otro sitio: con dos
                        // copias en el disco las ventanas son identicas y no
                        // habia forma de distinguirlas desde dentro. Al lado,
                        // cuando la hay, la que ha salido.
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(muted)
                                .child(brand_mark(28.))
                                .child(heading(format!("Arca {}", env!("CARGO_PKG_VERSION"))))
                                .children(
                                    shell
                                        .read(cx)
                                        .controller
                                        .state
                                        .update
                                        .as_ref()
                                        .map(|r| format!("· {}", r.tag)),
                                ),
                        ),
                )
        }
        ModalKind::NewFolder | ModalKind::Mask => {
            let (title, hint, confirm) = match kind {
                ModalKind::NewFolder => (s.new_folder, s.folder_name, s.new_folder),
                _ => {
                    let adding = shell
                        .read(cx)
                        .controller
                        .state
                        .picking_group
                        .unwrap_or(true);
                    (
                        if adding {
                            s.select_group
                        } else {
                            s.deselect_group
                        },
                        s.mask_hint,
                        s.start,
                    )
                }
            };
            let field = shell.read(cx).name_input.clone();
            let ok = weak.clone();
            dialog
                .title(heading(title))
                .child(
                    DialogDescription::new().child(if kind == ModalKind::NewFolder {
                        format!("{hint}: /{}", shell.read(cx).controller.state.current_dir)
                    } else {
                        hint.to_string()
                    }),
                )
                .child(Input::new(&field))
                .footer(
                    DialogFooter::new()
                        .child(cancel_button("name-cancel", s.cancel))
                        .child(
                            DialogAction::new()
                                .child(brand_button("name-ok").label(confirm).primary()),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let _ = ok.update(cx, |this, cx| this.confirm_name(kind, cx));
                    true
                })
        }
    }
}

/// What is about to be compressed, one row each, with a cross to take a row
/// back out.
///
/// The list is the answer to the question the count alone cannot answer: a
/// selection built out of several passes through the native dialog is only
/// trustworthy if it can be read back before the work starts. It is also where
/// a mixed pile of files and folders can be dropped in one go, which is the
/// one gesture Windows has that its file dialog does not.
pub(super) fn pending_list(
    shell: &Entity<GpuiShell>,
    weak: &WeakEntity<GpuiShell>,
    s: &'static Strings,
    cx: &mut App,
) -> Stateful<gpui::Div> {
    let inputs = shell.read(cx).controller.state.pending_inputs.clone();
    let dropped = weak.clone();
    let mut list = div()
        .id("add-inputs")
        .flex()
        .flex_col()
        .gap_px()
        .p_1()
        .min_h(px(96.))
        .max_h(px(200.))
        .overflow_scroll()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        // The modal is painted above the shell, so the drop has to be taken
        // here as well: the window behind it never sees the pointer.
        .drag_over::<ExternalPaths>(|style, _, _, cx| {
            style
                .bg(cx.theme().drop_target)
                .border_color(cx.theme().drag_border)
        })
        .on_drop(move |paths: &ExternalPaths, window, cx| {
            let paths = paths.paths().to_vec();
            let _ = dropped.update(cx, |this, cx| this.drop_external(paths, window, cx));
        });
    if inputs.is_empty() {
        return list.child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child(s.drop_inputs_here),
        );
    }
    for (index, path) in inputs.iter().enumerate() {
        // ponytail: one stat per row per frame, to pick the icon. Fine for a
        // list a hand builds; carry the kind in `pending_inputs` if it ever
        // holds more than a screenful.
        let folder = path.is_dir();
        let drop_it = weak.clone();
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_1()
                .rounded(cx.theme().radius)
                .hover(|style| style.bg(cx.theme().accent))
                .child(
                    Icon::new(if folder {
                        IconName::Folder
                    } else {
                        IconName::File
                    })
                    .size_4()
                    .text_color(cx.theme().muted_foreground),
                )
                .child(div().flex_1().truncate().child(path.display().to_string()))
                .child(
                    brand_button(("add-drop-input", index))
                        .icon(IconName::Close)
                        .accessibility_label(s.remove_input.to_string())
                        .tooltip(s.remove_input)
                        .ghost()
                        .compact()
                        .on_click(move |_, _, cx| {
                            let _ = drop_it.update(cx, |this, cx| {
                                // By index and not by value: the same path
                                // cannot be in the list twice, but reading the
                                // list back to find it would be one more place
                                // for the two to disagree.
                                if index < this.controller.state.pending_inputs.len() {
                                    this.controller.state.pending_inputs.remove(index);
                                }
                                cx.notify();
                            });
                        }),
                ),
        );
    }
    list
}
