# AGENTS.md

Geldt voor de hele monorepo. Een `AGENTS.md` in een submap mag regels aanvullen voor die map.

## Structuur

- `apps/copycraft`, `apps/ticker`: macOS menubar-apps (bins).
- `crates/mac-ui`: gedeelde look & feel en UI-frameworks. App-specifieke logica en assets blijven in de app.
- Gedeelde dependency-versies staan in `[workspace.dependencies]` in de root-`Cargo.toml`; apps gebruiken `{ workspace = true }`.
- Profielen (`[profile.*]`) alleen in de root-`Cargo.toml`.

## Stack

- Rust Cargo workspace, edition 2024 via `[workspace.package]`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.
- Geen `unwrap`/`expect` in library-code tenzij invariant + comment waarom.
- Target: macOS.

## Commands

Kleinste opdracht die de change dekt:

- Check: `cargo check -p <crate>`
- Test: `cargo test -p <crate> <filter>`
- Lint: `cargo clippy -p <crate> --all-targets -- -D warnings`
- Format: `cargo fmt --all`
  Workspace-breed (`--workspace`) alleen als meerdere crates wijzigen of `crates/mac-ui` wijzigt.

## Regels

- Focus puur op code-wijzigingen. Geen drive-by refactors.
- Volg bestaande module-indeling en naming.
- Geen nieuwe crate of dependency zonder te vragen.
- Wijzig je `crates/mac-ui`, check dan beide apps (`cargo check --workspace`).
- Klaar = code is geschreven, compileert lokaal, relevante tests zijn groen en clippy is clean.
- Geen generated files of `Cargo.lock` aanraken tenzij deps écht wijzigen.
- Copycraft is offline by design (gevoelige klembordgegevens): geen netwerkcode, geen netwerk-dependencies (ook niet via mac-ui-features) en geen `com.apple.security.network.*`-entitlement. Netwerk hoort alleen in ticker. Entitlements van copycraft: alleen `app-sandbox` en `files.user-selected.read-write` (geen `com.apple.security.cs.*`, geen Apple Events); dialogen en panels native in-process, nooit `osascript`. Geen subprocessen met klembordgegevens (geen `std::process::Command` met copied content, ook geen formatters zoals `rustfmt`): alles in-process. CI bewaakt dit: `apps/copycraft/clippy.toml` verbiedt `std::process::Command`, `std::net` en `std::os::unix::net` (clippy leest die config alleen voor copycraft), en `scripts/check_copycraft_offline.sh` faalt op netwerk-crates in de dependency tree van copycraft (alleen `socket2` via polars → tokio is toegestaan, zie het script) en op `osascript` in de bronnen.

## Git & Workflow (Strikte Restricties)

Deze regels gelden voor **Grok Build** (lokaal, om tokens te besparen). Bots zoals Edsger mogen wél git gebruiken: committen en pushen naar `main`, maar alleen als Marcel daar expliciet om vraagt en nadat fmt/clippy/test schoon zijn.

- **GEEN Git-commando's (Grok Build):** Voer NOOIT git-commando's uit (geen `git checkout`, `git add`, `git commit`, `git push`, etc.).
- **Zelfwerkzaamheid:** Alle branch-afhandeling en commits worden handmatig door de gebruiker gedaan in de macOS terminal buiten Grok Build om.
- **Geen wijzigingscontroles:** Vraag niet naar de git-status en controleer niet of de werkmap schoon is. Ga er altijd van uit dat de huidige code in de workspace de juiste basis is om op te bouwen.
- **Stoppen na code-oplevering:** Zodra de code correct is aangepast en de lokale cargo-checks (check/test/clippy) succesvol zijn uitgevoerd, rapporteer je dat de taak klaar is. Doe geen suggesties voor commits of PR-teksten.
