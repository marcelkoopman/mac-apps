//! Where the functions of a code card start and end, so one of them can go to the model instead
//! of the whole text ([`function_spans`]). Lexical, no parser: Rust and Java by their braces
//! ([`super::rust_fns`], [`super::java_methods`]), Python by its indentation.

use std::ops::Range;

use super::FormatKind;

/// One function (or method) of a code text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSpan {
    pub name: String,
    /// The line the function is declared on, trimmed (for a multi-line signature only its
    /// first line).
    pub signature_line: String,
    /// The function as a byte range of the text: its doc comment, attributes or decorators
    /// directly above it, its signature and its body.
    pub byte_range: Range<usize>,
}

/// The functions of `text` (code of `kind`) in file order: methods in `impl` blocks and classes
/// included, a nested function after the one around it (and inside its range). Other kinds
/// give none.
pub fn function_spans(text: &str, kind: FormatKind) -> Vec<FnSpan> {
    let mut spans = match kind {
        FormatKind::Rust => super::rust_fns::spans(text),
        FormatKind::Java => super::java_methods::spans(text),
        FormatKind::Python => python_spans(text),
        _ => Vec::new(),
    };
    spans.sort_by_key(|span| span.byte_range.start);
    spans
}

/// The line of `text` that holds byte `at`, trimmed.
pub(crate) fn line_at(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let end = text[at..].find('\n').map_or(text.len(), |p| at + p);
    text[start..end].trim().to_string()
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |p| p + 1)
}

/// The start of the item whose `fn` keyword is at `keyword`: the start of its line, moved up
/// over the doc comments (`///`) and attributes (`#[…]`) directly above it.
pub(crate) fn rust_item_start(text: &str, keyword: usize) -> usize {
    let mut start = line_start(text, keyword);
    while start > 0 {
        let above = line_start(text, start - 1);
        let line = text[above..start].trim_start();
        if line.starts_with("///") || line.starts_with("#[") || line.starts_with("#![") {
            start = above;
        } else {
            break;
        }
    }
    start
}

/// `at` moved to the start of its line, and further up over a block comment (Javadoc) that ends
/// right above it.
pub(crate) fn with_leading_comment(text: &str, at: usize) -> usize {
    let start = line_start(text, at);
    // Only whitespace may sit between the start of the line and `at`.
    let start = if text[start..at].trim().is_empty() {
        start
    } else {
        at
    };
    let above = text[..start].trim_end();
    if above.ends_with("*/")
        && let Some(open) = above.rfind("/*")
    {
        return line_start(text, open);
    }
    start
}

/// One line of Python and whether it starts inside a triple-quoted string.
struct PyLine {
    start: usize,
    end: usize,
    /// Starts inside a triple-quoted string.
    in_string: bool,
    /// Parentheses still open at the end of the line (outside strings and comments).
    open: i32,
}

fn python_lines(text: &str) -> Vec<PyLine> {
    let mut lines = Vec::new();
    let mut triple: Option<u8> = None;
    let mut open = 0i32;
    let mut start = 0;
    for raw in text.split_inclusive('\n') {
        let b = raw.as_bytes();
        let in_string = triple.is_some();
        let mut single: Option<u8> = None;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if let Some(q) = triple {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == q && b[i..].starts_with(&[q, q, q]) {
                    triple = None;
                    i += 3;
                    continue;
                }
            } else if let Some(q) = single {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == q {
                    single = None;
                }
            } else {
                match c {
                    b'#' => break,
                    b'"' | b'\'' if b[i..].starts_with(&[c, c, c]) => {
                        triple = Some(c);
                        i += 3;
                        continue;
                    }
                    b'"' | b'\'' => single = Some(c),
                    b'(' | b'[' | b'{' => open += 1,
                    b')' | b']' | b'}' => open -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
        lines.push(PyLine {
            start,
            end: start + raw.len(),
            in_string,
            open: open.max(0),
        });
        open = open.max(0);
        start += raw.len();
    }
    lines
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// `def` and `async def` of a Python text: from the decorators above to the last line of the
/// body (the lines indented deeper than the `def`, a multi-line signature and docstring
/// included; trailing blank lines are not).
fn python_spans(text: &str) -> Vec<FnSpan> {
    let lines = python_lines(text);
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        if line.in_string {
            continue;
        }
        let raw = &text[line.start..line.end];
        let trimmed = raw.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("async def ")
            .or_else(|| trimmed.strip_prefix("def "))
        else {
            continue;
        };
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let indent = indent_of(raw);
        // The header ends on the line where the parentheses are closed again.
        let mut header_end = n;
        while lines[header_end].open > 0 && header_end + 1 < lines.len() {
            header_end += 1;
        }
        // The body: until the next line that is not blank, not inside a string and not indented
        // deeper than the `def`.
        let mut last = header_end;
        for (k, next) in lines.iter().enumerate().skip(header_end + 1) {
            let content = &text[next.start..next.end];
            if content.trim().is_empty() {
                continue;
            }
            if !next.in_string && indent_of(content) <= indent {
                break;
            }
            last = k;
        }
        // Decorators directly above.
        let mut first = n;
        while first > 0 {
            let above = &lines[first - 1];
            let content = &text[above.start..above.end];
            if !above.in_string && indent_of(content) == indent && content.trim().starts_with('@') {
                first -= 1;
            } else {
                break;
            }
        }
        let end = lines[last].end;
        let end = text[..end].trim_end_matches(['\n', '\r']).len();
        out.push(FnSpan {
            name,
            signature_line: trimmed.trim().to_string(),
            byte_range: lines[first].start..end,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use FormatKind as K;

    fn names(text: &str, kind: FormatKind) -> Vec<String> {
        function_spans(text, kind)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    fn slice<'a>(text: &'a str, spans: &[FnSpan], i: usize) -> &'a str {
        &text[spans[i].byte_range.clone()]
    }

    #[test]
    fn rust_functions_in_file_order_with_methods_and_nesting() {
        let src = "fn a() {\n    fn inner() {}\n}\nstruct S;\nimpl S {\n    pub fn m(&self) -> u8 { 1 }\n    async fn n() {}\n}\ntrait T { fn decl(); }\nfn z() {}\n";
        let spans = function_spans(src, K::Rust);
        let got: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(got, ["a", "inner", "m", "n", "z"]);
        assert_eq!(slice(src, &spans, 0), "fn a() {\n    fn inner() {}\n}");
        assert_eq!(slice(src, &spans, 1), "    fn inner() {}");
        assert_eq!(slice(src, &spans, 2), "    pub fn m(&self) -> u8 { 1 }");
        assert_eq!(spans[2].signature_line, "pub fn m(&self) -> u8 { 1 }");
        assert_eq!(slice(src, &spans, 4), "fn z() {}");
    }

    #[test]
    fn rust_braces_in_strings_chars_and_comments_do_not_count() {
        let src = "fn a() {\n    let s = \"}}\";\n    let r = r#\"{ \"}\" \"#;\n    let c = '}';\n    let d = '{';\n    // }\n    /* } */\n}\nfn b() {}\n";
        let spans = function_spans(src, K::Rust);
        assert_eq!(spans.len(), 2);
        assert!(slice(src, &spans, 0).ends_with("/* } */\n}"));
        assert_eq!(slice(src, &spans, 1), "fn b() {}");
    }

    #[test]
    fn rust_doc_comments_and_attributes_directly_above_come_along() {
        let src = "// not this\n\n/// Adds.\n/// More.\n#[inline]\npub fn add() {}\n\n/// Detached\n\nfn other() {}\n";
        let spans = function_spans(src, K::Rust);
        assert_eq!(
            slice(src, &spans, 0),
            "/// Adds.\n/// More.\n#[inline]\npub fn add() {}"
        );
        assert_eq!(slice(src, &spans, 1), "fn other() {}");
    }

    #[test]
    fn rust_signatures_with_fn_pointers_where_clauses_and_arrays() {
        let src = "fn a(f: fn(u8) -> u8, b: [u8; 4]) -> impl Fn(u8) -> u8\nwhere\n    u8: Copy,\n{\n    move |x| f(x)\n}\nfn b() {}\n";
        let spans = function_spans(src, K::Rust);
        assert_eq!(names(src, K::Rust), ["a", "b"]);
        assert!(slice(src, &spans, 0).ends_with("f(x)\n}"));
        assert_eq!(
            spans[0].signature_line,
            "fn a(f: fn(u8) -> u8, b: [u8; 4]) -> impl Fn(u8) -> u8"
        );
    }

    #[test]
    fn rust_unbalanced_text_does_not_panic() {
        assert!(function_spans("fn a() { { ", K::Rust).is_empty());
        assert!(function_spans("fn", K::Rust).is_empty());
        assert!(function_spans("fn é() { 'é' }", K::Rust).len() <= 1);
    }

    #[test]
    fn java_methods_with_javadoc_annotations_and_strings() {
        let src = "public class A {\n    /** Says hi. */\n    @Override\n    public void hi() {\n        String s = \"}\";\n        char c = '{';\n    }\n\n    abstract int abs(int x);\n    private static <T> T id(T t) throws Exception { return t; }\n}\n";
        let spans = function_spans(src, K::Java);
        let got: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(got, ["hi", "abs", "id"]);
        assert!(slice(src, &spans, 0).starts_with("    /** Says hi. */\n    @Override\n"));
        assert!(slice(src, &spans, 0).ends_with("char c = '{';\n    }"));
        assert_eq!(slice(src, &spans, 1), "    abstract int abs(int x);");
        assert_eq!(spans[0].signature_line, "public void hi() {");
        assert!(slice(src, &spans, 2).ends_with("return t; }"));
    }

    #[test]
    fn java_constructors_and_calls_are_not_methods() {
        let src = "class A {\n    A() { init(); }\n    void init() { foo(1); }\n}\n";
        assert_eq!(names(src, K::Java), ["init"]);
    }

    #[test]
    fn python_defs_methods_nested_and_async() {
        let src = "import os\n\n@decorator\ndef a(x):\n    def inner():\n        return 1\n    return inner\n\n\nclass C:\n    def m(self,\n          y):\n        \"\"\"Doc\ndef fake():\n\"\"\"\n        return y\n\n    async def n(self):\n        pass\n\n# end\ndef z(): pass\n";
        let spans = function_spans(src, K::Python);
        let got: Vec<&str> = spans.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(got, ["a", "inner", "m", "n", "z"]);
        assert_eq!(
            slice(src, &spans, 0),
            "@decorator\ndef a(x):\n    def inner():\n        return 1\n    return inner"
        );
        assert_eq!(slice(src, &spans, 1), "    def inner():\n        return 1");
        assert!(slice(src, &spans, 2).starts_with("    def m(self,\n          y):"));
        assert!(slice(src, &spans, 2).ends_with("        return y"));
        assert_eq!(
            slice(src, &spans, 3),
            "    async def n(self):\n        pass"
        );
        assert_eq!(slice(src, &spans, 4), "def z(): pass");
        assert_eq!(spans[0].signature_line, "def a(x):");
    }

    #[test]
    fn other_kinds_have_no_functions() {
        assert!(function_spans("fn a() {}", K::Text).is_empty());
        assert!(function_spans("{\"a\": 1}", K::Json).is_empty());
    }
}
