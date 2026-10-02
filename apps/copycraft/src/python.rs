//! Python: detection by scoring Python-only hints, and a structural check for the card title
//! (brackets via [`crate::brackets`], a block header without an indented body, tabs mixed with
//! spaces). There is no Python formatter; the card shows the copy as-is, highlighted.

use std::collections::BTreeSet;

use crate::format::FormatKind;

/// Score a copy needs to count as Python, with at least one strong hint.
///
/// Strong hints (2 each): `def f(…):`, `class C:` / `class C(B):`, `import x`, `from x import y`,
/// `elif …:`, `with open(`, `if __name__`, a `python` shebang. Weak hints (1 each): `self`,
/// `None` / `True` / `False`, a line ending in `:` followed by a deeper indented line, `print(`,
/// an f-string, a `#` comment line, `try:` / `except …:`, a decorator line. Each hint counts once
/// however often it appears. Signs of other languages (a line ending in `;`, `function`, `var` /
/// `let` / `const`, `=>`, `console.`, `puts`, `require`, `elsif`, a bare `end`, `do |…|`, a
/// Markdown code fence) cost 3 each.
///
/// So a lone `f = open("demofile.txt")` (no hint) or `print("hi")` (1) stays plain text, while a
/// `def` with an indented body (2 + 1) or `import os` plus a `print(` call (2 + 1) is Python.
pub const THRESHOLD: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Hint {
    Def,
    Class,
    Import,
    FromImport,
    Elif,
    WithOpen,
    MainGuard,
    Shebang,
    SelfRef,
    Constant,
    Block,
    Print,
    FString,
    Comment,
    TryExcept,
    Decorator,
    /// A line of another language; counted per line, not as a hint.
    Foreign,
}

impl Hint {
    fn weight(self) -> i32 {
        match self {
            Self::Def
            | Self::Class
            | Self::Import
            | Self::FromImport
            | Self::Elif
            | Self::WithOpen
            | Self::MainGuard
            | Self::Shebang => 2,
            Self::Foreign => -3,
            _ => 1,
        }
    }

    fn strong(self) -> bool {
        self.weight() == 2
    }
}

/// `true` when `text` scores at least [`THRESHOLD`] with a strong hint.
pub fn looks_like_python(text: &str) -> bool {
    let hints = hints(text);
    // Python hints count once each; every foreign line counts.
    let score = hints.iter().map(|hint| hint.weight()).sum::<i32>()
        + Hint::Foreign.weight() * foreign_lines(text) as i32;
    score >= THRESHOLD && hints.iter().any(|hint| hint.strong())
}

fn hints(text: &str) -> BTreeSet<Hint> {
    let mut found = BTreeSet::new();
    let lines: Vec<&str> = text.lines().collect();
    if lines
        .first()
        .is_some_and(|first| first.starts_with("#!") && first.contains("python"))
    {
        found.insert(Hint::Shebang);
    }
    for (index, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            if !t.starts_with("#!") {
                found.insert(Hint::Comment);
            }
            continue;
        }
        if is_def(t) {
            found.insert(Hint::Def);
        }
        if is_class(t) {
            found.insert(Hint::Class);
        }
        if is_import(t) {
            found.insert(Hint::Import);
        }
        if is_from_import(t) {
            found.insert(Hint::FromImport);
        }
        if t.starts_with("elif ") && t.ends_with(':') {
            found.insert(Hint::Elif);
        }
        if t.starts_with("with open(") {
            found.insert(Hint::WithOpen);
        }
        if t.starts_with("if __name__") {
            found.insert(Hint::MainGuard);
        }
        if t.contains("self.") || t.contains("(self") {
            found.insert(Hint::SelfRef);
        }
        if ["None", "True", "False"]
            .iter()
            .any(|word| has_word(t, word))
        {
            found.insert(Hint::Constant);
        }
        if t.ends_with(':') && next_is_deeper(&lines, index) {
            found.insert(Hint::Block);
        }
        if has_call(t, "print") {
            found.insert(Hint::Print);
        }
        if has_fstring(t) {
            found.insert(Hint::FString);
        }
        if t == "try:" || (t.starts_with("except") && t.ends_with(':')) {
            found.insert(Hint::TryExcept);
        }
        if t.starts_with('@')
            && t[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && !t.contains(' ')
        {
            found.insert(Hint::Decorator);
        }
    }
    found
}

fn foreign_lines(text: &str) -> usize {
    text.lines().filter(|line| is_foreign(line.trim())).count()
}

/// Lines that are not Python: JS, Ruby, C-family or a Markdown fence.
fn is_foreign(t: &str) -> bool {
    const STARTS: [&str; 13] = [
        "function ",
        "var ",
        "let ",
        "const ",
        "console.",
        "puts ",
        "require ",
        "require(",
        "elsif ",
        "#include",
        "<?php",
        "```",
        "~~~",
    ];
    t.ends_with(';')
        || STARTS.iter().any(|start| t.starts_with(start))
        || t == "end"
        || t.contains("=>")
        || t.contains(" do |")
        || t.ends_with(" do")
}

fn ident_end(s: &str) -> usize {
    s.find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(s.len())
}

fn is_def(t: &str) -> bool {
    let rest = t
        .strip_prefix("async def ")
        .or_else(|| t.strip_prefix("def "));
    let Some(rest) = rest else {
        return false;
    };
    let end = ident_end(rest);
    end > 0 && rest[end..].starts_with('(') && (t.ends_with(':') || t.contains("):"))
}

fn is_class(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("class ") else {
        return false;
    };
    let end = ident_end(rest);
    let after = &rest[end..];
    end > 0 && (after == ":" || (after.starts_with('(') && after.ends_with("):")))
}

fn is_module_path(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// `import os`, `import os.path, sys`, `import numpy as np`.
fn is_import(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("import ") else {
        return false;
    };
    rest.split(',').all(|part| {
        let mut words = part.split_whitespace();
        match (words.next(), words.next(), words.next(), words.next()) {
            (Some(module), None, _, _) => is_module_path(module),
            (Some(module), Some("as"), Some(alias), None) => {
                is_module_path(module) && ident_end(alias) == alias.len()
            }
            _ => false,
        }
    })
}

/// `from os import path`, `from . import x`, `from x import (a, b)`, `from x import *`.
fn is_from_import(t: &str) -> bool {
    let Some(rest) = t.strip_prefix("from ") else {
        return false;
    };
    let Some((module, names)) = rest.split_once(" import ") else {
        return false;
    };
    let names = names.trim();
    is_module_path(module)
        && !names.is_empty()
        && names
            .chars()
            .all(|c| c.is_alphanumeric() || " _,()*".contains(c))
}

fn has_word(t: &str, word: &str) -> bool {
    t.match_indices(word).any(|(at, _)| {
        let before = t[..at].chars().next_back();
        let after = t[at + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn has_call(t: &str, name: &str) -> bool {
    t.match_indices(&format!("{name}(")).any(|(at, _)| {
        !t[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
    })
}

fn has_fstring(t: &str) -> bool {
    let chars: Vec<char> = t.chars().collect();
    chars.windows(2).enumerate().any(|(i, pair)| {
        matches!(pair[0], 'f' | 'F')
            && matches!(pair[1], '"' | '\'')
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
    })
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// The next non-blank, non-comment line after `index` is indented deeper than it.
fn next_is_deeper(lines: &[&str], index: usize) -> bool {
    let here = indent_of(lines[index]);
    lines[index + 1..]
        .iter()
        .find(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .is_some_and(|next| {
            let there = indent_of(next);
            there.len() > here.len() && there.starts_with(here)
        })
}

/// A structural problem in Python code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Bracket(crate::brackets::Problem),
    /// A block header (`def`, `if`, `for`, …) ending in `:` with no deeper indented body.
    ExpectedIndent,
    /// Indentation uses tabs on some lines and spaces on others (or both on one line).
    MixedIndent,
}

impl Problem {
    /// Title suffix: `missing )`, `expected indent`, `mixed tabs and spaces`.
    pub fn label(self) -> String {
        match self {
            Self::Bracket(problem) => problem.label(),
            Self::ExpectedIndent => "expected indent".to_string(),
            Self::MixedIndent => "mixed tabs and spaces".to_string(),
        }
    }
}

const BLOCK_KEYWORDS: [&str; 14] = [
    "if", "elif", "else", "for", "while", "def", "class", "try", "except", "finally", "with",
    "async", "match", "case",
];

/// First problem in Python `source`: brackets, then tabs mixed with spaces, then a block header
/// without a body. `None` when it looks structurally fine.
pub fn check(source: &str) -> Option<Problem> {
    if let Some(problem) = crate::brackets::check(source, FormatKind::Python) {
        return Some(Problem::Bracket(problem));
    }
    let lines = code_lines(source);
    let indented = lines
        .iter()
        .filter(|line| line.starts_clean && !line.code.trim().is_empty())
        .map(|line| indent_of(&line.code));
    let (mut tabs, mut spaces) = (false, false);
    for indent in indented {
        tabs |= indent.contains('\t');
        spaces |= indent.contains(' ');
    }
    if tabs && spaces {
        return Some(Problem::MixedIndent);
    }
    for (index, line) in lines.iter().enumerate() {
        let code = line.code.trim();
        if !line.starts_clean || !code.ends_with(':') {
            continue;
        }
        let first = &code[..ident_end(code)];
        if !BLOCK_KEYWORDS.contains(&first) {
            continue;
        }
        let here = indent_of(&line.code);
        let body = lines[index + 1..]
            .iter()
            .find(|next| !next.code.trim().is_empty());
        let has_body = body.is_some_and(|next| {
            let there = indent_of(&next.code);
            there.len() > here.len() && there.starts_with(here)
        });
        if !has_body {
            return Some(Problem::ExpectedIndent);
        }
    }
    None
}

/// One source line with comments dropped and string contents blanked (`""`), plus whether it
/// starts outside brackets and strings (a logical line of its own).
struct CodeLine {
    starts_clean: bool,
    code: String,
}

fn code_lines(source: &str) -> Vec<CodeLine> {
    let chars: Vec<char> = source.chars().collect();
    let mut lines = Vec::new();
    let mut code = String::new();
    let mut starts_clean = true;
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\n' => {
                lines.push(CodeLine {
                    starts_clean,
                    code: std::mem::take(&mut code),
                });
                starts_clean = depth == 0 && !code_continues(&lines);
                i += 1;
            }
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '"' | '\'' => {
                let end = crate::brackets::skip_python_string(&chars, i);
                // Newlines inside a triple-quoted string start lines that are not clean.
                let newlines = chars[i..end].iter().filter(|c| **c == '\n').count();
                code.push_str("\"\"");
                for _ in 0..newlines {
                    lines.push(CodeLine {
                        starts_clean,
                        code: std::mem::take(&mut code),
                    });
                    starts_clean = false;
                }
                i = end;
            }
            '(' | '[' | '{' => {
                depth += 1;
                code.push(ch);
                i += 1;
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                code.push(ch);
                i += 1;
            }
            _ => {
                code.push(ch);
                i += 1;
            }
        }
    }
    lines.push(CodeLine { starts_clean, code });
    lines
}

/// The line just ended with a `\` continuation.
fn code_continues(lines: &[CodeLine]) -> bool {
    lines
        .last()
        .is_some_and(|line| line.code.trim_end().ends_with('\\'))
}

#[cfg(test)]
mod tests {
    use super::{Problem, check, looks_like_python};
    use crate::brackets::Problem as Bracket;

    #[test]
    fn marcels_one_liner_is_not_enough() {
        assert!(!looks_like_python("f = open(\"demofile.txt\")"));
        assert!(!looks_like_python("print(\"hi\")"));
        assert!(!looks_like_python("import os"));
    }

    #[test]
    fn python_snippets_are_python() {
        for src in [
            "def greet(name):\n    print(f\"Hello {name}\")\n",
            "import os\n\nprint(os.getcwd())\n",
            "with open(\"demofile.txt\") as f:\n    print(f.read())\n",
            "class Dog(Animal):\n    def __init__(self, name):\n        self.name = name\n",
            "from pathlib import Path\n\nfor p in Path(\".\").iterdir():\n    print(p)\n",
            "#!/usr/bin/env python3\nimport sys\nsys.exit(main())\n",
            "if x > 1:\n    y = 2\nelif x == 1:\n    y = None\nelse:\n    y = 0\n",
            "@app.route(\"/\")\ndef index():\n    return \"hi\"\n",
        ] {
            assert!(looks_like_python(src), "{src}");
        }
    }

    #[test]
    fn ruby_is_not_python() {
        for src in [
            "class Dog\n  def initialize(name)\n    @name = name\n  end\nend\n",
            "require 'json'\n\ndef greet(name)\n  puts \"Hello #{name}\"\nend\n",
            "items.each do |item|\n  puts item\nend\n",
            "if x > 1\n  y = 2\nelsif x == 1\n  y = nil\nend\n",
        ] {
            assert!(!looks_like_python(src), "{src}");
        }
    }

    #[test]
    fn javascript_is_not_python() {
        for src in [
            "import React from 'react';\n\nfunction App() {\n  return null;\n}\n",
            "const add = (a, b) => a + b;\nconsole.log(add(1, 2));\n",
            "import fs from \"fs\"\nconst data = fs.readFileSync(\"x\")\n",
            "class Dog {\n  constructor(name) {\n    this.name = name;\n  }\n}\n",
        ] {
            assert!(!looks_like_python(src), "{src}");
        }
    }

    #[test]
    fn yaml_is_not_python() {
        for src in [
            "name: app\nservices:\n  web:\n    image: nginx\n    debug: True\n",
            "# config\nimport:\n  - base.yaml\nclass: worker\nfrom: here\n",
            "steps:\n  - run: print(\"hi\")\n  - with: open\n",
        ] {
            assert!(!looks_like_python(src), "{src}");
        }
    }

    #[test]
    fn prose_is_not_python() {
        for src in [
            "Please import the data from the old system.\nThen print it and send it to Jan.\n",
            "From here import duties apply.\nNone of this is True or False:\n  it is policy.\n",
            "def: a definition.\nclass: a group of things.\n",
            "# Shopping\n\n- milk\n- bread\n",
        ] {
            assert!(!looks_like_python(src), "{src}");
        }
    }

    #[test]
    fn structure_problems_are_named() {
        assert_eq!(
            check("def f(x):\n    return max(x, 1\n"),
            Some(Problem::Bracket(Bracket::Missing(')')))
        );
        assert_eq!(Problem::Bracket(Bracket::Missing(')')).label(), "missing )");
        assert_eq!(
            check("def f(x):\nreturn x\n"),
            Some(Problem::ExpectedIndent)
        );
        assert_eq!(check("for i in range(3):\n"), Some(Problem::ExpectedIndent));
        assert_eq!(
            check("if x:\n    # only a comment\ny = 1\n"),
            Some(Problem::ExpectedIndent)
        );
        assert_eq!(Problem::ExpectedIndent.label(), "expected indent");
        assert_eq!(
            check("def f():\n    a = 1\n\treturn a\n"),
            Some(Problem::MixedIndent)
        );
        assert_eq!(Problem::MixedIndent.label(), "mixed tabs and spaces");
    }

    #[test]
    fn sound_python_has_no_problem() {
        for src in [
            "def f(x):\n    return x\n",
            "if x: y = 1\n",
            "d = {\n    \"a\":\n        1,\n}\nxs = items[1:]\nf = lambda: 0\n",
            "def f():\n    \"\"\"Docs:\nnot code:\n\"\"\"\n    return 1\n",
            "total = a + \\\n    b\nclass C:\n\tpass\n",
            "s = \"if x:\"  # else:\n",
        ] {
            assert_eq!(check(src), None, "{src}");
        }
    }
}
