# Migration plan for `arca-gui` to GPUI Kit

Status: **superseded.** `arca-gui` and `arca-setup` went back to egui; see `migration-to-egui.md` for why and for the measurements.

## Summary

Migrate only Arca's interface (not `arca-core`, `arca-zip`, `arca-tar`, the CLI or the formats) to GPUI Kit, keeping the current behaviour and making the transition in phases. The GPUI dependency is pinned by `Cargo.lock` to avoid accidental API changes.

The real starting point is `arca-gui`: a window of roughly 4,600 lines in `src/main.rs`, plus `theme.rs`, `tree.rs`, `glyphs.rs`, `clipboard.rs`, `i18n.rs` and `build.rs`. The current UI includes hierarchical browsing of ZIP/TAR/TAR.GZ, a table with configurable columns, row and folder selection, sorting/resizing, filter, breadcrumbs, double click, shortcuts, light/dark/system themes, English/Spanish, progress, dialogs, drag-and-drop, Windows clipboard and drag-out through `arca-drag`.

## Phases

### 1. Baseline and GPUI spike

- Record the current state with:
  - `cargo test --workspace`.
  - `cargo build --release`.
  - Windows-specific build and test.
  - manual testing of ZIP, TAR and TAR.GZ, password, conflicts, selection, clipboard and drag-and-drop.
- Capture a behaviour matrix and minimum window sizes to use as the parity criterion.
- Add GPUI Kit as a dependency of `arca-gui` from the pinned version.
- Create a minimal GPUI window that builds and starts on the supported platforms.
- Verify in this spike:
  - minimum required Rust version;
  - window/rendering backend on Windows, Linux and macOS;
  - text, fonts, images, keyboard, wheel, selection, menus, dialogs, drag-and-drop and accessibility;
  - integration with `rfd`, `clipboard-win` and `arca-drag`.
- If the viable commit does not support Rust 1.75, raise `rust-version` and update CI/documentation to the minimum GPUI requires. No artificial compatibility will be kept with a Rust version GPUI does not support.

### 2. Separate application state from the previous toolkit

Without rewriting the compression logic:

- Extract from `Arca` a toolkit-independent application state/controller for:
  - open archive, entries, current folder and history;
  - selection, cursor, filter and sorting;
  - settings, language, theme and columns;
  - active jobs, progress, errors and notices;
  - pending password, conflict, delete and drop states.
- Keep `Job`, `Message`, `Answer`, `Pending`, `Format`, `Columns`, `SortColumn` and the archive functions in plain Rust, isolated from the visual APIs.
- Keep the controller decoupled through explicit events/actions: open, extract, compress, delete, add, copy, paste, navigate and cancel.
- Keep the workers on separate threads with the message channel; GPUI only receives events and requests a view update. No disk or compression operation may block the UI thread.
- Keep the existing tests of `tree.rs` and `main.rs`; move the rules for selection, navigation, sorting, free names, progress and dialog transitions into pure tests.

### 3. GPUI application shell

G3 is implemented with GPUI Kit as the only backend. The shell uses
`AppController`, a toolbar, a table and GPUI dialogs. GPUI requires Rust 1.97.1.

- Use GPUI Kit's application, window and root view lifecycle.
- Preserve:
  - dynamic title `name — Arca`;
  - compact initial size for command-line actions;
  - normal initial size for browsing;
  - minimum window size;
  - icon embedded through `build.rs` on Windows;
  - automatic close for operations launched from the shell, and a results window for interactive operations.
- Create a single GPUI root view that derives its rendering from the independent state and processes the actions produced by the components.
- Replace `request_repaint`/`request_repaint_after` with GPUI's invalidation/notification and timer mechanism, keeping progress updates at roughly 100 ms and continuous redraws only when needed.

### 4. UI migration by surface

Implement and validate each surface with GPUI Kit:

1. **Toolbar and navigation**
   - Open, compress, extract all, extract selection, password and overflow menu.
   - Filter field with focus, placeholder and shortcuts.
   - Back, forward, up and breadcrumbs with truncation and a menu of hidden folders.
   - Visible/selected counter.

2. **File list**
   - Use GPUI's virtualized list for the table inside `arca-gui`.
   - Keep the Name, Size, Packed, Method, Saved, Modified and CRC32 columns.
   - Keep configurable columns and their persistence in `gui.conf`.
   - Keep sorting, the triangle indicator, resizing from the header, folder rows before files and icon rendering.
   - Keep single selection, Ctrl/Cmd, Shift, folder selection, keyboard cursor, Home/End/PageUp/PageDown and scroll to cursor.
   - Keep double click, Enter, context menu, rubber-band selection and autoscroll while selecting.

3. **Empty states and status bar**
   - Empty state with a drop hint.
   - Progress, current file, errors, notices and a summary of the open archive.
   - Operation view with progress, duration, result and close.

4. **Dialogs and overlays**
   - Settings for language, theme, format, codec, level and extract to subfolder.
   - New/current/required password, password visibility and Enter/Escape.
   - Destination conflict with Replace/Skip/Rename and their "all" variants.
   - Delete confirmation.
   - Confirmation for opening or adding an archive dropped onto another archive.
   - Shortcuts window.
   - Every dialog will be modal or will explicitly block background actions while it waits for an answer.

### 5. Theme, typography, icons and painting

- Turn `theme.rs` into Arca's own theme tokens: colours, backgrounds, borders, selection, cursor, radii, spacing and type sizes.
- Keep light, dark and system themes, and save/load the existing preference.
- Reuse system fonts on Windows and the GPUI backend's fallback fonts.
- Port `glyphs.rs` to whatever drawing primitive GPUI offers; do not add an icon library to replace eight shapes that are already drawn.
- Port the file-type icon and the per-extension cache. The cache must store the GPUI equivalent of a texture/image and remember failures as it does now.
- Port the special shapes: sort triangle, cursor, rubber-band selection, drop overlay and autoscroll pointer.
- Review contrast and accessible semantics of buttons, rows, menus, fields and dialogs through AccessKit/GPUI.

### 6. Platform integrations

- Keep `rfd` for file and folder selection.
- Keep `clipboard-win` and its CF_HDROP implementation for copying/cutting/pasting files on Windows.
- Keep `arca-drag` for drag-out on Windows and preserve its release/cancel handling.
- Keep dropping files onto the window on the platforms where GPUI exposes it; only adapt the event bridge.
- Keep opening files with the system application and the temporary-file behaviour.
- If a GPUI capability has no direct equivalent, wrap only that bridge in a platform module; do not pollute business state with GPUI APIs.

### 7. Visual redesign with GPUI Kit

After reaching functional parity, validating accessibility and completing the platform matrix, do a full visual redesign of the GPUI surface using [GPUI Kit](https://gpui-kit.com/apps/) as the reference for components and visual direction.

#### Decisions made when the phase opened

- **GPUI Kit becomes a real dependency, not just a reference.** The pin to a zed
  rev is dropped: `gpui-component` builds against `gpui-pre ^0.3`, and keeping
  the git rev left two copies of GPUI in the graph, which do not link.
  `arca-gui` now depends on `gpui-pre` and `gpui-pre-platform`, renamed to
  `gpui` and `gpui_platform` in `Cargo.toml`, so no `use gpui::...` changes.
  Reproducibility comes from `Cargo.lock`, as it came from the rev before.
- **Monochrome with a brand tint.** Six greys per mode, hue 225 (the night blue
  of `brand/BRAND.md`) at 8-12% saturation, inverted between light and dark.
- **No accent colour.** Selection, keyboard cursor and focus ring are the text
  colour at different strengths, so contrast is guaranteed by construction. The
  only saturated pixels are `danger` and `warning`, which tell "extracted" from
  "not extracted" and are not decoration.
- **4/6 px radius, 26 px row, 13 px base font.**
- The tokens live in `arca-gui/src/gpui_theme.rs` and are `gpui-component`'s;
  Arca adds no token layer of its own.
- Keep `AppController`, `AppAction`, the workers, the `gui.conf` format and the platform integrations intact.
- Redesign toolbar, navigation, table, empty states, progress, notifications, menus and dialogs with a coherent system of tokens, spacing, typography, icons, hover/focus/disabled states and responsive behaviour.
- Keep AccessKit roles, visible focus, keyboard, contrast and screen reader support; the redesign must not degrade the accessibility matrix.
- Compare before/after manually in compact and normal windows, with empty archives, large lists, errors, progress and open dialogs.
- Run this phase with a model specialized in interface design and visual review, keeping aesthetic decisions separate from logic changes.

#### Status

- **G7.1 done**: GPUI Kit dependency, monochrome light/dark/system
  `gpui_theme.rs`, and the 67 hardcoded colours in `gpui_shell.rs` replaced
  with tokens. `cargo test --workspace` and `cargo test -p arca-gui` green.
- **G7.2 done**: layout. The reference is **Nohrs** (same problem: a file
  explorer) with the density of **DBFlux**. The window stops being a stack of
  strips floating in padding and becomes full-bleed regions separated by 1 px
  lines:

  ```text
  ┌─ action bar (40 px) ───────────────────────── [filter] ─┐
  ├─ ← → ↑ │ path (34 px) ──────────────────────────────────┤
  │ folders    │  full-bleed table                          │
  │ (224 px)   │                                            │
  ├────────────┴────────────────────────────────────────────┤
  │ archive summary                N visible │ M selected   │
  └─────────────────────────────────────────────────────────┘
  ```

  - **Sidebar with the archive's folder tree**
    (`gpui_component::sidebar`). It is new content, not chrome: before, a deep
    archive could only be walked by double-clicking down and going back. The
    branch of the current folder opens on its own; the rest stays closed. The
    tree is cached by (archive path, number of entries).
  - **Status bar** (`gpui_component::status_bar`) welded to the bottom, with
    the summary on the left and the counters on the right, which used to sit
    in the middle of the navigation row.
  - **Borderless buttons**, with the background appearing only under the
    pointer. A row of seven outlined boxes read as seven things competing.
  - **Icon arrows** (`IconName`, via `gpui-kit-assets`) instead of ‹ › ↑.
  - **Floating menus**: overflow and hidden folders were `absolute`; before,
    they were drawn in the flow and pushed half the window down.
  - **Full-bleed table**, with no border or radius of its own, a pinned header
    and alternating stripes.
- **G7.3 done**: the internal widgets are `gpui-component`'s: `Input`,
  `Button`, `Dialog` on `Root`, menus anchored to their trigger, `Table`,
  `Progress`, `Kbd`, `Radio`, `Tree` and the kit's scrollbar. The per-phase
  detail is in git history (`docs/todos/migracion-gpui-kit.md`, deleted when
  the migration finished).
- **G7.4 done**: the settings dialog exists on the GPUI surface: language and
  theme as radios, and format, compressor, level and code page as menus.

### 8. Retiring the previous backend

When the GPUI view reaches the parity matrix:

- Keep only GPUI and GPUI Kit in `Cargo.toml` and regenerate `Cargo.lock`.
- Remove imports and types of the previous toolkit from the interface modules.
- Delete the adapter and the tests that depend on the previous backend's coordinates; keep their invariants as tests of the new model.
- Review comments so they document GPUI or Arca's behaviour, not the previous implementation.
- Update the README and any build documentation with the new dependency, MSRV and platform requirements.

## Acceptance criteria

- `cargo test --workspace` passes without regressions.
- `cargo build --release` passes on the supported platforms, and the Windows build keeps the icon, clipboard and drag-out.
- The UI starts with and without an archive, and also in the `--extract-here`, `--extract-to-folder`, `--test`, `--add` and `--add-quick` modes.
- Parity is kept for ZIP, TAR, TAR.GZ, AES-256 encryption, extraction, compression, test, delete and add.
- The interaction matrix is kept: keyboard, mouse, double click, multiple selection, cursor, filter, sorting, resizing, wheel/autoscroll, menus and Escape.
- The existing language, theme and column settings are kept without changing the `gui.conf` format.
- Screen readers and keyboard focus are validated on Windows.
- It is checked that no heavy operation runs on the UI thread and that progress keeps updating during compression/extraction.
- A manual visual comparison of toolbar, table, dialogs, themes and empty states is made against the baseline, accepting only GPUI-specific differences that do not alter hierarchy or legibility.

## Fixed assumptions

- The scope is all of `arca-gui`, not a migration of the compression crates.
- The goal is full parity, not a prototype or a temporary reduction in features.
- The transition is incremental, but GPUI will be the single final backend.
- GPUI will be pinned to a reproducible commit/tag of the Zed repository; the exact choice will be made in the build spike and recorded in `Cargo.toml`/`Cargo.lock`.
- Existing dependencies (`rfd`, `clipboard-win`, `arca-drag`) will be reused, and no table or icon library will be added unless the spike shows GPUI cannot cover an essential capability.
- The archive logic will not be migrated and no new business abstractions will be introduced: only the minimum state needed for the view not to depend on the toolkit will be separated.
