#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    Json,
    Yaml,
    Rust,
    Java,
    /// Python: highlighted and structure-checked, not reformatted.
    Python,
    Url,
    Xml,
    /// An HTML page (`<!DOCTYPE html>` or a root `<html>`): highlighted and formatted like XML,
    /// but no XML validation title and no XSD schema.
    Html,
    Markdown,
    Csv,
    Tsv,
    Dataframe,
    Image,
    /// A decoded JWT (the Decode view): highlight only, never the result of [`detect`]. Its
    /// Header and Payload JSON is colored like [`FormatKind::Json`].
    Jwt,
    Text,
    Plain,
}

impl FormatKind {
    pub fn menu_symbol(self) -> &'static str {
        match self {
            Self::Json => "{}",
            Self::Yaml => "---",
            Self::Rust => "fn",
            Self::Java => "Jv",
            Self::Python => "py",
            Self::Url => "://",
            Self::Xml => "</>",
            Self::Html => "<>",
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Dataframe => "DF",
            Self::Image => "img",
            Self::Jwt => "JWT",
            Self::Text => "¶",
            Self::Plain => "Aa",
        }
    }

    pub fn source_heading(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Rust => "Rust",
            Self::Java => "Java",
            Self::Python => "Python",
            Self::Url => "URL",
            Self::Xml => "XML",
            Self::Html => "HTML",
            Self::Markdown => "Markdown",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Dataframe => crate::locale::t("table_view"),
            Self::Image => crate::locale::t("title_image"),
            Self::Jwt => "JWT",
            Self::Text | Self::Plain => crate::locale::t("title_content"),
        }
    }

    pub fn preview_heading(self) -> &'static str {
        match self {
            Self::Json => crate::locale::t("formatted_json"),
            Self::Yaml => "YAML",
            Self::Rust => crate::locale::t("formatted_rust"),
            Self::Java => crate::locale::t("formatted_java"),
            Self::Python => "Python",
            Self::Xml => crate::locale::t("formatted_xml"),
            Self::Html => "HTML",
            Self::Markdown => crate::locale::t("formatted_markdown"),
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Dataframe => crate::locale::t("table_view"),
            Self::Image => crate::locale::t("title_image"),
            _ => crate::locale::t("title_content"),
        }
    }

    pub fn suggested_extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Rust => "rs",
            Self::Java => "java",
            Self::Python => "py",
            Self::Url => "txt",
            Self::Xml => "xml",
            Self::Html => "html",
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            // Save in the Dataframe view: CSV by default, Parquet on choice
            // ([`crate::dataframe::TableFile`]).
            Self::Dataframe => "csv",
            Self::Image => "png",
            Self::Jwt => "json",
            Self::Text | Self::Plain => "txt",
        }
    }

    pub fn suggested_filename(self) -> String {
        format!("clipboard.{}", self.suggested_extension())
    }
}

/// Format of `text`. Remembered for large texts (see [`crate::memo`]): the card asks many times
/// per redraw, and each look can parse the whole copy.
pub fn detect(text: &str) -> FormatKind {
    DETECTED.get_or_compute(text, detect_uncached)
}

static DETECTED: crate::memo::Memo<FormatKind> = crate::memo::Memo::new(8);

/// Drop the remembered formats (Wipe).
pub fn forget_detected() {
    DETECTED.clear();
}

fn detect_uncached(text: &str) -> FormatKind {
    // JSON with a mistake is JSON too ("JSON · Invalid"), not YAML or text.
    if crate::clipboard::try_format_json(text).is_some()
        || crate::validate::broken_json(text).is_some()
    {
        return FormatKind::Json;
    }
    // A document that starts `<!DOCTYPE html>` or `<html` is HTML, also on one line where
    // `color: teal` in a style would make the whole line a YAML mapping.
    if looks_like_html(text) {
        return FormatKind::Html;
    }
    // Before Markdown: `# comment` lines would read as headings.
    if crate::python::looks_like_python(text) {
        return FormatKind::Python;
    }
    if looks_like_markdown(text) {
        return FormatKind::Markdown;
    }
    if looks_like_rust(text) {
        return FormatKind::Rust;
    }
    if looks_like_java(text) {
        return FormatKind::Java;
    }
    if crate::dataframe::looks_like_tsv(text) {
        return FormatKind::Tsv;
    }
    if crate::dataframe::looks_like_csv(text) {
        return FormatKind::Csv;
    }
    if crate::transform::looks_like_yaml(text) {
        return FormatKind::Yaml;
    }
    if looks_like_url(text) {
        return FormatKind::Url;
    }
    if looks_like_xml(text) {
        return FormatKind::Xml;
    }
    if text.contains('\n') {
        FormatKind::Text
    } else {
        FormatKind::Plain
    }
}

mod html;

pub fn format_text(text: &str) -> String {
    match detect(text) {
        FormatKind::Json => {
            crate::clipboard::try_format_json(text).unwrap_or_else(|| text.to_string())
        }
        FormatKind::Yaml => {
            crate::transform::pretty_yaml(text).unwrap_or_else(|_| text.to_string())
        }
        // Built in: copied code is never handed to an outside program (rustfmt, …).
        FormatKind::Rust | FormatKind::Java => indent_braces(text),
        FormatKind::Xml => pretty_xml(text),
        FormatKind::Html => html::pretty_html(text),
        FormatKind::Markdown => format_markdown(text),
        FormatKind::Url => format_url(text),
        FormatKind::Python
        | FormatKind::Csv
        | FormatKind::Tsv
        | FormatKind::Dataframe
        | FormatKind::Image
        | FormatKind::Jwt
        | FormatKind::Text
        | FormatKind::Plain => text.to_string(),
    }
}

pub(crate) fn looks_like_url(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.contains(['\n', '\t']) {
        return false;
    }
    let rest = strip_http_scheme(text).unwrap_or(text);
    if rest.is_empty() || rest.starts_with('/') {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority_host(authority).is_none() {
        return false;
    }
    let bare = strip_http_scheme(text).is_none() && !rest.contains('/') && !rest.contains('?');
    if bare && is_filename(authority) {
        return false;
    }
    true
}

mod url;
#[allow(unused_imports)]
pub use url::*;
mod xml;
#[allow(unused_imports)]
pub use xml::*;
mod markdown;
#[allow(unused_imports)]
pub use markdown::*;
mod code;
#[allow(unused_imports)]
pub use code::*;
#[cfg(test)]
mod tests;
