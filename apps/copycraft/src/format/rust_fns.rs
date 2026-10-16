//! Function count for a Rust card's meta line: one lexical pass, no parser.

/// `fn` items (free functions and methods, whatever their qualifiers): the keyword followed by a
/// name. Comments, strings (also raw and byte strings), char literals and `fn(u8) -> u8` pointer
/// types do not count.
pub fn count(text: &str) -> usize {
    let mut found = 0;
    walk(text, |_| found += 1);
    found
}

/// A `fn` item the lexical pass found: where its keyword starts and where its name does.
struct Found {
    keyword: usize,
    name: std::ops::Range<usize>,
}

/// The `fn` items of `text` in order, each reported to `on_fn`. One lexical pass: comments,
/// strings, char literals and `fn(u8)` pointer types are skipped.
fn walk(text: &str, mut on_fn: impl FnMut(Found)) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block_comment(b, i),
            b'"' => i = skip_string(b, i + 1),
            b'\'' => i = skip_char(text, i),
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &b[start..i];
                if matches!(word, b"r" | b"br") && matches!(b.get(i), Some(b'"' | b'#')) {
                    i = skip_raw_string(b, i);
                } else if word == b"b" && b.get(i) == Some(&b'"') {
                    i = skip_string(b, i + 1);
                } else if word == b"fn" {
                    let mut j = i;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_') {
                        let mut end = j;
                        while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                            end += 1;
                        }
                        on_fn(Found {
                            keyword: start,
                            name: j..end,
                        });
                    }
                }
            }
            _ => i += 1,
        }
    }
}

/// Past the block comment (nested ones too) that starts at `i`.
fn skip_block_comment(b: &[u8], mut i: usize) -> usize {
    let mut depth = 1;
    i += 2;
    while i < b.len() && depth > 0 {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            depth += 1;
            i += 2;
        } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    i
}

/// Past the char literal ('a', '\n', '\u{1F600}', 'é') at `i`; for a lifetime ('a) just the `'`.
fn skip_char(text: &str, mut i: usize) -> usize {
    let b = text.as_bytes();
    if b.get(i + 1) == Some(&b'\\') {
        i += 2;
        while i < b.len() && b[i] != b'\'' && b[i] != b'\n' {
            i += 1;
        }
        i + 1
    } else {
        let ch_len = text[i + 1..].chars().next().map_or(1, char::len_utf8);
        if b.get(i + 1 + ch_len) == Some(&b'\'') {
            i + 2 + ch_len
        } else {
            i + 1
        }
    }
}

/// Past the `}` that closes the body of the item whose signature follows `from`; `None` for an
/// item without a body (`fn f();`) or an unbalanced one. Parentheses and brackets of the
/// signature are skipped first, so `fn(u8) -> u8` types and `[u8; 4]` do not end it.
fn body_end(text: &str, from: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = from;
    let mut nest = 0usize;
    let mut braces = 0usize;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block_comment(b, i),
            b'"' => i = skip_string(b, i + 1),
            b'\'' => i = skip_char(text, i),
            b'(' | b'[' if braces == 0 => {
                nest += 1;
                i += 1;
            }
            b')' | b']' if braces == 0 => {
                nest = nest.saturating_sub(1);
                i += 1;
            }
            b';' if braces == 0 && nest == 0 => return None,
            b'{' if braces > 0 || nest == 0 => {
                braces += 1;
                i += 1;
            }
            b'}' if braces > 0 => {
                braces -= 1;
                i += 1;
                if braces == 0 {
                    return Some(i);
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &b[start..i];
                if matches!(word, b"r" | b"br") && matches!(b.get(i), Some(b'"' | b'#')) {
                    i = skip_raw_string(b, i);
                } else if word == b"b" && b.get(i) == Some(&b'"') {
                    i = skip_string(b, i + 1);
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// The functions of a Rust text in file order, methods in `impl` blocks and nested functions
/// included (a nested one after the one around it). A span starts at the line of the doc
/// comment and attributes directly above the function and ends after its closing `}`;
/// declarations without a body (trait methods) are left out.
pub fn spans(text: &str) -> Vec<super::fn_spans::FnSpan> {
    let mut out = Vec::new();
    walk(text, |found| {
        let Some(end) = body_end(text, found.name.end) else {
            return;
        };
        let start = super::fn_spans::rust_item_start(text, found.keyword);
        out.push(super::fn_spans::FnSpan {
            name: text[found.name.clone()].to_string(),
            signature_line: super::fn_spans::line_at(text, found.keyword),
            byte_range: start..end,
        });
    });
    out
}

/// Past the closing `"` of a string whose body starts at `i`.
fn skip_string(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// Past a raw string whose `#`s or `"` start at `i` (`r#"…"#`).
fn skip_raw_string(b: &[u8], mut i: usize) -> usize {
    let mut hashes = 0;
    while b.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    if b.get(i) != Some(&b'"') {
        return i;
    }
    i += 1;
    while i < b.len() {
        if b[i] == b'"'
            && b[i + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == b'#')
                .count()
                == hashes
        {
            return i + 1 + hashes;
        }
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn counts_items_and_methods_with_qualifiers() {
        let src = "pub fn a() {}\nasync fn b() {}\nconst fn c() -> u8 { 1 }\nunsafe fn d() {}\npub(crate) extern \"C\" fn e() {}\nimpl S {\n    fn f(&self) {}\n    pub async unsafe fn g() {}\n}\ntrait T { fn h(); }\nfn\n  spaced() {}\n";
        assert_eq!(count(src), 9);
    }

    #[test]
    fn skips_comments_strings_chars_and_pointer_types() {
        let src = r##"
// fn in_line_comment() {}
/* fn in_block() { /* fn nested() */ } */
/// fn in_doc() {}
fn real<'a>(x: &'a str, f: fn(u8) -> u8, g: Box<dyn Fn(u8)>) -> char {
    let s = "fn not_this() \" fn nor_this()";
    let r = r#"fn raw() "quoted" fn"#;
    let b = b"fn bytes()";
    let c = '"';
    let e = '\'';
    let fnord = 1;
    'f'
}
"##;
        assert_eq!(count(src), 1);
    }

    #[test]
    fn rust_meta_line_says_functions() {
        let src = "fn main() {\n    helper();\n}\n\nfn helper() {}\n";
        assert_eq!(crate::format::detect(src), crate::format::FormatKind::Rust);
        assert!(crate::commands::text_meta(src).contains(" · 2 functions · "));
        let one = "use std::io;\n\nfn main() {\n    let x = 1;\n}\n";
        assert!(crate::commands::text_meta(one).contains(" · 1 function · "));
    }
}
