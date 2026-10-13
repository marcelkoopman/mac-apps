//! The core for indentation: Python.

use super::FnScore;
use super::lex::{K, Tok};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A block, by the indent of its header line.
    Block(usize),
    /// A lambda, until its parentheses close, a `,` at this depth or the line ends.
    Lambda(usize),
}

struct Open {
    name: String,
    first_line: usize,
    last_line: usize,
    score: u32,
    indent: usize,
    base: usize,
}

fn close(open: Open, done: &mut Vec<FnScore>) {
    done.push(FnScore {
        name: open.name,
        first_line: open.first_line,
        last_line: open.last_line,
        score: open.score,
    });
}

pub(super) fn score(t: &[Tok]) -> Vec<FnScore> {
    let mut done = Vec::new();
    let mut funcs: Vec<Open> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut parens = 0usize;
    let mut bools: Vec<Option<&'static str>> = vec![None];
    for (i, tok) in t.iter().enumerate() {
        let statement = tok.first && parens == 0;
        if statement {
            frames.retain(|f| matches!(f, Frame::Block(ind) if *ind < tok.indent));
            while funcs.last().is_some_and(|f| tok.indent <= f.indent) {
                if let Some(open) = funcs.pop() {
                    close(open, &mut done);
                }
            }
            bools = vec![None];
        }
        let nesting = funcs
            .last()
            .map_or(0, |f| frames.len().saturating_sub(f.base)) as u32;
        if let Some(open) = funcs.last_mut() {
            open.last_line = tok.line;
        }
        match &tok.k {
            K::P('(' | '[' | '{') => {
                parens += 1;
                bools.push(None);
            }
            K::P(')' | ']' | '}') => {
                frames.retain(|f| !matches!(f, Frame::Lambda(p) if *p >= parens));
                parens = parens.saturating_sub(1);
                if bools.len() > 1 {
                    bools.pop();
                }
            }
            K::P(',') => {
                frames.retain(|f| !matches!(f, Frame::Lambda(p) if *p == parens));
                if let Some(last) = bools.last_mut() {
                    *last = None;
                }
            }
            K::Word(w) => {
                let w = w.as_str();
                // `async def`, `async for`: the keyword after counts as the statement.
                let statement = statement
                    || (i > 0 && t[i - 1].is_word("async") && t[i - 1].first && parens == 0);
                let add = |n: u32, funcs: &mut Vec<Open>| {
                    if let Some(open) = funcs.last_mut() {
                        open.score += n;
                    }
                };
                match w {
                    "def" if statement => {
                        if let Some(name) = t.get(i + 1).and_then(Tok::word) {
                            let indent = if t[i].first {
                                tok.indent
                            } else {
                                t[i - 1].indent
                            };
                            funcs.push(Open {
                                name: name.to_string(),
                                first_line: tok.line,
                                last_line: tok.line,
                                score: 0,
                                indent,
                                base: frames.len(),
                            });
                        }
                    }
                    "if" | "for" | "while" | "except" if statement => {
                        add(1 + nesting, &mut funcs);
                        frames.push(Frame::Block(tok.indent));
                    }
                    "match"
                        if statement
                            && !t.get(i + 1).is_some_and(|n| {
                                n.is_p('=') || n.is_p('.') || n.is_p(':') || n.is_p(',')
                            }) =>
                    {
                        add(1 + nesting, &mut funcs);
                        frames.push(Frame::Block(tok.indent));
                    }
                    "elif" | "else" if statement => {
                        add(1, &mut funcs);
                        frames.push(Frame::Block(tok.indent));
                    }
                    // A conditional expression: `a if c else b`.
                    "if" if inline_else(t, i) => add(1 + nesting, &mut funcs),
                    "lambda" => frames.push(Frame::Lambda(parens)),
                    "and" | "or" => {
                        let op = if w == "and" { "and" } else { "or" };
                        let last = bools.last_mut().expect("never empty");
                        if *last != Some(op) {
                            *last = Some(op);
                            add(1, &mut funcs);
                        }
                    }
                    _ => {
                        let recursive = funcs.last().is_some_and(|f| f.name == w)
                            && t.get(i + 1).is_some_and(|n| n.is_p('('))
                            && !t.get(i.wrapping_sub(1)).is_some_and(|p| p.is_word("def"))
                            && match i.checked_sub(1).map(|p| &t[p].k) {
                                Some(K::P('.')) => i
                                    .checked_sub(2)
                                    .is_some_and(|p| matches!(t[p].word(), Some("self" | "cls"))),
                                _ => true,
                            };
                        if recursive {
                            add(1, &mut funcs);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    while let Some(open) = funcs.pop() {
        close(open, &mut done);
    }
    done
}

/// An `else` follows the `if` at `at` on the same logical line and bracket depth.
fn inline_else(t: &[Tok], at: usize) -> bool {
    let mut depth = 0i32;
    for tok in &t[at + 1..] {
        if tok.first && depth == 0 {
            return false;
        }
        match tok.k {
            K::P('(' | '[' | '{') => depth += 1,
            K::P(')' | ']' | '}') => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ if depth == 0 && tok.is_word("else") => return true,
            _ => {}
        }
    }
    false
}
