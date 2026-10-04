use super::*;

#[derive(Clone)]
pub(crate) struct DirectoryPane {
    pub(crate) directory: String,
    pub(crate) cursor: Option<usize>,
    pub(crate) selected: HashSet<usize>,
    pub(crate) scroll_offset: f32,
    pub(crate) filter: String,
    pub(crate) order: (SortColumn, bool),
}

impl DirectoryPane {
    fn new(directory: String) -> Self {
        Self {
            directory,
            cursor: None,
            selected: HashSet::new(),
            scroll_offset: 0.0,
            filter: String::new(),
            order: (SortColumn::Name, true),
        }
    }
}

pub(crate) struct BrowserState {
    pub(crate) panes: Vec<DirectoryPane>,
    pub(crate) active: usize,
    pub(crate) columns: bool,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            panes: vec![DirectoryPane::new(String::new())],
            active: 0,
            columns: false,
        }
    }
}

impl AppController {
    // Legacy shell fields are authoritative only for the active pane. Inactive
    // selections never flow into archive commands until explicitly activated.
    pub(crate) fn snapshot_active_pane(&mut self) {
        let pane = &mut self.state.browser.panes[self.state.browser.active];
        pane.directory = self.state.current_dir.clone();
        pane.cursor = self.state.cursor;
        pane.filter = self.state.filter.clone();
        pane.order = self.state.order;
        pane.selected = self
            .state
            .checked
            .iter()
            .enumerate()
            .filter_map(|(i, on)| (*on && i < self.state.entries.len()).then_some(i))
            .collect();
    }

    fn restore_active_pane(&mut self) {
        let pane = &self.state.browser.panes[self.state.browser.active];
        self.state.current_dir = pane.directory.clone();
        self.state.cursor = pane.cursor;
        self.state.filter = pane.filter.clone();
        self.state.order = pane.order;
        self.state.checked = (0..self.state.entries.len())
            .map(|i| pane.selected.contains(&i))
            .collect();
        self.state.last_click = None;
        self.state.renaming = None;
        self.cancel_preview();
    }

    pub(crate) fn pane_rows(&self, pane: usize) -> Vec<Row> {
        let Some(state) = self.state.browser.panes.get(pane) else {
            return Vec::new();
        };
        if pane == self.state.browser.active {
            self.visible_rows()
        } else {
            self.rows_for(
                &state.directory,
                &state.filter,
                state.order,
                !self.state.browser.columns && self.state.settings.flat,
                !self.state.browser.columns,
            )
        }
    }

    pub(crate) fn activate_pane(&mut self, pane: usize) -> bool {
        if pane >= self.state.browser.panes.len() {
            return false;
        }
        self.snapshot_active_pane();
        if self.state.browser.active != pane {
            self.state.browser.active = pane;
            self.restore_active_pane();
            self.retain_visible_selection();
            self.record_pane_history();
        }
        true
    }

    pub(crate) fn opened_descendant(&self, pane: usize) -> Option<&str> {
        self.state
            .browser
            .panes
            .get(pane.checked_add(1)?)
            .map(|p| p.directory.as_str())
    }

    pub(crate) fn pane_is_checked(&self, pane: usize, row: &Row) -> bool {
        if pane == self.state.browser.active {
            return self.is_checked(row);
        }
        self.state
            .browser
            .panes
            .get(pane)
            .is_some_and(|pane| self.row_is_checked(row, |i| pane.selected.contains(&i)))
    }

    pub(crate) fn enter_folder_from_pane(&mut self, pane: usize, path: &str) -> bool {
        let path = normalized_dir(path);
        if !self
            .pane_rows(pane)
            .iter()
            .any(|r| r.is_dir && !r.up && r.path == path)
        {
            return false;
        }
        if !self.activate_pane(pane) {
            return false;
        }
        self.go_to(path);
        true
    }

    pub(crate) fn navigate_panes(&mut self, path: String) {
        self.snapshot_active_pane();
        let path = nearest_existing_dir(&self.state.entries, &path);
        let mut directories = vec![String::new()];
        let mut directory = String::new();
        for part in path.split('/').filter(|s| !s.is_empty()) {
            directory.push_str(part);
            directory.push('/');
            directories.push(directory.clone());
        }
        let common = self
            .state
            .browser
            .panes
            .iter()
            .zip(&directories)
            .take_while(|(pane, dir)| pane.directory == **dir)
            .count();
        self.state.browser.panes.truncate(common);
        self.state
            .browser
            .panes
            .extend(directories.into_iter().skip(common).map(DirectoryPane::new));
        self.state.browser.active = self.state.browser.panes.len() - 1;
        self.restore_active_pane();
        self.retain_visible_selection();
    }

    pub(crate) fn record_pane_history(&mut self) {
        if self.state.history.get(self.state.here) != Some(&self.state.current_dir) {
            self.state.history.truncate(self.state.here + 1);
            self.state.history.push(self.state.current_dir.clone());
            self.state.here = self.state.history.len() - 1;
        }
    }

    pub(crate) fn reset_browser_panes(&mut self) {
        let directory = self.state.current_dir.clone();
        let columns = self.state.browser.columns;
        self.state.browser = BrowserState::default();
        self.state.browser.columns = columns;
        self.state.current_dir.clear();
        self.state.filter.clear();
        self.state.checked = vec![false; self.state.entries.len()];
        self.state.cursor = None;
        self.navigate_panes(directory);
    }

    pub(crate) fn set_pane_filter(&mut self, pane: usize, filter: String) {
        if !self.activate_pane(pane) {
            return;
        }
        let cursor = self.cursor_path();
        self.state.filter = filter;
        self.restore_cursor_path(cursor);
        self.retain_visible_selection();
        self.cancel_preview();
        self.snapshot_active_pane();
    }

    pub(crate) fn set_pane_order(&mut self, pane: usize, order: (SortColumn, bool)) {
        if !self.activate_pane(pane) {
            return;
        }
        let cursor = self.cursor_path();
        self.state.order = order;
        self.restore_cursor_path(cursor);
        self.snapshot_active_pane();
    }

    pub(crate) fn set_pane_cursor(&mut self, pane: usize, cursor: Option<usize>) {
        if !self.activate_pane(pane) {
            return;
        }
        let cursor = cursor.filter(|i| *i < self.visible_rows().len());
        if self.state.cursor != cursor {
            self.cancel_preview();
        }
        self.state.cursor = cursor;
        self.snapshot_active_pane();
    }

    pub(crate) fn set_pane_scroll(&mut self, pane: usize, offset: f32) {
        if let Some(pane) = self.state.browser.panes.get_mut(pane) {
            pane.scroll_offset = if offset.is_finite() {
                offset.max(0.0)
            } else {
                0.0
            };
        }
    }

    pub(crate) fn transition_browser_view(&mut self, columns: bool, flat: bool) {
        self.snapshot_active_pane();
        let cursors: Vec<_> = self
            .state
            .browser
            .panes
            .iter()
            .enumerate()
            .map(|(i, pane)| {
                pane.cursor
                    .and_then(|cursor| self.pane_rows(i).get(cursor).map(|row| row.path.clone()))
            })
            .collect();
        self.state.browser.columns = columns;
        self.state.settings.flat = !columns && flat;
        for (pane, path) in cursors.into_iter().enumerate() {
            let cursor =
                path.and_then(|path| self.pane_rows(pane).iter().position(|row| row.path == path));
            self.state.browser.panes[pane].cursor = cursor;
            if pane == self.state.browser.active {
                self.state.cursor = cursor;
            }
        }
        self.retain_visible_selection();
        self.cancel_preview();
        self.snapshot_active_pane();
    }

    fn cursor_path(&self) -> Option<String> {
        self.state
            .cursor
            .and_then(|i| self.visible_rows().get(i).map(|r| r.path.clone()))
    }

    fn restore_cursor_path(&mut self, path: Option<String>) {
        self.state.cursor =
            path.and_then(|path| self.visible_rows().iter().position(|r| r.path == path));
    }

    fn retain_visible_selection(&mut self) {
        let mut valid = HashSet::new();
        for row in self.visible_rows().iter().filter(|r| !r.up) {
            match row.entry {
                Some(i) => {
                    valid.insert(i);
                }
                None => {
                    valid.extend(entries_under(&self.state.entries, &row.path));
                }
            }
        }
        for (i, on) in self.state.checked.iter_mut().enumerate() {
            *on &= valid.contains(&i);
        }
        self.snapshot_active_pane();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn entry(name: &str) -> arca_core::Entry {
        arca_core::Entry {
            name: name.into(),
            size: 0,
            compressed_size: 0,
            method: arca_core::Method::Store,
            crc32: None,
            is_dir: name.ends_with('/'),
            mtime: None,
            created: None,
            accessed: None,
            attributes: 0,
            offset: 0,
            raw_name: Vec::new(),
            utf8: true,
            encrypted: false,
            zipcrypto: false,
        }
    }

    fn controller() -> AppController {
        let mut c = AppController::new(Settings::default());
        c.state.entries = [
            "root.txt",
            "a/top.txt",
            "a/b/middle.txt",
            "a/b/c/deep.txt",
            "a/other/leaf.txt",
        ]
        .into_iter()
        .map(entry)
        .collect();
        c.state.checked = vec![false; c.state.entries.len()];
        c.transition_browser_view(true, false);
        c
    }

    fn paths(c: &AppController) -> Vec<&str> {
        c.state
            .browser
            .panes
            .iter()
            .map(|p| p.directory.as_str())
            .collect()
    }

    #[test]
    fn inactive_folder_checks_match_active_checks_for_partial_and_complete_selection() {
        for backslashes in [false, true] {
            let mut c = controller();
            if backslashes {
                for entry in &mut c.state.entries {
                    entry.name = entry.name.replace('/', "\\");
                }
            }
            let folder = c
                .visible_rows()
                .into_iter()
                .find(|r| r.path == "a/")
                .unwrap();
            c.state.checked[1] = true;
            assert!(!c.pane_is_checked(0, &folder));
            c.go_to("a/".into());
            assert!(!c.pane_is_checked(0, &folder));
            assert_eq!(c.pane_selected_roots(0), ["a/top.txt"]);
            c.activate_pane(0);
            c.set_checked(&folder, true);
            assert!(c.pane_is_checked(0, &folder));
            c.go_to("a/".into());
            assert!(c.pane_is_checked(0, &folder));
            assert_eq!(c.pane_selected_roots(0), ["a"]);
            assert!(!c.pane_is_checked(usize::MAX, &folder));
            c.transition_browser_view(false, false);
            let up = c.visible_rows()[0].clone();
            assert!(up.up);
            c.activate_pane(0);
            assert!(!c.pane_is_checked(1, &up));
        }
    }

    #[test]
    fn three_levels_keep_ancestors_and_replace_only_descendants() {
        let mut c = controller();
        assert!(c.enter_folder_from_pane(0, "a/"));
        assert!(c.enter_folder_from_pane(1, "a/b/"));
        assert!(c.enter_folder_from_pane(2, "a/b/c/"));
        assert_eq!(paths(&c), ["", "a/", "a/b/", "a/b/c/"]);
        assert_eq!(c.opened_descendant(1), Some("a/b/"));
        assert!(c.selected_names().is_empty());
        assert!(c.enter_folder_from_pane(1, "a/other/"));
        assert_eq!(paths(&c), ["", "a/", "a/other/"]);
        assert!(!c.enter_folder_from_pane(1, "a/b/c/"));
        assert!(!c.activate_pane(20));
    }

    #[test]
    fn pane_activation_snapshots_legacy_fields_and_targets_only_active_selection() {
        let mut c = controller();
        c.state.checked[0] = true;
        c.state.cursor = c.visible_rows().iter().position(|r| r.path == "root.txt");
        let root_cursor = c.state.cursor;
        c.set_pane_scroll(0, 42.0);
        c.go_to("a/b/".into());
        assert!(c.selected_names().is_empty());
        let middle = c
            .visible_rows()
            .into_iter()
            .find(|r| r.path == "a/b/middle.txt")
            .unwrap();
        c.set_checked(&middle, true);
        c.set_pane_filter(2, "middle".into());
        c.set_pane_order(2, (SortColumn::Size, false));
        c.set_pane_cursor(2, Some(0));
        c.set_pane_scroll(2, 96.0);
        assert_eq!(c.selected_roots(), ["a/b/middle.txt"]);
        assert!(c.activate_pane(0));
        assert_eq!(c.selected_names(), ["root.txt"]);
        assert_eq!(c.state.cursor, root_cursor);
        assert_eq!(c.state.browser.panes[0].scroll_offset, 42.0);
        assert!(c.state.filter.is_empty());
        assert!(c.activate_pane(2));
        assert_eq!(c.selected_names(), ["a/b/middle.txt"]);
        assert_eq!(c.state.filter, "middle");
        assert!(c.state.order.0 == SortColumn::Size && !c.state.order.1);
        assert_eq!(c.state.browser.panes[2].scroll_offset, 96.0);
        assert_eq!(c.state.cursor, Some(0));
    }

    #[test]
    fn inactive_pane_drag_roots_match_its_selection_without_changing_active_commands() {
        let mut c = controller();
        c.select_all_visible();
        let roots = c.selected_roots();
        c.go_to("a/b/".into());
        c.select_all_visible();
        let active = c.selected_roots();
        assert_eq!(c.pane_selected_roots(0), roots);
        assert_eq!(c.selected_roots(), active);
        assert_eq!(c.state.current_dir, "a/b/");
        assert!(c.pane_selected_roots(usize::MAX).is_empty());
        assert!(c.activate_pane(0));
        assert_eq!(c.selected_roots(), roots);
    }

    #[test]
    fn navigating_a_column_with_only_a_cursor_leaves_ancestor_markers_unselected() {
        let mut c = controller();
        for (pane, target) in ["a/", "a/b/", "a/b/c/"].into_iter().enumerate() {
            let cursor = c.pane_rows(pane).iter().position(|row| row.path == target);
            c.clear_picked();
            c.set_pane_cursor(pane, cursor);
            assert!(c.enter_folder_from_pane(pane, target));
            assert_eq!(c.opened_descendant(pane), Some(target));
            assert!(c.pane_selected_roots(pane).is_empty());
        }
        for pane in 0..3 {
            c.activate_pane(pane);
            assert!(c.selected_names().is_empty());
        }
    }

    #[test]
    fn history_view_switching_and_flat_mode_preserve_location_and_valid_selection() {
        let mut c = controller();
        c.go_to("a/".into());
        c.go_to("a/b/".into());
        c.go_back();
        assert_eq!(c.state.current_dir, "a/");
        c.go_forward();
        assert_eq!(c.state.current_dir, "a/b/");
        c.go_to("a/b/c/".into());
        c.select_all_visible();
        c.set_pane_cursor(3, Some(0));
        assert_eq!(c.selected_roots(), ["a/b/c/deep.txt"]);
        c.transition_browser_view(false, false);
        assert_eq!(c.state.cursor, Some(1));
        assert_eq!(c.selected_names(), ["a/b/c/deep.txt"]);
        let up = c.visible_rows()[0].clone();
        c.set_checked(&up, true);
        assert_eq!(c.selected_names(), ["a/b/c/deep.txt"]);
        c.transition_browser_view(false, true);
        assert_eq!(c.state.current_dir, "a/b/c/");
        assert_eq!(c.visible_rows().len(), 5);
        c.select_all_visible();
        c.transition_browser_view(true, false);
        assert_eq!(c.selected_names(), ["a/b/c/deep.txt"]);
        assert_eq!(c.state.cursor, Some(0));
    }

    #[test]
    fn filters_clear_hidden_selection_and_stale_rows_cannot_select_inactive_entries() {
        let mut c = controller();
        let root = c
            .visible_rows()
            .into_iter()
            .find(|r| r.path == "root.txt")
            .unwrap();
        c.set_checked(&root, true);
        c.set_pane_filter(0, "absent".into());
        assert!(c.selected_names().is_empty());
        c.go_to("a/b/".into());
        c.set_checked(&root, true);
        assert!(c.selected_names().is_empty());
        c.set_pane_cursor(2, Some(999));
        assert_eq!(c.state.cursor, None);
        c.set_pane_scroll(2, f32::NAN);
        assert_eq!(c.state.browser.panes[2].scroll_offset, 0.0);
    }

    #[test]
    fn reload_drops_all_pane_indices_and_invalid_directory_segments() {
        let mut c = controller();
        c.select_all_visible();
        c.go_to("a/b/c/".into());
        c.select_all_visible();
        c.state.entries = vec![entry("a/new.txt")];
        c.reset_browser_panes();
        assert_eq!(paths(&c), ["", "a/"]);
        assert!(c
            .state
            .browser
            .panes
            .iter()
            .all(|p| p.selected.is_empty() && p.cursor.is_none()));
        assert_eq!(c.state.checked, [false]);
    }

    #[test]
    fn tree_action_establishes_an_explicit_parent_pane_destination() {
        let mut c = controller();
        c.go_to("a/other/".into());
        c.pick_folder("a/b/");
        assert_eq!(c.state.current_dir, "a/");
        assert_eq!(c.selected_roots(), ["a/b"]);
    }

    #[test]
    fn view_switch_preserves_inactive_pane_cursor_identity() {
        let mut c = controller();
        c.go_to("a/b/".into());
        c.set_pane_cursor(2, Some(1));
        assert_eq!(c.visible_rows()[1].path, "a/b/middle.txt");
        c.activate_pane(1);
        c.transition_browser_view(false, false);
        c.activate_pane(2);
        assert_eq!(c.state.cursor, Some(2));
        assert_eq!(c.visible_rows()[2].path, "a/b/middle.txt");
        assert_eq!(c.opened_descendant(usize::MAX), None);
    }
}
