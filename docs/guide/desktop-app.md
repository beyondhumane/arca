---
description: Browse archives like folders in a fast, keyboard-friendly native window.
group: Desktop
order: 8
keywords: gui window arca-gui desktop app gpui accessibility nvda narrator
---

# The desktop workspace

`arca-gui` browses the disk and archives in a native GPUI window. The title bar keeps the Arca branding and window controls; one navigation band contains history, breadcrumbs, view selection, filtering, extraction, testing and the **More** menu.

## Browsing the disk

Started without an archive, the window shows the folder it was last in (or your home folder) as a normal file explorer. Folders open in the same Columns or Details views used for archives; each folder is read when you enter it, not ahead of time. The sidebar lists **Places** (home and the standard user folders), **Pinned** folders and **Devices** (the mounted volumes, or the drives on Windows). Click one to jump there; right-click a folder row and choose **Pin to sidebar** to keep it at hand, and right-click a pinned entry to unpin it. **More > View > Show hidden files** toggles dot files and hidden entries.

Double-clicking an archive in a supported format (`.zip`, `.7z`, `.tar`, `.iso` and the ZIP containers) opens it in place: the breadcrumbs, tree and panes switch to its contents and the usual archive actions become available. **Up** or **Backspace** at the archive root, or the **Close archive** button, returns to the folder it came from with the cursor on the archive. Other files open with the system application; the preview panel shows local files the same way it shows archive entries.

On the disk, **Compress** uses the ticked rows as its input, **Copy path** puts absolute paths on the clipboard, and the archive-only actions (extract, test, rename, delete, paste, new folder) stay off. The last folder visited, the pinned folders and the hidden-files preference are remembered between sessions.

## Navigation and layout

**Open** and **Create** are in the sidebar, alongside recent archives and, when an archive is open, its folder tree. The sidebar button cycles between expanded, icon rail and hidden. Drag the divider to resize the expanded sidebar. **More > View** also controls its visibility.

The default **Columns** view keeps a directory in each vertical pane. Click a folder to open its child pane; ancestors remain to the left. Choosing another folder replaces only the descendants. Each pane retains its own cursor, selection, scrolling, filter and sort order. Click or focus a pane before working on its contents. Drag pane dividers to resize them, and scroll horizontally to reach ancestors. The active pane is revealed when navigation changes it.

Breadcrumbs navigate to the archive root or a parent folder. A menu exposes middle segments when the path is too long. The tree and breadcrumbs follow the active pane. Back/forward buttons and **Alt+Left/Right** use the same history.

**Details** retains the sortable metadata table. Right-click its header to choose metadata columns. **More > View > Flat view** switches to Details and lists entries without the folder hierarchy. Switching views keeps the location and selection that remains valid.

Layout choices and widths are remembered. Narrow windows temporarily reduce the sidebar to a rail and suspend the preview before squeezing the browser; widening restores the preferred panels.

## Selection and archive actions

- Click a file to select it; Ctrl+click toggles and Shift+click selects a range. Use these modifiers to select folders without opening them in Columns.
- **Space** toggles the cursor row; **Ctrl+A** selects visible rows. Selection commands operate only in the active pane. The parent row in Details is never a command target.
- Right-click a row for extraction, preview, rename, delete and other applicable actions. Right-click empty pane space for destination actions.
- Drag entries onto a folder or directory pane to move them within a writable ZIP. Drop external files onto a directory to add them. The target highlights during a drag.
- New-folder dialogs and paste menus name their destination. ZIP mutations are serialized; RAR remains read-only.
- File clipboard copy/cut/paste and drag-out retain their existing Windows integration. **Ctrl+Shift+C** copies names as text.
- **More** includes selection tools, password changes, verification, undo, settings and shortcut help.

Double-clicking a file, or pressing **Enter** on it, explicitly extracts it to a temporary location and opens it with the system application. Previewing does not do this.

## Integrated preview

The right panel follows the active file cursor. **F3** enables preview; its close button hides it and returns focus. The footer can show it again. Drag its divider to change its remembered width.

The panel shows the filename, size, date and type, with unavailable metadata labelled explicitly. Text uses a monospaced, line-numbered list; hexadecimal and supported-image views are available in the same panel. Images fit the panel.

Loading, empty, password-required, wrong-password, unsupported, oversized and error states are shown in place. Reads, text preparation and image decoding run in a bounded background worker, independently of mutation jobs. Navigation or selection changes invalidate old results. Passwords and preview contents are not written to settings.

## Passwords

The create dialog offers ZIP and 7z passwords; 7z also offers **Hide file names**.
Hidden 7z headers prompt before listing. Content validation runs in a background
worker before extract/test/preview/open actions; retry resumes that action and
cancel leaves the destination untouched. **Remove password** and **Set password…**
apply only to ZIP, never to an existing 7z.

## 7z

Create 7z using Store or LZMA2 and the existing levels. Creation and extraction
are sequential. Selected members of solid archives share one decoder per block
per pass, rather than reopening it for each member. Preview, open-file, Copy and
Extract use the same validated read path. See [Encryption](encryption.md) and
[resource limits](architecture.md#7z-boundaries).

## Format limits

Adding, deleting, renaming and changing passwords in an existing archive remain
ZIP-only. 7z virtual-file drag-out is disabled to avoid repeatedly decoding solid
blocks; use Copy or Extract instead. Copying to the system file clipboard is
platform-dependent.

## Jobs and keyboard access

Progress, pause, cancel, completion and errors remain visible without a browsing modal when the operation permits navigation. Pause and cancel also act inside a single large entry: a cancelled extraction removes the file it was writing and keeps the ones it had finished. Mutation commands cannot start another job while one is running. Explorer one-shot operations retain their existing completion behavior.

The footer reports the active directory, visible/selected counts and job status, with **F1** help. Tab reaches navigation, sidebar, browser, preview controls and footer. Text fields keep their editing keys; closing dialogs restores focus. Controls expose AccessKit labels and focus/selection indicators.

See [Keyboard shortcuts](keyboard-shortcuts.md).
