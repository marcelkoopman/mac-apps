//! TOML detection for copied text (a file is TOML by its `.toml` name, see
//! [`super::hint_by_name`]).

/// Every meaningful line is a `[table]` / `[[array]]` header, a `key = value` with a TOML value
/// (quoted, a number, a date, `true`/`false`, `[` or `{`), or part of a multi-line array or
/// string; with at least one key, and a header or two keys. INI files (`name=bare text`) fail.
pub fn looks_like_toml(text: &str) -> bool {
    let mut keys = 0;
    let mut headers = 0;
    let mut depth: i32 = 0;
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(close) = fence {
            if line.contains(close) {
                fence = None;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if depth > 0 {
            depth += bracket_balance(line);
            continue;
        }
        if is_header(line) {
            headers += 1;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let (key, value) = (key.trim(), value.trim());
        if !is_key(key) || !is_value(value) {
            return false;
        }
        keys += 1;
        for open in ["\"\"\"", "'''"] {
            if let Some(rest) = value.strip_prefix(open)
                && !rest.contains(open)
            {
                fence = Some(open);
            }
        }
        if fence.is_none() {
            depth = bracket_balance(value).max(0);
        }
    }
    keys >= 1 && (headers >= 1 || keys >= 2)
}

fn is_header(line: &str) -> bool {
    let line = strip_comment(line);
    let inner = line
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .or_else(|| {
            line.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        });
    inner.is_some_and(|name| !name.trim().is_empty() && is_key(name.trim()))
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.split('.').all(|part| {
            let part = part.trim();
            (part.len() >= 2 && part.starts_with('"') && part.ends_with('"'))
                || (!part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        })
}

fn is_value(value: &str) -> bool {
    let value = strip_comment(value);
    let Some(first) = value.chars().next() else {
        return false;
    };
    match first {
        '"' | '\'' | '[' | '{' => true,
        '0'..='9' | '+' | '-' => value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | ':' | ' ')),
        _ => matches!(value, "true" | "false" | "inf" | "nan"),
    }
}

/// `line` without a trailing `# comment` outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '#') => return line[..at].trim_end(),
            _ => {}
        }
    }
    line.trim_end()
}

fn bracket_balance(text: &str) -> i32 {
    let mut quote = None;
    let mut depth = 0;
    for c in strip_comment(text).chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            _ => {}
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::looks_like_toml;

    const CARGO: &str = "[workspace]\nresolver = \"2\"\nmembers = [\n    \"apps/copycraft\",\n    \"crates/mac-ui\",\n]\n\n[workspace.package]\nedition = \"2024\"\nversion = \"0.4.1\"\npublish = false\n\n[[bin]]\nname = \"copycraft\" # the app\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n";

    #[test]
    fn cargo_toml_is_toml() {
        assert!(looks_like_toml(CARGO));
        assert_eq!(
            crate::format::detect(CARGO),
            crate::format::FormatKind::Toml
        );
        assert!(looks_like_toml("title = \"Copycraft\"\nport = 8080\n"));
        assert!(looks_like_toml(
            "[server]\nstarted = 2026-10-13T19:00:00Z\nnote = \"\"\"\nline\n\"\"\"\n"
        ));
    }

    #[test]
    fn ini_and_other_text_is_not_toml() {
        assert!(!looks_like_toml(
            "[general]\nname=Copycraft app\nenabled=yes\n"
        ));
        assert!(!looks_like_toml(
            "Just a sentence with = in it.\nAnd another line."
        ));
        assert!(!looks_like_toml("x = 1\n"), "one key alone is too little");
        assert!(!looks_like_toml("[link](https://example.com)\n"));
        assert!(!looks_like_toml("a = b + c\nd = e\n"));
    }

    #[test]
    fn a_toml_file_name_makes_it_toml() {
        let text = "x = 1 # only in settings.TOML\n";
        crate::format::hint_by_name("settings.TOML", text);
        assert_eq!(crate::format::detect(text), crate::format::FormatKind::Toml);
    }

    #[test]
    fn toml_is_colored() {
        use crate::highlight::{TokenKind, tokens};
        let toks = tokens(CARGO, crate::format::FormatKind::Toml);
        let has = |kind: TokenKind, text: &str| toks.iter().any(|(k, t)| *k == kind && t == text);
        assert!(has(TokenKind::Type, "[workspace]"));
        assert!(has(TokenKind::Type, "[[bin]]"));
        assert!(has(TokenKind::Key, "resolver"));
        assert!(has(TokenKind::String, "\"2\""));
        assert!(has(TokenKind::Keyword, "false"));
        assert!(has(TokenKind::Comment, "# the app"));
        assert!(has(TokenKind::Key, "version"));
        let joined: String = toks.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(joined, CARGO);
    }
}
