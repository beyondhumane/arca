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

Double-clicking an archive (`.zip`, `.7z`, `.tar`, `.tar.gz`, `.tar.xz`, `.xz`, `.tar.bz2`, `.bz2`, `.tar.zst`, `.zst`, `.rar`, `.iso`, the ZIP containers and the [other read-only formats](more-formats.md)) opens it in place. Inside, the menu offers **Extract selection**, **Extract here**, **Extract all** and **Test**, plus **Rename**, **Delete**, **New folder** and **Paste** in a ZIP, a TAR (plain, `.tar.gz`, `.tar.xz`, `.tar.bz2` or `.tar.zst`) or a 7z; a password is asked for when the archive needs one. **Up** or **Backspace** at the root, or **Close archive**, return to the folder with the cursor on the archive. Other files open with the system application.

## Panes, details and the sidebar

The **Panes** view puts one folder in each vertical pane. Each pane has its own header with **Reload**, **Up**, **Sort**, **Filter** and **Close**, and its own cursor, selection, scrolling, filter and sort order. Three settings in **Settings > Views** (also in the **View** menu) decide how panes behave:

- **Open folders in the next pane** (`next_pane`, off by default): on, selecting a folder opens it in the pane to the right and the ancestors stay to the left, Miller-column style; off, each pane navigates on its own and **Open in a new pane** in the context menu adds one.
- **Peek into folders on hover** (`folder_peek`, off by default): resting the pointer on a folder row shows a card with its first entries without opening it.
- **Focus follows the pointer** (`pointer_focus`, on by default): the pane under the pointer is the active one, so the path bar, filter, shortcuts and actions act on it, without a click and without moving the panes. Any key hands control to the keyboard until the pointer moves again; **Tab**, **Shift+Tab**, **←** and **→** walk the panes. The row of panes scrolls only when navigation opens a pane out of view, with its scrollbar, or with **Shift+wheel**.

**Details** (the default) is a sortable table; right-click its header to choose the columns. **Flat view** lists everything under the current folder without the hierarchy. **Filter** (**Ctrl+F**) belongs to the pane in Panes and to the whole list in Details.

The sidebar button shows or hides the sidebar; when the window leaves too little room for the folders beside it, the sidebar shrinks to a column of icons on its own and comes back when there is room again. Drag the dividers to resize the sidebar, the panes and the preview; every width is remembered.

## Selection and actions

- Click selects, **Ctrl+click** toggles, **Shift+click** selects a range, **Space** toggles the cursor row and **Ctrl+A** selects the visible rows of the active pane.
- **Compress** opens the compress box: archive name with automatic extension, destination folder, format (ZIP, TAR, TAR.GZ, TAR.XZ, 7z, XZ, TAR.BZ2, BZ2, TAR.ZST, ZST or RAR), method, a 0–9 level slider, an optional AES-256 password (with hidden names in 7z), a volume size to split the archive into parts (none, 100 MB, 700 MB, 4 GB for FAT32 or your own, where a plain number means MB; not for RAR; see [split volumes](creating-archives.md#split-volumes)) and the list of what goes in. Opening any part of a split set, `.001` or later, shows the whole archive, read-only.
- **Extract…** (**Ctrl+E**) asks where to unpack; **Extract here** (**Alt+W**) unpacks beside the archive.
- **Properties** shows size, dates, type, permissions and what a folder contains.
- **Checksums** (row menu) works out CRC32, SHA-1 and SHA-256 of the picked files on the disk, folders included, or of the picked entries inside the open archive, all of them when nothing is picked; entries are read straight out of the archive and never written to disk. It runs in **Operations** with progress, pause and cancel, and the result opens in a dialog: pick the algorithm and **Copy** puts the lines on the clipboard in the `sha256sum` format, ready for `arca hash --check` or `sha256sum -c`. A file that can’t be read shows its error in its row, while a folder that can’t be listed stops the task; in TAR, 7z and RAR a damaged entry stops the whole pass, because each entry is decoded from the stream before it.
- **Compare with archive…** and **Compare with folder…** (row menu) set the open archive against another archive or a folder you pick. On the disk they do the same for the one item picked, and with two items picked they compare those two with each other. The comparison runs in **Operations** with progress, pause and cancel, and reads entries straight out of the archives, as [`arca diff`](cli-reference.md#diff) does. The result opens in a dialog: filter it to **All**, **Only in the first**, **Only in the second** or **Changed**, and **Copy** puts the lines shown on the clipboard in the `arca diff` format. The open archive’s password is also tried on the other side; an archive that needs a different one fails the task with the password error.
- **More** includes selection tools, password changes, verification, settings, the shortcut list, the [benchmark](#benchmark) and **Check for updates**; a new version opens a dialog with its release notes and installs itself from inside the app.

## Integrated preview

The right panel (**F3**) follows the cursor: a header with the type icon, name, size, date and MIME type, and buttons to open the file with the system application, switch between **Text**, **Hex** and **Image**, toggle line wrapping and close. Text comes with line numbers and syntax highlighting for shell, Rust, C-like languages, Python, JavaScript and TypeScript, JSON, TOML, YAML, INI, HTML and XML, CSS and Markdown; only the visible lines are laid out, so large files stay fluid. Hex shows offsets in the gutter and images fit the panel. Loading, password and error states are shown in place; passwords and preview contents are never written to settings.

## Passwords

The create dialog offers ZIP and 7z passwords; 7z also offers **Hide file names**.
Hidden 7z headers prompt before listing. Content validation runs in a background
worker before extract/test/preview/open actions; retry resumes that action and
cancel leaves the destination untouched. **Remove password** and **Set password…**
apply only to ZIP, never to an existing 7z. Adding, deleting or renaming entries
of an encrypted 7z asks for its password first if the window does not have it
yet; the rewritten archive keeps that password, and hides its names again if
they were hidden.

**Settings > Passwords** keeps passwords for later, each with a label and a mask
(`*.zip`, `backup-*.7z`, a folder, or none for every archive). The ones with
**Try without asking** on are tried before the password box appears, most
specific mask first; the rest are offered in the box as **Use …** buttons. The
box also has **Save this password**, which keeps what you type once it has
opened the archive. Without a master password, saved passwords last until Arca
closes; with one, they are written encrypted beside the settings and the vault
locks after 10 minutes unused. When it is locked, the password box can unlock it
in place. See [saved passwords](encryption.md#saved-passwords) for the details.

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

Create 7z with LZMA2, LZMA, PPMd, BZip2, Deflate or Store, picked under Method in
the compress box (hover it for when each one helps), in solid blocks with the
filter each program or WAV file calls for; LZMA2 runs on as many cores as fit in
2 GiB. Editing keeps each block's method. See [Creating archives](creating-archives.md#7z); extraction decodes on one. Selected members of solid archives share one decoder per block
per pass, rather than reopening it for each member. Preview, open-file, Copy and
Extract use the same validated read path. See [Encryption](encryption.md) and
[resource limits](architecture.md#7z-boundaries).

## XZ and TAR.XZ

The create dialog offers TAR.XZ and XZ with LZMA2 at the usual levels. XZ takes
exactly one file; with a folder or several files the dialog asks for TAR.XZ
instead. A standalone `.xz` opens as a list with one entry named after the
archive without `.xz`. Test, preview, Copy and Extract read the stream to the end
and check it, and a stopped or failed creation leaves no half-written archive.

TAR.BZ2, BZ2, TAR.ZST and ZST work the same way, with bzip2 and Zstandard as the
fixed method. A standalone `.bz2` shows a size of 0, because bzip2 does not
record it.

## Format limits

Adding, deleting, renaming, moving and making folders work in ZIP, TAR,
TAR.GZ, TAR.XZ, TAR.BZ2, TAR.ZST and 7z; changing a password is ZIP-only. RAR
can be created but not modified afterwards, and ISO, the ZIP containers and
standalone XZ, BZ2 and ZST files are never modified.

Every change writes a new archive beside the old one and reads it back in full
before it takes the old one's place, so a stop or a failure halfway leaves the
archive as it was, and **Undo** puts the previous version back. A TAR is copied
entry by entry with its headers as they were (owner, permissions, times, links
and pax records), the renamed ones changing only their name, and compressed
again with its own codec: at the level picked for new archives when files are
added, at level 6 otherwise. A hard link follows the entry it points to when that
entry is renamed, and deleting or replacing an entry a hard link still points to
is refused.
The `./` entry some tools put at the top of a TAR is not kept. A 7z is decoded
and encoded again block by block, keeping each entry's name, times, attributes,
empty folders and empty files; a stored block stays stored. Memory stays at one
decoder and one encoder however large a solid block is, but the time is that of
decompressing and compressing the whole archive, and of reading it back.

7z virtual-file drag-out is disabled to avoid repeatedly decoding solid
blocks; use Copy or Extract instead. Copy, cut and paste use the system clipboard on
Windows and Linux, so files move between Arca and the Explorer, Thunar, Dolphin
or Nautilus. On Linux the other programs cannot tell a cut from a copy, so a
cut pasted there arrives as a copy, and cutting out of an archive is offered on
Windows only.

## Benchmark

**More > Benchmark…** opens the same benchmark as [`arca bench --codecs`](benchmarks.md#measure-your-own-machine): every codec Arca writes, on data generated in memory, on one thread and on the number of threads picked at the top (every core by default). **Start** runs it in the background and the table fills in as each codec is measured, with a bar for the progress; **Stop**, **Close**, Escape or quitting the app stop it between runs. When it ends the window shows the rating, and **Copy** puts on the clipboard the same text the command line prints, ending with the command that reproduces it. The run uses the defaults of the command, 16 MiB and 1 second per measurement.

## Comments and repair

**More > Comment…** opens the archive comment in a multi-line editor; in a ZIP
it can be changed or emptied to remove it, and in a RAR it is shown read-only.
**Comment…** in the row menu of a ZIP entry does the same for that entry, and
an entry that carries a comment shows it as a tooltip on its row. Saving
rewrites the archive like any other change: every entry is copied unchanged,
so no password is needed even for encrypted entries, the new file is read back
before it takes the old one's place, and **Undo** puts the previous version
back. Only ZIP comments can be changed; 7z has none, and the comments TAR keeps
in pax records are shown by [`arca comment`](cli-reference.md#comment) only.

**Repair…**, in the row menu of a ZIP, TAR, 7z or RAR on the disk and under
**More** for the open archive, runs [`arca repair`](cli-reference.md#repair) as
a task in **Operations**: a ZIP whose directory is gone opens this way even
though it cannot be opened directly. The damaged archive is only read; the
result goes to a new file beside it, `name (repaired).zip`, read back before it
takes that name. When the task ends, a dialog says how many entries came back
and how many were lost, and offers to open the repaired copy. A RAR can only be
repaired from a recovery record it was made with; when that record finds
nothing wrong, nothing is written.

## Jobs and keyboard access

Every operation (extract, test, compress, add, delete, rename, change password, copy) is a row in the **Operations** panel, which floats over the lower-right corner of the workspace and can be hidden and reopened from the footer button that counts them. Each row shows its own progress, elapsed and remaining time, pause/resume and cancel; finished rows keep their result until cleared. Pause and cancel also act inside a single large entry: a cancelled extraction removes the file it was writing and keeps the ones it had finished.

Operations run at the same time when they touch different files. Rewrites of the same archive wait for each other in the order they were asked for, and a rewrite also waits for any extraction or test of that archive that is already running. While the open archive is being rewritten its listing is read-only; extracting, testing or compressing leaves the window free to keep browsing and start more work. Overwrite questions name the operation they belong to. Explorer one-shot operations show the same panel as the whole window and retain their existing completion behavior.

The footer reports the active directory, visible/selected counts and job status, with **F1** help. Tab reaches navigation, sidebar, browser, preview controls and footer. Text fields keep their editing keys; closing dialogs restores focus. Controls expose AccessKit labels and focus/selection indicators.

See [Keyboard shortcuts](keyboard-shortcuts.md).
