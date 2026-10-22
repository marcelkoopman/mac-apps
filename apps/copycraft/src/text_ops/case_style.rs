//! Case style… in Text ▾: every line written as camelCase, PascalCase, snake_case,
//! kebab-case, CONSTANT_CASE, UPPERCASE or lowercase.
//!
//! Each line is one name. Its words are split at `_`, `-`, whitespace and any other
//! character that is not a letter or digit (those go), at a lowercase letter or digit
//! followed by a capital (`userId`), before the last capital of a run that a lowercase
//! letter follows (`HTTPServer` → `HTTP` `Server`), and after digits that a letter follows
//! (`md5sum` → `md5` `sum`). Digits stay with the word before them, so `userID2` is `user`
//! `ID2`: `user_id2`, `userId2`. UPPERCASE and lowercase change only the letters, nothing is
//! split. Space before and after a line, its line ending, an empty line and a line without
//! letters or digits stay as they are.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

/// A case style, in the order of the submenu and the dialog's pop-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStyle {
    Camel,
    Pascal,
    Snake,
    Kebab,
    Constant,
    Upper,
    Lower,
}

impl CaseStyle {
    pub const ALL: [CaseStyle; 7] = [
        CaseStyle::Camel,
        CaseStyle::Pascal,
        CaseStyle::Snake,
        CaseStyle::Kebab,
        CaseStyle::Constant,
        CaseStyle::Upper,
        CaseStyle::Lower,
    ];

    /// Its name in the menu, written in the style itself.
    pub fn title(self) -> &'static str {
        match self {
            CaseStyle::Camel => crate::locale::t("case_camel"),
            CaseStyle::Pascal => crate::locale::t("case_pascal"),
            CaseStyle::Snake => crate::locale::t("case_snake"),
            CaseStyle::Kebab => crate::locale::t("case_kebab"),
            CaseStyle::Constant => crate::locale::t("case_constant"),
            CaseStyle::Upper => crate::locale::t("case_upper"),
            CaseStyle::Lower => crate::locale::t("case_lower"),
        }
    }
}

/// Every line of `text` in `style`; line endings (LF, CRLF, mixed) come back as copied.
pub fn convert(text: &str, style: CaseStyle) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for piece in text.split_inclusive('\n') {
        let body = piece
            .strip_suffix('\n')
            .map_or(piece, |rest| rest.strip_suffix('\r').unwrap_or(rest));
        out.push_str(&convert_line(body, style));
        out.push_str(&piece[body.len()..]);
    }
    out
}

/// Lines `convert` changes.
pub fn changed_lines(text: &str, style: CaseStyle) -> usize {
    text.lines()
        .filter(|line| convert_line(line, style) != *line)
        .count()
}

/// One line (without its ending) in `style`.
pub fn convert_line(line: &str, style: CaseStyle) -> String {
    let core = line.trim();
    if core.is_empty() {
        return line.to_string();
    }
    let lead = &line[..line.len() - line.trim_start().len()];
    let tail = &line[line.trim_end().len()..];
    let body = match style {
        CaseStyle::Upper => core.to_uppercase(),
        CaseStyle::Lower => core.to_lowercase(),
        _ => {
            let words = words(core);
            if words.is_empty() {
                return line.to_string();
            }
            join_words(&words, style)
        }
    };
    format!("{lead}{body}{tail}")
}

fn join_words(words: &[&str], style: CaseStyle) -> String {
    let lower = |word: &str| word.to_lowercase();
    let capital = |word: &str| {
        let mut chars = word.chars();
        chars.next().map_or_else(String::new, |first| {
            first
                .to_uppercase()
                .chain(chars.as_str().to_lowercase().chars())
                .collect()
        })
    };
    match style {
        CaseStyle::Camel => words
            .iter()
            .enumerate()
            .map(|(at, word)| if at == 0 { lower(word) } else { capital(word) })
            .collect(),
        CaseStyle::Pascal => words.iter().map(|word| capital(word)).collect(),
        CaseStyle::Snake => words
            .iter()
            .map(|word| lower(word))
            .collect::<Vec<_>>()
            .join("_"),
        CaseStyle::Kebab => words
            .iter()
            .map(|word| lower(word))
            .collect::<Vec<_>>()
            .join("-"),
        CaseStyle::Constant => words
            .iter()
            .map(|word| word.to_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
        CaseStyle::Upper | CaseStyle::Lower => unreachable!("not split"),
    }
}

/// The words of `text` (see the module docs).
pub fn words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    for run in text.split(|c: char| !c.is_alphanumeric()) {
        if run.is_empty() {
            continue;
        }
        let chars: Vec<(usize, char)> = run.char_indices().collect();
        let mut start = 0;
        for at in 1..chars.len() {
            let (index, here) = chars[at];
            let before = chars[at - 1].1;
            let next = chars.get(at + 1).map(|(_, c)| *c);
            let boundary = if here.is_uppercase() {
                // userId, ID2Name; HTTPServer before the S.
                before.is_lowercase()
                    || before.is_numeric()
                    || (before.is_uppercase() && next.is_some_and(char::is_lowercase))
            } else if here.is_alphabetic() {
                // md5sum
                before.is_numeric()
            } else {
                false
            };
            if boundary {
                words.push(&run[start..index]);
                start = index;
            }
        }
        words.push(&run[start..]);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(text: &str) -> Vec<String> {
        CaseStyle::ALL
            .iter()
            .map(|style| convert(text, *style))
            .collect()
    }

    #[test]
    fn each_style() {
        assert_eq!(
            all("user name"),
            [
                "userName",
                "UserName",
                "user_name",
                "user-name",
                "USER_NAME",
                "USER NAME",
                "user name"
            ]
        );
        for text in [
            "userName",
            "UserName",
            "user_name",
            "user-name",
            "USER_NAME",
        ] {
            assert_eq!(all(text)[..5], all("user name")[..5], "{text}");
        }
        // UPPERCASE and lowercase only change the letters.
        assert_eq!(
            convert("Hello, wörld-2", CaseStyle::Upper),
            "HELLO, WÖRLD-2"
        );
        assert_eq!(
            convert("Hello, WÖRLD_2", CaseStyle::Lower),
            "hello, wörld_2"
        );
    }

    #[test]
    fn acronyms() {
        assert_eq!(words("HTTPServer"), ["HTTP", "Server"]);
        assert_eq!(convert("HTTPServer", CaseStyle::Camel), "httpServer");
        assert_eq!(convert("HTTPServer", CaseStyle::Snake), "http_server");
        assert_eq!(convert("HTTPServer", CaseStyle::Pascal), "HttpServer");
        assert_eq!(
            convert("parseHTTPResponse", CaseStyle::Kebab),
            "parse-http-response"
        );
        assert_eq!(convert("getURL", CaseStyle::Snake), "get_url");
        assert_eq!(convert("URL", CaseStyle::Camel), "url");
    }

    #[test]
    fn digits_stay_with_the_word_before_them() {
        assert_eq!(words("userID2"), ["user", "ID2"]);
        assert_eq!(convert("userID2", CaseStyle::Snake), "user_id2");
        assert_eq!(convert("userID2", CaseStyle::Camel), "userId2");
        assert_eq!(convert("userID2", CaseStyle::Constant), "USER_ID2");
        // And back.
        assert_eq!(convert("user_id2", CaseStyle::Camel), "userId2");
        assert_eq!(words("md5sum"), ["md5", "sum"]);
        assert_eq!(words("sha256Hash"), ["sha256", "Hash"]);
        assert_eq!(words("v2API"), ["v2", "API"]);
        assert_eq!(convert("x86_64", CaseStyle::Kebab), "x86-64");
    }

    #[test]
    fn snake_to_camel() {
        assert_eq!(convert("user_id", CaseStyle::Camel), "userId");
        assert_eq!(convert("__user__id__", CaseStyle::Camel), "userId");
        assert_eq!(convert("USER_ID", CaseStyle::Pascal), "UserId");
    }

    #[test]
    fn per_line_with_empty_lines_and_endings_kept() {
        let text = "HTTPServer\nuser_id\n\n  first name  \r\n---\nlast";
        assert_eq!(
            convert(text, CaseStyle::Camel),
            "httpServer\nuserId\n\n  firstName  \r\n---\nlast"
        );
        assert_eq!(convert("a b\n", CaseStyle::Snake), "a_b\n");
        assert_eq!(convert("", CaseStyle::Snake), "");
        assert_eq!(convert("\n\n", CaseStyle::Kebab), "\n\n");
        assert_eq!(changed_lines(text, CaseStyle::Camel), 3);
        assert_eq!(changed_lines("userId\nother", CaseStyle::Camel), 0);
    }

    #[test]
    fn letters_beyond_ascii() {
        assert_eq!(convert("Ève größe", CaseStyle::Camel), "èveGröße");
        assert_eq!(convert("naïveÉcole", CaseStyle::Snake), "naïve_école");
    }
}
