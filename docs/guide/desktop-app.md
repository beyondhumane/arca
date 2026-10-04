---
description: Browse archives like folders in a fast, keyboard-friendly native window.
group: Desktop
order: 8
keywords: gui window arca-gui desktop app gpui accessibility nvda narrator
---

# The desktop window

The `arca-gui` window opens archives like folders. It’s GPU-rendered with GPUI and designed so it can be driven entirely from the keyboard.

## Browsing

- Double-click a folder to go into it.
- Double-click a file to pull that one entry out to a temporary folder and hand it to whatever your system opens it with.
- The whole row answers to the mouse, not only the name, and the cursor changes to show it.

## Navigation

The mouse back and forward buttons move through where you’ve been, and so do <kbd>Alt+←</kbd> and <kbd>Alt+→</kbd>. The three arrows on the toolbar do the same thing, plus one level up.

## Native file icons

Each row carries the icon your desktop shows for that kind of file, so a listing looks like the file manager next to it. On Windows that’s a single shell call, asked by name without opening anything, because the entries don’t exist on disk. Elsewhere the window draws its own icons.

## Selecting

- Click for one entry, Ctrl+click to add or drop one, Shift+click for everything in between.
- Press on the list and drag for a rectangle that takes whatever it touches.
- From the keyboard: <kbd>Space</kbd> ticks a row, <kbd>Shift+↓</kbd> ticks a run, and <kbd>Ctrl+A</kbd> ticks everything, or unticks it if it’s all ticked.

## Columns and context menus

Right-click a row for what can be done to it. Right-click the header to switch columns on or off (size, packed, method, saved, modified and CRC32), and your choice is remembered between runs. Name always stays, so the list never shows sizes without names.

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

## XZ and TAR.XZ

The create dialog offers TAR.XZ and XZ with LZMA2 at the usual levels. XZ takes
exactly one file; with a folder or several files the dialog asks for TAR.XZ
instead. A standalone `.xz` opens as a list with one entry named after the
archive without `.xz`. Test, preview, Copy and Extract read the stream to the end
and check it, and a stopped or failed creation leaves no half-written archive.

## Deliberately missing

Adding, deleting, renaming and changing passwords in an existing archive remain
ZIP-only. 7z virtual-file drag-out is disabled to avoid repeatedly decoding solid
blocks; use Copy or Extract instead. Copying to the system file clipboard is
platform-dependent.

## Accessibility

The window declares itself through AccessKit, so Narrator and NVDA can read it. Every action has a keyboard path; see [Keyboard shortcuts](keyboard-shortcuts.md).
