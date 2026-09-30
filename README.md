# mac-apps

Monorepo for small macOS menu bar apps written in Rust, organised as one Cargo workspace.

| Path | Kind | Description |
| --- | --- | --- |
| [`apps/copycraft`](apps/copycraft/README.md) | app | Clipboard transformer. A global hotkey opens a card at the cursor for whatever you copied: format, convert, decode, table view, schema, OCR/QR, with sensitivity labels and in-memory history. |
| [`apps/ticker`](apps/ticker/README.md) | app | Menu bar price ticker (Bitcoin, ETH, gold, TTF gas, NL fuel and power prices) with day change and price-watch notifications. |
| `crates/mac-ui` | library | Shared look & feel for the apps. Owns and re-exports the UI frameworks (tray-icon, winit, objc2*) and holds generic menu bar helpers (`tray`: info/version/quit rows; `icon`: RGBA canvas, image-file icons, SF Symbols; `glass`: Liquid Glass window background with a frosted fallback before macOS 26; opt-in features `theme`: System/Light/Dark appearance, `widgets`: AppKit label/field/button/box constructors). Both apps depend on it. |

## Layout

```
.
├── Cargo.toml              # workspace: members, shared [workspace.dependencies], all [profile.*]
├── Cargo.lock
├── AGENTS.md               # rules for coding agents
├── apps/
│   ├── copycraft/          # bin: Cargo.toml, src/, assets/, LICENSE (Apache-2.0)
│   └── ticker/             # bin: Cargo.toml, src/, assets/, config.toml, LICENSE (MIT)
├── crates/
│   └── mac-ui/             # lib: shared UI frameworks + menu bar helpers
├── scripts/
│   └── tag_release.sh      # bump + tag + push a release for one app
└── .github/workflows/
    ├── ci.yml              # fmt, clippy, test (macos-latest); cargo audit weekly/manual
    ├── release-copycraft.yml
    └── release-ticker.yml
```

## Requirements

- macOS (both apps target macOS; the release DMGs are built for Apple silicon, `aarch64-apple-darwin`)
- Rust stable (edition 2024)
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

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests on `macos-latest` for pushes and PRs to `main`. `cargo audit` runs weekly and on manual dispatch.

## Release

```bash
scripts/tag_release.sh copycraft   # or: ticker
```

The script needs a clean working tree. It switches to `main` (override with `BRANCH=...`), pulls, and bumps the patch version in `apps/<app>/Cargo.toml` (both the package and bundle `version`). After you confirm, it commits `<app>: bump version to X.Y.Z` with `Cargo.lock`, tags `<app>-vX.Y.Z`, and pushes the branch and tag. It uses BSD `sed -i ''`, so run it on macOS.

The tag triggers `release-<app>.yml`:

1. check that the tag matches the version in `Cargo.toml`
2. `cargo bundle --release --target aarch64-apple-darwin`
3. ad-hoc sign the app (`codesign --sign -`)
4. build a DMG with `create-dmg`
5. publish `<app>-X.Y.Z.dmg` with `gh release create` (auto-generated notes)

Downloads: [Releases](https://github.com/marcelkoopman/mac-apps/releases).

The apps are ad-hoc signed, not notarized, so Gatekeeper may block the first launch.

## License

Each app carries its own license:

- copycraft: Apache-2.0 ([`apps/copycraft/LICENSE`](apps/copycraft/LICENSE))
- ticker: MIT ([`apps/ticker/LICENSE`](apps/ticker/LICENSE))

The repo root and `crates/mac-ui` have no license file.
