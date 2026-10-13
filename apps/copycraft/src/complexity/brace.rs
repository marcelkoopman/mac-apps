//! The core for languages with `{}` blocks: Rust and Java.

use super::lex::{K, Tok};
use super::{FnScore, Lang};

/// Java words before `(` that make it a statement, not a method name.
const JAVA_NOT_NAMES: &[&str] = &[
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
];
/// Java words before a name that make it a call or an expression, not a declaration.
const JAVA_NOT_BEFORE: &[&str] = &[
    "return",
    "new",
    "throw",
    "else",
    "case",
    "assert",
    "yield",
    "do",
    "class",
    "interface",
    "enum",
    "record",
    "extends",
    "implements",
    "instanceof",
    "import",
    "package",
];
const JAVA_MODIFIERS: &[&str] = &[
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
];

/// Something that makes what is inside it one level deeper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A block, by the brace depth inside it.
    Brace(usize),
    /// An expression lambda or closure, until its parentheses close or a `,`/`;` at this
    /// paren depth.
    Expr(usize),
}

struct Open {
    name: String,
    first_line: usize,
    score: u32,
    /// Brace depth inside the body.
    body: usize,
    /// Frames open when the body started.
    base: usize,
}

/// Index of the `)` matching the `(` at `open`, if any.
fn matching_paren(t: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, tok) in t.iter().enumerate().skip(open) {
        match tok.k {
            K::P('(') => depth += 1,
            K::P(')') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(j);
                }
            }
            K::P('{' | '}' | ';') if depth > 0 && tok.k != K::P('{') => return None,
            _ => {}
        }
    }
    None
}

/// Is the Java name at `at` (followed by `(`) a method or constructor declaration with a body?
fn java_declaration(t: &[Tok], at: usize) -> bool {
    let Some(name) = t[at].word() else {
        return false;
    };
    if JAVA_NOT_NAMES.contains(&name) || !t.get(at + 1).is_some_and(|n| n.is_p('(')) {
        return false;
    }
    let before = at.checked_sub(1).map(|p| &t[p]);
    let typed = match before.map(|b| &b.k) {
        Some(K::Word(w)) => !JAVA_NOT_BEFORE.contains(&w.as_str()),
        Some(K::P('>' | ']')) => true,
        _ => false,
    };
    let constructor = match before.map(|b| &b.k) {
        None | Some(K::P('{' | '}' | ';')) => true,
        Some(K::Word(w)) => JAVA_MODIFIERS.contains(&w.as_str()),
        _ => false,
    };
    if !typed && !constructor {
        return false;
    }
    let Some(close) = matching_paren(t, at + 1) else {
        return false;
    };
    matches!(t.get(close + 1).map(|n| &n.k), Some(K::P('{')))
        || t.get(close + 1).is_some_and(|n| n.is_word("throws"))
}

/// Can a `|` or `||` at `at` start a Rust closure (an expression starts there)?
fn rust_closure_start(t: &[Tok], at: usize) -> bool {
    match at.checked_sub(1).map(|p| &t[p].k) {
        None => true,
        Some(K::P('(' | ',' | '=' | '{' | '[' | ';')) | Some(K::Op("=>")) => true,
        Some(K::Word(w)) => matches!(w.as_str(), "move" | "return" | "in"),
        _ => false,
    }
}

/// A binary `&&` / `||` follows an operand.
fn after_operand(t: &[Tok], at: usize) -> bool {
    match at.checked_sub(1).map(|p| &t[p].k) {
        Some(K::Word(w)) => !matches!(w.as_str(), "return" | "in" | "move"),
        Some(K::Lit | K::P(')' | ']')) => true,
        Some(K::P('?')) => true,
        _ => false,
    }
}

pub(super) fn score(t: &[Tok], lang: Lang) -> Vec<FnScore> {
    let mut done = Vec::new();
    let mut funcs: Vec<Open> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut braces = 0usize;
    let mut parens = 0usize;
    // A function whose body is the next `{`.
    let mut pending_fn: Option<(String, usize)> = None;
    // A structure whose block is the next `{`, by the paren depth it was seen at.
    let mut pending_nest: Option<usize> = None;
    let mut skip_if = false;
    let mut in_case = false;
    // The last boolean operator of the expression, per paren depth.
    let mut bools: Vec<Option<&'static str>> = vec![None];
    let mut i = 0;
    while i < t.len() {
        let tok = &t[i];
        let nesting = funcs
            .last()
            .map_or(0, |f| frames.len().saturating_sub(f.base)) as u32;
        let structure = |f: &mut Vec<Open>| {
            if let Some(open) = f.last_mut() {
                open.score += 1 + nesting;
            }
        };
        match &tok.k {
            K::P('{') => {
                braces += 1;
                if let Some((name, line)) = pending_fn.take() {
                    funcs.push(Open {
                        name,
                        first_line: line,
                        score: 0,
                        body: braces,
                        base: frames.len(),
                    });
                    pending_nest = None;
                } else if pending_nest.take().is_some() {
                    frames.push(Frame::Brace(braces));
                }
                bools.push(None);
            }
            K::P('}') => {
                while let Some(Frame::Brace(d)) = frames.last() {
                    if *d < braces {
                        break;
                    }
                    frames.pop();
                }
                if funcs.last().is_some_and(|f| f.body == braces)
                    && let Some(open) = funcs.pop()
                {
                    frames.truncate(open.base);
                    done.push(FnScore {
                        name: open.name,
                        first_line: open.first_line,
                        last_line: tok.line,
                        score: open.score,
                    });
                }
                braces = braces.saturating_sub(1);
                if bools.len() > 1 {
                    bools.pop();
                }
            }
            K::P('(') => {
                parens += 1;
                bools.push(None);
            }
            K::P(')') => {
                while let Some(Frame::Expr(p)) = frames.last() {
                    if *p < parens {
                        break;
                    }
                    frames.pop();
                }
                parens = parens.saturating_sub(1);
                if bools.len() > 1 {
                    bools.pop();
                }
            }
            K::P(',') | K::P(';') => {
                let semicolon = tok.is_p(';');
                while let Some(Frame::Expr(p)) = frames.last() {
                    if *p < parens || (!semicolon && *p != parens) {
                        break;
                    }
                    frames.pop();
                }
                if semicolon {
                    if pending_nest == Some(parens) {
                        pending_nest = None;
                    }
                    if parens == 0 {
                        pending_fn = None;
                    }
                }
                if let Some(last) = bools.last_mut() {
                    *last = None;
                }
            }
            K::P(':') if in_case => in_case = false,
            K::P('?') if lang == Lang::Java => {
                let wildcard = i
                    .checked_sub(1)
                    .is_some_and(|p| t[p].is_p('<') || t[p].is_p(','))
                    || t.get(i + 1)
                        .is_some_and(|n| n.is_p('>') || n.is_word("extends") || n.is_word("super"));
                if !wildcard {
                    structure(&mut funcs);
                }
            }
            K::Op("->") if lang == Lang::Java => {
                if in_case {
                    in_case = false;
                } else if t.get(i + 1).is_some_and(|n| n.is_p('{')) {
                    pending_nest = Some(parens);
                } else {
                    frames.push(Frame::Expr(parens));
                }
            }
            K::Op(op @ ("&&" | "||")) => {
                if lang == Lang::Rust && *op == "||" && rust_closure_start(t, i) {
                    rust_closure_body(t, i + 1, parens, &mut frames, &mut pending_nest);
                } else if after_operand(t, i) {
                    let last = bools.last_mut().expect("never empty");
                    if *last != Some(op) {
                        *last = Some(op);
                        if let Some(open) = funcs.last_mut() {
                            open.score += 1;
                        }
                    }
                }
            }
            K::P('|') if lang == Lang::Rust && rust_closure_start(t, i) => {
                // Past the parameters to the closing `|`.
                let mut j = i + 1;
                while j < t.len() && !t[j].is_p('|') && !t[j].is_p(';') && !t[j].is_p('{') {
                    j += 1;
                }
                if t.get(j).is_some_and(|n| n.is_p('|')) {
                    rust_closure_body(t, j + 1, parens, &mut frames, &mut pending_nest);
                    i = j;
                }
            }
            K::Word(w) => {
                let w = w.as_str();
                match w {
                    "fn" if lang == Lang::Rust => {
                        if let Some(name) = t.get(i + 1).and_then(Tok::word) {
                            pending_fn = Some((name.to_string(), tok.line));
                            pending_nest = None;
                            i += 2;
                            continue;
                        }
                    }
                    "if" => {
                        if std::mem::take(&mut skip_if) {
                        } else {
                            structure(&mut funcs);
                        }
                        pending_nest = Some(parens);
                    }
                    "else" => {
                        if let Some(open) = funcs.last_mut() {
                            open.score += 1;
                        }
                        skip_if = t.get(i + 1).is_some_and(|n| n.is_word("if"));
                        pending_nest = Some(parens);
                    }
                    "for" | "while" => {
                        // The `while (…);` closing a Java do-while is the do's.
                        let do_while = lang == Lang::Java
                            && i.checked_sub(1).is_some_and(|p| t[p].is_p('}'))
                            && t.get(i + 1).is_some_and(|n| n.is_p('('))
                            && matching_paren(t, i + 1)
                                .is_some_and(|c| t.get(c + 1).is_some_and(|n| n.is_p(';')));
                        if !do_while {
                            structure(&mut funcs);
                            pending_nest = Some(parens);
                        }
                    }
                    "loop" if lang == Lang::Rust => {
                        structure(&mut funcs);
                        pending_nest = Some(parens);
                    }
                    "match" if lang == Lang::Rust => {
                        structure(&mut funcs);
                        pending_nest = Some(parens);
                    }
                    "do" | "switch" | "catch" if lang == Lang::Java => {
                        structure(&mut funcs);
                        pending_nest = Some(parens);
                    }
                    "case" | "default" if lang == Lang::Java => {
                        // `default` before a method is a modifier; in a switch, a label.
                        in_case = t
                            .get(i + 1)
                            .is_some_and(|n| !(w == "default" && n.word().is_some()));
                    }
                    "break" | "continue" => {
                        let labelled = t
                            .get(i + 1)
                            .and_then(Tok::word)
                            .is_some_and(|label| lang == Lang::Java || label.starts_with('\''));
                        if labelled && let Some(open) = funcs.last_mut() {
                            open.score += 1;
                        }
                    }
                    "goto" => {
                        if let Some(open) = funcs.last_mut() {
                            open.score += 1;
                        }
                    }
                    _ => {
                        if lang == Lang::Java && java_declaration(t, i) {
                            pending_fn = Some((w.to_string(), tok.line));
                            pending_nest = None;
                        } else if t.get(i + 1).is_some_and(|n| n.is_p('('))
                            && let Some(open) = funcs.last_mut()
                            && open.name == w
                            && direct_call(t, i)
                        {
                            open.score += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    done
}

/// After a closure's parameters at `at`: a block body nests through its `{`, an expression
/// body until its expression ends.
fn rust_closure_body(
    t: &[Tok],
    mut at: usize,
    parens: usize,
    frames: &mut Vec<Frame>,
    pending_nest: &mut Option<usize>,
) {
    if t.get(at).is_some_and(|n| n.k == K::Op("->")) {
        while at < t.len() && !t[at].is_p('{') && !t[at].is_p(';') {
            at += 1;
        }
    }
    if t.get(at).is_some_and(|n| n.is_p('{')) {
        *pending_nest = Some(parens);
    } else {
        frames.push(Frame::Expr(parens));
    }
}

/// A call of the name at `at` on nothing, `this.`, `self.` or `Self::`: not on another object.
fn direct_call(t: &[Tok], at: usize) -> bool {
    match at.checked_sub(1).map(|p| &t[p].k) {
        Some(K::P('.')) | Some(K::Op("::")) => at
            .checked_sub(2)
            .is_some_and(|p| matches!(t[p].word(), Some("this" | "self" | "Self"))),
        Some(K::Word(w)) => matches!(w.as_str(), "return" | "else" | "in" | "await"),
        _ => true,
    }
}
