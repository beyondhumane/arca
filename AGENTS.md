# Arca contribution guide

This file defines the project-wide rules for agents and contributors. More-specific `AGENTS.md` files override these rules for their directory.

## Project shape

Arca is a cross-platform archiver written in Rust. The workspace crates live under `crates/`; the root also holds `docs/`, `site/`, `packaging/`, `brand/`, `benchmarks/` and `windows/`.

- `arca-core`: shared errors, bounded parsing, limits, and path safety.
- `arca-zip`: ZIP/Zip64 reading and writing, compression, and encryption.
- `arca-tar`: TAR reading and writing.
- `arca-xz`: XZ streams in safe Rust.
- `arca-stream`: standalone gzip, XZ, bzip2 and Zstandard streams, with or without a TAR inside.
- `arca-7z`: bounded 7z reading, solid extraction, and Store/LZMA2 writing.
- `arca-rar`: RAR/CBR reading and RAR5 creation.
- `arca-iso`: read-only ISO 9660.
- `arca-extra`: read-only CPIO, AR, DEB, RPM, CAB and LZH.
- `arca-disk`: read-only VHD, VHDX, VMDK, DMG and raw disk images.
- `arca-compare`: entry checksums and archive/folder comparison without extracting.
- `arca-bench`: codec benchmarks on data generated in memory.
- `arca-cli`: the `arca` command-line binary.
- `arca-gui`: the desktop window.
- `arca-setup`: the installer window and engine; releases still ship the Inno Setup one.
- `arca-sfx`: the self-extracting stub that `arca create --sfx` prepends to a ZIP.
- `arca-notify`: broadcasts environment and file-association changes to Windows.
- `arca-brand`: brand colours, typefaces and colour helpers shared by `arca-gui` and `arca-setup`.
- `arca-icons`: desktop file-type icons.
- `arca-drag`: drag-and-drop integration.
- `arca-net`: networking support.
- `windows/arca-shell`: Windows Explorer integration; it is outside the workspace.

Vendored third-party crates live in `crates/arca-7z/vendor`, `crates/arca-rar/vendor` and `crates/arca-xz/vendor`; they are outside the workspace and their patches are listed in each `PATCHES.md`. Paths in scripts, workflows and docs include the `crates/` prefix.

Read `README.md` before making broad changes. Design rationale lives in `docs/plans/design-rationale.md`; other plans and project history live in `docs/plans/`.

## General rules

- Keep changes focused and minimal. Do not rewrite unrelated code or discard existing working-tree changes.
- Reuse existing helpers, types, and patterns before adding new ones.
- Preserve the safe-Rust boundary: parsers that process untrusted archive bytes must remain free of `unsafe` code.
- Every parser change must include a test for malformed or truncated input and must not panic.
- Keep internal identifiers and source strings ASCII-only. User-facing Spanish text and Markdown documentation must use correct accents.
- Do not add comments to `src/` unless they explain a non-obvious safety or correctness invariant. Build files and CI configuration may include explanatory comments.
- Never publish benchmark numbers without the command needed to reproduce them.

## Rust workflow

Run the narrowest relevant checks while iterating, then run the full checks before handing work over:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --no-default-features
cargo build --release
bash interop.sh
```

For Windows-only changes, also run the commands from the relevant directory, including `cargo test` and `cargo clippy --release --all-targets -- -D warnings` in `windows/arca-shell` when applicable.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) in **English**.

Format:

```text
<type>(<scope>): <imperative description>
```

Rules:

- Use a lowercase type and scope; omit the scope when it adds no information.
- Write the subject in English, imperative mood, in sentence case, without a trailing period.
- Keep the subject concise (preferably 72 characters or fewer).
- Use the body to explain **why**, not to restate the diff. Write it in English too.
- Add `!` after the type or scope for a breaking change and explain the migration in the body.
- Do not combine unrelated changes in one commit.

Allowed types:

- `feat`: user-visible functionality
- `fix`: bug fix
- `perf`: measurable performance improvement
- `refactor`: behavior-preserving code change
- `test`: tests only
- `docs`: documentation only
- `build`: build system or dependency changes
- `ci`: continuous-integration changes
- `chore`: maintenance that does not fit the categories above
- `revert`: revert a previous commit

Good examples:

```text
feat(arca-cli): add archive password command
fix(arca-zip): reject truncated central directory
perf(arca-gui): avoid rebuilding rows during drag selection
docs: document interoperability checks
```

Pull request titles should follow the same format. The description should state the user-visible or technical outcome, relevant tests, and any platform limitations.

## Issues and pull requests

- Issue forms live in `.github/ISSUE_TEMPLATE/` and the pull request template in `.github/pull_request_template.md`. Keep them in line with this guide and with `docs/guide/contributing.md` when the checks or the workflow change.
- Fill in the pull request template; delete a checklist item only if it truly does not apply, and say why.
- Bug reports must name the component, `arca --version` (or the GUI version), the operating system and the exact command or steps. Ask for a minimal archive that reproduces it, never for one that contains private data.
- Labels in use for issues are `bug`, `enhancement` and `question`; do not invent new ones.

## Documentation and releases

- Put architectural plans in `docs/plans/`.
- Keep `README.md` focused on user-facing usage, supported formats, interoperability, and reproducible checks.
- Keep brand rules in `brand/BRAND.md`.
- Releases come from release-please: merge its release pull request once CI is green. Do not tag or edit `CHANGELOG.md` and the version by hand.
- There are exactly two update channels, Stable and Preview; do not add others. Stable is the GitHub release marked latest, made by release-please. Preview is a prerelease made by running the Release workflow by hand with `preview` ticked: it is tagged `vX.Y.(Z+1)-preview.<run>`, the GUI is built with `ARCA_VERSION` set to that name, and it never reaches the package managers.
- The GUI asks about updates once at startup when `updates = yes` in `gui.conf`, and whenever Settings > Updates > Check now is pressed. `channel = stable|preview` picks which release it asks for: `releases/latest` or the newest of `releases`.
