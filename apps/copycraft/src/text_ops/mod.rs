//! Unix-style line tools for the Text ▾ menu: first and last lines (head, tail),
//! reverse (tac) and one column (awk '{print $N}').
//!
//! Platform-independent and allocation-light: head and tail slice the text, so line endings
//! (LF or CRLF) come back exactly as copied. The other steps keep the text's own line ending
//! and its final newline, if it had one.

/// What a Text ▾ job on its thread found, for the main thread.
#[derive(Clone)]
pub enum TextResult {
    /// The step's text; `None` when it would leave nothing or change nothing.
    Step {
        op: TextOp,
        text: Option<zeroize::Zeroizing<String>>,
    },
    /// A decode the text did not fit: why, without the text.
    Invalid {
        codec: encoding::Codec,
        reason: encoding::Invalid,
    },
}

/// Copied text is never printed.
impl std::fmt::Debug for TextResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextResult::Step { text, .. } => write!(f, "Step(changed: {})", text.is_some()),
            TextResult::Invalid { codec, reason } => write!(f, "Invalid({codec:?}, {reason:?})"),
        }
    }
}

impl TextResult {
    /// Work `op` out on `text`. Runs on a background thread.
    pub fn work(text: &str, op: &TextOp) -> Self {
        if let TextOp::Encode(codec) = op
            && let Err(reason) = codec.apply(text)
        {
            return TextResult::Invalid {
                codec: *codec,
                reason,
            };
        }
        let out = op
            .apply(text)
            .filter(|out| out.as_str() != text)
            .map(zeroize::Zeroizing::new);
        TextResult::Step {
            op: op.clone(),
            text: out,
        }
    }
}

/// A step that makes a new copy of the text (⌘Z goes back to the one it was taken on).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum TextOp {
    /// The first `n` lines.
    Head(usize),
    /// The last `n` lines.
    Tail(usize),
    /// The lines in reverse order.
    Reverse,
    /// Field `n` (1-based) of every line that has it.
    Column(Separator, usize),
    /// The lines `pattern` matches (`invert`: does not match), like grep.
    Filter {
        pattern: String,
        ignore_case: bool,
        invert: bool,
    },
    /// Agent instructions: their shell commands, one per line ([`crate::agent::commands`]).
    AgentCommands,
    /// Tracking parameters out of every URL ([`url_clean`]).
    CleanUrls,
    /// The JSON laid out again ([`json_style`]).
    Json(json_style::JsonStyle),
    /// Every line in a case style ([`case_style`]).
    Case(case_style::CaseStyle),
    /// The whole text encoded or decoded ([`encoding`]).
    Encode(encoding::Codec),
    /// The whole text's SHA-256, SHA-1 or MD5 as lowercase hex ([`hash`]).
    Hash(hash::HashKind),
    /// YAML, XML or SQL formatted or minified, JSON ⇄ YAML ([`code_style`]).
    Code(code_style::CodeOp),
}

impl TextOp {
    /// The step's text, or `None` when it would leave nothing.
    pub fn apply(&self, text: &str) -> Option<String> {
        let out = match self {
            TextOp::Head(n) => head(text, *n).to_string(),
            TextOp::Tail(n) => tail(text, *n).to_string(),
            TextOp::Reverse => reverse(text),
            TextOp::Column(separator, n) => column(text, *separator, *n),
            TextOp::Filter {
                pattern,
                ignore_case,
                invert,
            } => {
                let compiled = filter::LineFilter::new(pattern, *ignore_case, *invert).ok()?;
                filter::filter(text, &compiled)
            }
            TextOp::AgentCommands => {
                let commands = crate::agent::commands(text);
                crate::diag::log(crate::diag::LogEvent::AgentCommands {
                    count: commands.len(),
                });
                commands.join("\n")
            }
            TextOp::CleanUrls => url_clean::clean(text).text,
            TextOp::Json(style) => json_style::restyle(text, *style).ok()?,
            TextOp::Case(style) => case_style::convert(text, *style),
            TextOp::Encode(codec) => codec.apply(text).ok()?,
            TextOp::Hash(kind) => kind.hex(text),
            TextOp::Code(op) => op.apply(text)?,
        };
        (!out.trim().is_empty()).then_some(out)
    }
}

/// How a line splits into columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Tab,
    Comma,
    Semicolon,
    Pipe,
    /// Runs of spaces and tabs, like awk's default.
    Whitespace,
}

impl Separator {
    /// In the order of the separator pop-up.
    pub const ALL: [Separator; 5] = [
        Separator::Tab,
        Separator::Comma,
        Separator::Semicolon,
        Separator::Pipe,
        Separator::Whitespace,
    ];

    fn char(self) -> Option<char> {
        match self {
            Separator::Tab => Some('\t'),
            Separator::Comma => Some(','),
            Separator::Semicolon => Some(';'),
            Separator::Pipe => Some('|'),
            Separator::Whitespace => None,
        }
    }

    /// Its name in the pop-up.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn title(self) -> &'static str {
        match self {
            Separator::Tab => crate::locale::t("sep_tab"),
            Separator::Comma => crate::locale::t("sep_comma"),
            Separator::Semicolon => crate::locale::t("sep_semicolon"),
            Separator::Pipe => crate::locale::t("sep_pipe"),
            Separator::Whitespace => crate::locale::t("sep_whitespace"),
        }
    }
}

/// Lines looked at to guess the separator and to fill the preview.
const SAMPLE_LINES: usize = 50;

/// The separator of `text`: tab, comma, semicolon or pipe when each of the first non-empty
/// lines has it the same number of times (the most often wins), else runs of whitespace.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn detect_separator(text: &str) -> Separator {
    let sample: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(SAMPLE_LINES)
        .collect();
    let mut best: Option<(Separator, usize)> = None;
    for separator in Separator::ALL {
        let Some(sep) = separator.char() else {
            continue;
        };
        let Some(first) = sample.first() else {
            break;
        };
        let per_line = first.matches(sep).count();
        if per_line == 0
            || sample
                .iter()
                .any(|line| line.matches(sep).count() != per_line)
        {
            continue;
        }
        if best.is_none_or(|(_, most)| per_line > most) {
            best = Some((separator, per_line));
        }
    }
    best.map_or(Separator::Whitespace, |(separator, _)| separator)
}

/// The first `n` lines, line endings included.
pub fn head(text: &str, n: usize) -> &str {
    if n == 0 {
        return "";
    }
    match text.match_indices('\n').nth(n - 1) {
        Some((at, _)) => &text[..=at],
        None => text,
    }
}

/// The last `n` lines, line endings included.
pub fn tail(text: &str, n: usize) -> &str {
    if n == 0 {
        return "";
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    match body.rmatch_indices('\n').nth(n - 1) {
        Some((at, _)) => &text[at + 1..],
        None => text,
    }
}

/// The line ending `text` uses (CRLF when its first line ends so) and whether it ends with one.
fn ending(text: &str) -> (&'static str, bool) {
    let crlf = text.find('\n').is_some_and(|at| text[..at].ends_with('\r'));
    (if crlf { "\r\n" } else { "\n" }, text.ends_with('\n'))
}

pub(crate) fn join<'a>(text: &str, lines: impl Iterator<Item = &'a str>) -> String {
    let (newline, trailing) = ending(text);
    let mut out = String::with_capacity(text.len());
    for (index, line) in lines.enumerate() {
        if index > 0 {
            out.push_str(newline);
        }
        out.push_str(line);
    }
    if trailing && !out.is_empty() {
        out.push_str(newline);
    }
    out
}

/// The lines in reverse order, in the text's own line ending.
pub fn reverse(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    join(text, lines.into_iter().rev())
}

/// Field `n` (1-based) of each line that has it.
pub fn column_lines(text: &str, separator: Separator, n: usize) -> impl Iterator<Item = &str> {
    text.lines()
        .filter_map(move |line| field(line, separator, n))
}

fn field(line: &str, separator: Separator, n: usize) -> Option<&str> {
    let index = n.checked_sub(1)?;
    match separator.char() {
        Some(sep) => line.split(sep).nth(index),
        None => line.split_whitespace().nth(index),
    }
}

/// Field `n` (1-based) of each line that has it, in the text's own line ending.
pub fn column(text: &str, separator: Separator, n: usize) -> String {
    join(text, column_lines(text, separator, n))
}

/// The first lines of [`column`], for the live preview; quick on a long text.
pub fn column_preview(text: &str, separator: Separator, n: usize, lines: usize) -> String {
    text.lines()
        .take(SAMPLE_LINES.max(lines))
        .filter_map(|line| field(line, separator, n))
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A typed line count or column number: a whole number from 1.
pub fn parse_count(typed: &str) -> Option<usize> {
    typed.trim().parse::<usize>().ok().filter(|&n| n > 0)
}

pub mod case_style;
pub mod code_style;
pub mod curl;
pub mod encoding;
pub mod filter;
pub mod hash;
pub mod json_style;
pub mod pager;
pub mod url_clean;

#[cfg(test)]
mod tests;
