# Migration of `arca-gui` and `arca-setup` back to egui

Status: **done.** Both windows draw with egui 0.36 / eframe over Glow (OpenGL). GPUI, GPUI Kit and `gpui-component` are no longer in `Cargo.lock`.

## Why

The move to GPUI (`migration-to-gpui.md`, commit 1816bbb) was not measured before it was made. Measuring it afterwards showed it cost a lot of binary size, dependencies, build time and memory, and gained nothing that could be measured. egui also draws on the GPU (OpenGL through Glow, or WGPU), so going back does not lose GPU rendering. What GPUI does offer is a retained model that only repaints what changed, somewhat finer text, and the ready-made widgets from `gpui-component`. egui has a stable, well-documented API, and more people and agents know it.

## What changed

Only the view. `controller/`, `model.rs`, `archive_ops/`, `tree.rs`, `i18n.rs` and the settings are the same as on `main`. The new view lives in `arca-gui/src/ui/`:

- `mod.rs`: `Shell`, the eframe `App`. It repaints only while there is work in flight (jobs, listing, preview, file dialogs, update check), and on the 100 ms `TICK` while that lasts. It also handles the startup size, files dropped onto the window, dragging entries out of it, and shortcuts.
- `header.rs`, `sidebar.rs`, `browser.rs`, `preview.rs`, `jobs.rs`, `dialogs.rs`: the toolbar and footer, places and the folder tree, Details/List and Columns, the preview, tasks, and every dialog.
- `theme.rs`, `widgets.rs`: Tokyo Night dark and light, with contrast checked against WCAG in tests; Sora and Inter; Phosphor icons; AccessKit roles and labels.

[Strata](https://github.com/lgse/strata) was a visual reference for the parts the GPUI shell did not have. It is not a dependency, and no code was taken from it.

Gotcha: Inter has glyphs in the Private Use Area at the same codepoints as some Phosphor icons (arrows, carets, archive). Icons are therefore drawn with a font family that contains only Phosphor (`theme::icon_font`, `theme::icon_text`). Phosphor cannot go first in `Proportional`, because it also has glyphs for a–z.

## Measurements

Machine: 8 vCPU Intel Xeon Platinum 8559C VM, Linux/X11, **no physical GPU**: both frameworks render through Mesa llvmpipe/lavapipe, so frame rates are not representative and are left out. GPUI is `main` before this change and egui is this branch, both with the workspace `release` profile (fat LTO, `strip = true`).

| | egui (this branch) | GPUI (`main`) |
| --- | ---: | ---: |
| `arca-gui` release binary | 17.8 MiB | 34.4 MiB |
| `.text` (`cargo bloat`) | 13.1 MiB | 27.0 MiB |
| Clean release build of the workspace | 195 s | 414 s |
| Incremental release build after touching `arca-gui/src/main.rs` | 126 s | 261 s |
| PSS, empty window | 56 MiB | 113 MiB |
| PSS, 100 000-entry zip | 82 MiB | 140 MiB |
| Threads | 22 | 53 |
| Time until the window is mapped | 88–168 ms | 114–171 ms |
| Idle CPU, 100 000-entry zip | 0 % | 270 % |

The GPUI idle figure comes from the shell calling `cx.notify()` on every 100 ms tick, not from GPUI itself. egui was also at 87 % until an update check that ends with no newer release stopped being waited on forever (`AppController::receive` now drops the channel once the thread hangs up).

Reproduce (scripts kept outside the repository; each one is a few lines of `xdotool` and `/proc`):

```sh
# size and crates
cargo build --release -p arca-gui && ls -l target/release/arca-gui
cargo bloat --release -p arca-gui --crates -n 25
# clean and incremental builds
rm -rf target && time cargo build --release
touch arca-gui/src/main.rs && time cargo build --release -p arca-gui
# memory, threads and idle CPU: start the window, wait for it to be mapped
# (`xdotool search --onlyvisible --pid`), wait 3 s, then sample
# /proc/<pid>/{stat,status,smaps_rollup} for 10 s
target/release/arca-gui big.zip   # big.zip: 100 folders x 1000 small files
```

## Left out

- WGPU: Glow starts sooner and does not pull in the Vulkan/DX12/Metal runtimes. Switching is a matter of eframe features if a platform ever needs it.
- Frame rates on real GPUs still need measuring on Windows and macOS hardware.
