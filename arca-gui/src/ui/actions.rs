use super::{COMPACT_SIZE, NORMAL_SIZE};
use crate::*;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowAction {
    Open,
    ExtractSelection,
    ExtractHere,
    TestSelection,
    View,
    Rename,
    Delete,
    Copy,
    Cut,
    Paste,
    CopyNames,
    SelectAll,
    NewFolder,
    Compress,
    CopyPath,
    Pin,
    OpenArchive,
    ExtractAll,
    TestArchive,
}

impl RowAction {
    pub(crate) const ALL: [RowAction; 19] = [
        RowAction::Open,
        RowAction::ExtractSelection,
        RowAction::ExtractHere,
        RowAction::TestSelection,
        RowAction::View,
        RowAction::Rename,
        RowAction::Delete,
        RowAction::Copy,
        RowAction::Cut,
        RowAction::Paste,
        RowAction::CopyNames,
        RowAction::SelectAll,
        RowAction::NewFolder,
        RowAction::Compress,
        RowAction::CopyPath,
        RowAction::Pin,
        RowAction::OpenArchive,
        RowAction::ExtractAll,
        RowAction::TestArchive,
    ];

    /// What the menu offers where there is no row under the pointer: the
    /// space under the last one is the folder itself, not an entry.
    /// The menu over the empty part of the list also carries what the
    /// toolbar used to: opening another archive, compressing, extracting
    /// and testing the whole thing.
    pub(crate) const EMPTY_SPACE: [RowAction; 7] = [
        RowAction::NewFolder,
        RowAction::Paste,
        RowAction::SelectAll,
        RowAction::OpenArchive,
        RowAction::Compress,
        RowAction::ExtractAll,
        RowAction::TestArchive,
    ];

    /// Whether the entry is about the place being pointed at rather than the
    /// thing sitting in it. The tree runs the first kind inside the folder and
    /// the second from the folder above it, with that folder picked alone.
    pub(crate) fn about_the_place(self) -> bool {
        matches!(
            self,
            RowAction::Open
                | RowAction::Paste
                | RowAction::SelectAll
                | RowAction::NewFolder
                | RowAction::OpenArchive
                | RowAction::ExtractAll
                | RowAction::TestArchive
        )
    }

    pub(crate) fn destination(self, row: Option<&Row>, directory: &str) -> Option<String> {
        matches!(self, Self::Paste | Self::NewFolder).then(|| {
            row.filter(|row| row.is_dir)
                .map_or(directory, |row| row.path.as_str())
                .to_string()
        })
    }

    /// Pasting into, or making a folder inside, the folder under the pointer
    /// says so by that folder's name; inside the folder being shown it says
    /// nothing, because that is what the entry always means.
    pub(crate) fn destination_label(
        self,
        label: &str,
        row: Option<&Row>,
        directory: &str,
        s: &'static Strings,
    ) -> String {
        match self.destination(row, directory) {
            Some(path) if path != directory => {
                let leaf = path
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(&path);
                format!("{label} {}", fill(s.in_named, &[("name", leaf)]))
            }
            _ => label.to_string(),
        }
    }

    /// What it is called, and the keys that do the same thing. A menu that does
    /// not name the shortcut is a menu nobody graduates from.
    pub(crate) fn label(self, s: &'static Strings) -> (&'static str, &'static str) {
        match self {
            RowAction::Open => (s.open_word, "Enter"),
            RowAction::ExtractSelection => (s.extract_selected, ""),
            RowAction::ExtractHere => (s.extract_here, "Alt+W"),
            RowAction::TestSelection => (s.test_selection, ""),
            RowAction::View => (s.view_word, "F3"),
            RowAction::Rename => (s.rename_word, "F2"),
            RowAction::Delete => (s.delete_word, "Supr"),
            RowAction::Copy => (s.copy_word, "Ctrl+C"),
            RowAction::Cut => (s.cut_word, "Ctrl+X"),
            RowAction::Paste => (s.paste_word, "Ctrl+V"),
            RowAction::CopyNames => (s.copy_names, "Ctrl+Shift+C"),
            RowAction::SelectAll => (s.select_all, "Ctrl+A"),
            RowAction::NewFolder => (s.new_folder, ""),
            RowAction::Compress => (s.compress_selection, "Ctrl+N"),
            RowAction::CopyPath => (s.copy_path, ""),
            RowAction::Pin => (s.pin_word, ""),
            RowAction::OpenArchive => (s.open, "Ctrl+O"),
            RowAction::ExtractAll => (s.extract_all, "Ctrl+E"),
            RowAction::TestArchive => (s.test_word, "Ctrl+T"),
        }
    }

    /// The Phosphor glyph drawn in front of the menu entry.
    pub(crate) fn glyph(self) -> &'static str {
        use egui_phosphor::regular as icon;
        match self {
            RowAction::Open => icon::ARROW_SQUARE_OUT,
            RowAction::ExtractSelection | RowAction::ExtractAll => icon::EXPORT,
            RowAction::ExtractHere => icon::DOWNLOAD_SIMPLE,
            RowAction::TestSelection | RowAction::TestArchive => icon::CHECK_CIRCLE,
            RowAction::View => icon::EYE,
            RowAction::Rename => icon::PENCIL_SIMPLE,
            RowAction::Delete => icon::TRASH,
            RowAction::Copy => icon::COPY,
            RowAction::Cut => icon::SCISSORS,
            RowAction::Paste => icon::CLIPBOARD,
            RowAction::CopyNames => icon::CLIPBOARD_TEXT,
            RowAction::SelectAll => icon::SELECTION_ALL,
            RowAction::NewFolder => icon::FOLDER_PLUS,
            RowAction::Compress => icon::PACKAGE,
            RowAction::CopyPath => icon::LINK,
            RowAction::Pin => icon::PUSH_PIN,
            RowAction::OpenArchive => icon::FOLDER_OPEN,
        }
    }

    /// Whether a rule goes above this one. Looking at an entry, changing it,
    /// moving it through the clipboard and working on the selection are four
    /// different things, and twelve entries in one run is a wall.
    pub(crate) fn starts_group(self) -> bool {
        matches!(
            self,
            RowAction::Rename
                | RowAction::Copy
                | RowAction::CopyNames
                | RowAction::NewFolder
                | RowAction::OpenArchive
        )
    }

    /// Copy, cut and paste are left out rather than greyed out where the shell
    /// has nowhere to put them: a menu entry that can never do anything is
    /// worse than no entry.
    pub(crate) fn writable_only(self) -> bool {
        matches!(
            self,
            Self::Rename | Self::Delete | Self::Cut | Self::Paste | Self::NewFolder
        )
    }

    pub(crate) fn offered(self) -> bool {
        !matches!(self, RowAction::Copy | RowAction::Cut | RowAction::Paste) || clipboard::AVAILABLE
    }

    /// Whether the entry makes sense where the list is showing: the disk has
    /// nothing to extract or test, and an archive has no path to copy.
    pub(crate) fn shown(self, on_disk: bool) -> bool {
        if on_disk {
            !matches!(
                self,
                Self::ExtractSelection
                    | Self::ExtractHere
                    | Self::TestSelection
                    | Self::Rename
                    | Self::Delete
                    | Self::Copy
                    | Self::Cut
                    | Self::Paste
                    | Self::NewFolder
                    | Self::ExtractAll
                    | Self::TestArchive
            )
        } else {
            !matches!(self, Self::Compress | Self::CopyPath | Self::Pin)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsControl {
    LangSystem,
    LangEn,
    LangEs,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    Updates,
    Subfolder,
}

impl SettingsControl {
    pub(crate) const ALL: [SettingsControl; 8] = [
        SettingsControl::LangSystem,
        SettingsControl::LangEn,
        SettingsControl::LangEs,
        SettingsControl::ThemeSystem,
        SettingsControl::ThemeLight,
        SettingsControl::ThemeDark,
        SettingsControl::Updates,
        SettingsControl::Subfolder,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    General,
    Archives,
    Appearance,
    Views,
    Keybindings,
    Updates,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalKind {
    Password,
    Conflict,
    Delete,
    Drop,
    Add,
    NewFolder,
    Mask,
    Settings,
    Shortcuts,
    Search,
}

pub(crate) fn column_text(
    row: &Row,
    column: SortColumn,
    s: &'static Strings,
    root: &str,
) -> String {
    {
        match column {
            SortColumn::Name => row.label.clone(),
            SortColumn::Size => human(row.size),
            SortColumn::Packed => human(row.packed),
            SortColumn::Method => {
                if row.is_dir {
                    format!("{} {}", row.count, s.items_word)
                } else if row.encrypted {
                    let scheme = if row.zipcrypto {
                        "ZipCrypto"
                    } else {
                        "AES-256"
                    };
                    format!("{scheme} {}", row.method)
                } else {
                    row.method.to_string()
                }
            }
            SortColumn::Saved => format!("{:.0}%", saved_of(row) * 100.0),
            SortColumn::Modified => when(row.mtime),
            SortColumn::Created => when(row.created),
            SortColumn::Accessed => when(row.accessed),
            SortColumn::Attributes => attribute_letters(row.attributes),
            SortColumn::Crc => {
                if row.is_dir {
                    "—".to_string()
                } else {
                    row.crc32
                        .map(|crc| format!("{crc:08X}"))
                        .unwrap_or_default()
                }
            }
            SortColumn::Type => arca_icons::cache_key(&row.label, row.is_dir),
            SortColumn::Path => {
                let folder = folder_of(&row.path);
                folder
                    .strip_prefix(root.trim_end_matches('/'))
                    .map(|rest| rest.trim_start_matches('/'))
                    .unwrap_or(folder)
                    .to_string()
            }
        }
    }
}

#[derive(Clone)]
pub(crate) enum DialogKind {
    Open,
    /// What to put in a new archive. Windows has one native dialog for files
    /// and another for folders, so which one this opens has to be said up
    /// front; the picks pile up in the same list either way.
    PickInputs {
        folders: bool,
    },
    Extract {
        only_checked: bool,
    },
    /// Files or folders to put inside the archive that is already open.
    AddFiles {
        folders: bool,
    },
    /// A copy of the open archive under another name, which is the thing to do
    /// before a change nobody is sure about.
    SaveCopy {
        name: String,
        directory: PathBuf,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OverflowAction {
    Release,
    AddFiles,
    AddFolders,
    NewFolder,
    Undo,
    SaveCopy,
}

/// A selection being drawn by pulling across the list.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Shortcut {
    Open,
    Compress,
    ExtractAll,
    ExtractHere,
    Test,
    Refresh,
    Invert,
    ClearSelection,
    CopyNames,
    Shortcuts,
    Undo,
    Rename,
    View,
    Back,
    Forward,
    EditPath,
    Search,
    /// A mask that picks names, or one that drops them.
    PickGroup(bool),
}

/// Reading a key press, with no state and no side effects, so the table of
/// shortcuts can be checked without a window.
///
/// `typing` only silences the keys that carry no modifier: F5 inside a filter
/// box is a key, but Ctrl+O is never text.
pub(crate) fn shortcut_for(
    secondary: bool,
    shift: bool,
    alt: bool,
    key: &str,
    typing: bool,
) -> Option<Shortcut> {
    if secondary && shift {
        return (key == "c").then_some(Shortcut::CopyNames);
    }
    if secondary {
        return match key {
            "o" => Some(Shortcut::Open),
            "n" => Some(Shortcut::Compress),
            "e" => Some(Shortcut::ExtractAll),
            "t" => Some(Shortcut::Test),
            "l" => Some(Shortcut::EditPath),
            "k" => Some(Shortcut::Search),
            "i" => Some(Shortcut::Invert),
            "z" if !typing => Some(Shortcut::Undo),
            _ => None,
        };
    }
    if alt {
        return match key {
            "w" => Some(Shortcut::ExtractHere),
            "left" | "arrowleft" if !typing => Some(Shortcut::Back),
            "right" | "arrowright" if !typing => Some(Shortcut::Forward),
            _ => None,
        };
    }
    if typing {
        return None;
    }
    match key {
        "f1" => Some(Shortcut::Shortcuts),
        "f2" => Some(Shortcut::Rename),
        "f3" => Some(Shortcut::View),
        "f5" => Some(Shortcut::Refresh),
        "escape" => Some(Shortcut::ClearSelection),
        // The keypad plus and minus, where WinRAR has kept picking a group by
        // name since before there were menus to put it in. Its third one, the
        // keypad star for inverting, cannot be told from any other asterisk by
        // a toolkit, so that one stays on Ctrl+I alone.
        "+" | "plus" | "add" => Some(Shortcut::PickGroup(true)),
        "-" | "minus" | "subtract" => Some(Shortcut::PickGroup(false)),
        _ => None,
    }
}

pub(crate) fn background_event_allowed(modal: bool, native_picker: bool) -> bool {
    !modal && !native_picker
}

pub(crate) fn clipboard_action_allowed(
    available: bool,
    idle: bool,
    has_archive: bool,
    selected: usize,
    needs_selection: bool,
) -> bool {
    available && idle && has_archive && (!needs_selection || selected > 0)
}

pub(crate) fn empty_state_aria_label(
    error: bool,
    filter: &str,
    s: &'static Strings,
) -> &'static str {
    if error {
        s.cannot_open
    } else if filter.trim().is_empty() {
        s.empty_folder
    } else {
        s.no_matches
    }
}

pub(crate) fn apply_startup(controller: &mut AppController, startup: Startup) {
    // A window opened by the Explorer entries exists for that one job and
    // closes when it is done; one opened to browse stays.
    controller.state.one_shot = !matches!(startup, Startup::Browse(_));
    match startup {
        Startup::Browse(Some(path)) => controller.open(path),
        Startup::Browse(None) => controller.start_browsing(),
        Startup::Run(job) => controller.run_job(job),
        Startup::Add(files) => controller.dispatch(AppAction::PrepareCompress(files)),
    }
}

pub(crate) fn shell_size(compact: bool) -> (f32, f32) {
    if compact {
        COMPACT_SIZE
    } else {
        NORMAL_SIZE
    }
}
