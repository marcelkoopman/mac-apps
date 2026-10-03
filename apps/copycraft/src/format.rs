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
            Self::Dataframe => "Dataframe",
            Self::Image => "Image",
            Self::Text | Self::Plain => "Content",
        }
    }

    pub fn preview_heading(self) -> &'static str {
        match self {
            Self::Json => "Formatted JSON",
            Self::Yaml => "YAML",
            Self::Rust => "Formatted Rust",
            Self::Java => "Formatted Java",
            Self::Python => "Python",
            Self::Xml => "Formatted XML",
            Self::Html => "HTML",
            Self::Markdown => "Formatted Markdown",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Dataframe => "Dataframe",
            Self::Image => "Image",
            _ => "Content",
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
    if looks_like_html(text) {
        return FormatKind::Html;
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
        FormatKind::Xml | FormatKind::Html => pretty_xml(text),
        FormatKind::Markdown => format_markdown(text),
        FormatKind::Url => format_url(text),
        FormatKind::Python
        | FormatKind::Csv
        | FormatKind::Tsv
        | FormatKind::Dataframe
        | FormatKind::Image
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

fn strip_http_scheme(text: &str) -> Option<&str> {
    let (scheme, rest) = text.split_once("://")?;
    if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") {
        Some(rest)
    } else {
        None
    }
}

pub(crate) fn authority_host(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.contains(' ') {
        return None;
    }
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    let host = match hostport.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty()
                && !port.is_empty()
                && port.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            host
        }
        Some(_) => return None,
        None => hostport,
    };
    is_url_host(host).then_some(host)
}

fn is_url_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let tld = labels[labels.len() - 1];
    labels.iter().all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    }) && tld.len() >= 2
        && tld.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn is_filename(name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "pdf"
            | "zip"
            | "txt"
            | "json"
            | "xml"
            | "csv"
            | "mp4"
            | "mp3"
    )
}

/// Adds `https://` when the scheme is missing and percent-encodes characters
/// that are not allowed in a URI path or query parameter.
fn format_url(text: &str) -> String {
    let text = text.trim();
    let (scheme, rest) = match strip_http_scheme(text) {
        Some(rest) => {
            let scheme = if text.to_ascii_lowercase().starts_with("http://") {
                "http"
            } else {
                "https"
            };
            (scheme, rest)
        }
        None => ("https", text),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let after = &rest[authority_end..];
    let (before_fragment, fragment) = split_marker(after, '#');
    let (path, query) = split_marker(before_fragment, '?');
    let mut out = String::new();
    out.push_str(scheme);
    out.push_str("://");
    out.push_str(authority);
    out.push_str(&encode_path(path));
    if let Some(query) = query {
        out.push('?');
        out.push_str(&encode_query(query));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&encode_component(fragment, fragment_byte));
    }
    out
}

fn split_marker(text: &str, marker: char) -> (&str, Option<&str>) {
    match text.split_once(marker) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    }
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| encode_component(segment, query_byte))
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_query(query: &str) -> String {
    query
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, value)) => format!(
                "{}={}",
                encode_component(key, query_byte),
                encode_component(value, query_byte)
            ),
            None => encode_component(part, query_byte),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_component(text: &str, allow: fn(u8) -> bool) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push(bytes[index + 1] as char);
            out.push(bytes[index + 2] as char);
            index += 3;
            continue;
        }
        let byte = bytes[index];
        if allow(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        index += 1;
    }
    out
}

fn query_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b':'
                | b'@'
        )
}

fn fragment_byte(byte: u8) -> bool {
    query_byte(byte) || byte == b'/' || byte == b'?'
}

/// An HTML page: `<!DOCTYPE html>` (any case) or a root `<html>` element, after leading
/// whitespace, a byte order mark, an `<?xml …?>` declaration and comments.
pub fn looks_like_html(text: &str) -> bool {
    let mut rest = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    loop {
        let head: String = rest
            .chars()
            .take(32)
            .collect::<String>()
            .to_ascii_lowercase();
        let skip_to = if head.starts_with("<?xml") {
            rest.find("?>").map(|end| end + 2)
        } else if head.starts_with("<!--") {
            rest.find("-->").map(|end| end + 3)
        } else {
            // `name` followed by `>` or whitespace, so `<htmlx>` and `<!DOCTYPE htmlx>` do not count.
            let word = |s: &str, name: &str| {
                s.strip_prefix(name)
                    .and_then(|after| after.chars().next())
                    .is_some_and(|c| c == '>' || c.is_whitespace())
            };
            let doctype = head
                .strip_prefix("<!doctype")
                .filter(|after| after.starts_with(char::is_whitespace))
                .is_some_and(|after| word(after.trim_start(), "html"));
            return doctype || word(&head, "<html");
        };
        let Some(end) = skip_to else {
            return false;
        };
        rest = rest[end..].trim_start();
    }
}

pub fn looks_like_xml(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !(trimmed.starts_with('<') && trimmed.contains('>')) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("<?xml")
        || lower.starts_with("<!doctype")
        || lower.starts_with("<svg")
        || lower.starts_with("<html")
        || tag_balance(trimmed)
}

fn tag_balance(text: &str) -> bool {
    let mut depth = 0i32;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut saw_tag = false;
    while i < chars.len() {
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        if starts_at(&chars, i, "<!--") {
            i += 4;
            while i + 2 < chars.len()
                && !(chars[i] == '-' && chars[i + 1] == '-' && chars[i + 2] == '>')
            {
                i += 1;
            }
            i = (i + 3).min(chars.len());
            continue;
        }
        if starts_at(&chars, i, "<?") || starts_at(&chars, i, "<!") {
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        let closing = i + 1 < chars.len() && chars[i + 1] == '/';
        let mut j = i + 1 + usize::from(closing);
        while j < chars.len() && chars[j] != '>' {
            j += 1;
        }
        if j >= chars.len() {
            return false;
        }
        saw_tag = true;
        let self_close = j > 0 && chars[j - 1] == '/';
        if closing {
            depth -= 1;
        } else if !self_close {
            depth += 1;
        }
        i = j + 1;
    }
    saw_tag && depth >= 0
}

fn starts_at(chars: &[char], i: usize, s: &str) -> bool {
    let w: Vec<char> = s.chars().collect();
    i + w.len() <= chars.len() && chars[i..i + w.len()] == w[..]
}

pub fn pretty_xml(src: &str) -> String {
    let tokens = xml_tokens(src.trim());
    if tokens.is_empty() {
        return src.to_string();
    }
    let mut out = String::new();
    let mut indent: i32 = 0;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            XmlToken::Decl(s) | XmlToken::Comment(s) | XmlToken::Empty(s) => {
                push_indent(&mut out, indent);
                out.push_str(s);
                out.push('\n');
                i += 1;
            }
            XmlToken::Open(open) => {
                if let Some((line, consumed)) = inline_leaf(&tokens, i) {
                    push_indent(&mut out, indent);
                    out.push_str(&line);
                    out.push('\n');
                    i += consumed;
                    continue;
                }
                push_indent(&mut out, indent);
                out.push_str(open);
                out.push('\n');
                indent += 1;
                i += 1;
            }
            XmlToken::Close(s) => {
                indent = (indent - 1).max(0);
                push_indent(&mut out, indent);
                out.push_str(s);
                out.push('\n');
                i += 1;
            }
            XmlToken::Text(s) => {
                let text = collapse_xml_text(s);
                if !text.is_empty() {
                    push_indent(&mut out, indent);
                    out.push_str(&text);
                    out.push('\n');
                }
                i += 1;
            }
        }
    }
    if out.ends_with('\n') {
        out.pop();
    }
    if out.trim().is_empty() {
        src.to_string()
    } else {
        out
    }
}

fn inline_leaf(tokens: &[XmlToken], i: usize) -> Option<(String, usize)> {
    let XmlToken::Open(open) = tokens.get(i)? else {
        return None;
    };
    match tokens.get(i + 1) {
        Some(XmlToken::Close(close)) => Some((format!("{open}{close}"), 2)),
        Some(XmlToken::Text(text)) => match tokens.get(i + 2) {
            Some(XmlToken::Close(close)) => {
                let t = collapse_xml_text(text);
                Some((format!("{open}{t}{close}"), 3))
            }
            _ => None,
        },
        _ => None,
    }
}

fn collapse_xml_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

enum XmlToken {
    Decl(String),
    Comment(String),
    Open(String),
    Close(String),
    Empty(String),
    Text(String),
}

fn xml_tokens(src: &str) -> Vec<XmlToken> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            if starts_at(&chars, i, "<!--") {
                let mut j = i + 4;
                while j + 2 < chars.len()
                    && !(chars[j] == '-' && chars[j + 1] == '-' && chars[j + 2] == '>')
                {
                    j += 1;
                }
                j = (j + 3).min(chars.len());
                tokens.push(XmlToken::Comment(chars[i..j].iter().collect()));
                i = j;
                continue;
            }
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j >= chars.len() {
                tokens.push(XmlToken::Text(chars[i..].iter().collect()));
                break;
            }
            j += 1;
            let tag: String = chars[i..j].iter().collect();
            if tag.starts_with("<?") || tag.starts_with("<!") {
                tokens.push(XmlToken::Decl(tag));
            } else if tag.starts_with("</") {
                tokens.push(XmlToken::Close(tag));
            } else if tag.ends_with("/>") {
                tokens.push(XmlToken::Empty(tag));
            } else {
                tokens.push(XmlToken::Open(tag));
            }
            i = j;
        } else {
            let mut j = i;
            while j < chars.len() && chars[j] != '<' {
                j += 1;
            }
            tokens.push(XmlToken::Text(chars[i..j].iter().collect()));
            i = j;
        }
    }
    tokens
}

fn push_indent(out: &mut String, indent: i32) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

fn looks_like_markdown(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return false;
    }
    if wrapped_fence(&lines) || has_gfm_table(&lines) {
        return true;
    }
    // `# comment` is also a Gitignore comment. A file of those comments plus
    // spaceless patterns is not a Markdown document.
    if hash_comment_file(&lines) {
        return false;
    }
    let headings = lines.iter().filter(|line| is_atx_heading(line)).count();
    if headings == 0 || yaml_comment_on_a_mapping(text) {
        return false;
    }
    let lists = lines
        .iter()
        .filter(|line| is_toplevel_list_item(line))
        .count();
    let quotes = lines.iter().filter(|line| is_quote_line(line)).count();
    lists > 0
        || quotes > 0
        || count_md_links(text) > 0
        || has_emphasis(text)
        || headings >= 2
        || heading_then_prose(&lines)
}

/// A `# comment` above a YAML mapping is not a Markdown heading.
fn yaml_comment_on_a_mapping(text: &str) -> bool {
    if !crate::transform::looks_like_yaml(text) {
        return false;
    }
    let lines: Vec<&str> = text.lines().collect();
    !lines.iter().any(|line| is_toplevel_list_item(line))
        && count_md_links(text) == 0
        && !has_emphasis(text)
        && !lines.iter().any(|line| is_quote_line(line))
}

fn heading_then_prose(lines: &[&str]) -> bool {
    let Some(start) = lines.iter().position(|line| is_atx_heading(line)) else {
        return false;
    };
    let mut saw_blank = false;
    for line in &lines[start + 1..] {
        if line.trim().is_empty() {
            saw_blank = true;
            continue;
        }
        if is_atx_heading(line) || is_toplevel_list_item(line) || is_quote_line(line) {
            return true;
        }
        if saw_blank && !is_yaml_key(line) {
            return true;
        }
    }
    false
}

fn is_yaml_key(line: &str) -> bool {
    let Some((label, value)) = line.split_once(':') else {
        return false;
    };
    let label = label.trim();
    !label.is_empty()
        && !value.trim().is_empty()
        && !label.starts_with(['-', '#', '[', '`'])
        && label
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | ' '))
}

fn hash_comment_file(lines: &[&str]) -> bool {
    let mut any = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        any = true;
        if is_single_hash_comment(trimmed) || is_spaceless_token(trimmed) {
            continue;
        }
        return false;
    }
    any
}

fn is_single_hash_comment(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    !rest.starts_with('#') && (rest.is_empty() || rest.starts_with([' ', '\t']))
}

fn is_spaceless_token(trimmed: &str) -> bool {
    !trimmed.chars().any(char::is_whitespace)
}

fn is_atx_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return false;
    }
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&hashes) {
        return false;
    }
    let rest = &trimmed[hashes..];
    rest.starts_with(' ') && !rest.trim().is_empty()
}

fn is_toplevel_list_item(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t') && is_list_marker(line)
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    indent <= 3 && is_list_marker(trimmed)
}

fn is_list_marker(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1] == b' ' {
        return true;
    }
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 || index > 9 || index + 1 >= bytes.len() {
        return false;
    }
    (bytes[index] == b'.' || bytes[index] == b')') && bytes[index + 1] == b' '
}

fn is_quote_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    indent <= 3 && trimmed.starts_with('>')
}

fn has_emphasis(text: &str) -> bool {
    wrapped_marker(text, "**") || wrapped_marker(text, "__")
}

fn wrapped_marker(text: &str, marker: &str) -> bool {
    let Some(start) = text.find(marker) else {
        return false;
    };
    let rest = &text[start + marker.len()..];
    let Some(end) = rest.find(marker) else {
        return false;
    };
    let inner = &rest[..end];
    !inner.is_empty() && !inner.contains('\n') && inner.trim() == inner
}

fn count_md_links(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0;
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '['
            && let Some(end) = md_link_end(&chars, index)
        {
            count += 1;
            index = end;
            continue;
        }
        index += 1;
    }
    count
}

fn md_link_end(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    if index >= chars.len() || chars[index] == ']' {
        return None;
    }
    while index < chars.len() && chars[index] != ']' && chars[index] != '\n' && chars[index] != '['
    {
        index += 1;
    }
    if index >= chars.len() || chars[index] != ']' {
        return None;
    }
    index += 1;
    if index >= chars.len() || chars[index] != '(' {
        return None;
    }
    index += 1;
    let url_start = index;
    while index < chars.len() && chars[index] != ')' && chars[index] != '\n' {
        index += 1;
    }
    if index >= chars.len() || chars[index] != ')' || index == url_start {
        return None;
    }
    Some(index + 1)
}

fn wrapped_fence(lines: &[&str]) -> bool {
    let mut nonempty = lines.iter().copied().filter(|line| !line.trim().is_empty());
    let Some(first) = nonempty.next() else {
        return false;
    };
    let Some((marker, len)) = opening_fence(first) else {
        return false;
    };
    let rest: Vec<&str> = nonempty.collect();
    let Some(last) = rest.last().copied() else {
        return false;
    };
    closes_fence(last, marker, len)
}

fn has_gfm_table(lines: &[&str]) -> bool {
    let separator = lines.iter().any(|line| is_table_separator(line));
    let row = lines
        .iter()
        .any(|line| is_table_row(line) && !is_table_separator(line));
    separator && row
}

fn is_table_separator(line: &str) -> bool {
    if !line.contains('|') {
        return false;
    }
    let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let core = cell.trim().trim_matches(':');
            !core.is_empty() && core.chars().all(|ch| ch == '-')
        })
}

fn is_table_row(line: &str) -> bool {
    line.matches('|').count() >= 2
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    if len < 3 {
        return None;
    }
    let rest = trimmed[len..].trim();
    if marker == '`' && rest.contains('`') {
        return None;
    }
    Some((marker, len))
}

fn closes_fence(line: &str, marker: char, len: usize) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == marker) && trimmed.chars().count() >= len
}

fn format_markdown(src: &str) -> String {
    let ends_with_newline = src.ends_with('\n');
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    let mut parts: Vec<&str> = normalized.split('\n').collect();
    if normalized.ends_with('\n') {
        parts.pop();
    }
    let front_end = front_matter_end(&parts);
    let mut out: Vec<String> = Vec::new();
    let mut prev = MdKind::Start;
    let mut in_fence = false;
    let mut fence_char = '`';
    let mut fence_len = 0usize;
    for (index, line) in parts.iter().enumerate() {
        if front_end.is_some_and(|end| index <= end) {
            out.push(line.trim_end().to_string());
            prev = if line.trim().is_empty() {
                MdKind::Blank
            } else {
                MdKind::Fence
            };
            continue;
        }
        if in_fence {
            if closes_fence(line, fence_char, fence_len) {
                in_fence = false;
                push_md(&mut out, &mut prev, MdKind::Fence, line.trim_end());
            } else {
                push_md(&mut out, &mut prev, MdKind::Fence, line);
            }
            continue;
        }
        if let Some((marker, len)) = opening_fence(line) {
            in_fence = true;
            fence_char = marker;
            fence_len = len;
            push_md(&mut out, &mut prev, MdKind::Fence, line.trim_end());
            continue;
        }
        if line.trim().is_empty() {
            if prev != MdKind::Blank && prev != MdKind::Start {
                out.push(String::new());
                prev = MdKind::Blank;
            }
            continue;
        }
        let kind = classify_md(line, prev);
        let text = if kind == MdKind::Heading {
            normalize_heading(line)
        } else {
            trim_markdown_line(line)
        };
        push_md(&mut out, &mut prev, kind, &text);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    if out.is_empty() {
        return src.to_string();
    }
    let mut text = out.join("\n");
    if ends_with_newline {
        text.push('\n');
    }
    text
}

fn front_matter_end(lines: &[&str]) -> Option<usize> {
    let mark = lines.first()?.trim();
    if mark != "---" && mark != "+++" {
        return None;
    }
    lines
        .iter()
        .enumerate()
        .skip(1)
        .take(40)
        .find(|(_, line)| line.trim() == mark)
        .map(|(index, _)| index)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MdKind {
    Start,
    Blank,
    Heading,
    Fence,
    List,
    Quote,
    Table,
    Break,
    Para,
}

fn classify_md(line: &str, prev: MdKind) -> MdKind {
    if is_atx_heading(line) {
        return MdKind::Heading;
    }
    if is_thematic_break(line) {
        return MdKind::Break;
    }
    if is_table_row(line) {
        return MdKind::Table;
    }
    if is_quote_line(line) {
        return MdKind::Quote;
    }
    if is_list_item(line) || (prev == MdKind::List && indent_of(line) >= 2) {
        return MdKind::List;
    }
    MdKind::Para
}

fn is_thematic_break(line: &str) -> bool {
    let marks: Vec<char> = line
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    marks.len() >= 3
        && marks.iter().all(|ch| *ch == marks[0])
        && matches!(marks[0], '-' | '*' | '_')
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn push_md(out: &mut Vec<String>, prev: &mut MdKind, kind: MdKind, text: &str) {
    if kind != MdKind::Blank && md_separated(*prev, kind) && *prev != MdKind::Blank {
        out.push(String::new());
    }
    out.push(text.to_string());
    *prev = kind;
}

fn md_separated(prev: MdKind, next: MdKind) -> bool {
    !matches!(
        (prev, next),
        (MdKind::Start | MdKind::Blank, _)
            | (MdKind::Fence, MdKind::Fence)
            | (MdKind::List, MdKind::List)
            | (MdKind::Quote, MdKind::Quote)
            | (MdKind::Table, MdKind::Table)
            | (MdKind::Para, MdKind::Para)
    )
}

fn normalize_heading(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    let title = trimmed[hashes..].trim();
    if title.is_empty() {
        return trim_markdown_line(line);
    }
    format!("{indent}{} {title}", "#".repeat(hashes))
}

fn trim_markdown_line(line: &str) -> String {
    if line.ends_with("  ") && !line.trim().is_empty() {
        format!("{}  ", line.trim_end())
    } else {
        line.trim_end().to_string()
    }
}

fn looks_like_rust(text: &str) -> bool {
    let rust_hits = [
        "fn ",
        "impl ",
        "pub fn",
        "let mut",
        "match ",
        "use crate",
        "#[derive",
    ];
    score(text, &rust_hits) >= 2
        || (text.contains("fn ") && (text.contains('{') || text.contains("->")))
}

fn looks_like_java(text: &str) -> bool {
    let java_hits = [
        "public class",
        "private class",
        "package ",
        "import java",
        "System.out",
        "public static void",
        "@Override",
    ];
    score(text, &java_hits) >= 1 && text.contains('{')
}

fn score(text: &str, needles: &[&str]) -> usize {
    needles.iter().filter(|n| text.contains(*n)).count()
}

/// Re-indent brace code (Rust, Java) by its brackets, four spaces a level: `{` `[` `(` open one,
/// their closers close it, and a line starting with closers is dedented by them. A line that
/// starts with `.` (a method chain) goes one level deeper. Brackets in strings, char literals
/// and comments do not count, and lines inside a multi-line string or block comment are kept
/// exactly as they are. Not a full formatter: spacing within a line is left alone.
pub fn indent_braces(src: &str) -> String {
    let mut indent: i32 = 0;
    let mut state = Scan::Code;
    let mut out = String::new();
    for raw in src.lines() {
        if state != Scan::Code {
            // Inside a string or comment that started on an earlier line.
            out.push_str(raw);
            out.push('\n');
            let (_, next) = scan_line(raw, state);
            state = next;
            continue;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        let closers = trimmed
            .chars()
            .take_while(|ch| matches!(ch, '}' | ']' | ')'))
            .count();
        let closers = i32::try_from(closers).unwrap_or(i32::MAX);
        let mut level = (indent - closers).max(0);
        if trimmed.starts_with('.') && !trimmed.starts_with("..") {
            level += 1;
        }
        for _ in 0..level {
            out.push_str("    ");
        }
        out.push_str(trimmed);
        out.push('\n');
        let (depth, next) = scan_line(trimmed, state);
        indent = (indent + depth).max(0);
        state = next;
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Where [`scan_line`] is at the end of a line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scan {
    Code,
    /// In a `"…"` string.
    Str,
    /// In a Rust raw string `r#"…"#` closed by `"` and this many `#`.
    RawStr(usize),
    /// In a `/* … */` comment, this deep (Rust nests them).
    BlockComment(usize),
}

/// The bracket depth change over `line` (`{` `[` `(` up, closers down), counting only code,
/// and the state the next line starts in.
fn scan_line(line: &str, mut state: Scan) -> (i32, Scan) {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i32;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            Scan::Str => match ch {
                '\\' => i += 1,
                '"' => state = Scan::Code,
                _ => {}
            },
            Scan::RawStr(hashes) => {
                if ch == '"'
                    && chars[i + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|c| **c == '#')
                        .count()
                        == hashes
                {
                    state = Scan::Code;
                    i += hashes;
                }
            }
            Scan::BlockComment(level) => {
                if ch == '*' && next == Some('/') {
                    state = if level > 1 {
                        Scan::BlockComment(level - 1)
                    } else {
                        Scan::Code
                    };
                    i += 1;
                } else if ch == '/' && next == Some('*') {
                    state = Scan::BlockComment(level + 1);
                    i += 1;
                }
            }
            Scan::Code => match ch {
                '/' if next == Some('/') => break,
                '/' if next == Some('*') => {
                    state = Scan::BlockComment(1);
                    i += 1;
                }
                '"' => state = Scan::Str,
                'r' if starts_raw_string(&chars, i).is_some() => {
                    let hashes = starts_raw_string(&chars, i).unwrap_or(0);
                    state = Scan::RawStr(hashes);
                    i += hashes + 1;
                }
                '\'' => i += char_literal_len(&chars[i..]).saturating_sub(1),
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => depth -= 1,
                _ => {}
            },
        }
        i += 1;
    }
    (depth, state)
}

/// `r"`, `r#"`, `br##"`, … at `i` (not the end of a name such as `bar"`): the number of `#`.
fn starts_raw_string(chars: &[char], i: usize) -> Option<usize> {
    let before = i.checked_sub(1).map(|at| chars[at]);
    let name_before = before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    if name_before && before != Some('b') {
        return None;
    }
    if before == Some('b') && i >= 2 && (chars[i - 2].is_alphanumeric() || chars[i - 2] == '_') {
        return None;
    }
    let hashes = chars[i + 1..].iter().take_while(|ch| **ch == '#').count();
    (chars.get(i + 1 + hashes) == Some(&'"')).then_some(hashes)
}

/// Length of the char literal (`'{'`, `'\''`, `'\u{7b}'`) that starts `rest`, or 1 for a quote
/// that starts none (a Rust lifetime such as `'a`).
fn char_literal_len(rest: &[char]) -> usize {
    match rest.get(1) {
        // The escaped character is taken as is, then the closing quote.
        Some('\\') => rest
            .iter()
            .skip(3)
            .take(10)
            .position(|ch| *ch == '\'')
            .map_or(1, |at| at + 4),
        Some(_) if rest.get(2) == Some(&'\'') => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{FormatKind, authority_host, detect, indent_braces, looks_like_url, pretty_xml};

    #[test]
    fn python_is_detected_without_stealing_other_kinds() {
        assert_eq!(detect("f = open(\"demofile.txt\")"), FormatKind::Plain);
        assert_eq!(
            detect("# read it\nwith open(\"demofile.txt\") as f:\n    print(f.read())\n"),
            FormatKind::Python
        );
        assert_eq!(
            detect("import os\n\n# where am I\nprint(os.getcwd())\n"),
            FormatKind::Python
        );
        assert_eq!(
            detect("name: app\nservices:\n  web:\n    image: nginx\n"),
            FormatKind::Yaml
        );
        assert_ne!(
            detect("class Dog\n  def initialize(name)\n    @name = name\n  end\nend\n"),
            FormatKind::Python
        );
        assert_ne!(
            detect("import React from 'react';\nconst App = () => null;\n"),
            FormatKind::Python
        );
        assert_eq!(
            detect("# Notes\n\nSome text about import os and print.\n\n- item\n"),
            FormatKind::Markdown
        );
        assert_eq!(
            detect("Please import the data.\nThen print it for Jan.\n"),
            FormatKind::Text
        );
        assert_eq!(FormatKind::Python.source_heading(), "Python");
        assert_eq!(FormatKind::Python.suggested_extension(), "py");
    }

    #[test]
    fn html_pages_are_html_not_xml() {
        for src in [
            "<!DOCTYPE html>\n<html><head><title>x</title></head><body><p>Hi</p></body></html>",
            "  \n<!doctype HTML>\n<html lang=\"nl\"><body></body></html>",
            "\u{feff}<!DocType html><html><body><br></body></html>",
            "<html>\n  <body><p>Hi</p></body>\n</html>",
            "<?xml version=\"1.0\"?>\n<!-- page -->\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><body/></html>",
        ] {
            assert_eq!(detect(src), FormatKind::Html, "{src}");
            assert_eq!(FormatKind::Html.source_heading(), "HTML");
        }
    }

    #[test]
    fn xml_that_only_looks_like_html_stays_xml() {
        for src in [
            "<root><html>x</html></root>",
            "<htmlish><a>1</a></htmlish>",
            "<!DOCTYPE note><note><to>Jan</to></note>",
            "<!DOCTYPE htmlx><htmlx><a>1</a></htmlx>",
        ] {
            assert_eq!(detect(src), FormatKind::Xml, "{src}");
        }
    }

    #[test]
    fn menu_symbols_are_type_marks() {
        use super::FormatKind::*;
        assert_eq!(Json.menu_symbol(), "{}");
        assert_eq!(Yaml.menu_symbol(), "---");
        assert_eq!(Rust.menu_symbol(), "fn");
        assert_eq!(Java.menu_symbol(), "Jv");
        assert_eq!(Url.menu_symbol(), "://");
        assert_eq!(Xml.menu_symbol(), "</>");
        assert_eq!(Markdown.menu_symbol(), "md");
        assert_eq!(Csv.menu_symbol(), "csv");
        assert_eq!(Tsv.menu_symbol(), "tsv");
        assert_eq!(Dataframe.menu_symbol(), "DF");
        assert_eq!(Image.menu_symbol(), "img");
        assert_eq!(Text.menu_symbol(), "¶");
        assert_eq!(Plain.menu_symbol(), "Aa");
    }

    #[test]
    fn suggested_extensions_match_kind() {
        assert_eq!(FormatKind::Json.suggested_extension(), "json");
        assert_eq!(FormatKind::Yaml.suggested_extension(), "yaml");
        assert_eq!(FormatKind::Rust.suggested_extension(), "rs");
        assert_eq!(FormatKind::Java.suggested_extension(), "java");
        assert_eq!(FormatKind::Xml.suggested_extension(), "xml");
        assert_eq!(FormatKind::Markdown.suggested_extension(), "md");
        assert_eq!(FormatKind::Csv.suggested_extension(), "csv");
        assert_eq!(FormatKind::Tsv.suggested_extension(), "tsv");
        assert_eq!(FormatKind::Dataframe.suggested_extension(), "csv");
        assert_eq!(FormatKind::Image.suggested_extension(), "png");
        assert_eq!(FormatKind::Plain.suggested_extension(), "txt");
        assert_eq!(FormatKind::Csv.suggested_filename(), "clipboard.csv");
        assert_eq!(FormatKind::Tsv.suggested_filename(), "clipboard.tsv");
        assert_eq!(FormatKind::Dataframe.suggested_filename(), "clipboard.csv");
    }

    #[test]
    fn formats_a_url_with_a_scheme_and_encoded_parameters() {
        assert_eq!(
            super::format_text("example.com/search?q=hello world&city=New York"),
            "https://example.com/search?q=hello%20world&city=New%20York"
        );
        assert_eq!(
            super::format_text("http://example.com/my file?x=a/b"),
            "http://example.com/my%20file?x=a%2Fb"
        );
        assert_eq!(
            super::format_text("https://example.com/search?q=hello%20world"),
            "https://example.com/search?q=hello%20world"
        );
        assert_eq!(
            super::format_text("HTTPS://example.com/path"),
            "https://example.com/path"
        );
        assert_eq!(detect("www.example.com/a"), FormatKind::Url);
        assert_eq!(detect("notes.txt"), FormatKind::Plain);
        assert_eq!(detect("https://example.com/path"), FormatKind::Url);
    }

    #[test]
    fn authority_host_drops_userinfo() {
        assert_eq!(
            authority_host("deploy:s3cr3t@github.com"),
            Some("github.com")
        );
        assert_eq!(
            authority_host("deploy:s3cr3t@github.com:443"),
            Some("github.com")
        );
        assert_eq!(authority_host("github.com"), Some("github.com"));
        assert!(
            !authority_host("deploy:s3cr3t@github.com").is_some_and(|host| host.contains("deploy"))
        );
        assert!(looks_like_url(
            "https://deploy:s3cr3t@github.com/acme/app.git"
        ));
    }

    #[test]
    fn detects_rust() {
        let src = "fn main() { let x = 1; }";
        assert_eq!(detect(src), FormatKind::Rust);
    }

    #[test]
    fn detects_markdown_and_leaves_code_yaml_and_prose() {
        let notes = "# Notes\n\n- call jan\n- send the report\n";
        assert_eq!(detect(notes), FormatKind::Markdown);
        assert_eq!(detect("hello\nworld"), FormatKind::Text);
        assert_eq!(detect("# comment\necho hello"), FormatKind::Text);
        assert_eq!(
            detect("# comment\nname: copycraft\nitems:\n  - one\n"),
            FormatKind::Yaml
        );
        let fenced = "```rust\nfn main() {}\n```\n";
        assert_eq!(detect(fenced), FormatKind::Markdown);
        let readme = "# Title\n\n```rust\nfn main() {\n    let x = 1;\n}\n```\n";
        assert_eq!(detect(readme), FormatKind::Markdown);
        assert_eq!(detect("fn main() {\n    let x = 1;\n}\n"), FormatKind::Rust);
        let gitignore = "\
# =====================================================================
# 1. RUST & CARGO
# =====================================================================
/target/
debug/

# Back-up bestanden van rustfmt negeren
**/*.rs.bk
.DS_Store
!.vscode/settings.json
";
        assert_eq!(detect(gitignore), FormatKind::Text);
        assert_eq!(super::format_text(gitignore), gitignore);
    }

    #[test]
    fn formats_markdown_spacing_and_keeps_a_tidy_document() {
        let messy = "# Title\nSome text\n- item\n- item\n## Next\npara\n";
        let formatted = super::format_text(messy);
        assert_eq!(
            formatted,
            "# Title\n\nSome text\n\n- item\n- item\n\n## Next\n\npara\n"
        );
        assert_eq!(super::format_text(&formatted), formatted);
        let tidy = "\
# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.

## Commands

Kleinste opdracht die de change dekt.
";
        assert_eq!(detect(tidy), FormatKind::Markdown);
        assert_eq!(super::format_text(tidy), tidy);
    }

    #[test]
    fn rust_module_list_is_not_csv() {
        let src = "\
mod appearance;
mod clipboard;
mod compress;
mod convert;
mod dataframe;
mod decode;
mod format;
mod highlight;
mod icon;
mod menubar;
mod preview;
mod redact;
mod settings;
mod toolbar_visibility;
mod transform;

#[cfg(target_os = \"macos\")]
mod macos_preview_text;
#[cfg(target_os = \"macos\")]
mod macos_window;

fn main() {
    if let Err(e) = menubar::run() {
        eprintln!(\"copycraft failed: {e}\");
        std::process::exit(1);
    }
}
";
        assert_eq!(detect(src), FormatKind::Rust);
        assert!(!crate::dataframe::looks_like_csv(src));
    }

    #[test]
    fn detects_java() {
        let src = "public class App { public static void main(String[] args) { } }";
        assert_eq!(detect(src), FormatKind::Java);
    }

    #[test]
    fn detects_yaml() {
        let src = "name: copycraft\nitems:\n  - one\n";
        assert_eq!(detect(src), FormatKind::Yaml);
    }

    #[test]
    fn detects_xml() {
        assert_eq!(detect("<root><item/></root>"), FormatKind::Xml);
        assert_eq!(detect("<?xml version=\"1.0\"?><a></a>"), FormatKind::Xml);
    }

    #[test]
    fn detects_csv_and_tsv() {
        assert_eq!(detect("name,age\nalice,30\nbob,40"), FormatKind::Csv);
        assert_eq!(detect("name\tage\nalice\t30\nbob\t40"), FormatKind::Tsv);
        assert_eq!(
            detect("Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900"),
            FormatKind::Csv
        );
        assert_eq!(
            detect(
                "Id,Naam,Geboortedatum,Adres,Telefoonnummer,Salaris\n\
1,Jan de Vries,1984-05-12,\"Hoofdstraat 45, Groningen\",06-12345678,3450\n\
2,Anja Bakker,1991-11-23,\"Kerkplein 2, Utrecht\",06-87654321,2900"
            ),
            FormatKind::Csv
        );
        assert_eq!(detect("just a sentence"), FormatKind::Plain);
        assert_eq!(detect("hello\nworld"), FormatKind::Text);
        assert_eq!(FormatKind::Csv.preview_heading(), "CSV");
        assert_eq!(FormatKind::Tsv.preview_heading(), "TSV");
        assert_eq!(FormatKind::Dataframe.preview_heading(), "Dataframe");
        assert_eq!(FormatKind::Image.source_heading(), "Image");
        assert_eq!(FormatKind::Image.preview_heading(), "Image");
        assert_eq!(FormatKind::Rust.source_heading(), "Rust");
        assert_eq!(FormatKind::Rust.preview_heading(), "Formatted Rust");
        assert_eq!(FormatKind::Json.source_heading(), "JSON");
        assert_eq!(FormatKind::Java.source_heading(), "Java");
    }

    #[test]
    fn pretty_prints_xml() {
        let out = pretty_xml("<root><item id=\"1\">hi</item><empty/></root>");
        assert!(out.contains("    <item id=\"1\">hi</item>"));
        assert!(out.contains("    <empty/>"));
        assert_eq!(out.lines().next().unwrap(), "<root>");
        assert!(!out.contains("\n        hi\n"));
    }

    #[test]
    fn pretty_prints_xml_leaf_tags_inline() {
        let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<PensioenAangifteResponse>
    <Bericht>
        <RespSrt>
            ACK
        </RespSrt>
        <IdBer>
            123456
        </IdBer>
        <LhNr>
            012345678L01
        </LhNr>
        <Empty></Empty>
    </Bericht>
    <SysteemMelding>
        Het bestand heeft geen geldige extentie.
    </SysteemMelding>
</PensioenAangifteResponse>"#;
        let out = pretty_xml(src);
        assert!(out.contains("<RespSrt>ACK</RespSrt>"));
        assert!(out.contains("<IdBer>123456</IdBer>"));
        assert!(out.contains("<LhNr>012345678L01</LhNr>"));
        assert!(out.contains("<Empty></Empty>"));
        assert!(
            out.contains(
                "<SysteemMelding>Het bestand heeft geen geldige extentie.</SysteemMelding>"
            )
        );
        assert!(out.contains("    <Bericht>"));
        assert!(!out.contains("\n            ACK\n"));
        assert!(!out.contains("\n            012345678L01\n"));
        assert!(out.lines().next().unwrap().starts_with("<?xml"));
    }

    #[test]
    fn indents_braces() {
        let src = "fn main(){\nlet x=1;\n}";
        let out = indent_braces(src);
        assert!(out.contains("    let x=1;"));
        assert!(out.lines().last().unwrap().starts_with('}'));
    }

    #[test]
    fn rust_is_formatted_in_process_by_brackets() {
        let messy = "fn a(x: u8) -> u8 {\nif x > 1 {\nlet v = f(\nx,\n);\nreturn v;\n}\nlet items = list\n.iter()\n.count();\nmatch x {\n0 => 1,\n_ => 2,\n}\n}";
        let tidy = "fn a(x: u8) -> u8 {
    if x > 1 {
        let v = f(
            x,
        );
        return v;
    }
    let items = list
        .iter()
        .count();
    match x {
        0 => 1,
        _ => 2,
    }
}";
        assert_eq!(detect(messy), FormatKind::Rust);
        assert_eq!(super::format_text(messy), tidy);
        // Already tidy code stays as it is.
        assert_eq!(super::format_text(tidy), tidy);
    }

    #[test]
    fn brackets_in_strings_chars_and_comments_do_not_indent() {
        let src = "fn a() {\nlet s = \"{ ( [\";\nlet c = '{';\nlet q = '\\'';\nlet u = '\\u{7b}';\n// a { comment\n/* and { another */\nlet r = r#\"raw {\"#;\nfn b<'a>(x: &'a str) {}\nlet z = 1;\n}";
        let out = indent_braces(src);
        for line in out.lines().skip(1).take(9) {
            assert!(
                line.starts_with("    ") && !line.starts_with("     "),
                "{line:?}"
            );
        }
        assert_eq!(out.lines().last(), Some("}"));
    }

    #[test]
    fn multi_line_strings_and_comments_are_kept_as_they_are() {
        let src = "fn a() {\nlet s = \"first\n  { kept\n last\";\n/*\n   { note\n*/\nlet r = r#\"\n}\"#;\nok();\n}";
        let out = indent_braces(src);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[1], "    let s = \"first");
        assert_eq!(lines[2], "  { kept");
        assert_eq!(lines[3], " last\";");
        assert_eq!(lines[5], "   { note");
        assert_eq!(lines[6], "*/");
        assert_eq!(lines[8], "}\"#;");
        assert_eq!(lines[9], "    ok();");
        assert_eq!(lines[10], "}");
    }

    #[test]
    fn java_is_formatted_by_the_same_indenter() {
        let src = "public class A {\nprivate void f() {\nif (x) {\ny();\n}\nz();\n}\n}";
        assert_eq!(detect(src), FormatKind::Java);
        assert_eq!(
            super::format_text(src),
            "public class A {\n    private void f() {\n        if (x) {\n            y();\n        }\n        z();\n    }\n}"
        );
    }

    #[test]
    fn blank_line_record_stays_text() {
        let src = "\
Naam: Jan de Vries

Adres: Hoofdstraat 45, 9711 AB Groningen

E-mailadres: jan.devries@email.nl

Telefoonnummer: 06-12345678

Geboortedatum: 12 mei 1984

Salaris: € 3.450";
        assert_ne!(detect(src), FormatKind::Yaml);
        let formatted = super::format_text(src);
        assert!(formatted.contains("Adres:"));
        assert!(formatted.contains("Telefoonnummer:"));
        assert!(formatted.contains("jan.devries@email.nl"));
    }
}
