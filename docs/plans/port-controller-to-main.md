# Port `AppController` to today's `main.rs`

Status: transformation done; `cargo test --workspace`, `cargo test -p arca-gui` and `cargo fmt -p arca-gui -- --check` pass.

Done in this batch:

- `AppState` + `AppController` split from `Arca`; the GPUI surface keeps its gestures and textures.
- `AppAction` + `dispatch` available to GPUI.
- `spawn` and the job methods do not depend on the visual toolkit.
- `gpui_shell` uses `tree::Folder` and `state.folders`, with no duplicated cache.

## Where everything is

| | what it is |
| --- | --- |
| `gpui-kit-redesign-v2` | **the live branch**. It starts from today's `main`. It brings GPUI Kit and the shell/theme modules declared in `main.rs`. Green on `cargo test --workspace` and `cargo check -p arca-gui`. |
| `gpui-kit-redesign` | the old branch, on `c4c0354`. **Do not delete**: its `main.rs` is the reference implementation of the extraction. |
| PR #1 | points to the old branch, in draft and in conflict. Once v2 is complete, it gets retargeted or another one is opened. |

**The reference implementation is on the old branch.** `AppState`,
`AppController`, `AppAction` and `dispatch` were already written once, on the
4,441-line base. They do not have to be invented, they have to be redone on a
file that grew:

```sh
git show gpui-kit-redesign:arca-gui/src/main.rs > /tmp/referencia.rs
```

That file has `struct AppState` (55 fields), `struct AppController { state:
AppState }`, `struct Arca { controller, icons, band, wheel }`, the complete
`enum AppAction` and the 48 controller methods, with their receivers already
rewritten. The difference from what has to be done now is that `main` added 8
more fields and 10 more methods.

### Regenerating the inventories

The tables below were produced with this, in case the file moves again:

```sh
# contract the shell needs
rg -o 'controller\.state\.([a-z_0-9]+)' -r '$1' arca-gui/src/gpui_shell.rs | sort -u
rg -o 'controller\.([a-z_]+)\(' -r '$1'    arca-gui/src/gpui_shell.rs | sort -u
rg -o 'AppAction::([A-Za-z_]+)' -r '$1'    arca-gui/src/gpui_shell.rs | sort -u

# impl Arca methods, in order
rg -n '^    (pub )?fn [a-z_0-9]+' arca-gui/src/main.rs
```

The original `gpui-kit-redesign` branch was made on `c4c0354`. While it lived,
`main` moved 18 commits ahead, rewriting `arca-gui/src/main.rs`. Both sides
started from a 4,441-line file:

| | `main.rs` | what it added |
| --- | --- | --- |
| base `c4c0354` | 4,441 | — |
| `origin/main` | 6,103 | +1,662: rename inside the archive, viewer, flat view, mask groups, columns, folder tree, black-and-white dark mode |
| `gpui-kit-redesign` | 5,269 | +828: move the state into `AppController` |

The 46 merge hunks are the same conflict repeated: the branch renamed *every*
state access and `main` wrote 1,662 new lines against the old shape. Resolving
it is not picking a side; it is redoing the extraction on top of today's
`main`. That is why it is done this way rather than by resolving the merge:
**the compiler verifies the transformation**. A badly resolved hunk compiles; a
`self.archive` that escapes the rewrite does not.

## Contract `gpui_shell.rs` needs

Not negotiable: if any of this is missing, the GPUI window does not compile.

**14 `AppController` methods**
`can_go_back`, `can_go_forward`, `codec_name`, `cut_landed`, `dispatch`,
`drag_out`, `is_checked`, `level_name`, `open`, `receive`, `run_job`, `s`,
`summary`, `visible_rows`

**29 `controller.state` fields**
`add_password`, `archive`, `busy`, `checked`, `codec`, `confirm_delete`,
`confirm_drop`, `conflict`, `current_dir`, `current_file`, `cursor`,
`cut_pending`, `done_count`, `entries`, `error`, `filter`, `format`, `level`,
`notice`, `order`, `output_name`, `password_input`, `pending_inputs`,
`replies`, `settings`, `show_password`, `total_count`, `view`,
`waiting_on_password`, `window_title`

**28 `AppAction` variants**
`AnswerConflict`, `AnswerDrop`, `Back`, `BeginPasswordChange`, `CancelJob`,
`CancelPassword`, `ClearSelection`, `ConfirmDelete`, `Copy`, `Drop`,
`ExtractTo`, `Forward`, `InvertVisible`, `Navigate`, `Open`, `OpenFile`,
`Paste`, `PrepareCompress`, `RequestDelete`, `Run`, `SelectAllVisible`,
`SetChecked`, `SetFilter`, `SetPasswordInput`, `Sort`, `SubmitPassword`,
`ToggleColumn`, `TogglePasswordVisibility`

## Splitting the fields

`struct Arca` in `origin/main` has 63 fields. They split like this:

- **`Arca` keeps 4**: `controller`, `icons`, `band`, `wheel`. They are the only
  ones with visual-backend-specific types or mouse-gesture state.
- **`AppState` takes the other 60**, plus 3 that the extraction added and that
  exist neither in the base nor in `main`: `cancel_token`, `extract_dialog`,
  `window_title`.

The 8 new fields from `main` **all** go to `AppState`: they are plain data.

| field | why it goes to `AppState` |
| --- | --- |
| `types` | a per-extension text cache, not a texture (that is `icons`) |
| `renaming`, `rename_fresh` | path and half-typed text; GPUI will rename too |
| `folders` | `tree::Folder`, the folder tree |
| `viewing` | `Viewed` carries no visual-backend types |
| `picking_group`, `mask` | mask selection |
| `geometry` | four `f32` so `on_exit` has something to write |

## Splitting the 57 `impl Arca` methods

**To `impl AppController` (35)**
`s`, `level_name`, `codec_name`, `summary`, `visible_rows`, `spawn`,
`remember`, `open`, `run_job`, `receive`, `extract_here`, `ask_extract`,
`selected_names`, `selected_roots`, `copy_to_clipboard`, `cut_landed`,
`dragged_files`, `drag_out` (×2), `paste_from_clipboard`, `add_files`,
`dropped`, `view_entry`, `cancel_password`, `rename_to`, `set_checked`,
`open_file`, `go_to`, `clear_picked`, `can_go_back`, `can_go_forward`,
`go_back`, `go_forward`, `is_checked`

**Stay in `impl Arca` (22)**
`tree_panel`, `settings_row`, `format_row`, `drop_hint`,
`confirm_drop_window`, `shortcuts`, `viewer_window`, `group_window`,
`confirm_delete_window`, `password_window`, `conflict_window`, `toolbar`,
`breadcrumb`, `shortcuts_window`, `settings_window`, `add_view`,
`running_view`, `column_edges`, `wheel_scroll`, `rubber_band`, `keyboard`,
`table`

**`new` splits in two**: `AppController::new(settings)` and `Arca::new(...)`.

## The one non-mechanical point

`spawn` received a visual context in `main` and requested a repaint. The
extraction has to remove that parameter and replace the repaint with something
both backends can request. It is the only place where the transformation is
not a rename: everything else is moving the method and rewriting the receiver.

## Execution order

1. Split `struct Arca` into `AppState` + `AppController` + `Arca`.
2. Split `impl Arca` into two blocks, moving the UI methods to the end.
3. Rewrite receivers, one rule per block:
   - in `impl AppController`: `self.<field>` → `self.state.<field>`
   - in `impl Arca`: `self.<field>` → `self.controller.state.<field>` and
     `self.<moved_method>()` → `self.controller.<moved_method>()`
4. Add `enum AppAction` and `dispatch`.
5. Remove the visual context from `spawn`.
6. Compile and fix until `cargo check` goes quiet. **This is the step that
   verifies**: every access that escapes shows up as an error.
7. `mod gpui_shell` / `mod gpui_theme` and the `main` that starts GPUI Kit.
8. Simplify `gpui_shell.rs`: delete the hand-made `struct Folder` and its cache
   and use `tree::folders_of` and `state.folders`, which `main` already has.

## What to check at the end

Automatic:

```sh
cargo test --workspace
cargo test -p arca-gui
cargo fmt -p arca-gui -- --check
```

And **by hand, which is what really checks this**: `cargo test` does not touch
the UI, so if the extraction swallows one of the features `main` brought, the
suite stays just as green. Open the GPUI window (`cargo run -p arca-gui`) and
try each one:

- [ ] rename an entry with F2 and from the menu
- [ ] look at a file without extracting it (the viewer)
- [ ] the flat view
- [ ] pick a group by mask, and test only the picked entries
- [ ] extract here
- [ ] the columns: adjust, sort, and check they stay as left after reopening
- [ ] the folder tree in the side panel
- [ ] the black-and-white dark mode

And the GPUI window (`cargo run -p arca-gui`), which was working before the
conflict: action bar, path, side tree, table, status bar, light and dark.

## What was still pending from phase 7, apart from this

It is not lost sight of because of the merge detour; it is in
`migration-to-gpui.md`, section 7, "Status":

1. Replace the hand-made widgets with `gpui-component`'s: `Input` (deletes
   ~400 lines of `FilterInput` and its UTF-16 contract with the IME),
   `Modal`/`Root`, `Popover`, `Table`, `Notification`.
2. Menus float with `absolute` and fixed offsets (`top(38.) right(232.)`), not
   anchored to their trigger. It breaks if the filter width or the font size
   changes.
3. There is no settings dialog on the GPUI surface.
4. Platform matrix: Windows GNU only.
5. Screen reader not validated with Narrator/NVDA.
6. Three button labels rebuilt by hand in `gpui_shell.rs` that deserve a look:
   `"Set Password"`/`"Unlock"`, `"Keep Both"`/`"Keep Both Always"` and
   `"Delete"`.
