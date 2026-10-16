# mac-apps

[![Security audit](https://github.com/marcelkoopman/mac-apps/actions/workflows/security.yml/badge.svg)](https://github.com/marcelkoopman/mac-apps/actions/workflows/security.yml)

Monorepo for small macOS menu bar apps written in Rust, organised as one Cargo workspace.

| Path | Kind | Description |
| --- | --- | --- |
| [`apps/copycraft`](apps/copycraft/README.md) | app | Clipboard transformer. A global hotkey opens a card at the cursor for whatever you copied: format, convert, decode, table view, schema, OCR/QR, PDFs (the page itself with page turning, its text layer through PDFKit, or OCR on scans), line numbers for text and code, line tools (head, tail, reverse, column, regex filter) and a paged view for long text, on-device AI actions per card type (Apple Foundation Models, macOS 26 on Apple silicon; none on tables), with sensitivity labels and in-memory history shown as a strip of small cards (search, pause, an ignore list, and pins that still die on lock, sleep and quit), and a window size you can choose. Offline by design: no network code and no network entitlement. |
| `crates/mac-ai` | library | Swift bridge to Apple Foundation Models for Copycraft's local AI actions (compiled by `build.rs` with `swiftc`, all `unsafe` of the feature lives here). Needs macOS 26 and Apple silicon; elsewhere the actions report as unavailable. Copycraft only; ticker does not use it. |
| [`apps/ticker`](apps/ticker/README.md) | app | Menu bar price ticker (Bitcoin, ETH, gold, TTF gas, NL fuel and power prices) with day change and price-watch notifications. |
| `crates/mac-ui` | library | Shared look & feel for the apps. Owns and re-exports the UI frameworks (tray-icon, winit, objc2*) and holds generic helpers. Always on: `tray` (info/version/quit rows, grey titles for enabled rows, template icons, VoiceOver label of the menu bar button, the normal/flash/alert icon set and the timing of a short icon blink), `icon` (RGBA canvas, image-file icons with a plain-square fallback, template masks for monochrome menu bar icons, SF Symbols), `keys` (named key codes), `corners` (concentric corner radii), `find` (case-insensitive matches, "3/12" counter, wrapping steps), `wake` (earliest event-loop deadline as a winit control flow), and on macOS `glass` (Liquid Glass background with a frosted fallback before macOS 26, glass groups with a plain-view fallback). Opt-in features: `theme` (System/Light/Dark appearance), `widgets` (AppKit label/field/box, SF Symbol and pill buttons, read-only text view/scroller and one that Tab skips, target/action and text-delegate wiring, ⌘-chord and Shift checks on key events in `keys`, text view sizing, find marks and wiping of text views, fields and field editors, `button`: Liquid Glass push buttons with a FlexiblePush fallback before macOS 26, `fonts`: monospace font with a fallback list, `text`: attributed strings with font/colour/background runs over byte ranges and byte → UTF-16 ranges), `panel` (borderless floating panel setup, activation and near-cursor placement), `dialog` (modal NSAlert message, confirm, button choice, text prompt and list pick), `file_panel` (open and save panels, a save panel with a format popup), `progress` (busy spinner), `notify` (user notifications via UserNotifications from an app bundle; without one `send` returns `false` and ticker's debug builds fall back on osascript themselves: mac-ui starts no subprocesses), `wake` (a callback when the Mac goes to sleep and wakes up, from NSWorkspace's notifications) and `appkit-full`. Both apps depend on it (both need macOS 26). |

## Layout

```
.
├── Cargo.toml              # workspace: members, shared [workspace.dependencies], all [profile.*]
├── Cargo.lock
├── rust-toolchain.toml     # pinned Rust version (local and CI)
├── AGENTS.md               # rules for coding agents
├── apps/
│   ├── copycraft/          # bin: Cargo.toml, src/, assets/, LICENSE (proprietary)
│   └── ticker/             # bin: Cargo.toml, src/, assets/, config.toml, LICENSE (proprietary)
├── crates/
│   ├── mac-ai/             # lib: Apple Foundation Models bridge (Swift via build.rs), copycraft only
│   └── mac-ui/             # lib: shared UI frameworks + menu bar helpers
├── scripts/
│   ├── build_app_icon.sh   # compile the app icon into a .app with actool (before signing)
│   ├── sign_app.sh         # sign a .app: ad-hoc, or Developer ID with MACOS_SIGN_IDENTITY
│   ├── import_signing_cert.sh # CI: Developer ID .p12 into a temporary keychain
│   ├── notarize.sh         # notarytool submit --wait + staple + Gatekeeper check (.app/.dmg)
│   ├── check_copycraft_offline.sh # CI: no network crates in copycraft's tree, no osascript
│   ├── check_minos.sh      # CI/release: every binary's Mach-O minos must be 26.0
│   ├── count_unsafe.sh     # CI: `unsafe` blocks per crate must not exceed unsafe_baseline.txt
│   ├── unsafe_baseline.txt # upper bounds (copycraft 106, mac-ui 78, mac-ai 6 blocks)
│   ├── check_commits.sh    # CI: Conventional Commit subjects
│   ├── check_privacy_manifest.sh # plutil -lint of the privacy manifests
│   └── tag_release.sh      # maintainer only: bump + tag + push a release for one app
└── .github/workflows/
    ├── ci.yml              # fmt+clippy (macos-26), tests on macos-26, app icon check; cargo audit weekly/manual
    ├── release-copycraft.yml
    └── release-ticker.yml
```

## Requirements

- macOS (both apps target macOS; the release DMGs are built for Apple silicon, `aarch64-apple-darwin`). Both apps need macOS 26 (Tahoe) or later on Apple silicon (copycraft's local AI actions also need Apple Intelligence). One floor everywhere: `minimum_system_version = "26.0"` in each app's `Cargo.toml` and `MACOSX_DEPLOYMENT_TARGET=26.0` in `.cargo/config.toml`, so the Mach-O `minos` is 26.0 (CI and the release workflows check it with `otool -l`)
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

Also in CI: `scripts/count_unsafe.sh --check` (the `unsafe` count may not grow past `scripts/unsafe_baseline.txt`; lower it when it shrinks) and `scripts/check_minos.sh` on the release binaries. Copycraft's PDF support uses the `objc2-pdf-kit` bindings (PDFKit); its Word (.docx) support inflates `word/document.xml` with `flate2` (already in the tree) and draws the first page with Quick Look through `objc2-quick-look-thumbnailing` (all in memory; mac-ui `quicklook`); its AI actions use the `mac-ai` crate.

Per crate: `cargo check -p <crate>`, `cargo test -p <crate> <filter>`, `cargo clippy -p <crate> --all-targets -- -D warnings`.

CI (`.github/workflows/ci.yml`) runs for pushes and PRs to `main`: fmt and clippy once on `macos-26`, the tests on `macos-26` (Xcode 26.6) (no macOS 15 runner any more: Copycraft needs macOS 26 and links FoundationModels), and the app icon check. Changes that only touch Markdown, `docs/` or `LICENSE` files skip CI (`main` has no required checks, so nothing waits on a skipped run). `cargo audit` runs weekly and on manual dispatch.

CI and the release workflows are pinned: runner image `macos-26` (`ubuntu-24.04` for the audit), Xcode via `sudo xcode-select -s /Applications/Xcode_<XCODE_VERSION>.app` (`XCODE_VERSION` at the top of each workflow), Rust via `rust-toolchain.toml` (`rustup toolchain install`), and actions on major versions (`actions/checkout@v7`, `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`). Bump these on purpose, one at a time; the Xcode versions an image has are listed in its README in [actions/runner-images](https://github.com/actions/runner-images).

## Support

This is a private repository. Open contributions and public GitHub issues are not accepted.

- **Support:** email **mac-apps-support@proton.me** (see also [`apps/copycraft/SUPPORT.md`](apps/copycraft/SUPPORT.md)).
- **Security:** see [`SECURITY.md`](SECURITY.md) for supported versions, scanning and how to report a vulnerability.
- **Offline by design:** Copycraft has no network access.
- **Releases:** only the maintainer makes releases.

Today the apps are ad-hoc signed, not notarized, so Gatekeeper may block the first launch.

## License

Proprietary. Copyright (c) 2026 Marcel Koopman. All rights reserved.

See [`LICENSE`](LICENSE). The same notice is in each app and in `crates/mac-ui`.
