use super::actions::*;
use super::browser::{column_slot, scroll_target, ROW_HEIGHT};
use super::dialogs::{dialog_max_height, dialog_width, keystroke};
use super::layout::*;
use super::sidebar::folder_children;
use super::*;
use crate::tree::Folder;

#[test]
fn shell_keeps_compact_and_normal_startup_sizes() {
    assert_eq!(shell_size(true), COMPACT_SIZE);
    assert_eq!(shell_size(false), NORMAL_SIZE);
    assert!(MINIMUM_SIZE.0 <= NORMAL_SIZE.0);
    assert!(MINIMUM_SIZE.1 <= NORMAL_SIZE.1);
}

#[test]
fn dialogs_fit_compact_and_normal_viewports() {
    for (width, height) in [MINIMUM_SIZE, COMPACT_SIZE, NORMAL_SIZE, (480., 280.)] {
        for kind in [
            ModalKind::Add,
            ModalKind::Settings,
            ModalKind::Shortcuts,
            ModalKind::Conflict,
            ModalKind::Password,
        ] {
            assert!(dialog_width(kind, width) <= width - 32.);
            assert!(dialog_max_height(height) + 96. <= height.max(256.));
            assert!(dialog_max_height(height) >= 160.);
        }
    }
    assert_eq!(dialog_width(ModalKind::Add, NORMAL_SIZE.0), 600.);
    assert_eq!(dialog_width(ModalKind::Settings, 1200.), 860.);
}

#[test]
fn breadcrumbs_keep_root_and_nearest_folders() {
    assert_eq!(breadcrumb_indices(4, 1000.), ((0..4).collect(), Vec::new()));
    assert_eq!(breadcrumb_indices(6, 1000.), (vec![0, 2, 3, 4, 5], vec![1]));
    assert_eq!(breadcrumb_indices(6, 800.), (vec![0, 4, 5], vec![1, 2, 3]));
}

#[test]
fn conflict_actions_answer_every_supported_choice() {
    let answers = [
        Answer::Replace,
        Answer::ReplaceAll,
        Answer::Skip,
        Answer::SkipAll,
        Answer::Rename,
        Answer::RenameAll,
        Answer::Cancel,
    ];
    for answer in answers {
        let mut controller = AppController::new(Settings::default());
        let (tx, rx) = channel();
        controller.enqueue(crate::controller::Task::new(
            "asking",
            "x",
            Box::new(move |messages, replies, _, _| {
                let _ = messages.send(crate::controller::Message::Conflict(
                    "already-there.txt".into(),
                ));
                let _ = tx.send(replies.recv());
                let _ = messages.send(crate::controller::Message::Done(String::new()));
            }),
        ));
        while controller.conflict().is_none() {
            controller.receive();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        controller.dispatch(AppAction::AnswerConflict(answer));
        assert_eq!(rx.recv().unwrap().unwrap(), answer);
        assert!(controller.conflict().is_none());
    }
}

#[test]
fn background_events_are_blocked_by_gpui_modals_and_native_pickers() {
    assert!(background_event_allowed(false, false));
    assert!(!background_event_allowed(true, false));
    assert!(!background_event_allowed(false, true));
    assert!(!background_event_allowed(true, true));
}

#[test]
fn clipboard_actions_require_capability_idle_archive_and_selection() {
    assert!(clipboard_action_allowed(true, true, true, 1, true));
    assert!(!clipboard_action_allowed(false, true, true, 1, true));
    assert!(!clipboard_action_allowed(true, false, true, 1, true));
    assert!(!clipboard_action_allowed(true, true, false, 1, true));
    assert!(!clipboard_action_allowed(true, true, true, 0, true));
    assert!(clipboard_action_allowed(true, true, true, 0, false));
    assert!(!clipboard_action_allowed(true, true, false, 0, false));
}

#[test]
fn moving_the_cursor_scrolls_only_as_far_as_it_has_to() {
    let height = ROW_HEIGHT * 10.0;
    assert_eq!(scroll_target(0.0, height, 3), 0.0);
    assert_eq!(
        scroll_target(ROW_HEIGHT * 20.0, height, 5),
        ROW_HEIGHT * 5.0
    );
    assert_eq!(scroll_target(0.0, height, 10), ROW_HEIGHT);
    assert_eq!(scroll_target(0.0, height, 9), 0.0);
}

#[test]
fn every_column_reads_its_width_from_its_own_slot() {
    // The widths vector is the one written to gui.conf: the name first,
    // then `Columns::ALL` in order. A slot off by one would hand a column
    // the width of its neighbour and the file would still load.
    assert_eq!(column_slot(SortColumn::Name), 0);
    for (index, (column, _)) in Columns::ALL.iter().enumerate() {
        assert_eq!(column_slot(*column), index + 1);
    }
    let widths = Settings::default_widths();
    assert_eq!(widths.len(), Columns::ALL.len() + 1);
    // A column cannot be pulled below what it needs to stay readable.
    assert!(Settings::least(0) > Settings::least(1));
}

#[test]
fn read_only_archives_never_offer_mutation_actions() {
    for action in [
        RowAction::Rename,
        RowAction::Delete,
        RowAction::Cut,
        RowAction::Paste,
        RowAction::NewFolder,
    ] {
        assert!(action.writable_only());
    }
    for action in [
        RowAction::Open,
        RowAction::View,
        RowAction::Copy,
        RowAction::ExtractSelection,
        RowAction::TestSelection,
    ] {
        assert!(!action.writable_only());
    }
}

#[test]
fn the_row_menu_only_offers_the_clipboard_where_there_is_one() {
    // A menu entry that can never do anything is worse than no entry, so
    // copy, cut and paste are left out rather than greyed out.
    let offered: Vec<bool> = RowAction::ALL.iter().map(|a| a.offered()).collect();
    for action in [RowAction::Copy, RowAction::Cut, RowAction::Paste] {
        assert_eq!(offered[action as usize], clipboard::AVAILABLE);
    }
    assert!(offered[RowAction::Open as usize]);
    assert!(offered[RowAction::Delete as usize]);
    // The element id of each menu row is built from `action as usize`, so
    // an action added out of order would silently share an id.
    for (index, action) in RowAction::ALL.iter().enumerate() {
        assert_eq!(*action as usize, index);
    }
}

#[test]
fn the_menu_under_the_last_row_asks_for_nothing_that_needs_a_row() {
    // It is built with an index past the end, so an entry that reads the
    // row would do nothing at best, and act on a stale one at worst.
    for action in RowAction::EMPTY_SPACE {
        assert!(matches!(
            action,
            RowAction::NewFolder | RowAction::Paste | RowAction::SelectAll
        ));
    }
    // Making a folder is the reason the menu exists, and it is the one
    // entry the row menu must not repeat.
    assert!(RowAction::EMPTY_SPACE.contains(&RowAction::NewFolder));
}

#[test]
fn the_tree_works_a_place_from_inside_it_and_a_thing_from_above_it() {
    // `folder_action` navigates by this. Reading it the wrong way round
    // would make a folder in the folder above the one clicked, or rename
    // that one instead of the one the menu was opened on.
    for action in [
        RowAction::Open,
        RowAction::Paste,
        RowAction::SelectAll,
        RowAction::NewFolder,
    ] {
        assert!(action.about_the_place());
    }
    for action in [
        RowAction::Rename,
        RowAction::Delete,
        RowAction::Copy,
        RowAction::Cut,
        RowAction::CopyNames,
        RowAction::ExtractSelection,
        RowAction::TestSelection,
    ] {
        assert!(!action.about_the_place());
    }
}

#[test]
fn every_folder_of_the_tree_is_named_by_the_path_it_navigates_to() {
    // The id is what `Navigate` is dispatched with, so a nested folder
    // whose id is only its own name would send the window to the wrong
    // place -- or nowhere.
    let mut root = Folder::default();
    let src = root.kids.entry("src".to_string()).or_default();
    src.kids.entry("deep".to_string()).or_default();
    root.kids.entry("docs".to_string()).or_default();

    let items = folder_children(&root, "");
    let ids = items.iter().map(|item| item.1.clone()).collect::<Vec<_>>();
    assert_eq!(ids, vec!["docs/".to_string(), "src/".to_string()]);
    let nested = folder_children(items[1].2, &items[1].1);
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].1, "src/deep/");
    assert_eq!(nested[0].0, "deep");
}

#[test]
fn shortcuts_are_spelled_the_way_the_platform_spells_them() {
    assert_eq!(keystroke("ctrl-shift-c", false), "Ctrl+Shift+C");
    assert_eq!(keystroke("ctrl-shift-c", true), "⌘⇧C");
    assert_eq!(keystroke("alt-left", false), "Alt+←");
    assert_eq!(keystroke("+", false), "+");
    assert_eq!(keystroke("f5", false), "F5");
}

#[test]
fn a_modifier_shortcut_still_works_while_a_text_field_has_the_keyboard() {
    // Ctrl+O is never text, so it must not wait behind the filter box; F5
    // is a key a text field could want, so it must.
    assert_eq!(
        shortcut_for(true, false, false, "o", true),
        Some(Shortcut::Open)
    );
    assert_eq!(
        shortcut_for(true, true, false, "c", true),
        Some(Shortcut::CopyNames)
    );
    assert_eq!(
        shortcut_for(false, false, true, "w", true),
        Some(Shortcut::ExtractHere)
    );
    assert_eq!(shortcut_for(false, false, false, "f5", true), None);
    assert_eq!(shortcut_for(false, false, false, "escape", true), None);
    assert_eq!(
        shortcut_for(false, false, false, "f5", false),
        Some(Shortcut::Refresh)
    );
    // Ctrl+Shift+C is the names as text, not the files.
    assert_eq!(shortcut_for(true, true, false, "o", false), None);
    assert_eq!(shortcut_for(false, false, false, "q", false), None);
    // The keypad's plus picks a group and its minus drops one; both are
    // bare keys, so both stand aside for a text field.
    assert_eq!(
        shortcut_for(false, false, false, "+", false),
        Some(Shortcut::PickGroup(true))
    );
    assert_eq!(
        shortcut_for(false, false, false, "minus", false),
        Some(Shortcut::PickGroup(false))
    );
    assert_eq!(shortcut_for(false, false, false, "+", true), None);
    assert_eq!(
        shortcut_for(true, false, false, "z", false),
        Some(Shortcut::Undo)
    );
    assert_eq!(
        shortcut_for(false, false, false, "f3", false),
        Some(Shortcut::View)
    );
}

#[test]
fn history_shortcuts_yield_to_text_editing() {
    assert_eq!(shortcut_for(true, false, false, "z", true), None);
    for (key, action) in [("left", Shortcut::Back), ("right", Shortcut::Forward)] {
        assert_eq!(shortcut_for(false, false, true, key, false), Some(action));
        assert_eq!(shortcut_for(false, false, true, key, true), None);
        assert_eq!(shortcut_for(false, false, false, key, false), None);
    }
}

#[test]
fn context_destinations_name_the_clicked_folder_or_containing_directory() {
    let mut row = crate::up_row("parent/child/");
    for action in [RowAction::Paste, RowAction::NewFolder] {
        assert_eq!(
            action.destination(Some(&row), "parent/child/"),
            Some("parent/".into())
        );
        row.up = false;
        row.path = "parent/child/".into();
        assert_eq!(
            action.destination(Some(&row), "parent/"),
            Some(row.path.clone())
        );
        row.is_dir = false;
        assert_eq!(
            action.destination(Some(&row), "parent/"),
            Some("parent/".into())
        );
        assert_eq!(action.destination(None, ""), Some(String::new()));
        for lang in [super::super::Lang::En, super::super::Lang::Es] {
            let (label, _) = action.label(crate::i18n::strings(lang));
            assert_eq!(
                action.destination_label(label, None, ""),
                format!("{label}: /")
            );
        }
        row = crate::up_row("parent/child/");
    }
    for action in [RowAction::Delete, RowAction::Rename, RowAction::Copy] {
        assert!(action.destination(Some(&row), "parent/child/").is_none());
    }
}

#[test]
fn every_settings_control_has_its_own_slot_in_draw_order() {
    // The dialog builds each control's element id from `control as
    // usize`, so a control added out of order would silently share an
    // id with another one.
    for (index, control) in SettingsControl::ALL.iter().enumerate() {
        assert_eq!(*control as usize, index);
    }
}

#[test]
fn empty_folder_label_is_not_reported_as_archive_contents() {
    // Both languages, because the point of the label is that it says the
    // folder is empty rather than that the archive is unreadable, and a
    // translation that loses the difference is the same bug in Spanish.
    for lang in [super::super::Lang::En, super::super::Lang::Es] {
        let s = crate::i18n::strings(lang);
        assert_eq!(empty_state_aria_label(false, "", s), s.empty_folder);
        assert_eq!(empty_state_aria_label(false, "  ", s), s.empty_folder);
        assert_eq!(empty_state_aria_label(false, "zip", s), s.no_matches);
        assert_eq!(empty_state_aria_label(true, "", s), s.cannot_open);
        assert_ne!(s.empty_folder, s.cannot_open);
    }
}

#[test]
fn a_searched_row_says_where_it_is_below_the_folder_searched() {
    let row = crate::Row {
        label: "redist.txt".to_string(),
        path: "Game/_CommonRedist/DirectX/redist.txt".to_string(),
        kind: crate::tree::Kind::Text,
        is_dir: false,
        entry: Some(0),
        size: 1,
        packed: 1,
        method: "deflate",
        encrypted: false,
        zipcrypto: false,
        count: 0,
        mtime: None,
        created: None,
        accessed: None,
        attributes: 0,
        crc32: None,
        up: false,
    };
    let s = crate::i18n::strings(super::super::Lang::En);
    // Searching from a folder: the way back to the archive root is already
    // in the breadcrumbs and does not need repeating on every row.
    assert_eq!(
        column_text(&row, SortColumn::Path, s, "Game/_CommonRedist/"),
        "DirectX"
    );
    // A hit sitting directly in the folder being searched has nothing to add.
    assert_eq!(
        column_text(&row, SortColumn::Path, s, "Game/_CommonRedist/DirectX/"),
        ""
    );
    // The flat view reads from the top, so it keeps the whole folder.
    assert_eq!(
        column_text(&row, SortColumn::Path, s, ""),
        "Game/_CommonRedist/DirectX"
    );
}

#[test]
fn arrow_keys_move_the_cursor_by_the_names_egui_gives_them() {
    let name = |key: egui::Key| key.name().to_ascii_lowercase();
    assert_eq!(
        cursor_step(&name(egui::Key::ArrowDown), Some(0), 5),
        Some(1)
    );
    assert_eq!(cursor_step(&name(egui::Key::ArrowUp), Some(3), 5), Some(2));
    assert_eq!(
        cursor_step(&name(egui::Key::ArrowDown), Some(5), 5),
        Some(5)
    );
    assert_eq!(cursor_step(&name(egui::Key::ArrowUp), None, 5), Some(0));
    assert_eq!(cursor_step(&name(egui::Key::PageDown), Some(0), 5), Some(5));
    assert_eq!(cursor_step(&name(egui::Key::End), Some(0), 5), Some(5));
    assert_eq!(cursor_step(&name(egui::Key::Home), Some(4), 5), Some(0));
    assert_eq!(cursor_step(&name(egui::Key::A), Some(4), 5), None);
}
