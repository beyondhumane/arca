# G0/G1 GPUI spike baseline

Status: historical. `arca-gui` used GPUI and GPUI Kit until it moved back to egui (see `migration-to-egui.md`).

## G0 frozen

- G1 pin: `zed-industries/zed@3384317a9931a21bb5ad8706f0f9d82cb02a71ec`.
  **Superseded in G7**: `arca-gui` no longer depends on that rev but on the
  published GPUI Kit crates (`gpui-pre` / `gpui-pre-platform` /
  `gpui-component`), because `gpui-component` builds against `gpui-pre ^0.3`
  and keeping the git rev left two copies of GPUI in the graph. `spikes/gpui`
  keeps the original pin: it is the record of what was validated in G1 and is
  not rewritten.
- Target toolchain: Rust `1.97.1` (Arca's current tree declares MSRV `1.75`;
  this spike does not change that contract).
- Fixture: `spikes/gpui/fixtures/entries-6000.txt`, generated
  deterministically and validated by the spike's `cargo test`.
- Manual screenshots that must accompany the local run: `empty-window`,
  `fixture-6000`, `filter-ime`, `modal-blocking`, `file-drop` and
  `accessibility-list`. Screenshots are not faked in CI; the result is noted in
  this matrix with platform, backend and date.

### Manual matrix

| Case | Windows | Linux X11/Wayland | macOS | Result/date |
| --- | --- | --- | --- | --- |
| Empty window and close | validated manually | pending | pending | window visible and close available on Windows GNU |
| 6,000 virtualized rows | code + test + visual validation | pending | pending | window run; virtualized list visible |
| Ctrl/Shift, cursor and scroll to cursor | code | pending | pending | full manual validation pending |
| Resizable columns | code | pending | pending | real cells and relative delta; manual validation pending |
| Filter: selection, IME, focus and Tab | code + tests + focus observed | pending | pending | Filter field visible and editable; IME/Tab manual pending |
| Screen reader: rows/field/modal/progress | code | pending | pending | Narrator/NVDA pending |
| Modal blocks background clicks and Escape | validated manually | pending | pending | modal appeared, hid background and Escape closed it |
| RGBA PNG loaded | validated manually | pending | pending | icon loaded in Windows window |
| Explorer/file-drop | pending | pending | pending | manual test pending |
| Virtual `arca-drag`, cancellation and deferred extraction | pending | N/A | N/A | Windows manual test pending |

The run available on this machine is recorded as follows:

- Windows GNU: the `1.97.1-x86_64-pc-windows-gnu` toolchain is installed and
  `rustup run 1.97.1 cargo check --manifest-path spikes/gpui/Cargo.toml --locked`,
  `rustup run 1.97.1 cargo test --manifest-path spikes/gpui/Cargo.toml --locked`
  (4 tests) and `rustup run 1.97.1 cargo check --workspace --locked` pass. So
  do `cargo fmt --manifest-path spikes/gpui/Cargo.toml -- --check`, the
  `platform-probes` check and the manual inspection of window, filter, list,
  icon, modal and Escape.
- Linux and macOS: not marked as compiled; the `x86_64-unknown-linux-gnu` and
  `x86_64-apple-darwin` targets are not installed.
- Arca: `cargo check --workspace` and `cargo test --workspace` pass. The
  existing `cargo fmt --all -- --check` fails on Arca files because of
  formatting drift; those files are not reformatted so the production UI is
  left untouched.

No platform is marked as validated without a real manual test.

## `columns` persistence defect (resolved)

`arca-gui/src/main.rs::Settings::save` wrote `columns = ...` and
`Settings::load` did not read that key, so column changes were lost on restart
even though `gui.conf` kept the line. It was recorded here during G1 and
deliberately **not** fixed then: the production UI and the `gui.conf` format
were not to change during the spike.

`Settings::load` now reads the key. The `gui.conf` format did not change.

## Isolated G1

The binary lives in `spikes/gpui`, outside the workspace. It only adds `gpui`
and `gpui_platform` from the pin above; `gpui_platform` uses per-target
features. The binary imports no production crates on its normal path, does not
touch the production window and does not materialize entries for drag-out. The
`platform-probes` feature keeps isolated Windows compile probes for `rfd`,
`clipboard-win` and `arca-drag`; the check performed shows that their public
APIs compile together, but it does not replace an interactive test of OLE,
clipboard or dialogs. If input/IME, AccessKit, file-drop, `arca-drag` or the UI
loop fails, the migration stops.
