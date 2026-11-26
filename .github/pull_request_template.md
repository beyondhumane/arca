## What and why

<!-- The user-visible or technical outcome, and the reason for it. Link the issue: Closes #123 -->

## Type

<!-- Use the same type as the PR title: feat, fix, perf, refactor, test, docs, build, ci, chore, revert. -->

## Checks

<!-- Run the narrowest checks while iterating and the full set before asking for review. List anything you could not run, and why. -->

- [ ] The title follows Conventional Commits, in English: `<type>(<scope>): <imperative description>`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo build --release --no-default-features` and `cargo build --release`
- [ ] `bash interop.sh`
- [ ] Windows-only changes: `cargo test` and `cargo clippy --release --all-targets -- -D warnings` in `windows/arca-shell`

## Parsers and safety

- [ ] Not applicable: this PR does not touch code that reads archive bytes
- [ ] No `unsafe` in code that parses untrusted archive bytes
- [ ] A test covers malformed or truncated input and the code does not panic

## Other

- [ ] A bug fix comes with a regression test
- [ ] User-facing text and docs are updated (Spanish with correct accents; English and `docs/guide/es` kept in step)
- [ ] Benchmark numbers, if any, come with the command that reproduces them and the machine they ran on
- [ ] No hand edits to `CHANGELOG.md` or the version (release-please owns them)

## Platform limits and notes

<!-- Platforms you could not test, behaviour that differs between systems, follow-up work. -->
