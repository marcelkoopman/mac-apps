//! Meta line formatting and command constructors.

use super::*;

pub(crate) fn image_meta(facts: &ImageFacts) -> String {
    let mut parts = Vec::new();
    if !facts.format.is_empty() {
        parts.push(facts.format.clone());
    }
    if facts.width > 0 && facts.height > 0 {
        parts.push(format!("{}×{}", facts.width, facts.height));
    }
    if facts.byte_len > 0 {
        parts.push(format_bytes(facts.byte_len));
    }
    parts.join("  ")
}

/// Fit `line` into `max_width` (measured by `width_of`). The part before the first `  ·  `
/// is shortened in the middle so the rest (format, dimensions) stays. When that rest alone
/// is wider than the line, the whole line is shortened in the middle. The result is one line.
pub(crate) fn fit_leading_name(
    line: &str,
    max_width: f64,
    width_of: impl Fn(&str) -> f64,
) -> String {
    if width_of(line) <= max_width {
        return line.to_string();
    }
    let Some((name, tail)) = line.split_once("  ·  ") else {
        return shrink_middle(line, |candidate| width_of(candidate) <= max_width);
    };
    if width_of(tail) > max_width {
        return shrink_middle(line, |candidate| width_of(candidate) <= max_width);
    }
    let sep = "  ·  ";
    let name = shrink_middle(name, |candidate| {
        width_of(&format!("{candidate}{sep}{tail}")) <= max_width
    });
    format!("{name}{sep}{tail}")
}

/// Shorten `text` from the middle with `…` until `fits` accepts it.
pub(crate) fn shrink_middle(text: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (0..chars.len()).rev() {
        let candidate = middle_keep(&chars, keep);
        if fits(&candidate) {
            return candidate;
        }
    }
    "…".to_string()
}

pub(crate) fn middle_keep(chars: &[char], keep: usize) -> String {
    if keep == 0 || chars.is_empty() {
        return "…".to_string();
    }
    if chars.len() <= keep {
        return chars.iter().collect();
    }
    let left = keep / 2;
    let right = keep - left;
    let mut out: String = chars[..left].iter().collect();
    out.push('…');
    if right > 0 {
        out.extend(chars[chars.len() - right..].iter());
    }
    out
}

/// The column count of a CSV or TSV card (from its header line; cheap).
fn table_columns(text: &str) -> Option<String> {
    matches!(format::detect(text), FormatKind::Csv | FormatKind::Tsv)
        .then(|| crate::dataframe::header_columns(text))
        .flatten()
        .map(columns_label)
}

/// "3 # · 7 ##" on a Markdown card; a level without headings is left out.
fn markdown_headings(text: &str) -> Option<String> {
    if format::detect(text) != FormatKind::Markdown {
        return None;
    }
    let (h1, h2) = crate::format::md_headings::count(text);
    let parts: Vec<String> = [(h1, "#"), (h2, "##")]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, mark)| format!("{n} {mark}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// "1 functie" / "8 functies" on a Rust card.
fn rust_functions(text: &str) -> Option<String> {
    (format::detect(text) == FormatKind::Rust).then(|| match crate::format::rust_fns::count(text) {
        1 => crate::locale::t("one_function").to_string(),
        n => crate::locale::tf("n_functions", &[&n]),
    })
}

/// "5 public · 2 private" on a Java card; a level without methods is left out.
fn java_methods(text: &str) -> Option<String> {
    (format::detect(text) == FormatKind::Java)
        .then(|| crate::format::java_methods::count(text).label())
        .flatten()
}

/// "≈ 1234 tokens" on an agent instructions card (characters / 4).
fn agent_tokens(text: &str) -> Option<String> {
    (format::detect(text) == FormatKind::Agent)
        .then(|| crate::locale::tf("agent_tokens", &[&crate::agent::token_estimate(text)]))
}

/// "1 sectie" / "4 secties" on a TOML card.
fn toml_sections(text: &str) -> Option<String> {
    (format::detect(text) == FormatKind::Toml).then(|| match crate::format::toml::sections(text) {
        1 => crate::locale::t("one_section").to_string(),
        n => crate::locale::tf("n_sections", &[&n]),
    })
}

/// "2 kolommen" / "1 kolom".
pub(crate) fn columns_label(columns: usize) -> String {
    if columns == 1 {
        crate::locale::t("one_column").to_string()
    } else {
        crate::locale::tf("n_columns", &[&columns])
    }
}

pub(crate) fn text_meta(text: &str) -> String {
    text_meta_from(text, text)
}

/// [`text_meta`] for `body`, with the labels of `body` and of `source` together (a decoded
/// view: the copy's labels stay, and what decoding reveals is labelled too).
pub(crate) fn text_meta_joined(body: &str, source: &str) -> String {
    let size = text_meta_from(body, "");
    match joined_label_line(body, source) {
        Some(labels) => format!("{size}  ·  {labels}"),
        None => size,
    }
}

/// The labels of `source` and of `body` together as one line, "checking" while a check runs,
/// `None` without labels.
pub(crate) fn joined_label_line(body: &str, source: &str) -> Option<String> {
    use crate::sensitivity::{Labeling, labeling};
    let mut labels = Vec::new();
    for text in [source, body] {
        match labeling(text) {
            Labeling::Known(found) => labels.extend(found.labels),
            Labeling::Checking => return Some(crate::sensitivity::checking().to_string()),
        }
    }
    labels.sort();
    labels.dedup();
    (!labels.is_empty()).then(|| crate::sensitivity::label_line(&labels))
}

/// Size and line count come from `measured`. Classification comes from
/// `classified`, which stays the copied table when the card shows another rendering.
pub(crate) fn text_meta_from(measured: &str, classified: &str) -> String {
    let size = format_bytes(measured.len());
    let lines = measured.lines().count();
    let mut meta = if lines > 1 {
        match table_columns(classified)
            .or_else(|| toml_sections(classified))
            .or_else(|| rust_functions(classified))
            .or_else(|| java_methods(classified))
            .or_else(|| agent_tokens(classified))
            .or_else(|| markdown_headings(classified))
        {
            Some(count) => crate::locale::tf("lines_columns_size", &[&lines, &count, &size]),
            None => crate::locale::tf("lines_size", &[&lines, &size]),
        }
    } else {
        size
    };
    match crate::sensitivity::labeling(classified) {
        crate::sensitivity::Labeling::Known(found) => {
            let labels = &found.labels;
            if !labels.is_empty() {
                meta.push_str("  ·  ");
                meta.push_str(&crate::sensitivity::label_line(labels));
            }
        }
        crate::sensitivity::Labeling::Checking => {
            meta.push_str("  ·  ");
            meta.push_str(crate::sensitivity::checking());
        }
    }
    meta
}

/// The card's labels are still being checked: it cannot be revealed, and it is not kept with
/// its history entry (the next build has the labels).
pub fn is_checking(card: &WorkCard) -> bool {
    crate::sensitivity::meta_status(&card.meta) == Some(crate::sensitivity::checking())
}

/// Pasted size. Megabytes from 0.01 MB up; kilobytes below that.
pub(crate) fn format_bytes(n: usize) -> String {
    if n < 10_000 {
        format_kilobytes(n)
    } else {
        format_megabytes(n)
    }
}

pub(crate) fn format_megabytes(n: usize) -> String {
    let mb = n as f64 / 1_000_000.0;
    let places = if mb >= 100.0 {
        0
    } else if mb >= 10.0 {
        1
    } else {
        2
    };
    format!("{mb:.places$} MB")
}

pub(crate) fn format_kilobytes(n: usize) -> String {
    if n == 0 {
        return "0 KB".to_string();
    }
    let kb = n as f64 / 1_000.0;
    if kb >= 1.0 {
        return format!("{kb:.1} KB");
    }
    let mut body = format!("{kb:.3}");
    while body.contains('.') && body.ends_with('0') {
        body.pop();
    }
    format!("{body} KB")
}

pub(crate) fn clip_chars(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let body: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{body}…")
    } else {
        body
    }
}

pub(crate) fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::System => crate::locale::t("theme_system"),
        Theme::Light => crate::locale::t("theme_light"),
        Theme::Dark => crate::locale::t("theme_dark"),
    }
}

pub(crate) fn command(id: CommandId, title: &str, detail: &str, keywords: &str) -> Command {
    command_enabled(id, title, detail, keywords, true)
}

pub(crate) fn command_enabled(
    id: CommandId,
    title: &str,
    detail: &str,
    keywords: &str,
    enabled: bool,
) -> Command {
    Command {
        id,
        title: title.to_string(),
        detail: detail.to_string(),
        keywords: keywords.to_string(),
        enabled,
    }
}
