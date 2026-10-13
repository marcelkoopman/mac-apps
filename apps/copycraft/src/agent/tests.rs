use super::*;
use std::path::Path;

const AGENTS: &str = "# AGENTS.md

## Setup

```bash
# install the toolchain
rustup toolchain install stable
cargo build
```

## Build

- Build with `cargo build --release` first.
- Lint: `cargo clippy -- -D warnings` and `npm run lint`.
- Not a command: `README.md`.

## Test

```console
$ cargo test
running 3 tests
$ make check
```

## Code style

Use rustfmt.

## Code style

Twice.

## PR instructions

";

const README: &str = "# Copycraft

![build](https://img.shields.io/badge/build-passing-green)

A clipboard tool.

## Features

- Formats JSON.

## Installation

```bash
brew install copycraft
```

## Usage

Press the hotkey.

## Build

```bash
cargo build
```

## Test

```bash
cargo test
```

## License

MIT
";

#[test]
fn file_names_and_cursor_rules_are_agent_files() {
    for name in [
        "AGENTS.md",
        "/x/CLAUDE.md",
        "a/.cursorrules",
        "repo/.github/copilot-instructions.md",
        "GEMINI.md",
        "repo/.cursor/rules/style.mdc",
    ] {
        assert!(is_agent_path(Path::new(name)), "{name}");
    }
    for name in [
        "README.md",
        "notes.mdc",
        "repo/rules/style.mdc",
        "agents.txt",
    ] {
        assert!(!is_agent_path(Path::new(name)), "{name}");
    }
    let text = "Always use tabs.\n";
    crate::format::hint_by_path(Path::new("p/.cursor/rules/tabs.mdc"), text);
    assert_eq!(
        crate::format::detect(text),
        crate::format::FormatKind::Agent
    );
}

#[test]
fn pasted_instructions_are_recognised_and_a_readme_is_not() {
    assert!(looks_like_agent(AGENTS));
    assert_eq!(
        crate::format::detect(AGENTS),
        crate::format::FormatKind::Agent
    );
    assert!(!looks_like_agent(README));
    assert_eq!(
        crate::format::detect(README),
        crate::format::FormatKind::Markdown
    );
    // Plain Markdown notes, and headings without a shell block.
    let notes = "# Trip\n\n## Day 1\n\nWalk.\n\n## Day 2\n\nSwim.\n";
    assert!(!looks_like_agent(notes));
    let no_shell = "# Rules\n\n## Build\n\nUse cargo.\n\n## Test\n\nRun tests.\n\n## Conventions\n\nBe nice.\n";
    assert!(!looks_like_agent(no_shell));
}

#[test]
fn commands_come_from_shell_blocks_and_list_items_in_order_once() {
    assert_eq!(
        commands(AGENTS),
        vec![
            "rustup toolchain install stable",
            "cargo build",
            "cargo build --release",
            "cargo clippy -- -D warnings",
            "npm run lint",
            "cargo test",
            "make check",
        ]
    );
    let other = "```python\nprint(1)\n```\n- `go test ./...`\n- `go test ./...`\n";
    assert_eq!(commands(other), vec!["go test ./..."]);
    assert!(commands("").is_empty());
}

#[test]
fn the_outline_is_every_heading_with_its_line() {
    let heads = headings(AGENTS);
    let titles: Vec<(usize, &str, usize)> = heads
        .iter()
        .map(|h| (h.level, h.text.as_str(), h.line))
        .collect();
    assert_eq!(titles[0], (1, "AGENTS.md", 1));
    assert_eq!(titles[1], (2, "Setup", 3));
    assert_eq!(titles.len(), 7);
    // A `#` comment inside a code block is no heading.
    assert!(!titles.iter().any(|(_, t, _)| t.contains("install")));
}

#[test]
fn check_finds_empty_and_duplicate_sections() {
    let warnings = check(AGENTS, None);
    assert!(
        warnings.contains(&Warning::DuplicateHeading {
            line: 29,
            first: 25
        }),
        "{warnings:?}"
    );
    assert!(
        warnings.contains(&Warning::EmptySection { line: 33 }),
        "{warnings:?}"
    );
    assert!(!warnings.contains(&Warning::MissingBuild));
    assert!(!warnings.contains(&Warning::MissingTest));
}

#[test]
fn check_finds_missing_sections_and_length() {
    let text = "# Rules\n\nBe brief.\n";
    let warnings = check(text, None);
    assert!(warnings.contains(&Warning::MissingBuild));
    assert!(warnings.contains(&Warning::MissingTest));
    let long = "# Build\nx\n# Test\ny\n".to_string() + &"line\n".repeat(600);
    assert!(
        check(&long, None)
            .iter()
            .any(|w| matches!(w, Warning::TooLong { lines: 604, .. }))
    );
    let wordy = "# Build\nx\n# Test\n".to_string() + &"a".repeat(40_000);
    assert!(
        check(&wordy, None)
            .iter()
            .any(|w| matches!(w, Warning::TooLong { .. }))
    );
    assert_eq!(token_estimate("abcdefgh"), 2);
    assert_eq!(token_estimate("abcde"), 2);
}

#[test]
fn relative_links_are_checked_only_in_a_known_folder() {
    let dir = std::env::temp_dir().join(format!("copycraft-agent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("present.md"), "x").unwrap();
    let text = "# Build\n\nSee [ok](present.md) and [web](https://x.y/z).\n\n# Test\n\nSee [gone](missing.md#part) and [up](../outside.md).\n";
    let warnings = check(text, Some(&dir));
    let broken: Vec<&Warning> = warnings
        .iter()
        .filter(|w| matches!(w, Warning::BrokenLink { .. }))
        .collect();
    assert_eq!(broken, vec![&Warning::BrokenLink { line: 7 }]);
    // Without a folder (pasted, or no access) links are skipped.
    assert!(
        !check(text, None)
            .iter()
            .any(|w| matches!(w, Warning::BrokenLink { .. }))
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secrets_are_reported_by_line_and_kind_never_by_value() {
    let secret = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
    let text = format!("# Build\n\nx\n\n# Test\n\nexport GITHUB_TOKEN={secret}\n");
    let warnings = check(&text, None);
    let found: Vec<&Warning> = warnings
        .iter()
        .filter(|w| matches!(w, Warning::Secret { .. }))
        .collect();
    assert!(!found.is_empty(), "{warnings:?}");
    assert!(found.iter().all(|w| w.line() == Some(7)));
    for w in &warnings {
        assert!(
            !w.title().contains(secret) && !w.title().contains("ghp_"),
            "{}",
            w.title()
        );
    }
    let rows = crate::commands::agent_check_rows(&warnings);
    assert!(
        rows.iter()
            .all(|(cmd, _)| !cmd.title.contains("ghp_") && !cmd.detail.contains("ghp_"))
    );
    // The log events are counts only.
    let event = crate::diag::LogEvent::AgentChecked {
        warnings: warnings.len(),
        secrets: found.len(),
    };
    assert!(!event.line().contains("ghp_"));
    let event = crate::diag::LogEvent::AgentCommands { count: 1 };
    assert!(!event.line().contains("ghp_"));
}

#[test]
fn lines_menu_offers_the_agent_steps_only_for_agent_instructions() {
    use crate::commands::{CommandId, TextShown, text_menu_items};
    let mut edit = TextShown {
        labels: vec!["Original".into()],
        version: 0,
        note: None,
        code: false,
        agent: false,
    };
    let ids = |edit: &TextShown| -> Vec<CommandId> {
        text_menu_items(edit)
            .into_iter()
            .map(|(c, _)| c.id)
            .collect()
    };
    assert!(!ids(&edit).contains(&CommandId::AgentCommands));
    edit.agent = true;
    let all = ids(&edit);
    for id in [
        CommandId::AgentCommands,
        CommandId::AgentOutline,
        CommandId::AgentCheck,
    ] {
        assert!(all.contains(&id), "{id:?}");
    }
}

#[test]
fn the_commands_step_and_the_footer() {
    let step = crate::text_ops::TextOp::AgentCommands;
    let out = step.apply(AGENTS).unwrap();
    assert!(out.starts_with("rustup toolchain install stable\ncargo build\n"));
    assert_eq!(step.label(), crate::locale::t("agent_commands"));
    let meta = crate::commands::text_meta(AGENTS);
    assert!(
        meta.contains(&format!("≈ {} tokens", token_estimate(AGENTS))),
        "{meta}"
    );
}
