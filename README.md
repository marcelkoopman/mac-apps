# mac-apps

Monorepo for small macOS menu bar apps written in Rust, organised as one Cargo workspace.

| Path | Kind | Description |
| --- | --- | --- |
| [`apps/copycraft`](apps/copycraft/README.md) | app | Clipboard transformer. A global hotkey opens a card at the cursor for whatever you copied: format, convert, decode, table view, schema, OCR/QR, with sensitivity labels and in-memory history. |
| [`apps/ticker`](apps/ticker/README.md) | app | Menu bar price ticker (Bitcoin, ETH, gold, TTF gas, NL fuel and power prices) with day change and price-watch notifications. |
| `crates/mac-ui` | library | Shared look & feel for the apps. Owns and re-exports the UI frameworks (tray-icon, winit, objc2*) and holds generic menu bar helpers (`tray`: info/version/quit rows; `icon`: RGBA canvas, image-file icons, SF Symbols; `glass`: Liquid Glass window background with a frosted fallback before macOS 26; opt-in features `theme`: System/Light/Dark appearance, `widgets`: AppKit label/field/button/box, SF Symbol and pill buttons, read-only text view/scroller constructors, plus `fonts`: monospace font with a fallback list; `panel`: borderless floating panel setup, activation and near-cursor placement; `dialog`: modal NSAlert message, confirm, button choice, text prompt and list pick; `notify`: user notifications via UserNotifications (objc2-user-notifications), osascript without an app bundle). Both apps depend on it. |

## Layout

```
.
├── Cargo.toml              # workspace: members, shared [workspace.dependencies], all [profile.*]
├── Cargo.lock
├── rust-toolchain.toml     # pinned Rust version (local and CI)
├── AGENTS.md               # rules for coding agents
├── apps/
│   ├── copycraft/          # bin: Cargo.toml, src/, assets/, LICENSE (Apache-2.0)
│   └── ticker/             # bin: Cargo.toml, src/, assets/, config.toml, LICENSE (MIT)
├── crates/
│   └── mac-ui/             # lib: shared UI frameworks + menu bar helpers
├── scripts/
│   ├── build_app_icon.sh   # compile the app icon into a .app with actool (before signing)
│   └── tag_release.sh      # bump + tag + push a release for one app
└── .github/workflows/
    ├── ci.yml              # fmt, clippy, test + app icon check (macos-26, Xcode 26.6); cargo audit weekly/manual
    ├── release-copycraft.yml
    └── release-ticker.yml
```

## Requirements

- macOS (both apps target macOS; the release DMGs are built for Apple silicon, `aarch64-apple-darwin`)
- Rust as pinned in `rust-toolchain.toml` (rustup installs it on first use; edition 2024)
- [`cargo-bundle`](https://github.com/burtonageo/cargo-bundle), only to build a `.app` bundle locally

## Build and run

Run everything from the repo root and pick the package with `-p`:

```bash
cargo run -p copycraft
cargo run -p ticker
cargo build --release -p copycraft
```

Local `.app` bundle (run it from the package directory, because cargo-bundle resolves icon, resource and plist paths from there; the output lands in the workspace `target/`):

```bash
cd apps/copycraft && cargo bundle --release
# -> target/release/bundle/osx/Copycraft.app
```

## Test and lint

Same checks as CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Per crate: `cargo check -p <crate>`, `cargo test -p <crate> <filter>`, `cargo clippy -p <crate> --all-targets -- -D warnings`.

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests for pushes and PRs to `main`. `cargo audit` runs weekly and on manual dispatch.

CI and the release workflows are pinned: runner image `macos-26` (`ubuntu-24.04` for the audit), Xcode via `sudo xcode-select -s /Applications/Xcode_<XCODE_VERSION>.app` (`XCODE_VERSION` at the top of each workflow), Rust via `rust-toolchain.toml` (`rustup toolchain install`), and actions on major versions (`actions/checkout@v7`, `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`). Bump these on purpose, one at a time; the Xcode versions an image has are listed in its README in [actions/runner-images](https://github.com/actions/runner-images).

## Release

```bash
scripts/tag_release.sh copycraft   # or: ticker
```

The script needs a clean working tree. It switches to `main` (override with `BRANCH=...`), pulls, and bumps the patch version in `apps/<app>/Cargo.toml` (both the package and bundle `version`). After you confirm, it commits `<app>: bump version to X.Y.Z` with `Cargo.lock`, tags `<app>-vX.Y.Z`, and pushes the branch and tag. It uses BSD `sed -i ''`, so run it on macOS.

The tag triggers `release-<app>.yml`:

1. check that the tag matches the version in `Cargo.toml`
2. `cargo bundle --release --target aarch64-apple-darwin`
3. compile the app icon with `scripts/build_app_icon.sh` (actool: `Assets.car` + `AppIcon.icns`, `CFBundleIconName`/`CFBundleIconFile`). The source is the first of `apps/<app>/assets/AppIcon.icon` (Icon Composer, macOS 26; needs Xcode 26), `AppIcon.appiconset/`, or the existing `icon.icns`; without any of them the step is skipped
4. ad-hoc sign the app (`codesign --sign -`, also `scripts/sign_app.sh`)
5. build a DMG with `create-dmg`
6. publish `<app>-X.Y.Z.dmg` with `gh release create` (auto-generated notes)

Downloads: [Releases](https://github.com/marcelkoopman/mac-apps/releases).

The apps are ad-hoc signed, not notarized, so Gatekeeper may block the first launch.

## License

Each app carries its own license:

- copycraft: Apache-2.0 ([`apps/copycraft/LICENSE`](apps/copycraft/LICENSE))
- ticker: MIT ([`apps/ticker/LICENSE`](apps/ticker/LICENSE))

The repo root and `crates/mac-ui` have no license file.
