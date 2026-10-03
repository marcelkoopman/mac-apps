# mac-apps

Monorepo for small macOS menu bar apps written in Rust, organised as one Cargo workspace.

| Path | Kind | Description |
| --- | --- | --- |
| [`apps/copycraft`](apps/copycraft/README.md) | app | Clipboard transformer. A global hotkey opens a card at the cursor for whatever you copied: format, convert, decode, table view, schema, OCR/QR, with sensitivity labels and in-memory history. Offline by design: no network code and no network entitlement. |
| [`apps/ticker`](apps/ticker/README.md) | app | Menu bar price ticker (Bitcoin, ETH, gold, TTF gas, NL fuel and power prices) with day change and price-watch notifications. |
| `crates/mac-ui` | library | Shared look & feel for the apps. Owns and re-exports the UI frameworks (tray-icon, winit, objc2*) and holds generic helpers. Always on: `tray` (info/version/quit rows, template icons, VoiceOver label of the menu bar button, the normal/flash/alert icon set and the timing of a short icon blink), `icon` (RGBA canvas, image-file icons with a plain-square fallback, template masks for monochrome menu bar icons, SF Symbols), `keys` (named key codes), `corners` (concentric corner radii), `find` (case-insensitive matches, "3/12" counter, wrapping steps), `wake` (earliest event-loop deadline as a winit control flow), and on macOS `glass` (Liquid Glass background with a frosted fallback before macOS 26, glass groups with a plain-view fallback). Opt-in features: `theme` (System/Light/Dark appearance), `widgets` (AppKit label/field/box, SF Symbol and pill buttons, read-only text view/scroller and one that Tab skips, target/action and text-delegate wiring, ⌘-chord and Shift checks on key events in `keys`, text view sizing, find marks and wiping of text views, fields and field editors, `button`: Liquid Glass push buttons with a FlexiblePush fallback before macOS 26, `fonts`: monospace font with a fallback list, `text`: attributed strings with font/colour/background runs over byte ranges and byte → UTF-16 ranges), `panel` (borderless floating panel setup, activation and near-cursor placement), `dialog` (modal NSAlert message, confirm, button choice, text prompt and list pick), `file_panel` (open and save panels), `progress` (busy spinner), `link` (link previews with LinkPresentation: an `http(s)`-only metadata fetch with a timeout and cancel, and a rich link view labelled for VoiceOver; adds `objc2-link-presentation`), `notify` (user notifications via UserNotifications, osascript without an app bundle) and `appkit-full`. Both apps depend on it. |

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
│   ├── sign_app.sh         # sign a .app: ad-hoc, or Developer ID with MACOS_SIGN_IDENTITY
│   ├── import_signing_cert.sh # CI: Developer ID .p12 into a temporary keychain
│   ├── notarize.sh         # notarytool submit --wait + staple + Gatekeeper check (.app/.dmg)
│   └── tag_release.sh      # bump + tag + push a release for one app
└── .github/workflows/
    ├── ci.yml              # fmt+clippy (macos-26), tests on macos-26 and macos-15, app icon check; cargo audit weekly/manual
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

CI (`.github/workflows/ci.yml`) runs for pushes and PRs to `main`: fmt and clippy once on `macos-26`, the tests on `macos-26` (Xcode 26.6) and on `macos-15` (Xcode 16.4), so the pre-26 fallbacks (FlexiblePush buttons, frosted background, plain view instead of the glass group) are tested on a real older macOS, and the app icon check. Changes that only touch Markdown, `docs/` or `LICENSE` files skip CI (`main` has no required checks, so nothing waits on a skipped run). `cargo audit` runs weekly and on manual dispatch.

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
4. sign the app: ad-hoc (`codesign --sign -`, also `scripts/sign_app.sh`), or with Developer ID when the signing secrets exist (see below), then notarize and staple it
5. build a DMG with `create-dmg` (with Developer ID: sign, notarize and staple the DMG as well)
6. publish `<app>-X.Y.Z.dmg` with `gh release create` (auto-generated notes)

Downloads: [Releases](https://github.com/marcelkoopman/mac-apps/releases).

Today the apps are ad-hoc signed, not notarized, so Gatekeeper may block the first launch.

### Developer ID-signing en notarisatie (voorbereid, staat uit)

De release-workflows kunnen de app en de DMG met een Developer ID-certificaat signen, laten notariseren door Apple en het ticket stapelen. Dat gebeurt alleen als de secrets hieronder bestaan; zonder die secrets blijft alles zoals het is (ad-hoc signing, geen notarisatie). Een stap `Detect signing secrets` kijkt alleen óf ze er zijn en logt `Signing: ad-hoc|developer-id, notarize: true|false`.

Wat er met de secrets gebeurt (`release-<app>.yml`):

1. `scripts/import_signing_cert.sh` zet de `.p12` in een tijdelijke keychain (willekeurig wachtwoord, `set-key-partition-list` voor codesign, Apple's Developer ID G2-CA erbij als die ontbreekt). Aan het eind van de job wordt die keychain verwijderd.
2. `scripts/sign_app.sh <app> <App.app>` met `MACOS_SIGN_IDENTITY`: `codesign --options runtime --timestamp --entitlements apps/<app>/assets/entitlements.plist` (hardened runtime, App Sandbox, secure timestamp). Het script controleert de `Developer ID Application`-authority, de timestamp, de hardened runtime en `com.apple.security.app-sandbox`.
3. `scripts/notarize.sh <App.app>`: zip met `ditto`, `xcrun notarytool submit --wait`, bij een andere status dan `Accepted` het `notarytool log` en een rode build, daarna `xcrun stapler staple` + `stapler validate` en `spctl --assess`.
4. DMG bouwen met de gestapelde app, de DMG signen (`codesign --timestamp`), `scripts/notarize.sh <dmg>` en stapelen.

Alleen het certificaat (zonder notarisatie-secrets) geeft een Developer ID-gesigneerde maar niet-genotariseerde release, met een waarschuwing in de log. Alleen notarisatie-secrets (zonder certificaat) doen niets: een ad-hoc-signature kan niet genotariseerd worden.

#### Secrets (Settings → Secrets and variables → Actions → Repository secrets)

Signing, alle drie nodig:

| Secret | Inhoud |
| --- | --- |
| `MACOS_CERT_P12` | Het Developer ID Application-certificaat mét private key als `.p12`, base64: `base64 -i DeveloperID.p12 \| pbcopy` |
| `MACOS_CERT_PASSWORD` | Het wachtwoord dat je bij het exporteren van de `.p12` koos |
| `MACOS_SIGN_IDENTITY` | De naam van de identiteit, precies zoals `security find-identity -v -p codesigning` hem toont, bijvoorbeeld `Developer ID Application: Marcel Koopman (ABCDE12345)` |

Notarisatie, één van de twee sets (de API-key wint als beide er zijn):

| Secret | Inhoud |
| --- | --- |
| `APPLE_API_KEY_P8` | App Store Connect API-key: de inhoud van `AuthKey_<KEYID>.p8` (de tekst, of die tekst base64) |
| `APPLE_API_KEY_ID` | De Key ID van die key (10 tekens) |
| `APPLE_API_ISSUER_ID` | De Issuer ID (UUID) bovenaan de lijst met keys |
| *of* `APPLE_ID` | Het e-mailadres van je Apple-account |
| `APPLE_TEAM_ID` | Je Team ID (10 tekens, developer.apple.com → Account → Membership details) |
| `APPLE_APP_PASSWORD` | Een app-specifiek wachtwoord voor dat account |

#### Zo maak je ze

1. **Certificaat** (betaald Apple Developer Program nodig). Op developer.apple.com → Certificates → `+` → *Developer ID Application* (profiel G2 Sub-CA), met een CSR uit Sleutelhangertoegang (*Certificaatassistent → Vraag certificaat aan bij certificaatautoriteit*, opslaan op schijf). Download het `.cer` en dubbelklik het, zodat het bij je private key in de login-keychain komt.
2. **`.p12` exporteren**: in Sleutelhangertoegang onder *Mijn certificaten* het certificaat *Developer ID Application: …* (met de sleutel eronder) selecteren → *Exporteer* → `.p12`, met een sterk wachtwoord. Dan `base64 -i DeveloperID.p12 | pbcopy` en plakken als `MACOS_CERT_P12`; het wachtwoord wordt `MACOS_CERT_PASSWORD`. Gooi de `.p12` daarna weg of berg hem veilig op.
3. **Identiteit**: `security find-identity -v -p codesigning` en de tekst tussen de aanhalingstekens kopiëren naar `MACOS_SIGN_IDENTITY`.
4. **API-key** (aanbevolen): App Store Connect → Gebruikers en toegang → Integraties → *App Store Connect API* → Team Keys → `+`, rol *Developer*. Download `AuthKey_<KEYID>.p8` (kan maar één keer). Zet de inhoud (`pbcopy < AuthKey_<KEYID>.p8`) in `APPLE_API_KEY_P8`, de Key ID in `APPLE_API_KEY_ID` en de Issuer ID in `APPLE_API_ISSUER_ID`.
5. **Of Apple ID**: op account.apple.com → Inloggen en beveiliging → *App-specifieke wachtwoorden* een wachtwoord maken (`APPLE_APP_PASSWORD`), plus `APPLE_ID` en `APPLE_TEAM_ID`.

Met de GitHub CLI kan het ook: `gh secret set MACOS_CERT_P12 < <(base64 -i DeveloperID.p12)`, `gh secret set APPLE_API_KEY_P8 < AuthKey_<KEYID>.p8`, enzovoort.

Lokaal werkt hetzelfde zonder tijdelijke keychain (het certificaat staat in je login-keychain):

```bash
MACOS_SIGN_IDENTITY="Developer ID Application: … (TEAMID)" scripts/sign_app.sh copycraft
APPLE_API_KEY_P8="$(cat AuthKey_XXXX.p8)" APPLE_API_KEY_ID=XXXX APPLE_API_ISSUER_ID=… \
  scripts/notarize.sh target/aarch64-apple-darwin/release/bundle/osx/Copycraft.app
```

Pas na de eerste release-tag mét secrets is dit echt getest: CI op `main` draait de release-workflows niet.

## License

Each app carries its own license:

- copycraft: Apache-2.0 ([`apps/copycraft/LICENSE`](apps/copycraft/LICENSE))
- ticker: MIT ([`apps/ticker/LICENSE`](apps/ticker/LICENSE))

The repo root and `crates/mac-ui` have no license file.
