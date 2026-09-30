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

The create dialog has a password field. Opening an encrypted archive asks for the password before extracting, and the toolbar offers **Remove password** or **Set password…** depending on the archive.

## Deliberately missing

<kbd>Ctrl+V</kbd> isn’t there, and neither is <kbd>Ctrl+C</kbd> copying files onto the clipboard. Pasting means adding to an archive that already exists, which the writer can’t do yet; copying files out means handing the shell an object it can pull bytes from on demand. Both need real work beyond a key binding, so they’re left out until they work.

## Accessibility

The window declares itself through AccessKit, so Narrator and NVDA can read it. Every action has a keyboard path; see [Keyboard shortcuts](keyboard-shortcuts.md).
