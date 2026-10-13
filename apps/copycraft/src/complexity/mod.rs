//! Cognitive complexity per function for code cards (Lines… › Complexity): an approximation
//! of SonarSource's metric in one lexical pass, no parser.
//!
//! One generic core per block style ([`brace`] for Rust and Java, [`python`] for indentation)
//! and a small rule table per language ([`Lang`]). Increments: `if`, `else if`/`elif`, `else`,
//! the ternary, `switch`/`match`, loops, `catch`/`except`, labelled `break`/`continue`, each
//! sequence of mixed `&&`/`||` (`and`/`or`) and a direct recursive call; structures and
//! lambdas/closures nest, and a nested structure costs one more per level.

mod brace;
mod lex;
mod python;
#[cfg(test)]
mod tests;

use crate::format::FormatKind;

/// The programming languages the card detects. Data and markup formats have no functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Java,
    Python,
}

impl Lang {
    pub fn of(kind: FormatKind) -> Option<Lang> {
        match kind {
            FormatKind::Rust => Some(Lang::Rust),
            FormatKind::Java => Some(Lang::Java),
            FormatKind::Python => Some(Lang::Python),
            _ => None,
        }
    }
}

/// One function or method: its name, first and last line (1-based) and score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnScore {
    pub name: String,
    pub first_line: usize,
    pub last_line: usize,
    pub score: u32,
}

/// Up to 15 is fine, 16 to 25 high, above 25 very high.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Normal,
    High,
    VeryHigh,
}

impl Band {
    pub fn of(score: u32) -> Band {
        match score {
            0..=15 => Band::Normal,
            16..=25 => Band::High,
            _ => Band::VeryHigh,
        }
    }
}

/// The functions of `text` in `lang`, highest score first (then by line).
pub fn analyze(text: &str, lang: Lang) -> Vec<FnScore> {
    let tokens = lex::tokens(text, lang);
    let mut scores = match lang {
        Lang::Rust | Lang::Java => brace::score(&tokens, lang),
        Lang::Python => python::score(&tokens),
    };
    scores.sort_by(|a, b| b.score.cmp(&a.score).then(a.first_line.cmp(&b.first_line)));
    scores
}

/// Sum and average (one decimal) of `scores`.
pub fn total(scores: &[FnScore]) -> (u32, f64) {
    let sum: u32 = scores.iter().map(|s| s.score).sum();
    let avg = if scores.is_empty() {
        0.0
    } else {
        (f64::from(sum) / scores.len() as f64 * 10.0).round() / 10.0
    };
    (sum, avg)
}
