mod columns;
mod details;
mod dialogs;
mod header;
mod jobs;
mod layout;
mod preview;
mod sidebar;
mod table;
use dialogs::*;
use table::FileTable;

use super::{
    clock, fill, hex_line, human, parent_of, saved_of, when, Answer, AppAction, AppController,
    Codec, Columns, DropChoice, Format, Job, Lang, Level, Look, Pending, Row, Settings, SortColumn,
    Startup, Strings, ThemePreference, View, SIDEBAR_LEAST, SIDEBAR_MOST,
};
use crate::{
    clipboard, disk_dir, disk_path, gpui_theme,
    tree::{children_of, Folder, Kind},
    Place, PlaceKind,
};
use gpui::{actions, point};
use gpui::{
    canvas, div, prelude::*, px, size, uniform_list, App, Bounds, ClickEvent, Context, ElementId,
    Entity, ExternalPaths, FocusHandle, Focusable, KeyBinding, KeyDownEvent, Role, ScrollStrategy,
    Stateful, UniformListScrollHandle, WeakEntity, Window, WindowBounds, WindowOptions,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::dialog::{Dialog, DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::kbd::Kbd;
use gpui_component::list::ListItem;
use gpui_component::menu::ContextMenuExt;
use gpui_component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_component::progress::Progress;
use gpui_component::radio::RadioGroup;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::separator::Separator;
use gpui_component::switch::Switch;
use gpui_component::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use gpui_component::tree::{tree, TreeItem, TreeState};
use gpui_component::{h_resizable, resizable_panel, ResizableState};
use gpui_component::{ActiveTheme, Icon, IconName};
use gpui_component::{Disableable, Root, Selectable, Sizable as _, TitleBar, WindowExt};
use gpui_platform::application;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

fn brand_button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .font_family(gpui_theme::display_font())
        .font_weight(gpui::FontWeight::SEMIBOLD)
}

fn heading(label: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .font_family(gpui_theme::display_font())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.into())
}

fn brand_mark(width: f32) -> gpui::Img {
    gpui::img("brand/mark.svg")
        .w(px(width))
        .h(px(width * 139. / 220.))
        .flex_none()
}

/// The ring GPUI paints around whatever the keyboard is on.
///
/// One function rather than seven copies of the same closure, because a focus
/// ring that is not identical everywhere is a focus ring you have to look for.
/// It reads the tokens once and carries them into the closure, since
/// `focus_visible` runs without a context.
fn focus_ring(cx: &App) -> impl FnOnce(gpui::StyleRefinement) -> gpui::StyleRefinement {
    let ring = cx.theme().ring;
    move |style: gpui::StyleRefinement| style.border_2().border_color(ring)
}

/// The room a folder row has inside a sidebar `width` wide, before it has to be
/// scrolled to: both sides of the padding, plus the right border, which eats
/// into the content box and would otherwise clip the selected row's outline.
fn sidebar_inner(width: f32) -> f32 {
    (width - 17.0).max(1.0)
}

/// La ventana que abre el menu contextual del Explorador: existe para un
/// formulario y se cierra cuando el trabajo acaba.
///
/// Medida para que el formulario de anadir quepa entero. A 560 de ancho no
/// cabia -- el formulario pide 600 -- y asomaba la ventana por los lados.
const COMPACT_SIZE: (f32, f32) = (660.0, 380.0);
const NORMAL_SIZE: (f32, f32) = (1000.0, 660.0);
/// Lo mas pequena que se le deja ser a la ventana.
///
/// El ancho es el que necesita la fila de comandos con el campo de filtrar
/// entero detras. A 720 no le queda y el campo se sale por el borde derecho.
/// Medido: a 727 cortado, a 745 ya no, y 800 deja margen.
const MINIMUM_SIZE: (f32, f32) = (800.0, 320.0);

/// How many recently opened archives the menu keeps room for. Ten is about as
/// many as anybody scans before giving up and going to the folder instead.
const RECENT_MAX: usize = 10;

actions!(arca_gpui, [FocusFilter, CopyFiles, CutFiles, PasteFiles]);

/// What came back from a native file dialog, which runs off the main thread.
enum DialogResult {
    Open(Option<PathBuf>),
    /// What to compress into a new archive, added to whatever was picked
    /// before it.
    Inputs(Option<Vec<PathBuf>>),
    Extract {
        only_checked: bool,
        destination: Option<PathBuf>,
    },
    AddFiles(Option<Vec<PathBuf>>),
    SaveCopy(Option<PathBuf>),
}

struct GpuiShell {
    controller: AppController,
    filter: Entity<InputState>,
    /// The field that replaces a name while it is being renamed. Renaming
    /// happens in place, the way every file manager does it, so there is no
    /// dialog for it.
    rename_input: Entity<InputState>,
    /// Which panel the field belongs to. One field cannot be drawn twice, and
    /// a folder being renamed is a row of the list and a branch of the tree at
    /// the same time: whichever was pointed at is the one that gets to show it.
    renaming_in_tree: bool,
    password: Entity<InputState>,
    output_name: Entity<InputState>,
    add_password: Entity<InputState>,
    name_input: Entity<InputState>,
    focus_handle: FocusHandle,
    list_focus: FocusHandle,
    /// The same handle the kit's table scrolls with. The band needs the list's
    /// geometry and reads it from here, so both must be looking at one handle.
    list_scroll: UniformListScrollHandle,
    /// The kit's table, and the copy of the frame it draws from.
    table: Entity<TableState<FileTable>>,
    dialog: Option<Receiver<DialogResult>>,
    /// Which modal the kit's dialog stack currently holds. The shell derives
    /// its modal from controller state, the kit opens and closes one
    /// imperatively; this is what tells the two apart.
    open_modal: Option<ModalKind>,
    delete_trigger_focus: FocusHandle,
    drop_trigger_focus: FocusHandle,
    conflict_trigger_focus: FocusHandle,
    /// Whether the list of operations is on screen, and the newest task it
    /// has been opened for, so a new one opens it and a hidden one stays
    /// hidden.
    jobs_open: bool,
    jobs_seen: crate::controller::TaskId,
    /// Where the keyboard goes when a dialog closes, when somebody asked for
    /// it. None means leave it alone: a kit button holds its own focus and
    /// still has it when a native file dialog closes on top of it.
    dialog_return_focus: Option<FocusHandle>,
    modal_seen: Option<ModalKind>,
    dialog_primary_focus: FocusHandle,
    dialog_cancel_focus: FocusHandle,
    add_start_focus: FocusHandle,
    /// The twelve controls of the settings dialog, in the order they are drawn.
    /// One vector rather than twelve fields because every one of them is the
    /// same thing -- a row in a list of preferences -- and `SettingsControl`
    /// already says which is which.
    settings_focus: Vec<FocusHandle>,
    settings_section: SettingsSection,
    /// Which row the right button was pressed on, and where the pointer was,
    /// so the menu opens under it instead of in a fixed corner.
    /// The band being drawn, while it is being drawn.
    band: Option<Band>,
    /// The wheel used as a button: press it and the list runs towards the
    /// pointer until something puts it away.
    wheel: Option<WheelPan>,
    /// Where the pointer was last seen, so the tick that keeps the list running
    /// knows which way to go without an event of its own.
    pointer: gpui::Point<gpui::Pixels>,
    /// A selection is in the air. Where it lands is not decided until it leaves
    /// the list or is dropped on a folder.
    carrying: bool,
    /// How many rows the list last drew.
    ///
    /// Kept because `visible_rows` walks every entry, allocates a `Row` for
    /// each and sorts the lot: asking it for nothing but a count, twice per
    /// pointer move, was most of why dragging a band felt heavy. Render is the
    /// one place that already has the answer.
    row_count: usize,
    /// Which column edge is in hand: its slot in `Settings::widths`, where the
    /// pointer was when it was grabbed, and how wide the column was then.
    /// Kept as the state at the grab rather than as a running delta, so a
    /// dropped mouse-move event cannot make the column drift.
    resizing: Option<(usize, f32, f32)>,
    /// What the shared `Name` field currently holds. The controller has no
    /// business knowing about a half-typed folder name, so it stays here until
    /// the dialog is answered.
    name_value: String,
    /// Text, Hex and Picture, in that order.
    preview_password: Entity<InputState>,
    preview_return_focus: Option<FocusHandle>,
    viewer_scroll: UniformListScrollHandle,
    /// The decoded picture, kept by the name it came out of the archive under.
    /// Handing GPUI a fresh `Image` every frame would decode a thirty megabyte
    /// photograph sixty times a second.
    viewer_image: Option<(u64, std::sync::Arc<gpui::RenderImage>)>,
    /// The archive's folders as the kit's tree, and the archive they were
    /// grown from. Rebuilding the items every frame would throw away which
    /// branches are open, so they are grown once per archive and the key says
    /// when that archive stopped being the same one.
    folders: Entity<TreeState>,
    folders_key: Option<(Option<PathBuf>, usize, u64)>,
    /// How the sidebar and the file list divide the window between them.
    sidebar_state: Entity<ResizableState>,
    preview_state: Entity<ResizableState>,
    columns_state: Entity<ResizableState>,
    pane_surfaces: Vec<columns::PaneSurface>,
    columns_scroll: gpui::ScrollHandle,
    revealed_pane: Option<usize>,
    workspace_width: f32,
    workspace_height: f32,
    effective_layout: Option<layout::Layout>,
    filter_open: bool,
}

/// What the menu on a row offers. Everything here already exists as a
/// controller call or a dialog; the menu is only a second way in, for the
/// times the hand is already down on the list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAction {
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
}

impl RowAction {
    const ALL: [RowAction; 16] = [
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
    ];

    /// What the menu offers where there is no row under the pointer: the
    /// space under the last one is the folder itself, not an entry.
    const EMPTY_SPACE: [RowAction; 3] =
        [RowAction::NewFolder, RowAction::Paste, RowAction::SelectAll];

    /// Whether the entry is about the place being pointed at rather than the
    /// thing sitting in it. The tree runs the first kind inside the folder and
    /// the second from the folder above it, with that folder picked alone.
    fn about_the_place(self) -> bool {
        matches!(
            self,
            RowAction::Open | RowAction::Paste | RowAction::SelectAll | RowAction::NewFolder
        )
    }

    fn destination(self, row: Option<&Row>, directory: &str) -> Option<String> {
        matches!(self, Self::Paste | Self::NewFolder).then(|| {
            row.filter(|row| row.is_dir)
                .map_or(directory, |row| row.path.as_str())
                .to_string()
        })
    }

    fn destination_label(self, label: &str, row: Option<&Row>, directory: &str) -> String {
        self.destination(row, directory)
            .map_or_else(|| label.to_string(), |path| format!("{label}: /{path}"))
    }

    /// What it is called, and the keys that do the same thing. A menu that does
    /// not name the shortcut is a menu nobody graduates from.
    fn label(self, s: &'static Strings) -> (&'static str, &'static str) {
        match self {
            RowAction::Open => (s.open_word, "Enter"),
            RowAction::ExtractSelection => (s.extract_selected, "Ctrl+E"),
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
        }
    }

    /// Whether a rule goes above this one. Looking at an entry, changing it,
    /// moving it through the clipboard and working on the selection are four
    /// different things, and twelve entries in one run is a wall.
    fn starts_group(self) -> bool {
        matches!(
            self,
            RowAction::Rename | RowAction::Copy | RowAction::CopyNames | RowAction::NewFolder
        )
    }

    /// Copy, cut and paste are left out rather than greyed out where the shell
    /// has nowhere to put them: a menu entry that can never do anything is
    /// worse than no entry.
    fn writable_only(self) -> bool {
        matches!(
            self,
            Self::Rename | Self::Delete | Self::Cut | Self::Paste | Self::NewFolder
        )
    }

    fn offered(self) -> bool {
        !matches!(self, RowAction::Copy | RowAction::Cut | RowAction::Paste) || clipboard::AVAILABLE
    }

    /// Whether the entry makes sense where the list is showing: the disk has
    /// nothing to extract or test, and an archive has no path to copy.
    fn shown(self, on_disk: bool) -> bool {
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
            )
        } else {
            !matches!(self, Self::Compress | Self::CopyPath | Self::Pin)
        }
    }
}

fn row_action_icon(action: RowAction) -> Option<Icon> {
    match action {
        RowAction::Open => Some(Icon::new(IconName::FolderOpen)),
        RowAction::ExtractSelection => Some(Icon::empty().path("icons/file-output.svg")),
        RowAction::ExtractHere => Some(Icon::new(IconName::PanelBottomOpen)),
        RowAction::TestSelection => Some(Icon::new(IconName::Check)),
        RowAction::View => Some(Icon::new(IconName::Eye)),
        RowAction::Rename => Some(Icon::new(IconName::Replace)),
        RowAction::Delete => Some(Icon::empty().path("icons/trash-2.svg")),
        RowAction::Copy => Some(Icon::new(IconName::Copy)),
        RowAction::Cut => Some(Icon::empty().path("icons/scissors.svg")),
        RowAction::Paste => Some(Icon::empty().path("icons/clipboard-paste.svg")),
        RowAction::CopyNames => Some(Icon::new(IconName::FileText)),
        RowAction::SelectAll => Some(Icon::new(IconName::Check)),
        RowAction::NewFolder => Some(Icon::new(IconName::Folder)),
        RowAction::Compress => Some(Icon::new(IconName::Inbox)),
        RowAction::CopyPath => Some(Icon::new(IconName::Copy)),
        RowAction::Pin => Some(Icon::empty().path("icons/pin.svg")),
    }
}

/// A control in the settings dialog whose choices are named rather than
/// listed: the language, the theme, and the switches for updates and
/// extraction. The settings that pick a value out of a list go through `pick`
/// and need no name of their own.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsControl {
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
    const ALL: [SettingsControl; 8] = [
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
enum SettingsSection {
    General,
    Appearance,
    Keybindings,
    Updates,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModalKind {
    Password,
    Conflict,
    Delete,
    Drop,
    Add,
    NewFolder,
    Mask,
    Settings,
    Shortcuts,
}

/// One folder and everything under it, as the kit's tree items.
///
/// The id is the path the folder is reached by, which is what `Navigate`
/// takes, so the tree needs no table on the side to say what a row means.
fn folder_items(folder: &Folder, path: &str) -> Vec<TreeItem> {
    folder
        .kids
        .iter()
        .map(|(label, kid)| {
            let id = format!("{path}{label}/");
            TreeItem::new(id.clone(), label.clone()).children(folder_items(kid, &id))
        })
        .collect()
}

impl GpuiShell {
    fn new(window: &mut Window, cx: &mut Context<Self>, startup: Startup) -> Self {
        let owner = cx.weak_entity();
        let strings = super::strings(Settings::load().effective_lang());
        let filter = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(strings.find_word)
                .context_menu(false)
        });
        cx.subscribe_in(&filter, window, |shell, state, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                if value != shell.controller.state.filter {
                    shell.controller.dispatch(AppAction::SetFilter(value));
                }
                cx.notify();
            }
        })
        .detach();
        let rename_input = cx.new(|cx| InputState::new(window, cx).context_menu(false));
        let list_scroll = UniformListScrollHandle::new();
        let table = cx.new(|cx| {
            TableState::new(
                FileTable {
                    shell: owner.clone(),
                    rows: Vec::new(),
                    root: String::new(),
                    columns: Vec::new(),
                    widths: Settings::default_widths(),
                    checked: Vec::new(),
                    muted: Vec::new(),
                    cursor: None,
                    renaming: None,
                    rename_input: rename_input.clone(),
                    order: (SortColumn::Name, true),
                    strings,
                    idle: true,
                    writable: false,
                    on_disk: false,
                },
                window,
                cx,
            )
            // One row at a time is the kit's idea of a selection; this list
            // marks many, and keeps that count itself.
            .row_selectable(true)
            .col_selectable(false)
            .col_movable(false)
        });
        // The band measures the list through this handle, so it has to be the
        // one the table actually scrolls.
        table.update(cx, |state, _| {
            state.vertical_scroll_handle = list_scroll.clone();
        });
        cx.subscribe_in(
            &table,
            window,
            |shell, _, event: &TableEvent, window, cx| {
                match event {
                    // The kit moves its own selected row with the arrows and the
                    // mouse; the shell's cursor follows it, and the marking stays
                    // the shell's business.
                    TableEvent::SelectRow(row) => {
                        shell
                            .controller
                            .set_pane_cursor(shell.controller.state.browser.active, Some(*row));
                        cx.notify();
                    }
                    // Right clicking something that is not picked picks it, which
                    // is what every file list does; right clicking inside a
                    // selection leaves the selection alone.
                    TableEvent::RightClickedRow(Some(row)) => {
                        if let Some(target) = shell.controller.visible_rows().get(*row).cloned() {
                            if !shell.controller.is_checked(&target) {
                                shell.controller.clear_picked();
                                shell.controller.dispatch(AppAction::SetChecked {
                                    row: target,
                                    value: true,
                                });
                            }
                        }
                        shell
                            .controller
                            .set_pane_cursor(shell.controller.state.browser.active, Some(*row));
                        cx.notify();
                    }
                    // Written when the kit says the pull is over, so dragging an
                    // edge across the window is not a stream of writes to disk.
                    TableEvent::ColumnWidthsChanged(widths) => {
                        shell.remember_widths(widths, cx);
                    }
                    _ => {}
                }
                let _ = window;
            },
        )
        .detach();
        cx.subscribe_in(&rename_input, window, |shell, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => shell.commit_rename(window, cx),
                // Clicking away is how a file manager abandons a rename. It
                // cancels rather than commits: a name changed by accident in an
                // archive costs a full rewrite to undo.
                InputEvent::Blur => shell.cancel_rename(cx),
                _ => {}
            }
        })
        .detach();
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(strings.password_word)
                .masked(true)
        });
        let output_name = cx.new(|cx| InputState::new(window, cx).placeholder(strings.output_name));
        let add_password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(strings.password_optional)
                .masked(true)
        });
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(strings.folder_name));
        // Every dialog field lands in a different place, but they all land the
        // same way: one subscription each, so a new field is one arm here
        // rather than a second path that can be forgotten.
        cx.subscribe(&password, |shell, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let value = state.read(cx).value().to_string();
                shell
                    .controller
                    .dispatch(AppAction::SetPasswordInput(value));
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&output_name, |shell, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                shell.controller.state.output_name = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&add_password, |shell, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                shell.controller.state.add_password = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&name_input, |shell, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                shell.name_value = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        // The tree owns which folder is picked; the shell hears about it and
        // goes there. The guard is what keeps the two from chasing each other:
        // `sync_folders` puts the mark back on the folder the list is showing,
        // and that must not read as a request to move.
        let folders = cx.new(|cx| TreeState::new(cx));
        cx.observe(&folders, |shell, state, cx| {
            let Some(path) = state
                .read(cx)
                .selected_item()
                .map(|item| item.id.to_string())
            else {
                return;
            };
            if path == shell.controller.state.current_dir || !shell.background_idle() {
                return;
            }
            shell.controller.dispatch(AppAction::Navigate(path));
            shell.route_changed(cx);
        })
        .detach();
        let _ = owner;
        let mut controller = AppController::new(Settings::load());
        let columns =
            controller.state.settings.browser_view == crate::settings::BrowserView::Columns;
        let flat = controller.state.settings.flat;
        controller.transition_browser_view(columns, flat);
        apply_startup(&mut controller, startup);
        // The stored preference decides light, dark or whatever the desktop is
        // set to, and it has to land before the first frame or the window opens
        // in one theme and repaints into the other.
        gpui_theme::apply(controller.state.settings.theme, Some(window), cx);
        window.set_window_title(&controller.state.window_title);
        let view = cx.weak_entity();
        window
            .spawn(cx, async move |async_cx| loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if view
                    .update_in(async_cx, |shell, window, cx| {
                        shell.poll_dialog(window, cx);
                        // The list keeps running while the hand is still, which
                        // is the whole point of the gesture: without a step
                        // here it would only move on a stray mouse event.
                        let pointer = shell.pointer;
                        shell.tick_wheel(pointer);
                        shell.controller.ask_about_updates();
                        let conflict_was_open = shell.controller.conflict().is_some();
                        let close_window = shell.controller.receive();
                        if close_window {
                            window.remove_window();
                        } else if !conflict_was_open && shell.controller.conflict().is_some() {
                            shell.remember_conflict_focus();
                        }
                        if shell.controller.state.cut_pending.is_some() {
                            shell.controller.cut_landed();
                        }
                        shell.notice_new_tasks();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            })
            .detach();
        Self {
            controller,
            filter,
            rename_input,
            renaming_in_tree: false,
            password,
            output_name,
            add_password,
            name_input,
            focus_handle: cx.focus_handle(),
            list_focus: cx.focus_handle(),
            list_scroll,
            table,
            dialog: None,
            open_modal: None,
            delete_trigger_focus: cx.focus_handle(),
            drop_trigger_focus: cx.focus_handle(),
            conflict_trigger_focus: cx.focus_handle(),
            jobs_open: false,
            jobs_seen: 0,
            dialog_return_focus: None,
            modal_seen: None,
            dialog_primary_focus: cx.focus_handle().tab_stop(true),
            dialog_cancel_focus: cx.focus_handle().tab_stop(true),
            add_start_focus: cx.focus_handle().tab_stop(true),
            settings_section: SettingsSection::General,
            settings_focus: SettingsControl::ALL
                .iter()
                .map(|_| cx.focus_handle().tab_stop(true))
                .collect(),
            band: None,
            wheel: None,
            pointer: point(px(0.), px(0.)),
            carrying: false,
            row_count: 0,
            resizing: None,
            name_value: String::new(),
            preview_password: cx
                .new(|cx| InputState::new(window, cx).masked(true).context_menu(false)),
            preview_return_focus: None,
            viewer_scroll: UniformListScrollHandle::new(),
            viewer_image: None,
            folders,
            folders_key: None,
            sidebar_state: cx.new(|_| ResizableState::default()),
            preview_state: cx.new(|_| ResizableState::default()),
            columns_state: cx.new(|_| ResizableState::default()),
            pane_surfaces: Vec::new(),
            columns_scroll: gpui::ScrollHandle::new(),
            revealed_pane: None,
            workspace_width: 1000.,
            workspace_height: 660.,
            effective_layout: None,
            filter_open: false,
        }
    }

    fn begin_dialog(&mut self, kind: DialogKind, cx: &mut Context<Self>) {
        // Picking what to compress is the one dialog that opens on top of a
        // modal: the add box is where the list of picks lives, so adding to it
        // has to work from inside it.
        let modal_allows = match (&kind, self.modal_kind()) {
            (DialogKind::PickInputs { .. }, Some(ModalKind::Add)) => true,
            (_, modal) => modal.is_none(),
        };
        if self.dialog.is_some() || self.controller.blocked() || !modal_allows {
            return;
        }
        if matches!(kind, DialogKind::Extract { .. })
            && (self.controller.state.archive.is_none()
                || (matches!(kind, DialogKind::Extract { only_checked: true })
                    && !self.controller.state.checked.iter().any(|checked| *checked)))
        {
            return;
        }
        let (tx, rx) = channel();
        self.dialog = Some(rx);
        std::thread::spawn(move || {
            let result = match kind {
                DialogKind::Open => {
                    let dialog =
                        rfd::FileDialog::new().add_filter("Archives", &super::open_filter());
                    let dialog = if cfg!(feature = "rar") {
                        dialog.add_filter("All files (including RAR volumes)", &["*"])
                    } else {
                        dialog
                    };
                    DialogResult::Open(dialog.pick_file())
                }
                DialogKind::PickInputs { folders } => DialogResult::Inputs(if folders {
                    rfd::FileDialog::new().pick_folders()
                } else {
                    rfd::FileDialog::new().pick_files()
                }),
                DialogKind::Extract { only_checked } => DialogResult::Extract {
                    only_checked,
                    destination: rfd::FileDialog::new().pick_folder(),
                },
                DialogKind::AddFiles { folders } => DialogResult::AddFiles(if folders {
                    rfd::FileDialog::new().pick_folders()
                } else {
                    rfd::FileDialog::new().pick_files()
                }),
                DialogKind::SaveCopy { name, directory } => DialogResult::SaveCopy(
                    rfd::FileDialog::new()
                        .set_file_name(&name)
                        .set_directory(&directory)
                        .save_file(),
                ),
            };
            let _ = tx.send(result);
        });
        cx.notify();
    }

    fn poll_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else {
            return;
        };
        let Ok(result) = dialog.try_recv() else {
            return;
        };
        self.dialog = None;
        match result {
            DialogResult::Open(Some(path)) => self.controller.dispatch(AppAction::Open(path)),
            DialogResult::Inputs(Some(paths)) if !paths.is_empty() => {
                self.controller.dispatch(AppAction::PrepareCompress(paths))
            }
            DialogResult::Extract {
                only_checked,
                destination: Some(dest),
            } => self
                .controller
                .dispatch(AppAction::ExtractTo { only_checked, dest }),
            DialogResult::AddFiles(Some(paths)) if !paths.is_empty() => {
                self.controller.dispatch(AppAction::Add(paths))
            }
            DialogResult::SaveCopy(Some(dest)) => {
                if let Some(archive) = self.controller.state.archive.clone() {
                    // Copying an archive over itself is not a backup, it is a
                    // truncation.
                    if dest != archive {
                        self.controller
                            .dispatch(AppAction::Run(Job::CopyTo { archive, dest }));
                    }
                }
            }
            _ => {}
        }
        let return_focus = match self.modal_kind() {
            Some(kind) => self.modal_text_field(kind, cx),
            None => self.dialog_return_focus.take(),
        };
        if let Some(return_focus) = return_focus {
            window.on_next_frame(move |window, cx| window.focus(&return_focus, cx));
        }
        cx.notify();
    }

    fn background_blocked(&self) -> bool {
        !background_event_allowed(self.modal_kind().is_some(), self.dialog.is_some())
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_kind().is_some() || self.dialog.is_some() {
            cx.stop_propagation();
            return;
        }
        self.filter_open = true;
        cx.notify();
        let handle = self.filter.read(cx).focus_handle(cx).clone();
        window.focus(&handle, cx);
    }

    fn route_changed(&mut self, cx: &mut Context<Self>) {
        self.controller.cancel_preview();
        cx.notify();
    }

    /// A word that can be pressed.
    ///
    /// Ghost rather than outlined: a row of outlined boxes reads as seven
    /// competing things, and the same row without outlines reads as one
    /// toolbar. The kit's own hover and disabled treatment says the rest.
    fn button(
        id: impl Into<ElementId>,
        label: impl Into<gpui::SharedString>,
        accessible_name: String,
        enabled: bool,
    ) -> Button {
        brand_button(id)
            .label(label)
            .accessibility_label(accessible_name)
            .ghost()
            .compact()
            .xsmall()
            .disabled(!enabled)
    }

    /// A square button carrying one of the kit's icons instead of a word.
    ///
    /// Only for the controls whose meaning is a direction -- back, forward, up.
    /// A named action stays a word, because an icon that needs a tooltip to be
    /// understood has cost a word and bought nothing. The accessible name is
    /// still the word, so nothing changes for a screen reader.
    fn icon_button(
        cx: &App,
        id: impl Into<ElementId>,
        icon: impl Into<Icon>,
        accessible_name: String,
        enabled: bool,
    ) -> Button {
        brand_button(id)
            .icon(icon.into().size_4())
            .accessibility_label(accessible_name.clone())
            .tooltip(accessible_name)
            .custom(
                ButtonCustomVariant::new(cx)
                    .hover(cx.theme().button_hover)
                    .active(cx.theme().button_active),
            )
            .compact()
            .disabled(!enabled)
    }

    fn popup_action(
        owner: WeakEntity<GpuiShell>,
        label: impl Into<gpui::SharedString>,
        action: OverflowAction,
    ) -> PopupMenuItem {
        PopupMenuItem::new(label).on_click(move |_, _, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.overflow_action(action, cx);
                cx.notify();
            });
        })
    }

    /// The answer a modal gets when it is dismissed rather than answered.
    ///
    /// One map for every way out -- escape, the kit's close button, its
    /// backdrop -- because a dismissal that does not clear the state leaves
    /// the worker waiting on an answer that is never coming.
    fn cancel_modal(&mut self, kind: ModalKind) {
        match kind {
            ModalKind::Password => self.controller.dispatch(AppAction::CancelPassword),
            ModalKind::Conflict => self.answer_conflict(Answer::Cancel),
            ModalKind::Delete => self.controller.dispatch(AppAction::ConfirmDelete(false)),
            ModalKind::Drop => self
                .controller
                .dispatch(AppAction::AnswerDrop(DropChoice::Cancel)),
            ModalKind::Add => self.controller.cancel_compress(),
            ModalKind::NewFolder | ModalKind::Mask => self.close_name(),
            ModalKind::Settings => self.controller.state.show_settings = false,
            ModalKind::Shortcuts => self.controller.state.show_shortcuts = false,
        }
    }

    /// The modals that already draw with the kit's `Dialog`. The rest still
    /// render as overlays in `dialogs`, so the migration lands one kind at a
    /// time and never draws both for the same modal.
    fn kit_dialog(kind: ModalKind) -> bool {
        matches!(
            kind,
            ModalKind::Delete
                | ModalKind::Conflict
                | ModalKind::Drop
                | ModalKind::Shortcuts
                | ModalKind::Password
                | ModalKind::NewFolder
                | ModalKind::Mask
                | ModalKind::Settings
                | ModalKind::Add
        )
    }

    /// The field a dialog opens on, for the ones that ask for text.
    ///
    /// The kit focuses the dialog itself, which would leave the caret nowhere
    /// and the first keystroke lost.
    fn modal_text_field(&self, kind: ModalKind, cx: &App) -> Option<FocusHandle> {
        match kind {
            ModalKind::Password => Some(self.password.read(cx).focus_handle(cx).clone()),
            ModalKind::Add => Some(self.output_name.read(cx).focus_handle(cx).clone()),
            ModalKind::NewFolder | ModalKind::Mask => {
                Some(self.name_input.read(cx).focus_handle(cx).clone())
            }
            _ => None,
        }
    }

    /// Reconcile the derived modal with the kit's dialog stack.
    ///
    /// Must run between frames: opening a dialog notifies `Root`, and doing
    /// that inside `render` would repaint from inside a repaint.
    fn sync_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let want = self.modal_kind().filter(|kind| Self::kit_dialog(*kind));
        self.open_modal = self.shown_modal(window, cx);
        if want == self.open_modal {
            return;
        }
        if self.open_modal.is_some() {
            window.close_all_dialogs(cx);
        }
        if let Some(kind) = want {
            // Park the keyboard on the shell before the dialog exists. The row
            // that had the focus drops out of the accessibility tree as soon as
            // the modal opens, and leaving the focus on it until the kit
            // focuses its own dialog a frame later aborts the process the next
            // time something marks an active descendant.
            let neutral = self.focus_handle.clone();
            window.focus(&neutral, cx);
            // Every password box opens hidden, whatever the last one that was
            // open was left showing.
            match kind {
                ModalKind::Password => self
                    .password
                    .update(cx, |input, cx| input.set_masked(true, window, cx)),
                ModalKind::Add => self
                    .add_password
                    .update(cx, |input, cx| input.set_masked(true, window, cx)),
                _ => {}
            }
            let shell = cx.weak_entity();
            window.open_dialog(cx, move |dialog, window, cx| {
                let Some(shell) = shell.upgrade() else {
                    return dialog;
                };
                build_dialog(kind, &shell, dialog, window, cx)
            });
            // A frame later, so the field is in the tree to be focused.
            if let Some(field) = self.modal_text_field(kind, cx) {
                window.on_next_frame(move |window, cx| window.focus(&field, cx));
            }
        }
        self.open_modal = want;
    }

    /// The modal the kit is really showing.
    ///
    /// A dialog closes itself when its OK handler says so, without the shell
    /// hearing about it. A wrong password that comes back before the next
    /// frame asks for the same modal again, and only the kit's stack can tell
    /// that it is no longer on screen.
    fn shown_modal(&self, window: &mut Window, cx: &mut App) -> Option<ModalKind> {
        self.open_modal.filter(|_| window.has_active_dialog(cx))
    }

    fn modal_kind(&self) -> Option<ModalKind> {
        if self.controller.state.waiting_on_password.is_some() {
            Some(ModalKind::Password)
        } else if self.controller.conflict().is_some() {
            Some(ModalKind::Conflict)
        } else if self.controller.state.confirm_delete.is_some() {
            Some(ModalKind::Delete)
        } else if self.controller.state.confirm_drop.is_some() {
            Some(ModalKind::Drop)
        } else if matches!(self.controller.state.view, View::Add) {
            Some(ModalKind::Add)
        } else if self.controller.state.asking_folder {
            Some(ModalKind::NewFolder)
        } else if self.controller.state.picking_group.is_some() {
            Some(ModalKind::Mask)
        } else if self.controller.state.show_settings {
            Some(ModalKind::Settings)
        } else if self.controller.state.show_shortcuts {
            Some(ModalKind::Shortcuts)
        } else {
            None
        }
    }

    fn remember_background_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.modal_seen.is_none() && self.modal_kind().is_none() && self.dialog.is_none() {
            if let Some(focus) = window.focused(cx) {
                self.dialog_return_focus = Some(focus);
            }
        }
    }

    fn remember_conflict_focus(&mut self) {
        self.dialog_return_focus = Some(self.conflict_trigger_focus.clone());
    }

    fn sync_modal_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.modal_kind();
        if current == self.modal_seen {
            return;
        }
        // A kit dialog focuses itself when it opens and restores the previous
        // focus when it closes; steering focus at it would fight that.
        if current.is_some_and(Self::kit_dialog) {
            self.modal_seen = current;
            return;
        }
        if current == Some(ModalKind::Conflict) {
            self.remember_conflict_focus();
        }
        self.modal_seen = current;
        let target = match current {
            Some(ModalKind::Password) => self.password.read(cx).focus_handle(cx).clone(),
            Some(ModalKind::Conflict | ModalKind::Delete | ModalKind::Drop) => {
                self.dialog_primary_focus.clone()
            }
            Some(ModalKind::Add) => self.output_name.read(cx).focus_handle(cx).clone(),
            Some(ModalKind::NewFolder | ModalKind::Mask) => {
                self.name_input.read(cx).focus_handle(cx).clone()
            }
            Some(ModalKind::Settings) => self.settings_focus[0].clone(),
            Some(ModalKind::Shortcuts) => self.dialog_cancel_focus.clone(),
            None => match self.dialog_return_focus.clone() {
                Some(focus) => focus,
                None => return,
            },
        };
        let shell = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let Some(shell) = shell.upgrade() else {
                return;
            };
            let shell = shell.read(cx);
            // An asynchronous retry can open another modal before this runs.
            if shell.modal_kind() == current && shell.dialog.is_none() {
                window.focus(&target, cx);
            }
        });
    }

    fn answer_conflict(&mut self, answer: Answer) {
        self.remember_conflict_focus();
        self.controller.dispatch(AppAction::AnswerConflict(answer));
    }

    /// One place where a settings control does its work, so the click handler
    /// and Enter cannot drift apart.
    fn settings_activate(
        &mut self,
        control: SettingsControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match control {
            SettingsControl::LangSystem => {
                self.controller.dispatch(AppAction::SetLanguage(None));
            }
            SettingsControl::LangEn => {
                self.controller
                    .dispatch(AppAction::SetLanguage(Some(super::Lang::En)));
            }
            SettingsControl::LangEs => {
                self.controller
                    .dispatch(AppAction::SetLanguage(Some(super::Lang::Es)));
            }
            SettingsControl::ThemeSystem => {
                self.set_theme(super::ThemePreference::System, window, cx)
            }
            SettingsControl::ThemeLight => {
                self.set_theme(super::ThemePreference::Light, window, cx)
            }
            SettingsControl::ThemeDark => self.set_theme(super::ThemePreference::Dark, window, cx),
            SettingsControl::Updates => {
                self.controller.state.settings.updates = !self.controller.state.settings.updates;
                self.controller.state.settings.save();
            }
            SettingsControl::Subfolder => {
                self.controller.state.into_subfolder = !self.controller.state.into_subfolder;
            }
        }
    }

    /// The preference is stored by the controller and painted by GPUI, and both
    /// have to happen: saving without repainting leaves the window in the old
    /// theme until it is restarted.
    fn set_theme(
        &mut self,
        theme: super::ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.controller.dispatch(AppAction::SetTheme(theme));
        gpui_theme::apply(theme, Some(window), cx);
    }

    /// The entries of the overflow menu that change the archive itself.
    fn overflow_action(&mut self, action: OverflowAction, cx: &mut Context<Self>) {
        match action {
            OverflowAction::Release => self.controller.start_update(),
            OverflowAction::AddFiles => {
                self.begin_dialog(DialogKind::AddFiles { folders: false }, cx)
            }
            OverflowAction::AddFolders => {
                self.begin_dialog(DialogKind::AddFiles { folders: true }, cx)
            }
            // Asked for in a box rather than made as "New folder" and renamed
            // after: making it rewrites the whole archive, and doing that twice
            // for one folder would be silly.
            OverflowAction::NewFolder => {
                self.name_value.clear();
                self.controller.state.asking_folder = true;
            }
            OverflowAction::Undo => self.controller.undo_last(),
            OverflowAction::SaveCopy => {
                let Some(archive) = self.controller.state.archive.clone() else {
                    return;
                };
                let name = archive
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let directory = archive
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."));
                self.begin_dialog(DialogKind::SaveCopy { name, directory }, cx);
            }
        }
    }

    /// Shuts whichever text dialog is open and forgets what was typed in it.
    fn close_name(&mut self) {
        self.controller.state.asking_folder = false;
        self.controller.state.renaming = None;
        self.controller.state.picking_group = None;
        self.name_value.clear();
    }

    /// Start renaming in place, in the panel that was pointed at.
    ///
    /// One entry point for the shortcut, the row menu and the tree, so the
    /// field can never be shown without the state that says what it belongs to.
    fn begin_rename(
        &mut self,
        path: &str,
        in_tree: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.controller.writable() {
            return;
        }
        // The name on its own, never the path the row is labelled with while a
        // search is showing where each hit came from: a rename takes a name,
        // and a name with a folder in it is refused.
        let trimmed = path.trim_end_matches('/');
        let label = trimmed.rsplit('/').next().unwrap_or(trimmed).to_string();
        self.renaming_in_tree = in_tree;
        self.controller.state.renaming = Some((path.to_string(), label.clone()));
        self.rename_input.update(cx, |state, cx| {
            state.set_value(label, window, cx);
            // Renaming usually replaces the name rather than appends to it, so
            // the old one starts out selected and the first keystroke wipes it.
            state.select_all(window, cx);
        });
        let field = self.rename_input.read(cx).focus_handle(cx);
        window.focus(&field, cx);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, _)) = self.controller.state.renaming.clone() else {
            return;
        };
        let name = self.rename_input.read(cx).value().trim().to_string();
        // What is in the folder the entry lives in, which is what the name has
        // to be free of. Not the rows on screen: the tree renames folders the
        // list is not showing, and a search shows rows from everywhere.
        let rows = children_of(&self.controller.state.entries, &parent_of(&path));
        self.controller.state.renaming = None;
        self.renaming_in_tree = false;
        // Back to the list, or the keyboard would be left on a field that is
        // no longer drawn.
        let list = self.list_focus.clone();
        window.focus(&list, cx);
        self.controller.rename_to(&rows, &path, &name);
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming_in_tree = false;
        if self.controller.state.renaming.take().is_some() {
            cx.notify();
        }
    }

    /// Acts on what the shared text field holds, according to which dialog
    /// asked for it.
    fn confirm_name(&mut self, kind: ModalKind, cx: &mut Context<Self>) {
        let s = self.controller.s();
        let name = self.name_value.trim().to_string();
        match kind {
            ModalKind::NewFolder => {
                let Some(archive) = self.controller.state.archive.clone() else {
                    self.close_name();
                    return;
                };
                // The same rules a rename lives by: a name is a name and not a
                // path, and nothing here is called that already.
                if name.is_empty() || name.contains('/') || name.contains('\\') {
                    self.controller.state.notice = s.bad_name.to_string();
                    self.controller.state.error = true;
                    self.close_name();
                    return;
                }
                if self
                    .controller
                    .visible_rows()
                    .iter()
                    .any(|row| row.label.eq_ignore_ascii_case(&name))
                {
                    self.controller.state.notice = fill(s.name_taken, &[("name", &name)]);
                    self.controller.state.error = true;
                    self.close_name();
                    return;
                }
                let full = format!("{}{name}/", self.controller.state.current_dir);
                self.close_name();
                self.controller.dispatch(AppAction::Run(Job::NewFolder {
                    archive,
                    name: full,
                    password: self.controller.state.archive_password.clone(),
                }));
            }
            ModalKind::Mask => {
                // WinRAR's keypad plus and minus: a mask picks or drops every
                // name in this folder that matches it, in one go.
                let adding = self.controller.state.picking_group.unwrap_or(true);
                let rows = self.controller.visible_rows();
                self.close_name();
                if name.is_empty() {
                    return;
                }
                for row in &rows {
                    if super::matches_mask(&name, &row.label) {
                        self.controller.dispatch(AppAction::SetChecked {
                            row: row.clone(),
                            value: adding,
                        });
                    }
                }
            }
            _ => self.close_name(),
        }
        cx.notify();
    }

    fn start_add(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.controller.compression_job() else {
            return;
        };
        self.dialog_return_focus = Some(self.add_start_focus.clone());
        self.controller.dispatch(AppAction::Run(job));
        cx.notify();
    }

    /// Runs what the row menu was asked for and shuts it.
    fn row_action(
        &mut self,
        action: RowAction,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.background_idle() {
            return;
        }
        if action.writable_only() && !self.controller.writable() {
            return;
        }
        if !action.shown(self.controller.on_disk()) {
            return;
        }
        let rows = self.controller.visible_rows();
        let row = rows.get(index).cloned();
        if row.as_ref().is_some_and(|row| row.up) && !action.about_the_place() {
            return;
        }
        if let Some(destination) =
            action.destination(row.as_ref(), &self.controller.state.current_dir)
        {
            if destination != self.controller.state.current_dir {
                self.controller.go_to(destination);
                self.route_changed(cx);
            }
        }
        match action {
            RowAction::Open => {
                if let Some(row) = row {
                    if row.is_dir {
                        self.controller.dispatch(AppAction::Navigate(row.path));
                        self.route_changed(cx);
                    } else if let Some(entry) = row.entry {
                        self.controller.dispatch(AppAction::OpenFile(entry));
                    }
                }
            }
            RowAction::ExtractSelection => {
                self.begin_dialog(DialogKind::Extract { only_checked: true }, cx)
            }
            RowAction::ExtractHere => self.controller.extract_here(),
            // Only a file has anything to look at. A folder is a prefix on
            // some names, not a thing with bytes.
            RowAction::View => {
                if let Some(entry) = row.as_ref().and_then(|row| row.entry) {
                    self.show_preview(entry, window, cx);
                }
            }
            RowAction::TestSelection => {
                let names = self.controller.selected_names();
                if let Some(archive) = self.controller.state.archive.clone() {
                    self.controller.dispatch(AppAction::Run(Job::Test {
                        archive,
                        only: (!names.is_empty()).then(|| names.into_iter().collect()),
                        password: None,
                    }));
                }
            }
            // Only a zip can be written to in place, so anywhere else this is
            // left out rather than offered and refused.
            RowAction::Rename => {
                if let Some(row) = row {
                    self.begin_rename(&row.path, false, window, cx);
                }
            }
            RowAction::Delete => {
                self.dialog_return_focus = Some(self.delete_trigger_focus.clone());
                self.controller.dispatch(AppAction::RequestDelete);
            }
            RowAction::Copy => self.dispatch_clipboard(false, window, cx),
            RowAction::Cut => self.dispatch_clipboard(true, window, cx),
            RowAction::Paste => self.dispatch_paste(window, cx),
            RowAction::CopyNames => {
                let names = self.controller.selected_names();
                if !names.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(names.join("\r\n")));
                }
            }
            RowAction::SelectAll => self.controller.dispatch(AppAction::SelectAllVisible),
            RowAction::NewFolder => self.overflow_action(OverflowAction::NewFolder, cx),
            RowAction::Compress => {
                let inputs = self.controller.selected_disk_paths();
                self.controller.dispatch(AppAction::PrepareCompress(inputs));
            }
            RowAction::Pin => {
                if let Some(row) = row.filter(|row| row.is_dir && !row.up) {
                    self.controller
                        .dispatch(AppAction::TogglePinned(disk_path(&row.path)));
                }
            }
            RowAction::CopyPath => {
                let paths = self
                    .controller
                    .selected_disk_paths()
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>();
                if !paths.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(paths.join("\r\n")));
                }
            }
        }
        cx.notify();
    }

    /// A menu entry of the tree, run against the folder it was opened on.
    ///
    /// Opening it, pasting into it and making a folder in it are about the
    /// place, so the window goes there. Everything else is about the folder
    /// itself: it becomes the selection and the entry works on the selection
    /// the way it does anywhere else, with the list left where it was.
    /// Dragging the other panel somewhere to explain an action is not an
    /// explanation, which is why nothing here reaches into a row.
    fn folder_action(
        &mut self,
        action: RowAction,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.background_idle() {
            return;
        }
        // The name is edited where it was pointed at, which for the tree is
        // the branch itself. Nothing is picked and nothing moves.
        if action == RowAction::Rename {
            self.begin_rename(path, true, window, cx);
            return;
        }
        if action.about_the_place() {
            self.controller
                .dispatch(AppAction::Navigate(path.to_string()));
            self.route_changed(cx);
        } else {
            self.controller.pick_folder(path);
        }
        // No row: every entry the tree offers works on the selection or on the
        // folder now being shown, and none of them reads one.
        self.row_action(action, usize::MAX, window, cx);
    }

    /// The list's geometry, or nothing before it has been laid out once.
    fn list_view(&self, count: usize) -> Option<ListView> {
        let state = self.list_scroll.0.borrow();
        let row = row_height(f32::from(state.last_item_size?.contents.height), count)?;
        let bounds = state.base_handle.bounds();
        let top = f32::from(bounds.origin.y);
        let left = f32::from(bounds.origin.x);
        let height = f32::from(bounds.size.height);
        if height <= 0.0 {
            return None;
        }
        Some(ListView {
            top,
            bottom: top + height,
            left,
            right: left + f32::from(bounds.size.width),
            row,
            offset: -f32::from(state.base_handle.offset().y),
            reach: f32::from(state.base_handle.max_offset().y),
        })
    }

    /// Whether the pointer has left the list. What is being carried goes to the
    /// system from here; inside, it is still a move between folders.
    fn left_the_list(&self, at: gpui::Point<gpui::Pixels>) -> bool {
        if self.controller.state.browser.columns {
            return f32::from(at.x) < 0.
                || f32::from(at.x) > self.workspace_width
                || f32::from(at.y) < 0.
                || f32::from(at.y) > self.workspace_height;
        }
        let Some(view) = self.list_view(self.row_count) else {
            return false;
        };
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        // The sidebar sits to the left of the list and its folders take a drop,
        // so crossing onto it is not leaving the archive: on that side only the
        // window's own edge is. Its rows also start above the list's, which a
        // pull begun in the tree would otherwise cross on its first move.
        if x >= 0.0 && x < view.left {
            return false;
        }
        y < view.top || y > view.bottom || x > view.right
    }

    fn scroll_to(&self, view: &ListView, offset: f32) {
        let state = self.list_scroll.0.borrow();
        let x = state.base_handle.offset().x;
        state
            .base_handle
            .set_offset(point(x, px(-offset.clamp(0.0, view.reach))));
    }

    /// The row under a point, or nothing if that is past the last one.
    fn row_under(view: &ListView, y: f32, len: usize) -> Option<usize> {
        let local = y - view.top + view.offset;
        if local < 0.0 || view.row <= 0.0 {
            return None;
        }
        let index = (local / view.row) as usize;
        (index < len).then_some(index)
    }

    /// The width the shown columns take up, which is where the row ends and
    /// the dead space on its right begins.
    fn columns_span(&self) -> f32 {
        self.shown_columns()
            .into_iter()
            .map(|column| self.column_width(Self::column_slot(column)))
            .sum()
    }

    /// Pressing the left button inside the list, which is where a band begins.
    ///
    /// A band belongs to the empty space around the columns: whatever the
    /// columns leave over on the right, and the space under the last row.
    /// Pressing the row itself is a click or the start of a carry. That is the
    /// rule in the Explorer's details view, and it is the only one that lets
    /// both gestures share a button without guessing which was meant.
    fn begin_band(&mut self, at: gpui::Point<gpui::Pixels>, secondary: bool, shift: bool) {
        if !self.background_idle() || shift {
            return;
        }
        let rows = self.controller.visible_rows();
        let Some(view) = self.list_view(rows.len()) else {
            return;
        };
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        if y < view.top || y > view.bottom || x <= view.left + SIDEBAR_RESIZE_SLOP || x > view.right
        {
            return;
        }
        let anchor = Self::row_under(&view, y, rows.len());
        if anchor.is_some() && x > view.left && x < view.left + self.columns_span() {
            return;
        }
        self.band = Some(Band {
            origin: at,
            rows: rows.clone(),
            // Begun under the last row, where there is none to hang the band
            // on: it still picks everything between there and wherever it goes.
            anchor: anchor.unwrap_or(rows.len().saturating_sub(1)),
            base: if secondary {
                self.controller.state.checked.clone()
            } else {
                vec![false; self.controller.state.checked.len()]
            },
            head: at,
            live: false,
        });
    }

    /// Pressing the right button inside the list, where the kit only marks a
    /// row when the press landed on one.
    ///
    /// Under the last row there is no row to mark, and without a mark the kit
    /// asks its delegate for nothing -- or worse, for the menu of whatever was
    /// right clicked before. An index past the end is the mark for that space,
    /// and the delegate reads it as the folder's own menu.
    fn right_press(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if !self.background_idle() {
            return;
        }
        let rows = self.controller.visible_rows();
        let Some(view) = self.list_view(rows.len()) else {
            return;
        };
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        if y < view.top || y > view.bottom || x <= view.left || x > view.right {
            return;
        }
        if Self::row_under(&view, y, rows.len()).is_some() {
            return;
        }
        let past_the_end = rows.len();
        self.table.update(cx, |state, cx| {
            state.set_right_clicked_row(Some(past_the_end), cx);
        });
    }

    /// The band following the pointer, and the list following it past an edge.
    fn drag_band(&mut self, at: gpui::Point<gpui::Pixels>) -> bool {
        // Taken out and put back, so the rows frozen inside it can be read
        // while the controller is being written to.
        let Some(mut band) = self.band.take() else {
            return false;
        };
        band.head = at;
        let travelled = (f32::from(at.x) - f32::from(band.origin.x))
            .hypot(f32::from(at.y) - f32::from(band.origin.y));
        if !band.live && travelled < DRAG_SLOP {
            self.band = Some(band);
            return false;
        }
        band.live = true;
        let rows = &band.rows;
        let Some(view) = self.list_view(rows.len()) else {
            self.band = Some(band);
            return false;
        };
        let y = f32::from(at.y);
        let head = Self::row_under(&view, y.clamp(view.top, view.bottom), rows.len())
            .unwrap_or(rows.len() - 1);
        let (lo, hi) = if band.anchor <= head {
            (band.anchor, head)
        } else {
            (head, band.anchor)
        };
        self.controller.state.checked.clone_from(&band.base);
        for row in &rows[lo..=hi.min(rows.len() - 1)] {
            self.controller.set_checked(row, true);
        }
        // Past either edge the list follows the pointer, the way the Explorer
        // does it. Without this a selection could never be longer than the
        // window, because dragging no longer scrolls.
        let over = if y < view.top {
            y - view.top
        } else if y > view.bottom {
            y - view.bottom
        } else {
            0.0
        };
        if over != 0.0 {
            self.scroll_to(&view, view.offset + over.clamp(-24.0, 24.0));
        }
        self.band = Some(band);
        true
    }

    /// Drops the anchor, or picks it back up.
    fn toggle_wheel(&mut self, at: gpui::Point<gpui::Pixels>) {
        if self.wheel.is_some() || !self.background_idle() {
            self.wheel = None;
            return;
        }
        let Some(view) = self.list_view(self.row_count) else {
            return;
        };
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        if y < view.top || y > view.bottom || x < view.left || x > view.right {
            return;
        }
        self.wheel = Some(WheelPan {
            anchor: at,
            moved: false,
        });
    }

    /// One step of the list running towards the pointer, for the tick that
    /// keeps a gesture moving while the hand is still.
    ///
    /// ponytail: driven by the shell's existing 100 ms poll rather than by a
    /// frame callback, so the run is ten steps a second. Move it onto a frame
    /// request if the stepping ever reads as stutter.
    fn tick_wheel(&mut self, at: gpui::Point<gpui::Pixels>) -> bool {
        let Some(wheel) = &mut self.wheel else {
            return false;
        };
        let speed = super::wheel_speed(f32::from(at.y) - f32::from(wheel.anchor.y));
        wheel.moved |= speed != 0.0;
        if speed == 0.0 {
            return false;
        }
        let Some(view) = self.list_view(self.row_count) else {
            return false;
        };
        self.scroll_to(&view, view.offset + speed * 0.1);
        true
    }

    /// Whether the keyboard is inside a text field.
    ///
    /// A bare key means something different there -- F5 in a filter box is a
    /// key, not a command -- so the shortcuts that carry no modifier stand
    /// aside while one has the focus.
    fn typing(&self, window: &Window, cx: &App) -> bool {
        self.filter.read(cx).focus_handle(cx).is_focused(window)
            || self
                .rename_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
            || [
                &self.preview_password,
                &self.password,
                &self.output_name,
                &self.add_password,
                &self.name_input,
            ]
            .iter()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The shortcuts that belong to the window rather than to the list.
    ///
    /// One handler rather than a dozen `actions!` entries and twice as many
    /// key bindings: every one of these is the same shape -- a key, a guard,
    /// and an action already written -- and a table of them reads in one go.
    fn global_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.background_blocked() {
            return;
        }
        let typing = self.typing(window, cx);
        let Some(shortcut) = shortcut_for(
            event.keystroke.modifiers.secondary(),
            event.keystroke.modifiers.shift,
            event.keystroke.modifiers.alt,
            &event.keystroke.key.to_ascii_lowercase(),
            typing,
        ) else {
            return;
        };
        let archive = self.controller.state.archive.clone();
        let idle = self.background_idle();
        // The shortcuts window is the one that answers while an archive is
        // being read, because it is the one that says what to press.
        if !idle
            && !matches!(
                shortcut,
                Shortcut::Shortcuts | Shortcut::Back | Shortcut::Forward | Shortcut::View
            )
        {
            return;
        }
        match shortcut {
            Shortcut::Back => {
                self.controller.dispatch(AppAction::Back);
                self.route_changed(cx);
            }
            Shortcut::Forward => {
                self.controller.dispatch(AppAction::Forward);
                self.route_changed(cx);
            }
            Shortcut::Open => self.begin_dialog(DialogKind::Open, cx),
            Shortcut::Compress => {
                let inputs = self.controller.selected_disk_paths();
                self.controller.dispatch(AppAction::PrepareCompress(inputs));
            }
            Shortcut::ExtractAll if archive.is_some() => self.begin_dialog(
                DialogKind::Extract {
                    only_checked: false,
                },
                cx,
            ),
            // Everything out, beside the archive, without asking where: the
            // folder the archive is in is where an extraction goes nine times
            // out of ten, and the whole point is that it is one keystroke.
            Shortcut::ExtractHere if archive.is_some() => self.controller.extract_here(),
            // Verifying an archive was reachable from the shell menu and the
            // command line and from nowhere inside the window.
            Shortcut::Test => {
                if let Some(archive) = archive {
                    self.controller.dispatch(AppAction::Run(Job::Test {
                        archive,
                        only: None,
                        password: None,
                    }));
                }
            }
            Shortcut::Refresh => self.controller.dispatch(AppAction::Refresh),
            Shortcut::Invert => self.controller.dispatch(AppAction::InvertVisible),
            // Escape backs out of the innermost thing there is to back out of,
            // and while the list is running itself that is the running.
            Shortcut::ClearSelection => {
                if self.wheel.take().is_none() {
                    self.controller.dispatch(AppAction::ClearSelection);
                }
            }
            // The names as text, which is all a desktop without a file
            // clipboard can be given, and useful on one that has it too.
            Shortcut::CopyNames => {
                let names = self.controller.selected_names();
                if !names.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(names.join("\r\n")));
                }
            }
            Shortcut::Shortcuts => {
                self.controller.state.show_shortcuts = !self.controller.state.show_shortcuts;
            }
            // One step back from the last change to the archive, which is the
            // step anybody wants: the one they just took by mistake.
            Shortcut::Undo if self.controller.state.undo.is_some() => self.controller.undo_last(),
            Shortcut::Rename if archive.is_some() => {
                let cursor = self.controller.state.cursor;
                let rows = self.controller.visible_rows();
                match cursor.and_then(|index| rows.get(index)).cloned() {
                    Some(row) => self.begin_rename(&row.path, false, window, cx),
                    None => return,
                }
            }
            Shortcut::PickGroup(adding) if archive.is_some() => {
                self.name_value.clear();
                self.controller.state.picking_group = Some(adding);
            }
            Shortcut::View if archive.is_some() || self.controller.on_disk() => {
                let cursor = self.controller.state.cursor;
                let rows = self.controller.visible_rows();
                match cursor
                    .and_then(|index| rows.get(index))
                    .and_then(|row| row.entry)
                {
                    Some(entry) => self.show_preview(entry, window, cx),
                    None => return,
                }
            }
            Shortcut::ExtractAll
            | Shortcut::ExtractHere
            | Shortcut::Undo
            | Shortcut::Rename
            | Shortcut::View
            | Shortcut::PickGroup(_) => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The keys, and what each one does, in the two columns they are read in.
    fn background_idle(&self) -> bool {
        !self.controller.blocked() && self.modal_kind().is_none() && self.dialog.is_none()
    }

    /// Whether files dragged in from outside have somewhere to land. Wider than
    /// `background_idle` by exactly one box: the add box is a list of things to
    /// compress, so dropping onto it means something.
    fn accepts_drop(&self) -> bool {
        if self.controller.blocked() || self.dialog.is_some() {
            return false;
        }
        matches!(self.modal_kind(), None | Some(ModalKind::Add))
    }

    /// Files and folders dragged in from the Explorer.
    ///
    /// The one gesture on Windows that carries both at once, which is why the
    /// add box takes it straight into the list instead of asking what it meant.
    fn drop_external(&mut self, paths: Vec<PathBuf>, window: &Window, cx: &mut Context<Self>) {
        if paths.is_empty() || !self.accepts_drop() {
            return;
        }
        if matches!(self.controller.state.view, View::Add) {
            self.controller.dispatch(AppAction::PrepareCompress(paths));
        } else {
            // Keep the element that had focus before the drop. The existing
            // modal focus sync will move into the confirmation and return here
            // when it is answered.
            if let Some(focus) = window.focused(cx) {
                self.dialog_return_focus = Some(focus);
            }
            self.controller.dispatch(AppAction::Drop(paths));
        }
        cx.notify();
    }

    fn menu_enabled(&self) -> bool {
        !self.controller.blocked() && self.modal_kind().is_none() && self.dialog.is_none()
    }

    fn selected_count(&self) -> usize {
        self.controller
            .state
            .checked
            .iter()
            .filter(|checked| **checked)
            .count()
    }

    fn can_copy_files(&self) -> bool {
        clipboard_action_allowed(
            clipboard::AVAILABLE,
            self.menu_enabled(),
            self.controller.state.archive.is_some(),
            self.selected_count(),
            true,
        )
    }

    fn can_paste_files(&self) -> bool {
        clipboard_action_allowed(
            clipboard::AVAILABLE,
            self.menu_enabled(),
            self.controller.state.archive.is_some(),
            self.selected_count(),
            false,
        )
    }

    fn clipboard_focus_is_safe(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        !self.typing(window, cx) && self.modal_kind().is_none()
    }

    fn dispatch_clipboard(&mut self, cut: bool, window: &Window, cx: &mut Context<Self>) {
        if !self.clipboard_focus_is_safe(window, cx) || !self.menu_enabled() {
            return;
        }
        if self.controller.state.archive.is_none() || self.selected_count() == 0 {
            return;
        }
        if !clipboard::AVAILABLE {
            self.controller.state.notice =
                "File clipboard integration is available on Windows only.".into();
            self.controller.state.error = true;
            cx.notify();
            return;
        }
        self.controller.state.notice = if cut {
            "Preparing selected files to move…".into()
        } else {
            "Preparing selected files to copy…".into()
        };
        self.controller.state.error = false;
        self.controller.dispatch(AppAction::Copy { cut });
        cx.stop_propagation();
        cx.notify();
    }

    fn dispatch_paste(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.clipboard_focus_is_safe(window, cx) || !self.menu_enabled() {
            return;
        }
        if self.controller.state.archive.is_none() {
            return;
        }
        if !clipboard::AVAILABLE {
            self.controller.state.notice =
                "File clipboard integration is available on Windows only.".into();
            self.controller.state.error = true;
            cx.notify();
            return;
        }
        self.controller.dispatch(AppAction::Paste);
        cx.stop_propagation();
        cx.notify();
    }

    fn copy_files_action(&mut self, _: &CopyFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_clipboard(false, window, cx);
    }

    fn cut_files_action(&mut self, _: &CutFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_clipboard(true, window, cx);
    }

    fn paste_files_action(&mut self, _: &PasteFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_paste(window, cx);
    }

    fn status(&self) -> String {
        let s = self.controller.s();
        let state = &self.controller.state;
        let active: Vec<_> = self
            .controller
            .active_tasks()
            .filter(|t| !t.quiet)
            .collect();
        match active.as_slice() {
            [] => {}
            [task] => {
                let progress = match (task.status, task.total) {
                    (crate::controller::TaskStatus::Queued, _) => s.queued_word.to_string(),
                    (_, 0) => s.working_word.to_string(),
                    (_, total) if task.in_bytes => {
                        format!("{} / {}", human(task.done as u64), human(total as u64))
                    }
                    (_, total) => format!("{} / {total}", task.done),
                };
                return format!("{} · {}", task.verb, progress);
            }
            many => {
                return fill(s.operations_count, &[("n", &many.len().to_string())]);
            }
        }
        if state.busy || (matches!(state.view, View::Running) && state.notice.is_empty()) {
            let progress = if state.total_count == 0 {
                s.working_word.to_string()
            } else {
                format!("{} / {}", state.done_count, state.total_count)
            };
            return format!("{} · {}", state.title, progress);
        }
        if !state.notice.is_empty() {
            return state.notice.clone();
        }
        if matches!(state.view, View::Add) {
            return format!("{}: {}", s.add_to_archive, state.pending_inputs.len());
        }
        if state.entries.is_empty() {
            return s.drop_here.into();
        }
        self.controller.summary()
    }

    fn crumbs(&self) -> Vec<(String, String)> {
        let root = if let Some(archive) = &self.controller.state.archive {
            archive
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        } else if self.controller.on_disk() {
            self.controller.s().computer_word.to_string()
        } else {
            return Vec::new();
        };
        let mut crumbs = vec![(root, String::new())];
        let mut walked = String::new();
        for part in self
            .controller
            .state
            .current_dir
            .split('/')
            .filter(|part| !part.is_empty())
        {
            walked.push_str(part);
            walked.push('/');
            crumbs.push((part.to_string(), walked.clone()));
        }
        crumbs
    }

    #[cfg(test)]
    fn visible_crumb_indices(len: usize) -> (Vec<usize>, Vec<usize>) {
        if len <= 4 {
            return ((0..len).collect(), Vec::new());
        }
        let mut shown = vec![0];
        let hidden_end = len - 3;
        let hidden: Vec<usize> = (1..hidden_end).collect();
        shown.extend(hidden_end..len);
        (shown, hidden)
    }

    fn kind_icon(kind: Kind, cx: &App) -> (IconName, gpui::Hsla) {
        match kind {
            Kind::Dir => (IconName::Folder, cx.theme().link),
            Kind::Image => (IconName::GalleryVerticalEnd, cx.theme().link),
            Kind::Text => (IconName::FileText, cx.theme().muted_foreground),
            Kind::Archive => (IconName::Inbox, cx.theme().progress_bar),
            Kind::Audio => (IconName::File, cx.theme().muted_foreground),
            Kind::Video => (IconName::Play, cx.theme().link),
            Kind::Other => (IconName::File, cx.theme().muted_foreground),
        }
    }
}

/// What a row says under a column.
///
/// A free function rather than a method: the table's delegate draws the cells
/// and it has the strings but not the shell. `root` is the folder the list is
/// read from, so a searched row says where it is below that folder rather than
/// repeating the way back to the archive root.
fn column_text(row: &super::Row, column: SortColumn, s: &'static Strings, root: &str) -> String {
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
            SortColumn::Attributes => super::attribute_letters(row.attributes),
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
                let folder = super::folder_of(&row.path);
                folder
                    .strip_prefix(root.trim_end_matches('/'))
                    .map(|rest| rest.trim_start_matches('/'))
                    .unwrap_or(folder)
                    .to_string()
            }
        }
    }
}

impl GpuiShell {
    fn select_row(
        &mut self,
        index: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.background_blocked() || !event.standard_click() {
            return;
        }
        // A click is a drag of no distance. Anything further than that was a
        // band or a carry, and the row it started on is not being clicked.
        if let ClickEvent::Mouse(mouse) = event {
            let travelled = (f32::from(mouse.up.position.x) - f32::from(mouse.down.position.x))
                .hypot(f32::from(mouse.up.position.y) - f32::from(mouse.down.position.y));
            if travelled >= DRAG_SLOP {
                return;
            }
        }
        let rows = self.controller.visible_rows();
        let Some(target) = rows.get(index).cloned() else {
            return;
        };
        let modifiers = event.modifiers();
        if modifiers.shift {
            let from = self.controller.state.cursor.unwrap_or(index);
            let (lo, hi) = if from <= index {
                (from, index)
            } else {
                (index, from)
            };
            for row in &rows[lo..=hi] {
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: true,
                });
            }
        } else if modifiers.secondary() {
            let value = !self.controller.is_checked(&target);
            self.controller.dispatch(AppAction::SetChecked {
                row: target.clone(),
                value,
            });
        } else {
            self.controller.state.checked.fill(false);
            self.controller.dispatch(AppAction::SetChecked {
                row: target.clone(),
                value: true,
            });
        }
        self.controller
            .set_pane_cursor(self.controller.state.browser.active, Some(index));
        self.list_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
        window.focus(&self.list_focus, cx);
        if event.click_count() >= 2 && !modifiers.modified() {
            if target.is_dir {
                self.controller.dispatch(AppAction::Navigate(target.path));
                self.route_changed(cx);
            } else if let Some(entry) = target.entry {
                self.controller.dispatch(AppAction::OpenFile(entry));
            }
        }
        cx.notify();
    }

    fn list_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.background_blocked() {
            cx.stop_propagation();
            return;
        }
        // The table owns Space/Delete for row actions, but never while an input
        // inside a row has focus. Let the editor receive literal spaces and edits.
        if self.typing(window, cx) {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.to_ascii_lowercase();
        if key == "delete" && self.background_idle() && self.controller.writable() {
            self.dialog_return_focus = Some(self.delete_trigger_focus.clone());
            window.focus(&self.delete_trigger_focus, cx);
            self.controller.dispatch(AppAction::RequestDelete);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let rows = self.controller.visible_rows();
        if rows.is_empty() {
            if matches!(key.as_str(), "backspace" | "left" | "arrowleft")
                && !modifiers.alt
                && self.controller.can_go_up()
            {
                self.controller.dispatch(AppAction::Up);
                self.route_changed(cx);
                cx.stop_propagation();
            }
            return;
        }
        if modifiers.secondary() && key == "a" {
            self.controller.dispatch(AppAction::SelectAllVisible);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key == "space" {
            if let Some(index) = self.controller.state.cursor {
                if let Some(row) = rows.get(index) {
                    let value = !self.controller.is_checked(row);
                    self.controller.dispatch(AppAction::SetChecked {
                        row: row.clone(),
                        value,
                    });
                }
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if matches!(key.as_str(), "enter" | "right" | "arrowright") && !modifiers.alt {
            if let Some(row) = self
                .controller
                .state
                .cursor
                .and_then(|index| rows.get(index))
            {
                if row.is_dir {
                    self.controller
                        .dispatch(AppAction::Navigate(row.path.clone()));
                    self.route_changed(cx);
                } else if key == "enter" {
                    if let Some(entry) = row.entry {
                        self.controller.dispatch(AppAction::OpenFile(entry));
                    }
                }
            }
            cx.stop_propagation();
            return;
        }
        if matches!(key.as_str(), "backspace" | "left" | "arrowleft") && !modifiers.alt {
            if self.controller.can_go_up() {
                self.controller.dispatch(AppAction::Up);
                self.route_changed(cx);
            }
            cx.stop_propagation();
            return;
        }

        let last = rows.len() - 1;
        let current = self.controller.state.cursor;
        let page = 12;
        let next = match key.as_str() {
            "down" | "arrowdown" => Some(current.map_or(0, |index| (index + 1).min(last))),
            "up" | "arrowup" => Some(current.map_or(0, |index| index.saturating_sub(1))),
            "pagedown" | "page-down" => Some(current.map_or(0, |index| (index + page).min(last))),
            "pageup" | "page-up" => Some(current.map_or(0, |index| index.saturating_sub(page))),
            "home" => Some(0),
            "end" => Some(last),
            _ => None,
        };
        let Some(index) = next else { return };
        if modifiers.shift {
            let from = current.unwrap_or(index);
            let (lo, hi) = if from <= index {
                (from, index)
            } else {
                (index, from)
            };
            for row in &rows[lo..=hi] {
                self.controller.dispatch(AppAction::SetChecked {
                    row: row.clone(),
                    value: true,
                });
            }
        }
        self.controller
            .set_pane_cursor(self.controller.state.browser.active, Some(index));
        self.list_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
        cx.stop_propagation();
        cx.notify();
    }

    /// The archive, as the kit's table.
    ///
    /// The header, the column widths, the sorting arrows, the scrolling and the
    /// right button's menu are the kit's. What stays here is what the kit has
    /// no notion of: many rows marked at once, the band that sweeps them and a
    /// drag that leaves the window.
    fn file_table(&mut self, rows: Vec<super::Row>, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let enabled = !self.background_blocked();
        let strings = self.controller.s();
        self.sync_table(&rows, cx);
        let mut table = div()
            .id("file-table")
            .role(Role::Table)
            .aria_label(strings.archive_contents)
            .aria_row_count(rows.len() + 1)
            .aria_column_count(self.shown_columns().len())
            .track_focus(&self.list_focus)
            .tab_stop(enabled)
            .focus_visible(focus_ring(cx))
            // No border and no radius: the list is the content pane, not a card
            // floating inside it, so it runs to the edges and the bars above and
            // below draw the only lines.
            .flex_1()
            .min_h(px(1.))
            .flex()
            .flex_col()
            .child(DataTable::new(&self.table).stripe(false).bordered(false))
            .child(
                div()
                    .id("delete-trigger-focus")
                    .track_focus(&self.delete_trigger_focus)
                    .size_0(),
            );
        if enabled {
            table = table
                .focusable()
                .on_key_down(cx.listener(Self::list_key_down));
        }
        table
    }
}

#[derive(Clone)]
enum DialogKind {
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

impl Focusable for GpuiShell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&self.controller.state.window_title);
        self.remember_background_focus(window, cx);
        let s = self.controller.s();
        let modal = self.modal_kind();
        let idle = self.background_idle();
        self.sync_modal_focus(window, cx);
        let password_value = self.controller.state.password_input.clone();
        let add_password_value = self.controller.state.add_password.clone();
        self.password.update(cx, |input, cx| {
            input.set_placeholder(s.password_word, window, cx);
            input.set_disabled(!matches!(modal, Some(ModalKind::Password)), cx);
            if input.value() != password_value.as_str() {
                input.set_value(password_value.clone(), window, cx);
            }
        });
        // The shared text field takes the name of whichever dialog is asking.
        let name_label = match modal {
            Some(ModalKind::Mask) => s.mask_hint,
            _ => s.folder_name,
        };
        let name_value = self.name_value.clone();
        self.name_input.update(cx, |input, cx| {
            input.set_placeholder(name_label, window, cx);
            input.set_disabled(
                !matches!(modal, Some(ModalKind::NewFolder | ModalKind::Mask)),
                cx,
            );
            if input.value() != name_value.as_str() {
                input.set_value(name_value.clone(), window, cx);
            }
        });
        let output_value = self.controller.state.output_name.clone();
        self.output_name.update(cx, |input, cx| {
            input.set_placeholder(s.output_name, window, cx);
            input.set_disabled(!matches!(modal, Some(ModalKind::Add)), cx);
            if input.value() != output_value.as_str() {
                input.set_value(output_value.clone(), window, cx);
            }
        });
        let zip = self.controller.state.format == Format::Zip;
        self.add_password.update(cx, |input, cx| {
            input.set_placeholder(s.password_optional, window, cx);
            input.set_disabled(!(matches!(modal, Some(ModalKind::Add)) && zip), cx);
            if input.value() != add_password_value.as_str() {
                input.set_value(add_password_value.clone(), window, cx);
            }
        });
        let state_filter = self.controller.state.filter.clone();
        if self.filter.read(cx).value().as_ref() != state_filter {
            self.filter.update(cx, |input, cx| {
                input.set_value(state_filter.clone(), window, cx);
            });
        }
        self.sync_workspace(window, cx);
        let has_archive = self.controller.state.archive.is_some() || self.controller.on_disk();
        let selected = self.selected_count();
        let rows = self.controller.visible_rows();
        let visible = rows.iter().filter(|row| !row.up).count();
        self.row_count = rows.len();
        let header = self.header(cx);

        // Everything that used to sit in the flow — the list, the empty states,
        // the progress row — now goes inside the content pane beside the
        // sidebar, so the sidebar runs the full height of the window.
        let mut content = div()
            .id("archive-content")
            .flex_1()
            .min_w(px(1.))
            .min_h(px(1.))
            .flex()
            .flex_col()
            .bg(cx.theme().background);

        // GPUI/AccessKit has no aria-hidden builder. The supported equivalent
        // is a role-less background subtree; every actionable descendant also
        // drops its role, tab stop, and listener while the sibling overlay
        // owns focus and input.
        // The name of what is open, where a title bar puts it. Nothing in here
        // can be pressed on purpose: the bar owns the drag, and a control
        // inside it would move the window when the hand wobbled on the way to
        // pressing it.
        let title_bar = TitleBar::new().child(
            div()
                .id("window-title")
                .role(Role::Heading)
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .h_full()
                .text_xs()
                .text_color(cx.theme().foreground)
                .child(brand_mark(22.))
                .child(heading(self.controller.state.window_title.clone()).truncate()),
        );

        // Abierta desde el menu del Explorador para anadir, la ventana existe
        // para el formulario y nada mas: no hay archivo que navegar detras.
        // Dibujar la barra de herramientas, la lista vacia y el pie solo
        // ponia pellizcos de una Arca que no hace nada asomando por los lados
        // del formulario.
        let solo_el_formulario =
            self.controller.state.one_shot && matches!(modal, Some(ModalKind::Add));
        let mut root = div()
            .id("arca-gpui-background")
            .on_action(cx.listener(Self::focus_filter))
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(title_bar)
            .when(!solo_el_formulario, |root| root.child(header));

        // Files from outside arrive as an ordinary GPUI drag carrying
        // `ExternalPaths`, not as a `FileDropEvent`: the window translates
        // Entered into a MouseMove and Submit into a MouseUp before anything
        // gets to see them, so only `Exited` ever reaches a `FileDropEvent`
        // listener. Watching for the event was watching for something that
        // never comes, which is why nothing dropped on this window did
        // anything at all.
        root = root
            .drag_over::<ExternalPaths>(|style, _, _, cx| {
                style
                    .bg(cx.theme().drop_target)
                    .border_color(cx.theme().drag_border)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_external(paths.paths().to_vec(), window, cx);
            }));

        let drop_probe = {
            let view = cx.entity();
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    // A column edge in hand, a band being pulled, and the list
                    // running after the pointer all have to keep working once
                    // the pointer has left the thing it started on, so the move
                    // and the release are watched on the window.
                    let dragging = view.clone();
                    window.on_mouse_event(
                        move |event: &gpui::MouseMoveEvent, phase, window, app| {
                            if !phase.bubble() {
                                return;
                            }
                            let (leaving, banding) = dragging.update(app, |shell, cx| {
                                shell.pointer = event.position;
                                let mut moved = false;
                                if let Some((slot, from, width)) = shell.resizing {
                                    shell.set_column_width(
                                        slot,
                                        width + f32::from(event.position.x) - from,
                                    );
                                    moved = true;
                                }
                                moved |= shell.drag_band(event.position);
                                moved |= shell.wheel.is_some();
                                if moved {
                                    cx.notify();
                                }
                                (
                                    shell.carrying && shell.left_the_list(event.position),
                                    shell.band.is_some(),
                                )
                            });
                            // The press that started the band was in a row's
                            // dead space, so the row underneath took it as the
                            // start of a carry -- GPUI lets go of a row after
                            // two pixels, long before the band comes alive.
                            // One gesture to a button: the band keeps it.
                            if banding && app.has_active_drag() {
                                app.stop_active_drag(window);
                            }
                            // Out of the list is out of the archive. Started here
                            // and not at the press, because the instant the native
                            // drag begins the system takes the pointer and there is
                            // no way back into the list to drop on a folder.
                            if leaving {
                                app.stop_active_drag(window);
                                dragging.update(app, |shell, cx| {
                                    shell.carrying = false;
                                    if shell.controller.state.format == super::Format::SevenZ {
                                        shell.controller.state.notice =
                                            shell.controller.s().sevenz_copy.to_string();
                                    } else {
                                        shell.controller.drag_out();
                                    }
                                    cx.notify();
                                });
                            }
                        },
                    );
                    let pressed = view.clone();
                    window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _, app| {
                        if !phase.bubble() {
                            return;
                        }
                        pressed.update(app, |shell, cx| {
                            match event.button {
                                // Pressing the wheel again puts it away, the way
                                // it does in a browser.
                                gpui::MouseButton::Middle => {
                                    shell.toggle_wheel(event.position);
                                    cx.notify();
                                }
                                // Any other button is somebody asking for
                                // something else.
                                _ if shell.wheel.is_some() => {
                                    shell.wheel = None;
                                    cx.notify();
                                }
                                gpui::MouseButton::Left => {
                                    shell.begin_band(
                                        event.position,
                                        event.modifiers.secondary(),
                                        event.modifiers.shift,
                                    );
                                }
                                gpui::MouseButton::Right => {
                                    shell.right_press(event.position, cx);
                                }
                                _ => {}
                            }
                        });
                    });
                    let released = view.clone();
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, app| {
                        if !phase.bubble() {
                            return;
                        }
                        released.update(app, |shell, cx| {
                            let mut changed = shell.band.take().is_some_and(|band| band.live);
                            shell.carrying = false;
                            if shell.resizing.take().is_some() {
                                // Written when the hand lets go rather than on
                                // the way, so pulling an edge across the window
                                // is one visit to the disk and not one a frame.
                                shell.controller.state.settings.save();
                                changed = true;
                            }
                            // Held down and pulled: the gesture ends where the
                            // hand lets go. Let go without having pulled and it
                            // stays on, waiting.
                            if event.button == gpui::MouseButton::Middle
                                && shell.wheel.as_ref().is_some_and(|wheel| wheel.moved)
                            {
                                shell.wheel = None;
                                changed = true;
                            }
                            if changed {
                                cx.notify();
                            }
                        });
                    });
                    // The wheel turning is somebody scrolling by hand, which is
                    // asking for something other than the list running itself.
                    let spun = view.clone();
                    window.on_mouse_event(move |_: &gpui::ScrollWheelEvent, phase, _, app| {
                        if !phase.bubble() {
                            return;
                        }
                        spun.update(app, |shell, cx| {
                            if shell.wheel.take().is_some() {
                                cx.notify();
                            }
                        });
                    });
                },
            )
            .size_0()
        };
        root = root
            .child(drop_probe)
            .child(
                div()
                    .id("drop-trigger-focus")
                    .track_focus(&self.drop_trigger_focus)
                    .size_0(),
            )
            .child(
                div()
                    .id("conflict-trigger-focus")
                    .track_focus(&self.conflict_trigger_focus)
                    .size_0(),
            )
            .child(
                div()
                    .id("add-start-focus")
                    .track_focus(&self.add_start_focus)
                    .size_0(),
            );

        // A window opened for one job is that job: the list of operations is
        // all there is to show in it.
        let docked = self.controller.state.one_shot && !self.shown_tasks().is_empty();
        if docked {
            if let Some(panel) = self.jobs_panel(true, cx) {
                content = content.child(panel);
            }
        } else if self.controller.state.busy && self.controller.state.entries.is_empty() {
            content = content.child({
                let mut loading = div()
                    .id("loading-state")
                    .aria_label(s.opening)
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{}…", s.opening));
                if !self.background_blocked() {
                    loading = loading.role(Role::Status);
                }
                loading
            });
        } else if !has_archive {
            content = content.child({
                div()
                    .id("empty-state")
                    .when(!self.background_blocked(), |empty| empty.role(Role::Region))
                    .aria_label(if self.controller.state.error {
                        s.cannot_open
                    } else {
                        s.drop_here
                    })
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .h_full()
                            .min_h(px(224.))
                            .p_4()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .child(brand_mark(72.))
                            .child(
                                heading(if self.controller.state.error {
                                    s.cannot_open
                                } else {
                                    "Arca"
                                })
                                .text_xl()
                                .text_color(
                                    if self.controller.state.error {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().foreground
                                    },
                                ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(s.drop_here),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        brand_button("welcome-open")
                                            .icon(IconName::FolderOpen)
                                            .label(s.open)
                                            .primary()
                                            .disabled(!idle)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if this.background_idle() {
                                                    this.begin_dialog(DialogKind::Open, cx);
                                                }
                                            })),
                                    )
                                    .child(
                                        brand_button("welcome-compress")
                                            .icon(IconName::Inbox)
                                            .label(s.compress)
                                            .outline()
                                            .disabled(!idle)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if this.background_idle() {
                                                    let inputs =
                                                        this.controller.selected_disk_paths();
                                                    this.controller.dispatch(
                                                        AppAction::PrepareCompress(inputs),
                                                    );
                                                    cx.notify();
                                                }
                                            })),
                                    ),
                            ),
                    )
            });
        } else if self.controller.state.browser.columns {
            content = content.child(self.directory_columns(cx));
        } else if visible == 0 {
            let message = if self.controller.state.error {
                s.cannot_open
            } else if self.controller.state.filter.trim().is_empty() {
                if self.controller.state.entries.is_empty() {
                    s.empty_archive
                } else {
                    s.empty_folder
                }
            } else {
                s.no_matches
            };
            content = content.child({
                let mut empty = div()
                    .id("empty-state")
                    .aria_label(empty_state_aria_label(
                        self.controller.state.error,
                        &self.controller.state.filter,
                        s,
                    ))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(if self.controller.state.error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(
                        Icon::new(if self.controller.state.error {
                            IconName::CircleX
                        } else if self.controller.state.filter.trim().is_empty() {
                            IconName::Inbox
                        } else {
                            IconName::Search
                        })
                        .size(px(28.)),
                    )
                    .child(heading(message).text_sm());
                if !self.background_blocked() {
                    empty = empty.role(Role::Region);
                }
                empty
            });
        } else {
            content = content.child(self.file_table(rows, cx));
        }

        let body = self.workspace(content, cx);
        root = if solo_el_formulario {
            root.child(div().flex_1())
        } else {
            root.child(body).child(self.footer(visible, selected, cx))
        };

        let background = root;
        let mut root = div()
            .id("arca-gpui-shell")
            .role(Role::Application)
            .aria_label("Arca")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::copy_files_action))
            .on_action(cx.listener(Self::cut_files_action))
            .on_action(cx.listener(Self::paste_files_action))
            .on_key_down(cx.listener(Self::global_key_down))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(background);

        // The band, and the anchor the wheel dropped. Both are drawn over
        // everything rather than inside the list, because the hand is free to
        // wander off it while either gesture runs and a mark that vanished at
        // the edge would be worse than no mark at all.
        if let Some(band) = self.band.as_ref().filter(|band| band.live) {
            if let Some(view) = self.list_view(visible) {
                let (x0, x1) = minmax(band.origin.x, band.head.x);
                let (y0, y1) = minmax(band.origin.y, band.head.y);
                let top = y0.max(view.top);
                let bottom = y1.min(view.bottom);
                // Recortada a la lista tambien por los lados. Lo que marca son
                // filas, y filas solo hay aqui: a lo ancho se metia sobre el
                // panel de carpetas y parecia marcar algo de ahi, que no lo
                // hacia -- la seleccion la decide solo la altura.
                let x0 = x0.max(view.left);
                let x1 = x1.min(view.right);
                if bottom > top && x1 > x0 {
                    // A quarter of the selection ink, so the rows underneath
                    // stay readable while they are being swept: the band says
                    // what it is reaching, and a solid one would hide it.
                    // Nothing is occluded either, because the pointer has to
                    // keep being followed through it.
                    let mut fill = cx.theme().selection;
                    fill.a *= 0.25;
                    root = root.child(
                        div()
                            .id("selection-band")
                            .absolute()
                            .left(px(x0))
                            .top(px(top))
                            .w(px((x1 - x0).max(1.0)))
                            .h(px(bottom - top))
                            .bg(fill)
                            .border_1()
                            .border_color(cx.theme().ring),
                    );
                }
            }
        }
        if let Some(wheel) = &self.wheel {
            // A ring with a dot in it, left where the wheel went down: the mark
            // Windows leaves, so it reads as the same gesture rather than as
            // one of ours.
            root = root.child(
                div()
                    .id("wheel-anchor")
                    .absolute()
                    .left(px(f32::from(wheel.anchor.x) - 10.0))
                    .top(px(f32::from(wheel.anchor.y) - 10.0))
                    .size(px(20.))
                    .rounded_full()
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().muted_foreground)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(3.))
                            .rounded_full()
                            .bg(cx.theme().muted_foreground),
                    ),
            );
        }
        // The kit's stack is imperative and this shell's modal is derived, so
        // they are reconciled after the frame rather than during it.
        if self.modal_kind().filter(|kind| Self::kit_dialog(*kind)) != self.shown_modal(window, cx)
        {
            let shell = cx.weak_entity();
            window.on_next_frame(move |window, cx| {
                if let Some(shell) = shell.upgrade() {
                    shell.update(cx, |this, cx| this.sync_dialog(window, cx));
                }
            });
        }
        root
    }
}

/// The window's first view under `Root`: the shell, plus the kit's own layers.
///
/// `Root` owns the dialog, sheet and notification stacks but does not mount
/// them, so somebody has to. It cannot be `GpuiShell` itself: a dialog body
/// reads the shell, and mounting the layer inside the shell's own `render`
/// runs that read while the shell is exclusively borrowed, which aborts the
/// process. As a sibling of the shell rather than a part of it, the borrow is
/// over by the time the layer builds.
struct Frame {
    shell: Entity<GpuiShell>,
}

impl Render for Frame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.shell.clone())
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

/// The overflow entries that write to the archive, as a value rather than a
/// closure: the menu is built while the shell is still borrowed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OverflowAction {
    Release,
    AddFiles,
    AddFolders,
    NewFolder,
    Undo,
    SaveCopy,
}

/// A selection being drawn by pulling across the list.
struct Band {
    /// Where the button went down, in window coordinates.
    origin: gpui::Point<gpui::Pixels>,
    /// The row it went down on. Two row numbers rather than a rectangle: the
    /// list moves underneath while the drag happens, and a rectangle frozen
    /// where the button went down stops meaning anything the moment it does.
    anchor: usize,
    /// What was picked before it started, so that Ctrl adds to a selection and
    /// a plain drag replaces one.
    base: Vec<bool>,
    /// Where the pointer is now, which is the other end of the band.
    head: gpui::Point<gpui::Pixels>,
    /// The list as it was when the band began.
    ///
    /// Frozen for the length of the gesture rather than asked for on every
    /// pointer move: what the band picks cannot change the list it is picking
    /// from, and nothing else can change it either while a button is down.
    rows: Vec<super::Row>,
    /// Whether it has been pulled far enough to be a gesture rather than a
    /// click. A click is a drag of no distance, and under this it is left
    /// alone so clicking a row still means clicking a row.
    live: bool,
}

/// The anchor the wheel dropped, and whether the pointer has pulled away from
/// it yet. Letting the wheel go after it has ends the gesture; letting it go
/// before leaves it running until the next click, which is what makes
/// press-and-drag and click-and-go both work off the one button.
struct WheelPan {
    anchor: gpui::Point<gpui::Pixels>,
    moved: bool,
}

/// Where the list is on screen, how tall a row is and how far down it is
/// scrolled: everything the two pointer gestures need, read off the one scroll
/// handle rather than measured again.
struct ListView {
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
    row: f32,
    /// How far down the list is. GPUI keeps this as a negative offset; it is
    /// turned the right way up here so the arithmetic below reads like the
    /// list does.
    offset: f32,
    reach: f32,
}

/// How far a click may travel and still be a click.
///
/// Further than GPUI waits before calling a drag a drag, on purpose: a band
/// that appeared first would flash over the rows for the pixel or two between
/// the two thresholds every time a column was resized.
const DRAG_SLOP: f32 = 10.0;

/// The resize handle overlaps the list's left edge by five pixels.
const SIDEBAR_RESIZE_SLOP: f32 = 5.0;

/// The selection while it is in the air. An empty marker rather than the rows
/// themselves: what is carried is whatever is picked when it lands, and the
/// selection cannot change while the button is down.
struct DraggedRows;

/// What follows the pointer while rows are in the air. Without it a drag is
/// invisible until the pointer leaves the window and the system draws its own.
struct DragPreview {
    label: String,
    extra: usize,
    /// Where inside the row the press landed.
    offset: gpui::Point<gpui::Pixels>,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI paints this at the pointer less that offset, and a row is as wide
        // as the list: a press near the right edge would draw the badge back at
        // the file name, or off the left of the window where it is cut in half.
        // A wrapper padded by the same amount puts it back at the pointer; it
        // has to be a wrapper because the badge sets its own padding. The
        // vertical padding is short of the press by a badge's height, which is
        // what puts the badge above the pointer instead of under the hand. A
        // press in the top few pixels of a row cannot be lifted that far and is
        // left at the row's own top, which is above the pointer anyway.
        const LIFT: f32 = 30.0;
        let shift = div()
            .pl(self.offset.x)
            .pt(px((f32::from(self.offset.y) - LIFT).max(0.)));
        let centred = div().w(px(0.)).flex().justify_center();
        if self.label.is_empty() {
            return shift;
        }
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            // Opaque, not a tint: the badge is read against whatever it happens
            // to be over, which is a list of file names.
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_md()
            .text_xs()
            .flex_none()
            .whitespace_nowrap()
            .text_color(cx.theme().popover_foreground)
            .child(self.label.clone());
        if self.extra > 0 {
            row = row.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("+{}", self.extra)),
            );
        }
        // A box of no width with its one child centred: the free space is
        // negative, so the badge hangs half over each side of the pointer.
        // Nothing here knows how wide a file name draws, and this does not have
        // to ask.
        shift.child(centred.child(row))
    }
}

/// What a key press means to the window, as opposed to the list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shortcut {
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
    /// A mask that picks names, or one that drops them.
    PickGroup(bool),
}

/// Reading a key press, with no state and no side effects, so the table of
/// shortcuts can be checked without a window.
///
/// `typing` only silences the keys that carry no modifier: F5 inside a filter
/// box is a key, but Ctrl+O is never text.
fn shortcut_for(
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

/// How tall one row is, from the height of everything and how many there are.
///
/// Not `last_item_size.item`, whatever that name suggests: that field holds the
/// size of the whole viewport. Reading it as a row height divided by four
/// hundred instead of by twenty-eight, which put every pointer on the first row
/// and left the band picking nothing.
fn row_height(contents: f32, count: usize) -> Option<f32> {
    if count == 0 || contents <= 0.0 {
        return None;
    }
    let row = contents / count as f32;
    (row > 0.0).then_some(row)
}

/// The two ends of a span, in the order they are drawn in.
fn minmax(a: gpui::Pixels, b: gpui::Pixels) -> (f32, f32) {
    let (a, b) = (f32::from(a), f32::from(b));
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn background_event_allowed(modal: bool, native_picker: bool) -> bool {
    !modal && !native_picker
}

fn clipboard_action_allowed(
    available: bool,
    idle: bool,
    has_archive: bool,
    selected: usize,
    needs_selection: bool,
) -> bool {
    available && idle && has_archive && (!needs_selection || selected > 0)
}

fn empty_state_aria_label(error: bool, filter: &str, s: &'static Strings) -> &'static str {
    if error {
        s.cannot_open
    } else if filter.trim().is_empty() {
        s.empty_folder
    } else {
        s.no_matches
    }
}

fn apply_startup(controller: &mut AppController, startup: Startup) {
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

fn shell_size(compact: bool) -> (f32, f32) {
    if compact {
        COMPACT_SIZE
    } else {
        NORMAL_SIZE
    }
}

pub(crate) fn run() {
    let startup = super::parse_args();
    let compact = !matches!(startup, Startup::Browse(_));
    let (width, height) = shell_size(compact);
    let floor = if compact { COMPACT_SIZE } else { MINIMUM_SIZE };

    // The kit's icons are SVG assets, not glyphs; without an asset source every
    // `IconName` resolves to nothing and the toolbar renders blank.
    application()
        .with_assets(crate::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_theme::init(cx);
            cx.bind_keys([
                KeyBinding::new("ctrl-f", FocusFilter, None),
                KeyBinding::new("cmd-f", FocusFilter, None),
                KeyBinding::new("ctrl-c", CopyFiles, None),
                KeyBinding::new("cmd-c", CopyFiles, None),
                KeyBinding::new("ctrl-x", CutFiles, None),
                KeyBinding::new("cmd-x", CutFiles, None),
                KeyBinding::new("ctrl-v", PasteFiles, None),
                KeyBinding::new("cmd-v", PasteFiles, None),
            ]);
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            let window = cx
                .open_window(
                    // The bar across the top is Arca's, not the system's: the
                    // kit's options make the native caption transparent and
                    // hand the dragging, the double click and the three window
                    // buttons to `TitleBar`, which is drawn from the same
                    // tokens as everything below it.
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        // El suelo de la ventana de navegar no vale para la que
                        // abre el menu contextual: esa existe para un formulario
                        // y nada mas. Con un solo minimo para las dos, Windows
                        // estiraba la pequena hasta el ancho de la grande y el
                        // formulario quedaba flotando sobre una Arca vacia.
                        window_min_size: Some(size(px(floor.0), px(floor.1))),
                        ..TitleBar::window_options()
                    },
                    |window, cx| {
                        // The kit's `Root` has to be the window's first view:
                        // dialogs, sheets and notifications are looked up
                        // through it, and every kit component that opens one
                        // panics without it.
                        let shell = cx.new(|cx| GpuiShell::new(window, cx, startup));
                        let shell_focus = shell.read(cx).focus_handle.clone();
                        window.focus(&shell_focus, cx);
                        window.set_window_title("Arca");
                        let frame = cx.new(|_| Frame { shell });
                        cx.new(|cx| Root::new(frame, window, cx))
                    },
                )
                .expect("open GPUI shell window");
            window
                .update(cx, |_, _, cx| {
                    cx.activate(true);
                })
                .expect("activate GPUI shell window");
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let viewport = size(px(width), px(height));
            for kind in [
                ModalKind::Add,
                ModalKind::Settings,
                ModalKind::Shortcuts,
                ModalKind::Conflict,
                ModalKind::Password,
            ] {
                let (bounds, top) = dialog_dimensions(kind, viewport);
                assert!(bounds.width <= viewport.width - px(32.));
                assert!(top + bounds.height <= viewport.height - px(16.));
                assert!(bounds.height > px(200.));
            }
        }
        let viewport = size(px(NORMAL_SIZE.0), px(NORMAL_SIZE.1));
        assert_eq!(
            dialog_dimensions(ModalKind::Add, viewport).0.width,
            px(600.)
        );
        assert_eq!(
            dialog_dimensions(ModalKind::Settings, viewport).0.width,
            px(720.)
        );
    }

    #[test]
    fn breadcrumbs_keep_root_and_nearest_folders() {
        assert_eq!(
            GpuiShell::visible_crumb_indices(4),
            ((0..4).collect(), Vec::new())
        );
        assert_eq!(
            GpuiShell::visible_crumb_indices(6),
            (vec![0, 3, 4, 5], vec![1, 2])
        );
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
    fn the_row_height_comes_from_the_content_and_not_from_the_viewport() {
        // A list showing fifty rows of twenty-eight pixels has fourteen hundred
        // pixels of content and a viewport of whatever the window left it. The
        // viewport is what `last_item_size.item` holds, so reading that as a
        // row height divided by four hundred instead of by twenty-eight: every
        // pointer landed on the first row and the band picked nothing.
        assert_eq!(row_height(1400.0, 50), Some(28.0));
        assert_eq!(row_height(1400.0, 0), None, "an empty list has no rows");
        assert_eq!(row_height(0.0, 50), None, "nor has one not laid out yet");
    }

    #[test]
    fn a_point_lands_on_the_row_that_is_drawn_under_it() {
        // The list is virtualized, so which row a pointer is over is arithmetic
        // on the scroll offset and not a rectangle anybody kept. Off by one row
        // here and a band would pick everything one place along.
        let view = ListView {
            top: 100.0,
            bottom: 360.0,
            left: 0.0,
            right: 800.0,
            row: 26.0,
            offset: 0.0,
            reach: 1000.0,
        };
        assert_eq!(GpuiShell::row_under(&view, 100.0, 50), Some(0));
        assert_eq!(GpuiShell::row_under(&view, 125.9, 50), Some(0));
        assert_eq!(GpuiShell::row_under(&view, 126.0, 50), Some(1));
        // Above the first row is no row at all, not the first one.
        assert_eq!(GpuiShell::row_under(&view, 99.0, 50), None);
        // And past the last there is nothing either, however far down it is.
        assert_eq!(GpuiShell::row_under(&view, 100.0, 0), None);
        assert_eq!(GpuiShell::row_under(&view, 100.0 + 26.0 * 3.0, 3), None);

        // Scrolled down by ten rows, the top of the list is row ten.
        let scrolled = ListView {
            offset: 260.0,
            ..view
        };
        assert_eq!(GpuiShell::row_under(&scrolled, 100.0, 50), Some(10));
        assert_eq!(GpuiShell::row_under(&scrolled, 126.0, 50), Some(11));
    }

    #[test]
    fn every_column_reads_its_width_from_its_own_slot() {
        // The widths vector is the one written to gui.conf: the name first,
        // then `Columns::ALL` in order. A slot off by one would hand a column
        // the width of its neighbour and the file would still load.
        assert_eq!(GpuiShell::column_slot(SortColumn::Name), 0);
        for (index, (column, _)) in Columns::ALL.iter().enumerate() {
            assert_eq!(GpuiShell::column_slot(*column), index + 1);
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

        let items = folder_items(&root, "");
        let ids = items
            .iter()
            .map(|item| item.id.to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["docs/".to_string(), "src/".to_string()]);
        let nested = &items[1].children;
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].id.to_string(), "src/deep/");
        assert_eq!(nested[0].label.to_string(), "deep");
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
                let (label, _) = action.label(super::super::strings(lang));
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
            let s = super::super::strings(lang);
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
        let s = super::super::strings(super::super::Lang::En);
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
}
