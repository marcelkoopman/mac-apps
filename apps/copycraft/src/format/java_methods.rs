//! Method counts per access level for a Java card's meta line: one lexical pass, no parser.

/// Declared methods (and constructors) by access level.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub public: usize,
    pub protected: usize,
    pub private: usize,
    /// No access modifier (package-private), or an interface method with the `default` keyword.
    pub default: usize,
}

impl Counts {
    /// "5 public · 2 private"; levels without methods are left out. `None` without methods.
    pub fn label(self) -> Option<String> {
        let parts: Vec<String> = [
            (self.public, "public"),
            (self.protected, "protected"),
            (self.private, "private"),
            (self.default, "default"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, name)| format!("{n} {name}"))
        .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok<'a> {
    Ident(&'a str),
    Punct(u8),
    /// `->`
    Arrow,
    /// A number or literal: never a name or a type.
    Other,
}

/// Tokens of `text` without comments, strings, char literals and text blocks.
fn tokens(text: &str) -> Vec<Tok<'_>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i += 2;
        } else if b[i..].starts_with(b"\"\"\"") {
            i += 3;
            while i < b.len() && !b[i..].starts_with(b"\"\"\"") {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i += 3;
            out.push(Tok::Other);
        } else if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i += 1;
            out.push(Tok::Other);
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 {
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'$') || b[i] >= 0x80)
            {
                i += 1;
            }
            out.push(Tok::Ident(&text[start..i]));
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'.' | b'_')) {
                i += 1;
            }
            out.push(Tok::Other);
        } else if c == b'-' && b.get(i + 1) == Some(&b'>') {
            out.push(Tok::Arrow);
            i += 2;
        } else {
            if !c.is_ascii_whitespace() {
                out.push(Tok::Punct(c));
            }
            i += 1;
        }
    }
    out
}

/// Words before `(` that make it a statement or an expression, not a declaration.
const NOT_NAMES: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "synchronized",
    "try",
    "return",
    "new",
    "throw",
    "assert",
    "yield",
    "super",
    "this",
    "case",
    "else",
    "do",
    "record",
];
/// Words before the name that make it a call or an expression.
const NOT_BEFORE: &[&str] = &[
    "return",
    "new",
    "throw",
    "else",
    "case",
    "assert",
    "yield",
    "do",
    "record",
    "class",
    "interface",
    "enum",
    "extends",
    "implements",
    "instanceof",
    "import",
    "package",
];
const MODIFIERS: &[&str] = &[
    "public",
    "protected",
    "private",
    "static",
    "final",
    "abstract",
    "synchronized",
    "native",
    "strictfp",
    "default",
    "sealed",
    "transient",
];

/// Count method and constructor declarations by access level.
pub fn count(text: &str) -> Counts {
    let toks = tokens(text);
    let mut counts = Counts::default();
    for (at, tok) in toks.iter().enumerate() {
        let (Tok::Punct(b'('), Some(&Tok::Ident(name))) =
            (tok, at.checked_sub(1).map(|p| &toks[p]))
        else {
            continue;
        };
        if NOT_NAMES.contains(&name) {
            continue;
        }
        let before = at.checked_sub(2).map(|p| toks[p]);
        let method = match before {
            Some(Tok::Ident(word)) => !NOT_BEFORE.contains(&word),
            Some(Tok::Punct(b'>' | b']')) => true,
            _ => false,
        };
        let constructor = !method
            && match before {
                None | Some(Tok::Punct(b'{' | b'}' | b';')) => true,
                Some(Tok::Ident(word)) => MODIFIERS.contains(&word),
                _ => false,
            };
        if !method && !constructor {
            continue;
        }
        // Past the matching `)`.
        let mut depth = 0usize;
        let mut end = at;
        while end < toks.len() {
            match toks[end] {
                Tok::Punct(b'(') => depth += 1,
                Tok::Punct(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Tok::Punct(b';' | b'{' | b'}') => break,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            continue;
        }
        let mut next = end + 1;
        // `int m()[]`, an old array return.
        while matches!(toks.get(next), Some(Tok::Punct(b'[' | b']'))) {
            next += 1;
        }
        let ok = match toks.get(next) {
            Some(Tok::Punct(b'{')) => true,
            Some(Tok::Punct(b';')) => method,
            Some(Tok::Ident("throws")) => true,
            _ => false,
        };
        if !ok {
            continue;
        }
        // The modifiers: back to the end of the previous member.
        let mut level = None;
        for t in toks[..at - 1].iter().rev() {
            match t {
                Tok::Punct(b';' | b'{' | b'}') => break,
                Tok::Ident(w @ ("public" | "protected" | "private" | "default")) => {
                    level = Some(*w);
                }
                _ => {}
            }
        }
        match level {
            Some("public") => counts.public += 1,
            Some("protected") => counts.protected += 1,
            Some("private") => counts.private += 1,
            _ => counts.default += 1,
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(public: usize, protected: usize, private: usize, default: usize) -> Counts {
        Counts {
            public,
            protected,
            private,
            default,
        }
    }

    #[test]
    fn counts_methods_per_access_level() {
        let src = r#"
package a.b;
import java.util.List;
public class Hi extends Base implements Runnable {
    private static final String S = "void fake() {}";
    public Hi(int a) throws Exception { super(a); this.x = a; }
    Hi() { this(1); }
    @Override
    public void run() {
        if (ok()) { go(1); }
        for (int i = 0; i < n(); i++) { }
        while (x(y)) {}
        switch (k) { case 1 -> foo(); default -> bar(); }
        synchronized (this) { }
        try { a(); } catch (Exception e) { }
        Runnable r = () -> { inner(); };
        Object o = new Object() { };
        List<String> l = new ArrayList<>();
        return;
    }
    protected <T> List<T> items(Class<T> type) { return List.of(); }
    private int[] arr() { return new int[0]; }
    static synchronized long count(String s) throws IOException, X { return 0; }
    @SuppressWarnings("unchecked") private char c() { return '('; }
}
"#;
        assert_eq!(count(src), c(2, 1, 2, 2));
    }

    #[test]
    fn interface_default_and_abstract_methods() {
        let src = "interface I {\n  void a();\n  default int b() { return 1; }\n  public abstract String c(int x);\n  static void d() {}\n}\n";
        assert_eq!(count(src), c(1, 0, 0, 3));
    }

    #[test]
    fn comments_strings_chars_and_text_blocks_are_skipped() {
        let src = "class A {\n// public void a() {}\n/* private void b() {} */\nString t = \"\"\"\n  public void c() {}\n  \"\"\";\nString s = \"protected void d() {}\";\nchar q = '\"';\npublic void e() {}\n}\n";
        assert_eq!(count(src), c(1, 0, 0, 0));
    }

    #[test]
    fn calls_are_not_declarations() {
        let src = "class A {\n void m() {\n  foo();\n  a.b(c);\n  int x = f(1) + g(2);\n  throw new E(\"x\");\n  String s = (String) h(z);\n }\n}\n";
        assert_eq!(count(src), c(0, 0, 0, 1));
    }

    #[test]
    fn records_and_enums_constructors() {
        let src = "public record P(int x, int y) {\n  public P { }\n  int sum() { return x + y; }\n}\nenum E { A(1), B(2); private E(int v) {} }\n";
        assert_eq!(count(src), c(0, 0, 1, 1));
    }

    #[test]
    fn the_label_leaves_out_empty_levels() {
        assert_eq!(
            c(5, 0, 2, 0).label().as_deref(),
            Some("5 public · 2 private")
        );
        assert_eq!(c(0, 0, 0, 0).label(), None);
        assert_eq!(
            c(1, 1, 1, 1).label().as_deref(),
            Some("1 public · 1 protected · 1 private · 1 default")
        );
    }

    #[test]
    fn a_java_card_meta_line_shows_the_levels() {
        let src = "public class Hi {\n    public void a() {}\n    private int b() { return 1; }\n    public static void main(String[] args) {}\n}\n";
        assert_eq!(crate::format::detect(src), crate::format::FormatKind::Java);
        let meta = crate::commands::text_meta(src);
        assert!(meta.contains("2 public · 1 private"), "{meta}");
    }
}
