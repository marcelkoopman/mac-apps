# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.
- Geen `unwrap`/`expect` in library-code tenzij invariant + comment waarom.
- Target: macOS.

## Commands

Kleinste opdracht die de change dekt:

- Check: `cargo check -p <crate>` (anders `cargo check`)
- Test: `cargo test -p <crate> <filter>` (anders `cargo test`)
- Lint: `cargo clippy -p <crate> --all-targets -- -D warnings`
- Format: `cargo fmt`
  Workspace-breed alleen als meerdere crates wijzigen.

## Regels

- Focus puur op code-wijzigingen. Geen drive-by refactors.
- Volg bestaande module-indeling en naming.
- Geen nieuwe crate of dependency zonder te vragen.
- Klaar = code is geschreven, compileert lokaal, relevante tests zijn groen en clippy is clean.
- Geen generated files of `Cargo.lock` aanraken tenzij deps écht wijzigen.

## Git & Workflow (Strikte Restricties)

- **GEEN Git-commando's:** Voer NOOIT git-commando's uit (geen `git checkout`, `git add`, `git commit`, `git push`, etc.).
- **Zelfwerkzaamheid:** Alle branch-afhandeling en commits worden handmatig door de gebruiker gedaan in de macOS terminal buiten Grok Build om.
- **Geen wijzigingscontroles:** Vraag niet naar de git-status en controleer niet of de werkmap schoon is. Ga er altijd van uit dat de huidige code in de workspace de juiste basis is om op te bouwen.
- **Stoppen na code-oplevering:** Zodra de code correct is aangepast en de lokale cargo-checks (check/test/clippy) succesvol zijn uitgevoerd, rapporteer je dat de taak klaar is. Doe geen suggesties voor commits of PR-teksten.


