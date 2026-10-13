//! The token stream the cores walk: words, punctuation and a few two-character operators,
//! with their line. Comments, strings, char literals and text blocks are dropped (a string
//! leaves a [`K::Lit`]).

use super::Lang;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum K {
    Word(String),
    P(char),
    /// `&&`, `||`, `->`, `=>`, `::`
    Op(&'static str),
    /// A string, char or number literal.
    Lit,
}

#[derive(Debug, Clone)]
pub(super) struct Tok {
    pub k: K,
    /// 1-based.
    pub line: usize,
    /// The first token on its line.
    pub first: bool,
    /// Leading spaces of its line (a tab counts 4).
    pub indent: usize,
}

impl Tok {
    pub fn is_word(&self, w: &str) -> bool {
        matches!(&self.k, K::Word(s) if s == w)
    }
    pub fn is_p(&self, c: char) -> bool {
        self.k == K::P(c)
    }
    pub fn word(&self) -> Option<&str> {
        match &self.k {
            K::Word(s) => Some(s),
            _ => None,
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub(super) fn tokens(text: &str, lang: Lang) -> Vec<Tok> {
    let c: Vec<char> = text.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut line = 1;
    let mut indent = 0;
    let mut at_start = true;
    let mut i = 0;
    let push = |out: &mut Vec<Tok>, k: K, line: usize, at_start: &mut bool, indent: usize| {
        out.push(Tok {
            k,
            line,
            first: *at_start,
            indent,
        });
        *at_start = false;
    };
    // Lines a skipped span (comment, string) crosses.
    let newlines = |from: usize, to: usize| {
        c[from..to.min(c.len())]
            .iter()
            .filter(|&&ch| ch == '\n')
            .count()
    };
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            indent = 0;
            at_start = true;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            if at_start {
                indent += if ch == '\t' { 4 } else { 1 };
            }
            i += 1;
            continue;
        }
        let next = c.get(i + 1).copied();
        // Comments.
        let line_comment = match lang {
            Lang::Python => ch == '#',
            _ => ch == '/' && next == Some('/'),
        };
        if line_comment {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if lang != Lang::Python && ch == '/' && next == Some('*') {
            let start = i;
            let mut depth = 1;
            i += 2;
            while i < c.len() && depth > 0 {
                if lang == Lang::Rust && c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            line += newlines(start, i);
            continue;
        }
        // Strings and chars.
        if let Some(end) = literal_end(&c, i, lang) {
            let start_line = line;
            line += newlines(i, end);
            push(&mut out, K::Lit, start_line, &mut at_start, indent);
            i = end;
            continue;
        }
        if ch == '\'' && lang == Lang::Rust {
            // A lifetime or loop label: 'a, 'outer.
            let start = i;
            i += 1;
            while i < c.len() && is_word_char(c[i]) {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            push(&mut out, K::Word(word), line, &mut at_start, indent);
            continue;
        }
        if ch.is_ascii_digit() {
            while i < c.len()
                && (c[i].is_ascii_alphanumeric()
                    || c[i] == '_'
                    || (c[i] == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit)))
            {
                i += 1;
            }
            push(&mut out, K::Lit, line, &mut at_start, indent);
            continue;
        }
        if is_word_char(ch) {
            let start = i;
            while i < c.len() && is_word_char(c[i]) {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            push(&mut out, K::Word(word), line, &mut at_start, indent);
            continue;
        }
        let op = match (ch, next) {
            ('&', Some('&')) => Some("&&"),
            ('|', Some('|')) => Some("||"),
            ('-', Some('>')) => Some("->"),
            ('=', Some('>')) => Some("=>"),
            (':', Some(':')) => Some("::"),
            _ => None,
        };
        if let Some(op) = op {
            push(&mut out, K::Op(op), line, &mut at_start, indent);
            i += 2;
            continue;
        }
        push(&mut out, K::P(ch), line, &mut at_start, indent);
        i += 1;
    }
    out
}

/// End of the string or char literal starting at `i`, if one starts there.
fn literal_end(c: &[char], i: usize, lang: Lang) -> Option<usize> {
    let ch = c[i];
    match lang {
        Lang::Python => {
            if ch == '"' || ch == '\'' {
                return Some(crate::brackets::skip_python_string(c, i));
            }
            // A prefix: f"…", rb'…'.
            let mut j = i;
            while j < c.len()
                && j - i < 2
                && matches!(c[j].to_ascii_lowercase(), 'f' | 'r' | 'b' | 'u')
            {
                j += 1;
            }
            let before_ok = i == 0 || !is_word_char(c[i - 1]);
            (j > i && before_ok && matches!(c.get(j), Some('"' | '\'')))
                .then(|| crate::brackets::skip_python_string(c, j))
        }
        Lang::Java => {
            if ch == '"' && c.get(i + 1) == Some(&'"') && c.get(i + 2) == Some(&'"') {
                let mut j = i + 3;
                while j < c.len()
                    && !(c[j] == '"' && c.get(j + 1) == Some(&'"') && c.get(j + 2) == Some(&'"'))
                {
                    j += if c[j] == '\\' { 2 } else { 1 };
                }
                return Some((j + 3).min(c.len()));
            }
            (ch == '"' || ch == '\'').then(|| quoted_end(c, i))
        }
        Lang::Rust => {
            if ch == '"' {
                return Some(quoted_end(c, i));
            }
            let before_ok = i == 0 || !is_word_char(c[i - 1]);
            // b"…", r"…", r#"…"#, br#"…"#.
            if before_ok && (ch == 'r' || ch == 'b') {
                let mut j = i;
                if c[j] == 'b' {
                    j += 1;
                }
                if c.get(j) == Some(&'r') {
                    j += 1;
                    let mut hashes = 0;
                    while c.get(j) == Some(&'#') {
                        hashes += 1;
                        j += 1;
                    }
                    if c.get(j) == Some(&'"') {
                        j += 1;
                        while j < c.len() {
                            if c[j] == '"' && (0..hashes).all(|h| c.get(j + 1 + h) == Some(&'#')) {
                                return Some(j + 1 + hashes);
                            }
                            j += 1;
                        }
                        return Some(c.len());
                    }
                    return None;
                }
                if j > i && c.get(j) == Some(&'"') {
                    return Some(quoted_end(c, j));
                }
                if j > i && c.get(j) == Some(&'\'') {
                    return Some(quoted_end(c, j));
                }
                return None;
            }
            if ch == '\'' {
                // A char literal: '\n', '\u{1F600}', 'a' (not a lifetime 'a).
                if c.get(i + 1) == Some(&'\\') {
                    return Some(quoted_end(c, i));
                }
                if c.get(i + 2) == Some(&'\'') {
                    return Some(i + 3);
                }
            }
            None
        }
    }
}

/// Past the closing `quote` of the one-line literal opened at `i` (escapes skipped).
fn quoted_end(c: &[char], i: usize) -> usize {
    let quote = c[i];
    let mut j = i + 1;
    while j < c.len() && c[j] != quote && c[j] != '\n' {
        j += if c[j] == '\\' { 2 } else { 1 };
    }
    (j + 1).min(c.len())
}
