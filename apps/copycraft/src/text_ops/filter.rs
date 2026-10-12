//! Filter, like `grep` (and `grep -v`, `grep -i`): the lines a regular expression matches.
//!
//! The pattern is compiled with size limits ([`REGEX_SIZE_LIMIT`], [`DFA_SIZE_LIMIT`]), so a
//! heavy pattern is refused instead of growing without bound; the `regex` crate matches in
//! linear time, so no pattern can hang on a long text. A bad pattern is an error message, never
//! a panic.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::ops::Range;

use regex::{Regex, RegexBuilder};

use super::join;

/// Compiled program size a pattern may take.
pub const REGEX_SIZE_LIMIT: usize = 1 << 20;
/// Lazy DFA cache size a pattern may use.
pub const DFA_SIZE_LIMIT: usize = 2 << 20;

/// A compiled filter.
#[derive(Debug, Clone)]
pub struct LineFilter {
    regex: Regex,
    invert: bool,
}

impl LineFilter {
    /// `Err` holds a one-line reason (an invalid or too large pattern).
    pub fn new(pattern: &str, ignore_case: bool, invert: bool) -> Result<Self, String> {
        RegexBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(DFA_SIZE_LIMIT)
            .build()
            .map(|regex| Self { regex, invert })
            .map_err(|e| reason(&e))
    }

    /// The line stays: it matches (or, inverted, does not).
    pub fn keeps(&self, line: &str) -> bool {
        self.regex.is_match(line) != self.invert
    }

    /// Byte ranges of the matches in `line` to highlight. None for an inverted filter: its
    /// lines are the ones without a match.
    pub fn marks(&self, line: &str) -> Vec<Range<usize>> {
        if self.invert {
            return Vec::new();
        }
        self.regex
            .find_iter(line)
            .filter(|found| !found.is_empty())
            .map(|found| found.range())
            .collect()
    }
}

/// The last non-empty line of the error: the regex crate puts the pattern and a caret above
/// the reason.
fn reason(error: &regex::Error) -> String {
    match error {
        regex::Error::CompiledTooBig(_) => crate::locale::t("filter_too_big").to_string(),
        _ => {
            let text = error.to_string();
            text.lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or(&text)
                .trim()
                .trim_start_matches("error: ")
                .to_string()
        }
    }
}

/// The lines `filter` keeps, in the text's own line ending.
pub fn filter(text: &str, filter: &LineFilter) -> String {
    join(text, text.lines().filter(|line| filter.keeps(line)))
}

/// The first kept lines with their matches, for the live preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterPreview {
    /// The lines, joined with `\n`.
    pub text: String,
    /// Byte ranges of the matches in `text`.
    pub marks: Vec<Range<usize>>,
    /// Kept lines in the whole text.
    pub kept: usize,
}

/// The first `lines` kept lines of `text` and how many lines the filter keeps in all.
pub fn preview(text: &str, filter: &LineFilter, lines: usize) -> FilterPreview {
    let mut out = FilterPreview::default();
    for line in text.lines().filter(|line| filter.keeps(line)) {
        out.kept += 1;
        if out.kept > lines {
            continue;
        }
        if !out.text.is_empty() {
            out.text.push('\n');
        }
        let start = out.text.len();
        out.marks.extend(
            filter
                .marks(line)
                .into_iter()
                .map(|range| start + range.start..start + range.end),
        );
        out.text.push_str(line);
    }
    out
}
