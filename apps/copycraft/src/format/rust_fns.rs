//! Function count for a Rust card's meta line: one lexical pass, no parser.

/// `fn` items (free functions and methods, whatever their qualifiers): the keyword followed by a
/// name. Comments, strings (also raw and byte strings), char literals and `fn(u8) -> u8` pointer
/// types do not count.
pub fn count(text: &str) -> usize {
    let b = text.as_bytes();
    let mut i = 0;
    let mut found = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
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
            }
            b'"' => i = skip_string(b, i + 1),
            b'\'' => {
                // A char literal ('a', '\n', '\u{1F600}', 'é'); else a lifetime ('a).
                if b.get(i + 1) == Some(&b'\\') {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' && b[i] != b'\n' {
                        i += 1;
                    }
                    i += 1;
                } else {
                    let ch_len = text[i + 1..].chars().next().map_or(1, char::len_utf8);
                    if b.get(i + 1 + ch_len) == Some(&b'\'') {
                        i += 2 + ch_len;
                    } else {
                        i += 1;
                    }
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
                } else if word == b"fn" {
                    let mut j = i;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_') {
                        found += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    found
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
