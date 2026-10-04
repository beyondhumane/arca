use super::disk::ancestors_of;
use super::*;
use crate::test_support::Room;

fn settle(controller: &mut AppController) {
    let end = Instant::now() + std::time::Duration::from_secs(30);
    while controller.state.busy || controller.state.preview.status == PreviewStatus::Loading {
        assert!(!controller.receive());
        controller.poll_preview();
        assert!(Instant::now() < end, "worker did not complete");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn names(controller: &AppController) -> Vec<String> {
    controller
        .visible_rows()
        .iter()
        .filter(|row| !row.up)
        .map(|row| row.label.clone())
        .collect()
}

fn populated() -> Room {
    let room = Room::new();
    std::fs::create_dir_all(room.path("nested/deeper")).unwrap();
    std::fs::write(room.path("nested/deeper/leaf.txt"), b"leaf").unwrap();
    std::fs::write(room.path("nested/note.txt"), b"note").unwrap();
    std::fs::write(room.path("top.txt"), b"top level").unwrap();
    std::fs::write(room.path(".hidden"), b"shh").unwrap();
    room
}

#[test]
fn disk_paths_round_trip_through_entry_names() {
    let room = Room::new();
    let dir = disk_dir(&room.0);
    assert!(
        dir.ends_with('/'),
        "a folder name keeps its trailing slash: {dir}"
    );
    assert_eq!(disk_path(&dir), room.0);
    let file = format!("{dir}top.txt");
    assert_eq!(disk_path(&file), room.path("top.txt"));
    #[cfg(unix)]
    assert_eq!(disk_path(""), PathBuf::from("/"));
}

#[test]
fn ancestors_walk_down_from_the_top() {
    assert_eq!(ancestors_of(""), vec![String::new()]);
    assert_eq!(
        ancestors_of("home/user/docs/"),
        vec![
            String::new(),
            "home/".to_string(),
            "home/user/".to_string(),
            "home/user/docs/".to_string(),
        ]
    );
}

#[test]
fn browsing_lists_folders_as_they_are_entered() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    assert!(app.on_disk());
    assert!(!app.state.busy);
    assert_eq!(app.state.current_dir, disk_dir(&room.0));
    assert_eq!(names(&app), vec!["nested", "top.txt"]);

    let nested = disk_dir(&room.path("nested"));
    let loaded = |app: &AppController, dir: &str| {
        app.state
            .disk
            .as_ref()
            .is_some_and(|disk| disk.loaded.contains(dir))
    };
    assert!(
        !loaded(&app, &nested),
        "children are read on entry, not ahead"
    );

    app.go_to(nested.clone());
    assert!(loaded(&app, &nested));
    assert_eq!(names(&app), vec!["deeper", "note.txt"]);
    assert!(app.can_go_up());
    app.go_up();
    assert_eq!(app.state.current_dir, disk_dir(&room.0));
    assert_eq!(names(&app), vec!["nested", "top.txt"]);
    assert_eq!(
        app.state
            .entries
            .iter()
            .filter(|e| e.name == nested)
            .count(),
        1,
        "walking back does not list the folder twice"
    );
}

#[test]
fn hidden_files_follow_the_setting() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    assert!(!names(&app).iter().any(|name| name == ".hidden"));
    app.dispatch(AppAction::SetShowHidden(true));
    assert!(app.state.settings.show_hidden);
    assert!(names(&app).iter().any(|name| name == ".hidden"));
    app.dispatch(AppAction::SetShowHidden(false));
    assert!(!names(&app).iter().any(|name| name == ".hidden"));
}

#[test]
fn an_archive_on_disk_opens_in_place_and_closes_back_onto_its_row() {
    let room = populated();
    let archive = room.archive(None, false);
    let archive_name = archive.file_name().unwrap().to_string_lossy().to_string();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let index = app
        .state
        .entries
        .iter()
        .position(|entry| entry.name == disk_dir(&room.0) + &archive_name)
        .expect("the archive is listed");

    app.open_disk_entry(index);
    settle(&mut app);
    assert_eq!(app.state.archive.as_deref(), Some(archive.as_path()));
    assert!(!app.on_disk());
    assert!(app.can_close_archive());
    assert_eq!(app.state.format, Format::SevenZ);
    assert_eq!(names(&app), vec!["source"]);
    assert!(
        !app.writable(),
        "7z stays read-only when opened from the disk"
    );

    app.go_up();
    assert!(app.on_disk());
    assert!(app.state.archive.is_none());
    assert_eq!(app.state.current_dir, disk_dir(&room.0));
    let cursor = app.state.browser.panes[app.state.browser.active].cursor;
    let rows = app.visible_rows();
    let under_cursor = cursor
        .and_then(|i| rows.get(i))
        .map(|row| row.label.clone());
    assert_eq!(under_cursor.as_deref(), Some(archive_name.as_str()));
}

#[test]
fn a_folder_opened_from_the_disk_is_remembered_for_the_next_start() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let nested = disk_dir(&room.path("nested"));
    app.go_to(nested.clone());
    app.remember_folder();
    let remembered = app.state.settings.last_folder.clone();
    assert_eq!(
        remembered.as_deref(),
        Some(room.path("nested").to_str().unwrap())
    );

    let settings = Settings {
        last_folder: remembered,
        ..Settings::default()
    };
    let mut next = AppController::new(settings);
    next.start_browsing();
    assert!(next.on_disk());
    assert_eq!(next.state.current_dir, nested);

    let gone = Settings {
        last_folder: Some(room.path("missing").to_string_lossy().to_string()),
        ..Settings::default()
    };
    let mut fallback = AppController::new(gone);
    fallback.start_browsing();
    assert!(fallback.on_disk());
    assert_ne!(fallback.state.current_dir, disk_dir(&room.path("missing")));
}

#[test]
fn pinned_folders_toggle_and_persist_in_settings() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let nested = room.path("nested");
    assert!(app.pinned_places().is_empty());
    app.dispatch(AppAction::TogglePinned(nested.clone()));
    assert!(app.is_pinned(&nested));
    let pinned = app.pinned_places();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].kind, PlaceKind::Pinned);
    assert_eq!(pinned[0].label, "nested");
    assert_eq!(pinned[0].path, nested);
    app.dispatch(AppAction::TogglePinned(nested.clone()));
    assert!(!app.is_pinned(&nested));
    assert!(app.state.settings.pinned.is_empty());
}

#[test]
fn ticked_rows_resolve_to_absolute_paths_for_compressing() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let rows = app.visible_rows();
    for row in rows.iter().filter(|row| !row.up) {
        app.set_checked(row, true);
    }
    let mut paths = app.selected_disk_paths();
    paths.sort();
    assert_eq!(paths, vec![room.path("nested"), room.path("top.txt")]);
}

#[test]
fn archive_only_actions_stay_off_while_browsing_the_disk() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    assert!(!app.writable());
    assert!(!app.can_close_archive());
    let rows = app.visible_rows();
    for row in rows.iter().filter(|row| !row.up) {
        app.set_checked(row, true);
    }
    app.request_delete();
    assert!(app.state.confirm_delete.is_none());
    assert!(app.state.error);
    assert!(std::fs::metadata(room.path("top.txt")).is_ok());
}

#[test]
fn a_file_on_disk_previews_without_an_archive() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let index = app
        .state
        .entries
        .iter()
        .position(|entry| entry.name.ends_with("top.txt"))
        .unwrap();
    app.request_preview(index);
    settle(&mut app);
    assert_eq!(app.state.preview.status, PreviewStatus::Ready);
    assert_eq!(app.state.preview.index, Some(index));
    let shown = app.state.preview.entry.as_ref().map(|entry| entry.size);
    assert_eq!(shown, Some("top level".len() as u64));
}

#[test]
fn an_archive_listing_that_lands_after_closing_is_ignored() {
    let room = populated();
    let archive = room.archive(None, false);
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    app.open(archive.clone());
    assert!(app.state.busy && app.state.listing);
    app.close_archive();
    assert!(app.on_disk());
    let (tx, rx) = std::sync::mpsc::channel();
    app.state.channel = Some(rx);
    tx.send(Message::Listing(
        archive,
        Ok((Vec::new(), Vec::new())),
        None,
    ))
    .unwrap();
    app.receive();
    assert!(app.on_disk());
    assert!(app.state.archive.is_none());
    let shown = names(&app);
    assert!(shown.iter().any(|name| name == "top.txt"), "{shown:?}");
    assert!(!shown.iter().any(|name| name == "late"), "{shown:?}");
}

#[test]
fn refreshing_the_disk_keeps_the_selection_by_name() {
    let room = populated();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    let rows = app.visible_rows();
    let top = rows
        .iter()
        .position(|row| row.label == "top.txt")
        .expect("top.txt listed");
    app.set_pane_cursor(app.state.browser.active, Some(top));
    let entry = rows[top].entry.expect("a file row has an entry");
    app.state.checked[entry] = true;
    app.snapshot_active_pane();
    assert_eq!(app.selected_disk_paths(), vec![room.path("top.txt")]);

    std::fs::write(room.path("aaa.txt"), b"sorts first").unwrap();
    app.refresh_disk();
    assert_eq!(app.selected_disk_paths(), vec![room.path("top.txt")]);
    let rows = app.visible_rows();
    let cursor = app.state.cursor.expect("cursor survives a refresh");
    assert_eq!(rows[cursor].label, "top.txt");
    assert!(names(&app).contains(&"aaa.txt".to_string()));
}

#[cfg(unix)]
#[test]
fn a_name_with_a_backslash_is_left_out_rather_than_split() {
    let room = populated();
    let odd = room.path("a\\b");
    std::fs::create_dir(&odd).unwrap();
    std::fs::write(odd.join("inside.txt"), b"x").unwrap();
    let mut app = AppController::new(Settings::default());
    app.browse_disk(&room.0);
    assert_eq!(
        names(&app),
        vec!["nested".to_string(), "top.txt".to_string()]
    );
    assert!(app.state.entries.iter().all(|e| !e.name.contains('\\')));
}

#[test]
fn a_remembered_flat_view_does_not_apply_on_disk() {
    let room = populated();
    let mut app = AppController::new(Settings {
        flat: true,
        ..Default::default()
    });
    app.transition_browser_view(false, true);
    app.browse_disk(&room.0);
    app.enter_folder_from_pane(
        app.state.browser.active,
        &format!("{}nested/", disk_dir(&room.0)),
    );
    assert!(!app.flat());
    assert_eq!(
        names(&app),
        vec!["deeper".to_string(), "note.txt".to_string()]
    );
}
