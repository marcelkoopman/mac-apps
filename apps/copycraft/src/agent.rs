//! Agent instruction files (AGENTS.md, CLAUDE.md, .cursorrules, .cursor/rules/*.mdc,
//! copilot-instructions.md, GEMINI.md): recognising them, and the Lines… steps Commands,
//! Outline and Check. Only counts are logged, never the text.

use std::path::{Path, PathBuf};

/// File names that are agent instructions whatever they say.
const NAMES: &[&str] = &[
    "agents.md",
    "claude.md",
    ".cursorrules",
    "copilot-instructions.md",
    "gemini.md",
];

/// `path` names an agent instruction file: one of [`NAMES`] or a `.cursor/rules/*.mdc` rule.
pub fn is_agent_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if NAMES.contains(&name.as_str()) {
        return true;
    }
    let mut parents = path
        .ancestors()
        .skip(1)
        .filter_map(|p| p.file_name()?.to_str());
    name.ends_with(".mdc") && parents.next() == Some("rules") && parents.next() == Some(".cursor")
}

/// Headings typical of agent instructions.
const AGENT_HEADINGS: &[&str] = &[
    "build",
    "test",
    "setup",
    "code style",
    "coding style",
    "style",
    "conventions",
    "commands",
    "pr instructions",
    "pull request",
    "commit",
    "lint",
    "dev environment",
    "development environment",
    "project structure",
    "architecture",
];
/// Headings typical of a README: each one makes it less likely to be instructions.
const README_HEADINGS: &[&str] = &[
    "installation",
    "install",
    "usage",
    "license",
    "features",
    "contributing",
    "screenshots",
    "faq",
    "acknowledgements",
    "getting started",
    "quick start",
    "quickstart",
];
const SHELL_FENCES: &[&str] = &["bash", "sh", "shell", "console", "zsh"];

/// One Markdown heading: level, text and 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

/// `#`–`######` headings outside code fences.
pub fn headings(text: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || line.len() - trimmed.len() > 3 {
            continue;
        }
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) {
            let rest = &trimmed[level..];
            if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                let text = rest.trim().trim_end_matches('#').trim().to_string();
                out.push(Heading {
                    level,
                    text,
                    line: i + 1,
                });
            }
        }
    }
    out
}

fn heading_matches(heading: &str, words: &[&str]) -> bool {
    let h = heading.to_lowercase();
    let h = h.trim_matches(|c: char| !c.is_alphanumeric());
    words.iter().any(|w| {
        h == *w
            || h.starts_with(&format!("{w} "))
            || h.starts_with(&format!("{w}s"))
            || h.ends_with(&format!(" {w}"))
            || h.ends_with(&format!(" {w}s"))
            || h.contains(&format!(" {w} "))
    })
}

/// Fenced code blocks: (language, first line of code (1-based), code).
fn fences(text: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            match open.take() {
                Some(block) => out.push(block),
                None => {
                    let lang = trimmed[3..]
                        .trim()
                        .split(|c: char| c.is_whitespace() || c == '{' || c == ',')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    open = Some((lang, i + 2, String::new()));
                }
            }
            continue;
        }
        if let Some((_, _, code)) = open.as_mut() {
            code.push_str(line);
            code.push('\n');
        }
    }
    out
}

/// Pasted text that reads as agent instructions: Markdown with several typical headings
/// (Build, Test, Setup, Code style, Conventions, Commands, PR instructions) and a shell
/// block, and not README-like. Conservative: a README stays Markdown.
pub fn looks_like_agent(text: &str) -> bool {
    let heads = headings(text);
    if heads.len() < 3 {
        return false;
    }
    let mut score: i32 = 0;
    for h in &heads {
        if heading_matches(&h.text, AGENT_HEADINGS) {
            score += 1;
        }
        if heading_matches(&h.text, README_HEADINGS) {
            score -= 2;
        }
    }
    if let Some(first) = heads.first() {
        let t = first.text.to_lowercase();
        if [
            "agents.md",
            "claude.md",
            "gemini.md",
            "agent",
            "copilot",
            "instructions for",
        ]
        .iter()
        .any(|w| t.contains(w))
        {
            score += 2;
        }
    }
    let shell = fences(text)
        .iter()
        .any(|(lang, _, _)| SHELL_FENCES.contains(&lang.as_str()));
    // A README badge line or an image is a README.
    let badges = text.contains("![") && text.contains("](https://img.shields.io");
    shell && !badges && score >= 3
}

/// Programs an inline command in a list may start with.
const PROGRAMS: &[&str] = &[
    "cargo",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "make",
    "pytest",
    "go",
    "git",
    "gradle",
    "./gradlew",
    "mvn",
    "./mvnw",
];

fn starts_with_program(command: &str) -> bool {
    let first = command.split_whitespace().next().unwrap_or("");
    PROGRAMS.contains(&first)
}

/// Commands: the lines of shell/bash/sh/console blocks (a `$ ` prompt dropped, comments and
/// console output left out) and inline `code` in list items that starts with a known program,
/// in order, each once.
pub fn commands(text: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for (lang, first_line, code) in fences(text) {
        if !SHELL_FENCES.contains(&lang.as_str()) {
            continue;
        }
        let console = lang == "console";
        for (i, line) in code.lines().enumerate() {
            let line = line.trim();
            let command = match line.strip_prefix("$ ").or_else(|| line.strip_prefix("% ")) {
                Some(command) => command.trim(),
                // In a console block, a line without a prompt is output.
                None if console => continue,
                None => line,
            };
            if command.is_empty() || command.starts_with('#') {
                continue;
            }
            found.push((first_line + i, command.to_string()));
        }
    }
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
            .or_else(|| {
                let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
                (digits > 0)
                    .then(|| trimmed[digits..].strip_prefix(". "))
                    .flatten()
            });
        let Some(item) = item else {
            continue;
        };
        let mut rest = item;
        while let Some(start) = rest.find('`') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('`') else {
                break;
            };
            let code = after[..end].trim();
            if starts_with_program(code) {
                found.push((i + 1, code.to_string()));
            }
            rest = &after[end + 1..];
        }
    }
    found.sort_by_key(|(line, _)| *line);
    let mut out: Vec<String> = Vec::new();
    for (_, command) in found {
        if !out.contains(&command) {
            out.push(command);
        }
    }
    out
}

/// Rough token count: characters / 4.
pub fn token_estimate(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// What Check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A possible secret or personal data on this line, of this kind. Never the value.
    Secret {
        line: usize,
        label: crate::sensitivity::Label,
    },
    TooLong {
        lines: usize,
        tokens: usize,
    },
    EmptySection {
        line: usize,
    },
    DuplicateHeading {
        line: usize,
        first: usize,
    },
    BrokenLink {
        line: usize,
    },
    MissingBuild,
    MissingTest,
}

impl Warning {
    /// The line it is about, to scroll to.
    pub fn line(&self) -> Option<usize> {
        match self {
            Warning::Secret { line, .. }
            | Warning::EmptySection { line }
            | Warning::DuplicateHeading { line, .. }
            | Warning::BrokenLink { line } => Some(*line),
            _ => None,
        }
    }

    /// The row in the Check menu. Holds line numbers and kinds; a heading's own text is
    /// shown (it is on the card anyway), a secret never.
    pub fn title(&self) -> String {
        use crate::locale::{t, tf};
        match self {
            Warning::Secret { line, label } => tf("agent_secret", &[line, &label.name()]),
            Warning::TooLong { lines, tokens } => tf("agent_too_long", &[lines, tokens]),
            Warning::EmptySection { line } => tf("agent_empty_section", &[line]),
            Warning::DuplicateHeading { line, first } => {
                tf("agent_duplicate_heading", &[line, first])
            }
            Warning::BrokenLink { line } => tf("agent_broken_link", &[line]),
            Warning::MissingBuild => t("agent_missing_build").to_string(),
            Warning::MissingTest => t("agent_missing_test").to_string(),
        }
    }
}

pub const MAX_LINES: usize = 500;
pub const MAX_TOKENS: usize = 8000;

/// Check `text`. `folder` is the opened file's folder, when relative links may be checked
/// there; without it they are skipped.
pub fn check(text: &str, folder: Option<&Path>) -> Vec<Warning> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        for label in crate::sensitivity::scan_labels(line) {
            out.push(Warning::Secret { line: i + 1, label });
        }
        if !fenced && let Some(folder) = folder {
            for target in relative_links(line) {
                if link_is_broken(folder, &target) {
                    out.push(Warning::BrokenLink { line: i + 1 });
                    break;
                }
            }
        }
    }
    let lines = text.lines().count();
    let tokens = token_estimate(text);
    if lines > MAX_LINES || tokens > MAX_TOKENS {
        out.push(Warning::TooLong { lines, tokens });
    }
    let heads = headings(text);
    let all_lines: Vec<&str> = text.lines().collect();
    for (k, h) in heads.iter().enumerate() {
        // Up to the next heading of the same or a higher level: nothing but blank lines and
        // deeper headings without text of their own is an empty section.
        let next = heads[k + 1..].iter().find(|n| n.level <= h.level);
        let end = next.map_or(all_lines.len(), |n| n.line - 1);
        let body = all_lines[h.line..end]
            .iter()
            .any(|l| !l.trim().is_empty() && !headings(l).iter().any(|_| true));
        if !body {
            out.push(Warning::EmptySection { line: h.line });
        }
        let key = (h.level, h.text.to_lowercase());
        if let Some(first) = heads[..k]
            .iter()
            .find(|p| (p.level, p.text.to_lowercase()) == key)
        {
            out.push(Warning::DuplicateHeading {
                line: h.line,
                first: first.line,
            });
        }
    }
    if !heads
        .iter()
        .any(|h| heading_matches(&h.text, &["build", "setup", "install"]))
    {
        out.push(Warning::MissingBuild);
    }
    if !heads
        .iter()
        .any(|h| heading_matches(&h.text, &["test", "testing"]))
    {
        out.push(Warning::MissingTest);
    }
    out.sort_by_key(|w| w.line().unwrap_or(usize::MAX));
    out
}

/// Targets of `[text](target)` links on `line` that are relative paths (no scheme, no
/// anchor-only, no absolute path).
fn relative_links(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        let target = after[..end].split_whitespace().next().unwrap_or("");
        let target = target.split('#').next().unwrap_or("");
        let external = target.contains("://")
            || target.starts_with("mailto:")
            || target.starts_with('/')
            || target.starts_with('~');
        if !target.is_empty() && !external {
            out.push(target.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

/// A relative link that leaves the folder is not checked; one inside it is broken only when
/// the file system says it is not there (no access: skipped).
fn link_is_broken(folder: &Path, target: &str) -> bool {
    let target = Path::new(target);
    if target
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }
    match std::fs::symlink_metadata(folder.join(target)) {
        Ok(_) => false,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Folders of the last agent files opened by name, kept by a hash of their text, so Check
/// can look at relative links next to the file.
static FOLDERS: crate::memo::Memo<Option<PathBuf>> = crate::memo::Memo::with_min_len(4, 0);

/// Remember `path`'s folder for `text` when it is an agent file.
pub fn remember_folder(path: &Path, text: &str) {
    if is_agent_path(path) {
        FOLDERS.put(text, path.parent().map(Path::to_path_buf));
    }
}

/// The folder `text` was opened from, if remembered.
pub fn folder_of(text: &str) -> Option<PathBuf> {
    FOLDERS.get(text).flatten()
}

/// Drop the remembered folders (Wipe).
pub fn forget() {
    FOLDERS.clear();
}

#[cfg(test)]
mod tests;
