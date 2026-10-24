//! Cognitive complexity per function for code cards (Text ▾ › Complexity): an approximation
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// UTF-16 offset of the start of `line` (1-based) in `text`, clamped to its end.
pub fn line_offset_utf16(text: &str, line: usize) -> usize {
    let mut seen = 1;
    let mut offset = 0;
    for ch in text.chars() {
        if seen >= line {
            break;
        }
        offset += ch.len_utf16();
        if ch == '\n' {
            seen += 1;
        }
    }
    offset
}

/// Larger texts are not scored: the card says so instead.
pub const MAX_BYTES: usize = 512 * 1024;
/// More lines than this are not scored either.
pub const MAX_LINES: usize = 20_000;

/// Why there is no score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Over [`MAX_BYTES`] or [`MAX_LINES`].
    TooLarge,
    /// The scanner panicked (a bug); logged as an event with the language and size only.
    Failed,
}

/// [`analyze`] within the limits, with any panic caught: the card never goes down with it.
pub fn analyze_guarded(text: &str, lang: Lang) -> Result<Vec<FnScore>, Refused> {
    if text.len() > MAX_BYTES || text.lines().nth(MAX_LINES).is_some() {
        crate::diag::log(crate::diag::LogEvent::ComplexityTooLarge { bytes: text.len() });
        return Err(Refused::TooLarge);
    }
    std::panic::catch_unwind(|| analyze(text, lang)).map_err(|_| {
        crate::diag::log(crate::diag::LogEvent::ComplexityFailed { bytes: text.len() });
        Refused::Failed
    })
}
