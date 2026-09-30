---
name: add-rust-feature
description: Rules for adding a feature to a Rust project in this repo. Use when implementing a new feature, opening a feature branch, or wiring app logic before merge.
---

# Add a Rust feature

## Branch

- Create a dedicated branch, e.g. `feature/<short-name>`.
- Do not land unfinished or placeholder code on `main`.
- Open a PR into `main` when the feature is ready.

## Fmt and clippy are part of the feature

Every feature change **includes** rustfmt and clippy cleanup in the same commits. Do not open a PR that still needs a follow-up "fix CI" commit.

Required before considering the feature done:

1. `cargo fmt --all` (or write the files already in that layout).
2. `cargo clippy -- -D warnings` — apply the suggested fixes in the same diff.
3. `cargo test`.

Git hooks and CI enforce this, but they do **not** run on GitHub API commits (`push_files`, `create_or_update_file`). The agent must still ship fmt-clean and clippy-clean code.

If `cargo fmt` / `clippy` cannot run in the current environment:

- Break long calls (`MenuItem::with_id`, `format!`, `prompt_text`) the way rustfmt would.
- Do not add unused `pub fn`, unused imports, or extra `&` on `impl AsRef` / `Into` args (`needless_borrows_for_generic_args`).
- Prefer let-chains / collapsible `if`.
- Never leave dead code "for later".

Local hooks (once per clone: `./scripts/install-git-hooks.sh`):

- **pre-commit** — `cargo fmt --all`
- **pre-push** — `cargo clippy -- -D warnings` then `cargo test`

Do not use `--no-verify` on feature branches unless there is no Rust toolchain.

CI must stay green: rustfmt `--check`, clippy `-D warnings`, `cargo test`, cargo-machete.

## API and modules

- Match existing module boundaries; extend rather than rewrite when possible.
- If a function signature changes (extra args, new columns), update **all** call sites in the same change.
- Keep tests green; add tests next to the behavior you change.

## Before PR / merge

- [ ] Feature branch, not `main`
- [ ] Same change includes fmt + clippy fixes (not a later commit "because CI failed")
- [ ] Diff would pass `cargo fmt -- --check` and `cargo clippy -- -D warnings`
- [ ] Hooks installed when committing from a machine with Cargo
- [ ] CI green on the PR
- [ ] No unused deps or dead code

## PR hygiene

- Short title describing the feature.
- Body: what changed, how to test manually, any new config or files on disk.
