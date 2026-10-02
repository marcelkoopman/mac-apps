//! Bracket balance of copied code (Rust, Java, Python). Brackets inside string and char literals
//! and comments do not count. The highlighter's tokenizer only knows Rust line comments and double
//! quoted strings, so this scanner also skips block comments, char literals, Rust raw strings,
//! Java text blocks and Python's `#` comments and `'`/`"`/triple-quoted strings.

use crate::format::FormatKind;

/// The first bracket problem in a piece of code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// An opener is never closed; holds the closer the innermost one needs.
    Missing(char),
    /// A closer without its opener, or closing a different kind of bracket.
    Unbalanced(char),
}

impl Problem {
    /// Short title suffix: `missing }` or `unbalanced )`.
    pub fn label(self) -> String {
        match self {
            Self::Missing(closer) => format!("missing {closer}"),
            Self::Unbalanced(closer) => format!("unbalanced {closer}"),
        }
    }
}

/// `None` when every `(`, `[` and `{` in `source` is closed in order, or `kind` is not code.
pub fn check(source: &str, kind: FormatKind) -> Option<Problem> {
    let nested_comments = match kind {
        FormatKind::Rust => true,
        FormatKind::Java => false,
        FormatKind::Python => return check_python(source),
        _ => return None,
    };
    let chars: Vec<char> = source.chars().collect();
    let mut stack: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();
        match ch {
            '/' if next == Some('/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if next == Some('*') => {
                i = skip_block_comment(&chars, i, nested_comments);
                continue;
            }
            '"' => {
                i = skip_string(&chars, i);
                continue;
            }
            '\'' => {
                i = skip_char_literal(&chars, i);
                continue;
            }
            'r' | 'b' | 'c' if kind == FormatKind::Rust && starts_word(&chars, i) => {
                if let Some(end) = skip_raw_string(&chars, i) {
                    i = end;
                    continue;
                }
            }
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            // The guard pops the innermost opener; a matching closer falls through to `_`.
            ')' | ']' | '}' if stack.pop() != Some(ch) => return Some(Problem::Unbalanced(ch)),
            _ => {}
        }
        i += 1;
    }
    stack.pop().map(Problem::Missing)
}

/// Python: `#` comments and `'`, `"`, `'''`, `"""` strings (any prefix such as `f`, `r`, `b`;
/// the letters before the quote are ordinary code). `//` is floor division, not a comment.
fn check_python(source: &str) -> Option<Problem> {
    let chars: Vec<char> = source.chars().collect();
    let mut stack: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '"' | '\'' => {
                i = skip_python_string(&chars, i);
                continue;
            }
            '(' => stack.push(')'),
            '[' => stack.push(']'),
            '{' => stack.push('}'),
            // The guard pops the innermost opener; a matching closer falls through to `_`.
            ')' | ']' | '}' if stack.pop() != Some(ch) => return Some(Problem::Unbalanced(ch)),
            _ => {}
        }
        i += 1;
    }
    stack.pop().map(Problem::Missing)
}

/// Index after the Python string opened at `start` with `'` or `"`, triple-quoted included.
/// A one-line string ends at the line end when its closing quote is missing.
pub(crate) fn skip_python_string(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let triple = chars.get(start + 1) == Some(&quote) && chars.get(start + 2) == Some(&quote);
    let mut i = start + if triple { 3 } else { 1 };
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '\n' if !triple => return i,
            c if c == quote && !triple => return i + 1,
            c if c == quote
                && chars.get(i + 1) == Some(&quote)
                && chars.get(i + 2) == Some(&quote) =>
            {
                return i + 3;
            }
            _ => i += 1,
        }
    }
    chars.len()
}

/// `true` when `chars[i]` does not continue an identifier.
fn starts_word(chars: &[char], i: usize) -> bool {
    i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
}

/// Index after the comment opened at `start` (`/*`); Rust comments nest.
fn skip_block_comment(chars: &[char], start: usize, nested: bool) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') && (nested || depth == 0) {
            depth += 1;
            i += 2;
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    i
}

/// Index after the string opened at `start`, a Java text block (`"""`) included.
fn skip_string(chars: &[char], start: usize) -> usize {
    let text_block = chars.get(start + 1) == Some(&'"') && chars.get(start + 2) == Some(&'"');
    let mut i = start + if text_block { 3 } else { 1 };
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' if !text_block => return i + 1,
            '"' if chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') => {
                return i + 3;
            }
            _ => i += 1,
        }
    }
    chars.len()
}

/// Index after the char literal at `start` (`'x'`, `'\n'`, `'\u{1F600}'`). A Rust lifetime or
/// label (`'a`) is just the quote.
fn skip_char_literal(chars: &[char], start: usize) -> usize {
    if chars.get(start + 1) == Some(&'\\') {
        let mut i = start + 3;
        while i < chars.len() && chars[i] != '\'' && chars[i] != '\n' {
            i += 1;
        }
        return (i + 1).min(chars.len());
    }
    if chars.get(start + 2) == Some(&'\'') {
        return start + 3;
    }
    start + 1
}

/// Index after a Rust raw string (`r"…"`, `r#"…"#`, `br"…"`, `cr"…"`) starting at `start`,
/// `None` when there is none.
fn skip_raw_string(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    if matches!(chars[i], 'b' | 'c') {
        i += 1;
    }
    if chars.get(i) != Some(&'r') {
        return None;
    }
    i += 1;
    let mut hashes = 0;
    while chars.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    if chars.get(i) != Some(&'"') {
        return None;
    }
    i += 1;
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|n| chars.get(i + n) == Some(&'#')) {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(chars.len())
}

#[cfg(test)]
mod tests {
    use super::{Problem, check};
    use crate::format::FormatKind::{Java, Rust};

    const MISSING_BRACE: &str = "\
public class Main {
  public static void main(String[] args) {
    System.out.println(\"Hello World\");
  }
";

    #[test]
    fn marcels_java_misses_the_final_brace() {
        assert_eq!(check(MISSING_BRACE, Java), Some(Problem::Missing('}')));
        assert_eq!(Problem::Missing('}').label(), "missing }");
    }

    #[test]
    fn balanced_code_has_no_problem() {
        let java = format!("{MISSING_BRACE}}}\n");
        assert_eq!(check(&java, Java), None);
        let rust = "fn main() {\n    let v = vec![(1, 2)];\n    println!(\"{v:?}\");\n}\n";
        assert_eq!(check(rust, Rust), None);
    }

    #[test]
    fn brackets_in_strings_comments_and_chars_do_not_count() {
        let java = r#"class A {
  // a stray } in a comment
  /* and ( here */
  String s = "} ) ]";
  String esc = "quote \" then {";
  char c = '{';
  char q = '\'';
  String block = """
      { not code
      """;
}"#;
        assert_eq!(check(java, Java), None);
        let rust = r##"fn main() {
    /* outer /* nested } */ still a comment ( */
    let s = "}";
    let r = r#"a "quoted" ) "#;
    let b = br"]";
    let c = '(';
    let e = '\u{7B}';
    let l: &'static str = "x";
    'outer: loop { break 'outer; }
}"##;
        assert_eq!(check(rust, Rust), None);
    }

    #[test]
    fn an_extra_closer_is_unbalanced() {
        let src = "fn main() {\n    run());\n}\n";
        assert_eq!(check(src, Rust), Some(Problem::Unbalanced(')')));
        assert_eq!(Problem::Unbalanced(')').label(), "unbalanced )");
        assert_eq!(check("class A {}\n}", Java), Some(Problem::Unbalanced('}')));
    }

    #[test]
    fn a_mismatched_pair_is_unbalanced() {
        assert_eq!(
            check("class A { int[] a = f(1]; }", Java),
            Some(Problem::Unbalanced(']'))
        );
        assert_eq!(
            check("fn f() { (1, 2} }", Rust),
            Some(Problem::Unbalanced('}'))
        );
    }

    #[test]
    fn the_innermost_open_bracket_is_reported() {
        assert_eq!(check("fn f() { g(1,", Rust), Some(Problem::Missing(')')));
    }

    #[test]
    fn python_strings_and_comments_do_not_count() {
        use crate::format::FormatKind::Python;
        let src = "def f(x):  # returns ) here\n    s = 'it\\'s ('\n    t = f\"{x} ]\"\n    doc = \"\"\"\n    { not code\n    \"\"\"\n    return x // 2, [s, t, doc]\n";
        assert_eq!(check(src, Python), None);
        assert_eq!(
            check("print(\"Hello\"\n", Python),
            Some(Problem::Missing(')'))
        );
        assert_eq!(
            check("x = [1, 2)]\n", Python),
            Some(Problem::Unbalanced(')'))
        );
    }

    #[test]
    fn other_kinds_are_not_checked() {
        assert_eq!(check("{", crate::format::FormatKind::Text), None);
        assert_eq!(check("{", crate::format::FormatKind::Json), None);
    }
}
