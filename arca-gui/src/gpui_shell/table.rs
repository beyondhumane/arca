use super::*;

pub(super) struct FileTable {
    pub(super) shell: WeakEntity<GpuiShell>,
    pub(super) rows: Vec<super::Row>,
    /// The folder the rows are read from, so a path is shown from here down.
    pub(super) root: String,
    pub(super) columns: Vec<SortColumn>,
    pub(super) widths: Vec<f32>,
    pub(super) checked: Vec<bool>,
    /// Rows waiting to be moved by the clipboard, drawn in the muted ink.
    pub(super) muted: Vec<bool>,
    pub(super) cursor: Option<usize>,
    pub(super) renaming: Option<usize>,
    pub(super) rename_input: Entity<InputState>,
    pub(super) order: (SortColumn, bool),
    pub(super) strings: &'static Strings,
    pub(super) idle: bool,
    pub(super) writable: bool,
    pub(super) on_disk: bool,
}

impl FileTable {
    fn row_is_checked(&self, index: usize) -> bool {
        self.checked.get(index).copied().unwrap_or(false)
    }
}

impl TableDelegate for FileTable {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let column = self.columns[col_ix];
        let slot = GpuiShell::column_slot(column);
        let mut definition = Column::new(
            gpui::SharedString::from(format!("{}", column as usize)),
            Columns::label(column, self.strings),
        )
        .sortable()
        .resizable(true)
        .min_width(px(Settings::least(slot)));
        // The name runs to the edge of the window: it is the column anybody
        // widens the window for, and a fixed one would leave a gutter.
        definition = if column == SortColumn::Name {
            definition.width(px(self.widths.get(slot).copied().unwrap_or(240.)))
        } else {
            definition.width(px(self.widths.get(slot).copied().unwrap_or(96.)))
        };
        if self.order.0 == column {
            definition = if self.order.1 {
                definition.ascending()
            } else {
                definition.descending()
            };
        }
        definition
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        _: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let Some(column) = self.columns.get(col_ix).copied() else {
            return;
        };
        let _ = self.shell.update(cx, |shell, cx| {
            if shell.background_idle() {
                shell.controller.dispatch(AppAction::Sort(column));
                cx.notify();
            }
        });
    }

    /// The header cell, named the way a header that sorts has to be named: the
    /// kit draws the arrow, but only the shell knows to say which way it points.
    ///
    /// The whole cell sorts, not just the kit's little arrow: pressing the name
    /// of a column is how a file list has sorted since before any of this.
    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(column) = self.columns.get(col_ix).copied() else {
            return div().id("header-cell").into_any_element();
        };
        let s = self.strings;
        let label = Columns::label(column, s);
        let shell = self.shell.clone();
        let role = Role::ColumnHeader;
        let accessible = if self.order.0 == column {
            let direction = if self.order.1 {
                s.ascending
            } else {
                s.descending
            };
            format!("{} {label} ({direction})", s.sort_by)
        } else {
            format!("{} {label}", s.sort_by)
        };
        div()
            .id(("header-cell", col_ix))
            .role(role)
            .aria_column_index(col_ix + 1)
            .aria_keyshortcuts("Enter")
            .size_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .aria_label(accessible)
            .on_click(cx.listener(move |table, _, window, cx| {
                table
                    .delegate_mut()
                    .perform_sort(col_ix, ColumnSort::Default, window, cx);
            }))
            .child(label)
            // Que columnas se ven, pulsando con el derecho sobre cualquier
            // cabecera. Es donde lo tienen el Explorador, WinRAR y NanaZip, y
            // donde se mira primero; sin esto las once columnas no tenian mas
            // sitio que el menu de la barra, y por eso estaban alli mezcladas
            // con la disposicion de la ventana.
            //
            // Lo marcado se lee al abrirlo, no al dibujar la cabecera: el menu
            // se construye cuando se pide y tiene que decir lo que hay ahora.
            .context_menu(move |menu, _, cx| {
                let Some(shell) = shell.upgrade() else {
                    return menu;
                };
                let on = shell.read(cx).controller.state.settings.columns;
                let mut menu = menu;
                for (which, _) in Columns::ALL {
                    let owner = shell.downgrade();
                    menu = menu.item(
                        PopupMenuItem::new(Columns::label(which, s))
                            .checked(on.on(which))
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.controller.dispatch(AppAction::ToggleColumn(which));
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let cell = || div().id(("file-cell", row_ix * 16 + col_ix));
        let Some(row) = self.rows.get(row_ix) else {
            return cell();
        };
        let Some(column) = self.columns.get(col_ix).copied() else {
            return cell();
        };
        let muted = self.muted.get(row_ix).copied().unwrap_or(false);
        let ink = if muted {
            cx.theme().muted_foreground
        } else {
            cx.theme().foreground
        };
        if column != SortColumn::Name {
            return cell()
                .role(Role::Cell)
                .aria_column_index(col_ix + 1)
                .text_sm()
                .text_color(ink)
                .child(column_text(row, column, self.strings, &self.root));
        }
        let (icon, color) = GpuiShell::kind_icon(row.kind, cx);
        let icon = div()
            .w(px(18.))
            .flex_none()
            .child(Icon::new(icon).size(px(16.)).text_color(color));
        // Keep the editor in the name column instead of turning the whole row
        // into a form. Its width follows the current name, like Explorer.
        if self.renaming == Some(row_ix) {
            let width = (row.label.chars().count() as f32 * 8.0 + 20.0).clamp(96.0, 280.0);
            return cell()
                .role(Role::Cell)
                .aria_column_index(1)
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(ink)
                .child(icon)
                .child(
                    div()
                        .w(px(width))
                        .h(px(24.))
                        .flex_none()
                        .items_center()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .px_1()
                        .child(Input::new(&self.rename_input).small().appearance(false)),
                );
        }
        cell()
            .role(Role::Cell)
            .aria_column_index(1)
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .text_color(ink)
            .child(icon)
            .child(div().flex_1().truncate().child(row.label.clone()))
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<gpui::Div> {
        let checked = self.row_is_checked(row_ix);
        // The banding is drawn here rather than by the kit, which stripes the
        // whole pane: rows that do not exist should not be drawn as rows.
        let mut item = div().id(("file-row", row_ix)).bg(if checked {
            cx.theme().table_active
        } else if row_ix % 2 == 1 {
            cx.theme().table_even
        } else {
            cx.theme().table
        });
        if self.cursor == Some(row_ix) {
            item = item.border_1().border_color(cx.theme().table_active_border);
        }
        let Some(row) = self.rows.get(row_ix).cloned() else {
            return item;
        };
        // Whether the row is marked is half of what this list is about, so it
        // is said out loud rather than left to the colour of the background.
        let s = self.strings;
        let mut description = format!("{} {}", s.col_name, row.label);
        for column in self.columns.iter().copied().skip(1) {
            let label = Columns::label(column, s);
            description.push_str(&format!(
                "; {label} {}",
                column_text(&row, column, s, &self.root)
            ));
        }
        description.push_str("; ");
        description.push_str(if checked { s.checked } else { s.not_checked });
        item = item
            .aria_label(description)
            .role(Role::Row)
            // The header is row one, so the rows below it start at two.
            .aria_row_index(row_ix + 2);
        if self.idle {
            item = item.focusable();
            if self.cursor == Some(row_ix) {
                item = item.aria_active_descendant();
            }
        }
        // One row at a time is the kit's idea of a click; control, shift and a
        // plain click each mean something different to a file list, and that
        // stays the shell's to decide.
        if self.idle {
            let shell = self.shell.clone();
            item = item.on_click(move |event: &ClickEvent, window, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.select_row(row_ix, event, window, cx);
                });
            });
        }
        // A folder takes what is dropped on it and the entries move there,
        // which is a rewrite of the archive and not a copy out of it. Dropping
        // a folder into itself is not a move, so it is refused.
        if row.is_dir && self.idle && self.writable {
            let target = row.path.clone();
            let external = target.clone();
            let external_shell = self.shell.clone();
            let shell = self.shell.clone();
            item = item
                .drag_over::<ExternalPaths>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                .on_drop(move |paths: &ExternalPaths, window, cx| {
                    let _ = external_shell.update(cx, |shell, cx| {
                        shell.controller.go_to(external.clone());
                        shell.drop_external(paths.paths().to_vec(), window, cx);
                    });
                    cx.stop_propagation();
                })
                .drag_over::<DraggedRows>(|style, _, _, cx| {
                    style
                        .bg(cx.theme().drop_target)
                        .border_color(cx.theme().drag_border)
                })
                .on_drop(move |_: &DraggedRows, _, cx| {
                    let _ = shell.update(cx, |shell, cx| {
                        let carried = shell.controller.selected_roots();
                        let into_itself = carried.iter().any(|carried| {
                            carried.trim_end_matches('/') == target.trim_end_matches('/')
                        });
                        if !carried.is_empty() && !into_itself {
                            shell.controller.move_into(&carried, &target);
                        }
                        shell.carrying = false;
                        cx.notify();
                    });
                    cx.stop_propagation();
                });
        }
        // GPUI owns the threshold and the gesture's lifetime. Where the drag is
        // going is not decided here: a folder of this archive takes it as a
        // move, and leaving the list hands it to arca-drag's lazy IDataObject,
        // so no archive bytes are extracted merely to begin a drag.
        if self.idle && !row.up {
            let shell = self.shell.clone();
            let dragged = row.clone();
            item = item.on_drag(DraggedRows, move |_, offset, _, app| {
                let mut preview = DragPreview {
                    label: String::new(),
                    extra: 0,
                    offset,
                };
                let _ = shell.update(app, |shell, _| {
                    // A band was started in this row's dead space; the pull
                    // belongs to it, not to the row.
                    if shell.band.is_some() {
                        return;
                    }
                    // Pulling a row that was not picked carries that row on
                    // its own, the way pressing and releasing it would have
                    // picked it. Without this the only way to drag a file out
                    // would be to select it first.
                    if !shell.controller.is_checked(&dragged) {
                        shell.controller.state.checked.fill(false);
                        shell.controller.set_checked(&dragged, true);
                    }
                    shell.carrying = true;
                    preview.label = dragged.label.clone();
                    preview.extra = shell.controller.selected_roots().len().saturating_sub(1);
                });
                app.new(|_| preview)
            });
        }
        item
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let is_file = self.rows.get(row_ix).is_some_and(|row| row.entry.is_some());
        // The shell marks a press that landed past the last row with an index
        // no row has, which is how the menu of the folder gets asked for.
        let empty_space = self.rows.get(row_ix).is_none();
        let row = self.rows.get(row_ix);
        let writable = self.writable;
        let on_disk = self.on_disk;
        let strings = self.strings;
        let shell = self.shell.clone();
        let mut menu = menu;
        let mut drawn = false;
        let offered = if empty_space {
            RowAction::EMPTY_SPACE.to_vec()
        } else {
            RowAction::ALL.to_vec()
        };
        for action in offered.into_iter().filter(|action| {
            action.offered()
                && action.shown(on_disk)
                && (!action.writable_only() || writable)
                && (!row.is_some_and(|row| row.up) || action.about_the_place())
                && (*action != RowAction::Rename || writable)
                && (*action != RowAction::View || is_file)
                && (*action != RowAction::Pin || !is_file)
        }) {
            // Never as the first thing in the menu: a rule with nothing above
            // it is a line, not a grouping.
            if action.starts_group() && drawn {
                menu = menu.item(PopupMenuItem::separator());
            }
            drawn = true;
            let (label, keys) = action.label(strings);
            let label = action.destination_label(label, row, &self.root);
            let icon = row_action_icon(action);
            let shell = shell.clone();
            let item = PopupMenuItem::element(move |_, cx| {
                // Two children rather than one string with a tab in it:
                // GPUI lays out text and a tab is nothing at all there.
                let mut row = div().flex().w_full().gap_4().items_center();
                row = row.child(div().flex_1().truncate().child(label.clone()));
                if !keys.is_empty() {
                    row = row.child(
                        div()
                            .flex_none()
                            .text_color(cx.theme().muted_foreground)
                            .child(keys),
                    );
                }
                row
            });
            let item = match icon {
                Some(icon) => item.icon(icon),
                None => item,
            };
            menu = menu.item(item.on_click(move |_, window, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.row_action(action, row_ix, window, cx);
                });
            }));
        }
        menu
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        match (self.rows.get(row_ix), self.columns.get(col_ix).copied()) {
            (Some(row), Some(column)) => column_text(row, column, self.strings, &self.root),
            _ => String::new(),
        }
    }
}
