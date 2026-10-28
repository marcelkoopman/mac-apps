//! Crash logs and logs: a macOS crash report (`Translated Report`, `Exception Type:`,
//! `Thread 0 Crashed`, stack frames `0x… name + 16`, or the `.ips` JSON form) and a log whose
//! lines start with a timestamp. Such a copy is titled *Crashlog* / *Log*, and the sensitivity
//! check finds no personal or financial data in it: a process id, `User ID: 501`, a timestamp
//! or an address is no person and no card. A real key or token still counts. Pure string work
//! on the first part of the text; nothing is logged.

use std::sync::OnceLock;

use regex::Regex;

/// Only the start of a copy is read: a crash report says what it is in its first lines.
const HEAD_BYTES: usize = 64 * 1024;
/// Non-empty lines read at most.
const HEAD_LINES: usize = 400;
/// A log needs this many timestamped lines…
const LOG_MIN_LINES: usize = 3;
/// …and this share of its non-empty lines, in percent.
const LOG_SHARE: usize = 60;

/// What kind of log a copy is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogKind {
    /// A crash report.
    Crash,
    /// Timestamped lines.
    Log,
}

impl LogKind {
    /// The card's title: *Crashlog* / *Crash log*, *Log*.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Crash => crate::locale::t("title_crash_log"),
            Self::Log => crate::locale::t("title_log"),
        }
    }
}

/// Lines that only a crash report starts with.
const STRONG: &[&str] = &[
    "Translated Report",
    "Incident Identifier:",
    "Exception Type:",
    "Exception Codes:",
    "Crashed Thread:",
    "Termination Reason:",
    "Triggered by Thread:",
    "Binary Images:",
];

/// Header fields of a crash report (and of some other reports).
const HEADERS: &[&str] = &[
    "Process:",
    "Path:",
    "Identifier:",
    "Version:",
    "Code Type:",
    "Parent Process:",
    "Responsible:",
    "Coalition:",
    "User ID:",
    "Date/Time:",
    "Launch Time:",
    "OS Version:",
    "Hardware Model:",
    "Report Version:",
    "Anonymous UUID:",
];

struct Patterns {
    crashed: Regex,
    frame: Regex,
    stamp: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        crashed: Regex::new(r"^Thread \d+ [Cc]rashed").expect("crashed pattern compiles"),
        // `0   AppKit   0x18e7d1a2c -[NSThemeFrame dealloc] + 64`
        frame: Regex::new(r"^\d+\s+\S.*\s0x[0-9a-fA-F]{4,}\s+\S.*\s\+\s\d+\s*$")
            .expect("frame pattern compiles"),
        // `2026-10-28 11:02:17.482113+0100 …`, `[2026-10-28T11:02:17Z] …`,
        // `Oct 28 11:02:17 host …`, `11:02:17.123 …`. A space after the stamp, not a tab or a
        // comma: those are a table's cells.
        stamp: Regex::new(concat!(
            r"^\[?(?:\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(?::\d{2}(?:[.,]\d+)?)?",
            r"(?:Z|[+-]\d{2}:?\d{2})?",
            r"|[A-Z][a-z]{2} [ \d]\d \d{2}:\d{2}:\d{2}",
            r"|\d{2}:\d{2}:\d{2}(?:[.,]\d+)?)\]? +\S"
        ))
        .expect("stamp pattern compiles"),
    })
}

/// The first part of `text`, cut at a character boundary.
fn head(text: &str) -> &str {
    if text.len() <= HEAD_BYTES {
        return text;
    }
    let mut end = HEAD_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Is `text` a crash report?
pub(crate) fn is_crash_log(text: &str) -> bool {
    let head = head(text);
    let trimmed = head.trim_start();
    // The `.ips` form: one JSON line of metadata, then the report as JSON.
    if trimmed.starts_with('{')
        && head.contains("\"bug_type\"")
        && (head.contains("\"exception\"") || head.contains("\"threads\""))
    {
        return true;
    }
    let patterns = patterns();
    let (mut strong, mut headers, mut frames) = (0usize, 0usize, 0usize);
    for line in head
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(HEAD_LINES)
    {
        if STRONG.iter().any(|marker| line.starts_with(marker)) || patterns.crashed.is_match(line) {
            strong += 1;
        } else if HEADERS.iter().any(|field| line.starts_with(field)) {
            headers += 1;
        } else if patterns.frame.is_match(line) {
            frames += 1;
        }
    }
    strong >= 2 || (strong >= 1 && (headers >= 2 || frames >= 2)) || (headers >= 3 && frames >= 3)
}

/// Is `text` a log: at least [`LOG_MIN_LINES`] lines, [`LOG_SHARE`] percent of them, that start
/// with a timestamp and go on with text (not `2026-04-16,3.17`, a CSV row)?
pub(crate) fn is_log(text: &str) -> bool {
    let stamp = &patterns().stamp;
    let (mut lines, mut stamped) = (0usize, 0usize);
    for line in head(text)
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .take(HEAD_LINES)
    {
        lines += 1;
        if stamp.is_match(line) {
            stamped += 1;
        }
    }
    stamped >= LOG_MIN_LINES && stamped * 100 >= lines * LOG_SHARE
}

/// What kind of log `text` is, if any. A crash report first: it has timestamps too.
pub(crate) fn detect(text: &str) -> Option<LogKind> {
    if is_crash_log(text) {
        Some(LogKind::Crash)
    } else if is_log(text) {
        Some(LogKind::Log)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "log_text_tests.rs"]
mod tests;
