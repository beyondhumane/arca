---
description: A file explorer that opens archives in place, with panes, preview, devices and one right-click to compress or extract.
group: Desktop
order: 8
keywords: gui window arca-gui desktop app egui file explorer panes preview devices accessibility nvda narrator
---

# The desktop workspace

`arca-gui` is a file explorer and an archiver in the same window. It starts in the folder you left it in, shows the disk the way any explorer does, and opens an archive in place when you double-click it, with the same panes, filters, preview and context menus. The top band holds history, breadcrumbs (click their empty part, the pencil or press **Ctrl+L** to type a path), the filter, the **View** menu, search (**Ctrl+K**), settings and the **More** menu.

## Browsing the disk

The sidebar lists **Places** (home and the standard user folders), **Pinned** folders, **Devices** and **Recent** archives. Devices come from the system's volume list: removable drives, volumes mounted under `/media`, `/run/media` or `/mnt`, and volumes with a file system that are not mounted yet; on Windows, the drive letters. Clicking an unmounted volume mounts it first. Right-click a volume for **Mount** or **Unmount**, **Format…** (after a warning; ext4, exFAT, FAT or NTFS, with an optional label) and **Set label…**; on Linux these go through UDisks, so the desktop may ask for your password. **More > View > Show hidden files** toggles dot files.

Right-click a file or folder for the full menu: **New folder with selection**, **Open**, **Open with** (the applications that accept that type), **Quick preview**, **Cut**, **Copy**, **Paste**, **Duplicate**, **Rename**, **Move to…**, **Copy to…**, **Compress**, **Copy path**, **Copy name**, **Select all**, **New folder**, **Pin to sidebar**, **Properties**, **Move to Trash** (**Delete**) and **Permanently delete** (**Shift+Delete**, after confirmation). Drag rows onto a folder or onto another pane to move them.

Double-clicking an archive (`.zip`, `.7z`, `.tar`, `.tar.gz`, `.tar.xz`, `.xz`, `.rar`, `.iso` and the ZIP containers) opens it in place. Inside, the menu offers **Extract selection**, **Extract here**, **Extract all** and **Test**, plus **Rename**, **Delete**, **New folder** and **Paste** in a ZIP; a password is asked for when the archive needs one. **Up** or **Backspace** at the root, or **Close archive**, return to the folder with the cursor on the archive. Other files open with the system application.

## Panes, details and the sidebar

The **Panes** view (the default) puts one folder in each vertical pane. Each pane has its own header with **Reload**, **Up**, **Sort**, **Filter** and **Close**, and its own cursor, selection, scrolling, filter and sort order. Three settings in **Settings > Views** (also in the **View** menu) decide how panes behave:

- **Open folders in the next pane** (`next_pane`, off by default): on, selecting a folder opens it in the pane to the right and the ancestors stay to the left, Miller-column style; off, each pane navigates on its own and **Open in a new pane** in the context menu adds one.
- **Peek into folders on hover** (`folder_peek`, off by default): resting the pointer on a folder row shows a card with its first entries without opening it.
- **Focus follows the pointer** (`pointer_focus`, on by default): the pane under the pointer is the active one, so the path bar, filter, shortcuts and actions act on it, without a click and without moving the panes. Any key hands control to the keyboard until the pointer moves again; **Tab**, **Shift+Tab**, **←** and **→** walk the panes. The row of panes scrolls only when navigation opens a pane out of view, with its scrollbar, or with **Shift+wheel**.

**Details** is a sortable table; right-click its header to choose the columns. **Flat view** lists everything under the current folder without the hierarchy. **Filter** (**Ctrl+F**) belongs to the pane in Panes and to the whole list in Details.

The sidebar button shows or hides the sidebar; when the window leaves too little room for the folders beside it, the sidebar shrinks to a column of icons on its own and comes back when there is room again. Drag the dividers to resize the sidebar, the panes and the preview; every width is remembered.

## Selection and actions

- Click selects, **Ctrl+click** toggles, **Shift+click** selects a range, **Space** toggles the cursor row and **Ctrl+A** selects the visible rows of the active pane.
- **Compress** opens the compress box: archive name with automatic extension, destination folder, format (ZIP, TAR, TAR.GZ, TAR.XZ, 7z, XZ or RAR), method, a 0–9 level slider, an optional AES-256 password (with hidden names in 7z) and the list of what goes in.
- **Extract…** (**Ctrl+E**) asks where to unpack; **Extract here** (**Alt+W**) unpacks beside the archive.
- **Properties** shows size, dates, type, permissions and what a folder contains.
- **More** includes selection tools, password changes, verification, settings, the shortcut list and **Check for updates**; a new version opens a dialog with its release notes and installs itself from inside the app.

## Integrated preview

The right panel (**F3**) follows the cursor: a header with the type icon, name, size, date and MIME type, and buttons to open the file with the system application, switch between **Text**, **Hex** and **Image**, toggle line wrapping and close. Text comes with line numbers and syntax highlighting for shell, Rust, C-like languages, Python, JavaScript and TypeScript, JSON, TOML, YAML, INI, HTML and XML, CSS and Markdown; only the visible lines are laid out, so large files stay fluid. Hex shows offsets in the gutter and images fit the panel. Loading, password and error states are shown in place; passwords and preview contents are never written to settings.

## Passwords

The create dialog offers ZIP and 7z passwords; 7z also offers **Hide file names**.
Hidden 7z headers prompt before listing. Content validation runs in a background
worker before extract/test/preview/open actions; retry resumes that action and
cancel leaves the destination untouched. **Remove password** and **Set password…**
apply only to ZIP, never to an existing 7z.

## RAR

**Create** offers RAR next to ZIP and 7z and writes a new single-volume RAR5
archive at the chosen level. With RAR selected the dialog shows the fixed RAR
compressor and a note that the output is never encrypted; the password field,
**Hide file names** and the ZIP codec picker are not shown, and a password typed
for ZIP or 7z is kept for when you switch back. An existing output name is
refused before anything is written, with a message to pick another name; no
Replace option is offered because RAR creation never replaces a file. The new archive
opens read-only like any other RAR, and Add, Delete, Rename and password
actions stay disabled for it. CBR cannot be created. See [RAR](rar.md#creating-rar5-archives).

## 7z

Create 7z using Store or LZMA2 and the existing levels. Creation and extraction
are sequential. Selected members of solid archives share one decoder per block
per pass, rather than reopening it for each member. Preview, open-file, Copy and
Extract use the same validated read path. See [Encryption](encryption.md) and
[resource limits](architecture.md#7z-boundaries).

## XZ and TAR.XZ

The create dialog offers TAR.XZ and XZ with LZMA2 at the usual levels. XZ takes
exactly one file; with a folder or several files the dialog asks for TAR.XZ
instead. A standalone `.xz` opens as a list with one entry named after the
archive without `.xz`. Test, preview, Copy and Extract read the stream to the end
and check it, and a stopped or failed creation leaves no half-written archive.

## Format limits

Adding, deleting, renaming and changing passwords in an existing archive remain
ZIP-only; 7z and RAR can be created but not modified afterwards. 7z virtual-file drag-out is disabled to avoid repeatedly decoding solid
blocks; use Copy or Extract instead. Copying to the system file clipboard is
platform-dependent.

## Jobs and keyboard access

Every operation (extract, test, compress, add, delete, rename, change password, copy) is a row in the **Operations** panel, which floats over the lower-right corner of the workspace and can be hidden and reopened from the footer button that counts them. Each row shows its own progress, elapsed and remaining time, pause/resume and cancel; finished rows keep their result until cleared. Pause and cancel also act inside a single large entry: a cancelled extraction removes the file it was writing and keeps the ones it had finished.

Operations run at the same time when they touch different files. Rewrites of the same archive wait for each other in the order they were asked for, and a rewrite also waits for any extraction or test of that archive that is already running. While the open archive is being rewritten its listing is read-only; extracting, testing or compressing leaves the window free to keep browsing and start more work. Overwrite questions name the operation they belong to. Explorer one-shot operations show the same panel as the whole window and retain their existing completion behavior.

The footer reports the active directory, visible/selected counts and job status, with **F1** help. Tab reaches navigation, sidebar, browser, preview controls and footer. Text fields keep their editing keys; closing dialogs restores focus. Controls expose AccessKit labels and focus/selection indicators.

See [Keyboard shortcuts](keyboard-shortcuts.md).
