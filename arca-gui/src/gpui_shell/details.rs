use super::*;

fn archive_only(column: SortColumn) -> bool {
    matches!(
        column,
        SortColumn::Packed | SortColumn::Method | SortColumn::Saved | SortColumn::Crc
    )
}

impl GpuiShell {
    pub(super) fn shown_columns(&self) -> Vec<SortColumn> {
        let columns = self.controller.state.settings.columns;
        let on_disk = self.controller.on_disk();
        std::iter::once(SortColumn::Name)
            .chain(
                Columns::ALL
                    .iter()
                    .map(|(column, _)| *column)
                    .filter(move |column| columns.on(*column))
                    .filter(move |column| !on_disk || !archive_only(*column)),
            )
            .collect()
    }

    /// Where a column's width lives in `Settings::widths`: the name first,
    /// then the ones that can be turned off, in the order of `Columns::ALL`.
    /// A column keeps its width while it is off, so turning one back on does
    /// not lose how it was set.
    pub(super) fn column_slot(column: SortColumn) -> usize {
        Columns::ALL
            .iter()
            .position(|(candidate, _)| *candidate == column)
            .map_or(0, |index| index + 1)
    }

    /// The widths the kit ended a pull with, kept where the rest of the window
    /// settings live so they survive the window.
    pub(super) fn remember_widths(&mut self, widths: &[gpui::Pixels], cx: &mut Context<Self>) {
        for (column, width) in self.shown_columns().into_iter().zip(widths.iter()) {
            let slot = Self::column_slot(column);
            self.set_column_width(slot, f32::from(*width));
        }
        self.controller.state.settings.save();
        cx.notify();
    }

    /// Hand the table the frame it is about to draw.
    ///
    /// The delegate cannot read the shell while the shell is rendering, so what
    /// it needs is copied across first.
    pub(super) fn sync_table(&mut self, rows: &[super::Row], cx: &mut Context<Self>) {
        let columns = self.shown_columns();
        let checked: Vec<bool> = rows
            .iter()
            .map(|row| self.controller.is_checked(row))
            .collect();
        let muted: Vec<bool> = rows
            .iter()
            .map(|row| {
                row.entry.is_some_and(|entry| {
                    self.controller
                        .state
                        .cut_names
                        .contains(&self.controller.state.entries[entry].name)
                })
            })
            .collect();
        // Not while the tree is the one showing the field: one field cannot be
        // drawn in two panels at once.
        let renaming = self
            .controller
            .state
            .renaming
            .as_ref()
            .filter(|_| !self.renaming_in_tree)
            .and_then(|(path, _)| rows.iter().position(|row| row.path == *path));
        let widths = (0..Settings::default_widths().len())
            .map(|slot| self.column_width(slot))
            .collect();
        let cursor = self.controller.state.cursor;
        let order = self.controller.state.order;
        let strings = self.controller.s();
        let idle = self.background_idle();
        let writable = self.controller.writable();
        let on_disk = self.controller.on_disk();
        let root = self.controller.row_root().to_string();
        let rows = rows.to_vec();
        self.table.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            // `refresh` rebuilds the table's live column groups. Doing that on
            // every shell render overwrites a resize drag with the last saved
            // widths, so only rebuild when the column layout actually changed.
            let layout_changed = delegate.columns != columns || delegate.widths != widths;
            delegate.rows = rows;
            delegate.root = root;
            delegate.columns = columns;
            delegate.widths = widths;
            delegate.checked = checked;
            delegate.muted = muted;
            delegate.cursor = cursor;
            delegate.renaming = renaming;
            delegate.order = order;
            delegate.strings = strings;
            delegate.idle = idle;
            delegate.writable = writable;
            delegate.on_disk = on_disk;
            if layout_changed {
                state.refresh(cx);
            }
        });
    }

    pub(super) fn column_width(&self, slot: usize) -> f32 {
        self.controller
            .state
            .settings
            .widths
            .get(slot)
            .copied()
            .unwrap_or_else(|| Settings::default_widths()[slot])
    }

    pub(super) fn set_column_width(&mut self, slot: usize, width: f32) {
        if self.controller.state.settings.widths.len() <= slot {
            self.controller.state.settings.widths = Settings::default_widths();
        }
        self.controller.state.settings.widths[slot] = width.max(Settings::least(slot));
    }
}
