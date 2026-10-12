pub(super) fn looks_like_rust(text: &str) -> bool {
    let rust_hits = [
        "fn ",
        "impl ",
        "pub fn",
        "let mut",
        "match ",
        "use crate",
        "#[derive",
    ];
    score(text, &rust_hits) >= 2
        || (text.contains("fn ") && (text.contains('{') || text.contains("->")))
}

pub(super) fn looks_like_java(text: &str) -> bool {
    let java_hits = [
        "public class",
        "private class",
        "package ",
        "import java",
        "System.out",
        "public static void",
        "@Override",
    ];
    score(text, &java_hits) >= 1 && text.contains('{')
}

pub(super) fn score(text: &str, needles: &[&str]) -> usize {
    needles.iter().filter(|n| text.contains(*n)).count()
}

/// Re-indent brace code (Rust, Java) by its brackets, four spaces a level: `{` `[` `(` open one,
/// their closers close it, and a line starting with closers is dedented by them. A line that
/// starts with `.` (a method chain) goes one level deeper. Brackets in strings, char literals
/// and comments do not count, and lines inside a multi-line string or block comment are kept
/// exactly as they are. Not a full formatter: spacing within a line is left alone.
pub fn indent_braces(src: &str) -> String {
    let mut indent: i32 = 0;
    let mut state = Scan::Code;
    let mut out = String::new();
    for raw in src.lines() {
        if state != Scan::Code {
            // Inside a string or comment that started on an earlier line.
            out.push_str(raw);
            out.push('\n');
            let (_, next) = scan_line(raw, state);
            state = next;
            continue;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        let closers = trimmed
            .chars()
            .take_while(|ch| matches!(ch, '}' | ']' | ')'))
            .count();
        let closers = i32::try_from(closers).unwrap_or(i32::MAX);
        let mut level = (indent - closers).max(0);
        if trimmed.starts_with('.') && !trimmed.starts_with("..") {
            level += 1;
        }
        for _ in 0..level {
            out.push_str("    ");
        }
        out.push_str(trimmed);
        out.push('\n');
        let (depth, next) = scan_line(trimmed, state);
        indent = (indent + depth).max(0);
        state = next;
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Where [`scan_line`] is at the end of a line.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scan {
    Code,
    /// In a `"…"` string.
    Str,
    /// In a Rust raw string `r#"…"#` closed by `"` and this many `#`.
    RawStr(usize),
    /// In a `/* … */` comment, this deep (Rust nests them).
    BlockComment(usize),
}

/// The bracket depth change over `line` (`{` `[` `(` up, closers down), counting only code,
/// and the state the next line starts in.
pub(super) fn scan_line(line: &str, mut state: Scan) -> (i32, Scan) {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            Scan::Str => match ch {
                '\\' => i += 1,
                '"' => state = Scan::Code,
                _ => {}
            },
            Scan::RawStr(hashes) => {
                if ch == '"'
                    && chars[i + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|c| **c == '#')
                        .count()
                        == hashes
                {
                    state = Scan::Code;
                    i += hashes;
                }
            }
            Scan::BlockComment(level) => {
                if ch == '*' && next == Some('/') {
                    state = if level > 1 {
                        Scan::BlockComment(level - 1)
                    } else {
                        Scan::Code
                    };
                    i += 1;
                } else if ch == '/' && next == Some('*') {
                    state = Scan::BlockComment(level + 1);
                    i += 1;
                }
            }
            Scan::Code => match ch {
                '/' if next == Some('/') => break,
                '/' if next == Some('*') => {
                    state = Scan::BlockComment(1);
                    i += 1;
                }
                '"' => state = Scan::Str,
                'r' if starts_raw_string(&chars, i).is_some() => {
                    let hashes = starts_raw_string(&chars, i).unwrap_or(0);
                    state = Scan::RawStr(hashes);
                    i += hashes + 1;
                }
                '\'' => i += char_literal_len(&chars[i..]).saturating_sub(1),
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => depth -= 1,
                _ => {}
            },
        }
        i += 1;
    }
    (depth, state)
}

/// `r"`, `r#"`, `br##"`, … at `i` (not the end of a name such as `bar"`): the number of `#`.
pub(super) fn starts_raw_string(chars: &[char], i: usize) -> Option<usize> {
    let before = i.checked_sub(1).map(|at| chars[at]);
    let name_before = before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    if name_before && before != Some('b') {
        return None;
    }
    if before == Some('b') && i >= 2 && (chars[i - 2].is_alphanumeric() || chars[i - 2] == '_') {
        return None;
    }
    let hashes = chars[i + 1..].iter().take_while(|ch| **ch == '#').count();
    (chars.get(i + 1 + hashes) == Some(&'"')).then_some(hashes)
}

/// Length of the char literal (`'{'`, `'\''`, `'\u{7b}'`) that starts `rest`, or 1 for a quote
/// that starts none (a Rust lifetime such as `'a`).
pub(super) fn char_literal_len(rest: &[char]) -> usize {
    match rest.get(1) {
        // The escaped character is taken as is, then the closing quote.
        Some('\\') => rest
            .iter()
            .skip(3)
            .take(10)
            .position(|ch| *ch == '\'')
            .map_or(1, |at| at + 4),
        Some(_) if rest.get(2) == Some(&'\'') => 3,
        _ => 1,
    }
}
