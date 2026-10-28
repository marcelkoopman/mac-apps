//! Text ▾ › Code: format and minify YAML, XML and SQL, and convert JSON ⇄ YAML. Each step makes
//! a new copy, like every Text ▾ step (Format JSON… covers JSON's own format and compress).
//!
//! No new parser: YAML goes through `serde_norway` ([`crate::transform`]), JSON through
//! `serde_json` with the guards of [`super::json_style`], XML through the card's own tokenizer
//! ([`crate::format::pretty_xml`]). SQL is keyword based only: keywords in upper case, a new
//! line per clause and an indent, never parsed. TOML has no step: its crate is not a direct
//! dependency of CopyCraft. A step that would lose something (a YAML comment, an XML CDATA
//! section) does nothing. Neither the text nor the result is ever logged.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::curl::CurlTarget;

/// The language Text ▾ › Code offers steps for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLang {
    Json,
    Yaml,
    Xml,
    Sql,
    /// A `curl` command ([`super::curl`]).
    Curl,
}

impl CodeLang {
    /// The language of `text`, detected as `kind` by the card.
    pub fn of(text: &str, kind: crate::format::FormatKind) -> Option<CodeLang> {
        use crate::format::FormatKind;
        if super::curl::looks_like_curl(text) && super::curl::parse(text).is_some() {
            return Some(CodeLang::Curl);
        }
        match kind {
            FormatKind::Json => Some(CodeLang::Json),
            FormatKind::Yaml => Some(CodeLang::Yaml),
            FormatKind::Xml | FormatKind::Svg => Some(CodeLang::Xml),
            FormatKind::Text | FormatKind::Plain if looks_like_sql(text) => Some(CodeLang::Sql),
            _ => None,
        }
    }

    /// The steps for this language, in menu order.
    pub fn ops(self) -> &'static [CodeOp] {
        match self {
            CodeLang::Json => &[CodeOp::JsonToYaml],
            CodeLang::Yaml => &[CodeOp::FormatYaml, CodeOp::MinifyYaml, CodeOp::YamlToJson],
            CodeLang::Xml => &[CodeOp::FormatXml, CodeOp::MinifyXml],
            CodeLang::Sql => &[CodeOp::FormatSql, CodeOp::MinifySql],
            CodeLang::Curl => &[
                CodeOp::Curl(CurlTarget::Fetch),
                CodeOp::Curl(CurlTarget::Python),
                CodeOp::Curl(CurlTarget::Httpie),
                CodeOp::Curl(CurlTarget::Reqwest),
                CodeOp::Curl(CurlTarget::Swift),
            ],
        }
    }
}

/// One step of Text ▾ › Code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeOp {
    FormatYaml,
    MinifyYaml,
    YamlToJson,
    JsonToYaml,
    FormatXml,
    MinifyXml,
    FormatSql,
    MinifySql,
    /// A curl command as code in another language.
    Curl(CurlTarget),
}

impl CodeOp {
    /// Its menu item.
    pub fn title(self) -> &'static str {
        use crate::locale::t;
        match self {
            CodeOp::FormatYaml => t("code_format_yaml"),
            CodeOp::MinifyYaml => t("code_minify_yaml"),
            CodeOp::YamlToJson => t("code_yaml_to_json"),
            CodeOp::JsonToYaml => t("code_json_to_yaml"),
            CodeOp::FormatXml => t("code_format_xml"),
            CodeOp::MinifyXml => t("code_minify_xml"),
            CodeOp::FormatSql => t("code_format_sql"),
            CodeOp::MinifySql => t("code_minify_sql"),
            CodeOp::Curl(CurlTarget::Fetch) => t("code_curl_fetch"),
            CodeOp::Curl(CurlTarget::Python) => t("code_curl_python"),
            CodeOp::Curl(CurlTarget::Httpie) => t("code_curl_httpie"),
            CodeOp::Curl(CurlTarget::Reqwest) => t("code_curl_reqwest"),
            CodeOp::Curl(CurlTarget::Swift) => t("code_curl_swift"),
        }
    }

    /// A conversion (another language), not a layout of the same one: a line before it.
    pub fn converts(self) -> bool {
        matches!(self, CodeOp::YamlToJson | CodeOp::JsonToYaml)
    }

    /// The step's text; `None` when it does not fit the text or would lose something.
    pub fn apply(self, text: &str) -> Option<String> {
        match self {
            CodeOp::FormatYaml => {
                if has_yaml_comment(text) {
                    return None;
                }
                crate::transform::pretty_yaml(text).ok()
            }
            CodeOp::MinifyYaml => minify_yaml(text),
            CodeOp::YamlToJson => crate::transform::yaml_to_json(text),
            CodeOp::JsonToYaml => json_to_yaml(text),
            CodeOp::FormatXml => xml_safe(text).then(|| crate::format::pretty_xml(text)),
            CodeOp::MinifyXml => minify_xml(text),
            CodeOp::FormatSql => format_sql(text),
            CodeOp::MinifySql => minify_sql(text),
            CodeOp::Curl(target) => super::curl::convert(text, target),
        }
    }
}

/// A `#` comment line or a ` #` comment after a value: `serde_norway` would drop it.
fn has_yaml_comment(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('#') || (line.contains(" #") && !line.contains(['"', '\'']))
    })
}

/// YAML on one line, as a flow mapping or sequence (JSON is YAML). Anchors are expanded; a
/// comment or several documents leave it as it is.
fn minify_yaml(text: &str) -> Option<String> {
    if has_yaml_comment(text) {
        return None;
    }
    let pretty = crate::transform::yaml_to_json(text)?;
    let value: serde_json::Value = serde_json::from_str(&pretty).ok()?;
    serde_json::to_string(&value).ok()
}

/// JSON as block YAML, in the copied key order. JSON that Format JSON… refuses (a key twice,
/// a number too big to carry) does nothing here too.
fn json_to_yaml(text: &str) -> Option<String> {
    let style = super::json_style::JsonStyle {
        compress: true,
        sort: false,
    };
    let compact = super::json_style::restyle(text, style).ok()?;
    let value: serde_json::Value = serde_json::from_str(&compact).ok()?;
    serde_norway::to_string(&value).ok()
}

/// CDATA and `xml:space` text must keep its exact spacing: the tokenizer does not.
fn xml_safe(text: &str) -> bool {
    !text.contains("<![CDATA[") && !text.contains("xml:space") && !text.contains("<pre")
}

/// XML without the whitespace between tags; text runs keep single spaces. Comments stay.
fn minify_xml(text: &str) -> Option<String> {
    use crate::format::XmlToken;
    if !xml_safe(text) {
        return None;
    }
    let tokens = crate::format::xml_tokens(text.trim());
    if tokens.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    for token in &tokens {
        match token {
            XmlToken::Decl(s)
            | XmlToken::Comment(s)
            | XmlToken::Open(s)
            | XmlToken::Close(s)
            | XmlToken::Empty(s) => out.push_str(s),
            XmlToken::Text(s) => out.push_str(&crate::format::collapse_xml_text(s)),
        }
    }
    Some(out)
}

// ---- SQL ----

/// Keywords that are written in upper case.
const SQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "as",
    "asc",
    "between",
    "by",
    "case",
    "column",
    "constraint",
    "create",
    "cross",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "exists",
    "foreign",
    "from",
    "full",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "merge",
    "natural",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "set",
    "table",
    "then",
    "union",
    "unique",
    "update",
    "using",
    "values",
    "view",
    "when",
    "where",
    "with",
    "except",
    "over",
    "partition",
    "ilike",
    "true",
    "false",
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "coalesce",
    "cast",
    "fetch",
    "first",
    "next",
    "rows",
    "only",
    "recursive",
    "replace",
    "truncate",
    "lateral",
    "window",
];

/// Words that start a clause on a line of their own (the first word of a multi-word clause).
const SQL_CLAUSES: &[&str] = &[
    "select",
    "from",
    "where",
    "group",
    "order",
    "having",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
    "insert",
    "values",
    "update",
    "set",
    "delete",
    "returning",
    "with",
    "join",
    "left",
    "right",
    "inner",
    "full",
    "cross",
    "natural",
    "fetch",
    "window",
];

/// The first words a SQL statement starts with.
const SQL_STARTS: &[&str] = &[
    "select", "with", "insert", "update", "delete", "create", "alter", "drop", "merge", "truncate",
];

/// Text that reads as SQL: it starts with a statement word and has a second keyword a
/// statement needs (`FROM`, `INTO`, `SET`, `TABLE`, `VALUES`, `VIEW`, `INDEX`).
pub fn looks_like_sql(text: &str) -> bool {
    let tokens = sql_tokens(text);
    let mut words = tokens.iter().filter_map(|token| match token {
        SqlToken::Word(word) => Some(word.to_ascii_lowercase()),
        _ => None,
    });
    let Some(first) = words.next() else {
        return false;
    };
    // Prose ("Select the file from the list.") has no SQL punctuation or ends in a period.
    if !SQL_STARTS.contains(&first.as_str()) || text.trim_end().ends_with('.') {
        return false;
    }
    let needs = ["from", "into", "set", "table", "values", "view", "index"];
    let rest: Vec<String> = words.collect();
    rest.iter().any(|word| needs.contains(&word.as_str()))
        && (first != "select"
            || tokens.iter().any(|token| {
                matches!(token, SqlToken::Punct(c) if matches!(c, ',' | '*' | '=' | '(' | ';'))
            })
            || rest.len() <= 4)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SqlToken {
    /// A keyword or a name.
    Word(String),
    /// A quoted string or name, or a number, kept exactly.
    Literal(String),
    /// `-- …` or `/* … */`.
    Comment(String),
    /// One punctuation character.
    Punct(char),
    /// `<=`, `<>`, `!=`, `>=`, `||`, `::`.
    Operator(String),
}

fn sql_tokens(text: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            out.push(SqlToken::Comment(chars[start..i].iter().collect()));
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = i;
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            out.push(SqlToken::Comment(chars[start..i].iter().collect()));
        } else if matches!(c, '\'' | '"' | '`') || c == '[' {
            let close = if c == '[' { ']' } else { c };
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == close {
                    // `''` inside a string is a quote.
                    if close != ']' && chars.get(i + 1) == Some(&close) {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(chars.len());
            out.push(SqlToken::Literal(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            out.push(SqlToken::Literal(chars[start..i].iter().collect()));
        } else if c.is_alphanumeric() || matches!(c, '_' | '@' | '$' | '#' | ':' | '?') {
            if c == ':' && chars.get(i + 1) == Some(&':') {
                out.push(SqlToken::Operator("::".into()));
                i += 2;
                continue;
            }
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '#'))
            {
                i += 1;
            }
            out.push(SqlToken::Word(chars[start..i].iter().collect()));
        } else {
            let pair: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if matches!(pair.as_str(), "<=" | ">=" | "<>" | "!=" | "||") {
                out.push(SqlToken::Operator(pair));
                i += 2;
            } else {
                out.push(SqlToken::Punct(c));
                i += 1;
            }
        }
    }
    out
}

/// A keyword in upper case; a name (also one after `.`) as copied.
fn sql_word(word: &str, after_dot: bool) -> String {
    if !after_dot && SQL_KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
        word.to_ascii_uppercase()
    } else {
        word.to_string()
    }
}

/// Whether a space goes between `before` and `token` on one line.
fn sql_space(before2: Option<&SqlToken>, before: Option<&SqlToken>, token: &SqlToken) -> bool {
    let Some(before) = before else {
        return false;
    };
    // `INTO t (a, b)`, `TABLE t (id …)`: a name before its column list, not a call.
    let named = matches!(before2, Some(SqlToken::Word(w)) if matches!(w.to_ascii_lowercase().as_str(), "into" | "table" | "exists" | "view" | "on"));
    match (before, token) {
        (_, SqlToken::Punct(',' | ')' | ';' | '.')) => false,
        (SqlToken::Punct('(' | '.'), _) => false,
        (SqlToken::Operator(op), _) | (_, SqlToken::Operator(op)) if op == "::" => false,
        // A function call: `count(*)`, not `count (*)`; but `IN (`, `VALUES (`, `AS (`.
        (SqlToken::Word(word), SqlToken::Punct('(')) => {
            named
                || SQL_KEYWORDS.contains(&word.to_ascii_lowercase().as_str())
                    && !matches!(
                        word.to_ascii_lowercase().as_str(),
                        "count" | "sum" | "avg" | "min" | "max" | "coalesce" | "cast" | "replace"
                    )
        }
        _ => true,
    }
}

/// SQL on one line, single spaces, comments out. `None` when it does not read as SQL.
pub fn minify_sql(text: &str) -> Option<String> {
    if !looks_like_sql(text) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut previous: Option<SqlToken> = None;
    let mut before2: Option<SqlToken> = None;
    for token in sql_tokens(text) {
        if matches!(token, SqlToken::Comment(_)) {
            continue;
        }
        if sql_space(before2.as_ref(), previous.as_ref(), &token) {
            out.push(' ');
        }
        let after_dot = matches!(previous, Some(SqlToken::Punct('.')));
        push_sql(&mut out, &token, after_dot);
        before2 = previous.replace(token);
    }
    Some(out)
}

fn push_sql(out: &mut String, token: &SqlToken, after_dot: bool) {
    match token {
        SqlToken::Word(word) => out.push_str(&sql_word(word, after_dot)),
        SqlToken::Literal(text) | SqlToken::Comment(text) | SqlToken::Operator(text) => {
            out.push_str(text)
        }
        SqlToken::Punct(c) => out.push(*c),
    }
}

/// SQL laid out: keywords in upper case, every clause on a new line, the items of a
/// `SELECT`, `SET`, `GROUP BY` or `ORDER BY` list one per line and `AND`/`OR` indented, a
/// subquery one level in. Comments stay. `None` when it does not read as SQL.
pub fn format_sql(text: &str) -> Option<String> {
    if !looks_like_sql(text) {
        return None;
    }
    const INDENT: &str = "    ";
    let tokens = sql_tokens(text);
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    // The indent of the clauses at each subquery level; brackets that are not a subquery
    // are only counted.
    let mut levels: Vec<usize> = vec![0];
    let mut brackets: Vec<bool> = Vec::new();
    let mut base = 0usize;
    let mut previous: Option<SqlToken> = None;
    // A comma breaks the line in a list clause at its own bracket depth.
    let mut list_clause = false;
    let mut plain_depth = 0usize;
    let newline = |out: &mut String, indent: usize| {
        while out.ends_with(' ') {
            out.pop();
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        for _ in 0..indent {
            out.push_str(INDENT);
        }
    };
    for (at, token) in tokens.iter().enumerate() {
        let lower = match token {
            SqlToken::Word(word) => word.to_ascii_lowercase(),
            _ => String::new(),
        };
        let before2 = at.checked_sub(2).and_then(|i| tokens.get(i));
        let after_dot = matches!(previous, Some(SqlToken::Punct('.')));
        let prev_lower = match &previous {
            Some(SqlToken::Word(word)) => word.to_ascii_lowercase(),
            _ => String::new(),
        };
        // A clause word starts a line, unless it continues one (`LEFT OUTER JOIN`, `ORDER BY`,
        // `UNION ALL`, `INSERT INTO`, `DELETE FROM`).
        let continues = matches!(
            (prev_lower.as_str(), lower.as_str()),
            (
                "left" | "right" | "full" | "inner" | "cross" | "natural" | "outer",
                "join" | "outer"
            ) | ("delete", "from")
                | ("union" | "except" | "intersect", "all")
        );
        let clause = !after_dot
            && SQL_CLAUSES.contains(&lower.as_str())
            && !continues
            // `SET` inside `UPDATE … SET`, yes; the `ON CONFLICT DO UPDATE SET` stays too.
            && plain_depth == 0;
        if let SqlToken::Comment(comment) = token {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(comment);
            if comment.starts_with("--") {
                newline(&mut out, base + usize::from(list_clause));
            }
            previous = Some(token.clone());
            continue;
        }
        if clause {
            newline(&mut out, base);
            list_clause = matches!(lower.as_str(), "select" | "set" | "group" | "order");
            push_sql(&mut out, token, after_dot);
            previous = Some(token.clone());
            continue;
        }
        if plain_depth == 0 && !after_dot && matches!(lower.as_str(), "and" | "or") {
            // `BETWEEN a AND b` keeps its AND.
            let between = tokens[..at]
                .iter()
                .rev()
                .take(4)
                .any(|t| matches!(t, SqlToken::Word(w) if w.eq_ignore_ascii_case("between")));
            if !between {
                newline(&mut out, base + 1);
                push_sql(&mut out, token, false);
                previous = Some(token.clone());
                continue;
            }
        }
        match token {
            SqlToken::Punct('(') => {
                let subquery = matches!(
                    tokens.get(at + 1),
                    Some(SqlToken::Word(w)) if w.eq_ignore_ascii_case("select") || w.eq_ignore_ascii_case("with")
                );
                if sql_space(before2, previous.as_ref(), token) {
                    out.push(' ');
                }
                out.push('(');
                brackets.push(subquery);
                if subquery {
                    levels.push(base);
                    base += 1;
                    list_clause = false;
                } else {
                    plain_depth += 1;
                }
            }
            SqlToken::Punct(')') => {
                match brackets.pop() {
                    Some(true) => {
                        base = levels.pop().unwrap_or(0);
                        newline(&mut out, base);
                        list_clause = false;
                    }
                    Some(false) => plain_depth = plain_depth.saturating_sub(1),
                    None => {}
                }
                out.push(')');
            }
            SqlToken::Punct(',') if list_clause && plain_depth == 0 => {
                out.push(',');
                newline(&mut out, base + 1);
                previous = None;
                continue;
            }
            SqlToken::Punct(';') => {
                out.push(';');
                out.push('\n');
                base = 0;
                levels = vec![0];
                brackets.clear();
                plain_depth = 0;
                list_clause = false;
                previous = None;
                continue;
            }
            _ => {
                // The first item of a list goes on the next line, indented.
                let starts_list = list_clause
                    && plain_depth == 0
                    && matches!(&previous, Some(SqlToken::Word(w)) if matches!(w.to_ascii_lowercase().as_str(), "select" | "set" | "by" | "distinct"))
                    && !matches!(token, SqlToken::Word(w) if matches!(w.to_ascii_lowercase().as_str(), "by" | "distinct" | "all"));
                if starts_list {
                    newline(&mut out, base + 1);
                } else if sql_space(before2, previous.as_ref(), token)
                    && !out.ends_with(INDENT)
                    && !out.ends_with('\n')
                {
                    out.push(' ');
                }
                push_sql(&mut out, token, after_dot);
            }
        }
        previous = Some(token.clone());
    }
    while out.ends_with([' ', '\n']) {
        out.pop();
    }
    Some(out)
}

#[cfg(test)]
#[path = "code_style_tests.rs"]
mod tests;
