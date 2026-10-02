use crate::format::FormatKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Key,
    String,
    Number,
    Keyword,
    Function,
    Type,
    Macro,
    Comment,
    Punct,
    Text,
}

pub fn tokens(source: &str, kind: FormatKind) -> Vec<(TokenKind, String)> {
    match kind {
        FormatKind::Json => tokenize_json(source),
        FormatKind::Yaml => tokenize_yaml(source),
        FormatKind::Rust => tokenize_rust(source),
        FormatKind::Java => tokenize_code(source, kind),
        FormatKind::Python => tokenize_python(source),
        FormatKind::Xml | FormatKind::Html => tokenize_xml(source),
        FormatKind::Markdown => tokenize_markdown(source),
        FormatKind::Csv => tokenize_csv(source),
        FormatKind::Tsv => tokenize_tsv(source),
        FormatKind::Dataframe => tokenize_dataframe(source),
        _ if looks_redacted(source) => tokenize_redacted(source),
        _ => vec![(TokenKind::Text, source.to_string())],
    }
}

fn tokenize_xml(source: &str) -> Vec<(TokenKind, String)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            if starts_prefix(&chars, i, "<!--") {
                let mut j = i + 4;
                while j + 2 < chars.len()
                    && !(chars[j] == '-' && chars[j + 1] == '-' && chars[j + 2] == '>')
                {
                    j += 1;
                }
                j = (j + 3).min(chars.len());
                out.push((TokenKind::Comment, chars[i..j].iter().collect()));
                i = j;
                continue;
            }
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j >= chars.len() {
                out.push((TokenKind::Text, chars[i..].iter().collect()));
                break;
            }
            let tag: String = chars[i..=j].iter().collect();
            color_xml_tag(&mut out, &tag);
            i = j + 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && chars[j] != '<' {
            j += 1;
        }
        out.push((TokenKind::Text, chars[i..j].iter().collect()));
        i = j;
    }
    out
}

fn color_xml_tag(out: &mut Vec<(TokenKind, String)>, tag: &str) {
    let chars: Vec<char> = tag.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut j = i + 1;
            while j < chars.len() && chars[j] != quote {
                j += 1;
            }
            if j < chars.len() {
                j += 1;
            }
            out.push((TokenKind::String, chars[i..j].iter().collect()));
            i = j;
            continue;
        }
        if ch == '<' || ch == '>' || ch == '/' || ch == '?' || ch == '!' {
            out.push((TokenKind::Punct, ch.to_string()));
            i += 1;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' || ch == ':' {
            let (token, next) = take_while(&chars, i, |c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-')
            });
            let kind = if (i > 0 && chars[i - 1] == '<')
                || (i > 1 && chars[i - 1] == '/' && chars[i - 2] == '<')
            {
                TokenKind::Keyword
            } else {
                TokenKind::Key
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
}

fn tokenize_json(source: &str) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let (token, next) = take_string(&chars, i);
            let after = skip_ws(&chars, next);
            let kind = if after < chars.len() && chars[after] == ':' {
                TokenKind::Key
            } else {
                TokenKind::String
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let (token, next) = take_while(&chars, i, |c| {
                c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')
            });
            out.push((TokenKind::Number, token));
            i = next;
            continue;
        }
        if starts_with(&chars, i, "true")
            || starts_with(&chars, i, "false")
            || starts_with(&chars, i, "null")
        {
            let word = if starts_with(&chars, i, "true") {
                "true"
            } else if starts_with(&chars, i, "false") {
                "false"
            } else {
                "null"
            };
            out.push((TokenKind::Keyword, word.to_string()));
            i += word.chars().count();
            continue;
        }
        if matches!(ch, '{' | '}' | '[' | ']' | ':' | ',') {
            out.push((TokenKind::Punct, ch.to_string()));
            i += 1;
            continue;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
    out
}

fn tokenize_rust(source: &str) -> Vec<(TokenKind, String)> {
    let keywords = [
        "fn", "let", "mut", "pub", "impl", "struct", "enum", "match", "if", "else", "use", "mod",
        "return", "async", "await", "self", "Self", "crate", "const", "static", "as", "where",
        "for", "in", "loop", "while", "break", "continue", "ref", "move",
    ];
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut after_fn = false;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            let (token, next) = take_while(&chars, i, |c| c != '\n');
            out.push((TokenKind::Comment, token));
            i = next;
            continue;
        }
        if ch == '"' {
            let (token, next) = take_string(&chars, i);
            out.push((TokenKind::String, token));
            i = next;
            after_fn = false;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let (mut token, mut next) =
                take_while(&chars, i, |c| c.is_ascii_alphanumeric() || c == '_');
            if next < chars.len() && chars[next] == '!' {
                token.push('!');
                next += 1;
                out.push((TokenKind::Macro, token));
                after_fn = false;
                i = next;
                continue;
            }
            let kind = if after_fn {
                after_fn = false;
                TokenKind::Function
            } else if keywords.contains(&token.as_str()) {
                after_fn = token == "fn";
                TokenKind::Keyword
            } else if token.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        if ch.is_ascii_digit() {
            let (token, next) = take_while(&chars, i, |c| c.is_ascii_digit() || c == '.');
            out.push((TokenKind::Number, token));
            i = next;
            after_fn = false;
            continue;
        }
        if !ch.is_whitespace() {
            after_fn = false;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
    out
}

fn tokenize_code(source: &str, kind: FormatKind) -> Vec<(TokenKind, String)> {
    let keywords: &[&str] = match kind {
        FormatKind::Java => &[
            "public",
            "private",
            "protected",
            "class",
            "static",
            "void",
            "int",
            "long",
            "boolean",
            "return",
            "if",
            "else",
            "new",
            "package",
            "import",
            "final",
            "this",
        ],
        _ => &[],
    };
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let (token, next) = take_string(&chars, i);
            out.push((TokenKind::String, token));
            i = next;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let (token, next) = take_while(&chars, i, |c| c.is_ascii_alphanumeric() || c == '_');
            let kind = if keywords.contains(&token.as_str()) {
                TokenKind::Keyword
            } else if token.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        if ch.is_ascii_digit() {
            let (token, next) = take_while(&chars, i, |c| c.is_ascii_digit() || c == '.');
            out.push((TokenKind::Number, token));
            i = next;
            continue;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
    out
}

/// Python: keywords, `def` / `class` names, decorators (as macros), `#` comments, numbers and
/// strings with any prefix (`f`, `r`, `b`, `rb`, …), triple-quoted included.
fn tokenize_python(source: &str) -> Vec<(TokenKind, String)> {
    const KEYWORDS: [&str; 38] = [
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield", "match", "case", "self",
    ];
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut name_next = None;
    // Only whitespace so far on this line (decorators start a line).
    let mut line_start = true;
    while i < chars.len() {
        let ch = chars[i];
        let at_line_start = line_start;
        if ch == '\n' {
            line_start = true;
        } else if !ch.is_whitespace() {
            line_start = false;
        }
        if ch == '#' {
            let (token, next) = take_while(&chars, i, |c| c != '\n');
            out.push((TokenKind::Comment, token));
            i = next;
            continue;
        }
        if ch == '@' && at_line_start {
            let (token, next) = take_while(&chars, i + 1, |c| {
                c.is_alphanumeric() || c == '_' || c == '.'
            });
            out.push((TokenKind::Macro, format!("@{token}")));
            i = next;
            continue;
        }
        if ch == '"' || ch == '\'' {
            let end = crate::brackets::skip_python_string(&chars, i);
            out.push((TokenKind::String, chars[i..end].iter().collect()));
            i = end;
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let (token, next) = take_while(&chars, i, |c| c.is_alphanumeric() || c == '_');
            // A string prefix: f"…", rb'…'.
            let prefix = token.len() <= 2
                && token
                    .chars()
                    .all(|c| matches!(c.to_ascii_lowercase(), 'f' | 'r' | 'b' | 'u'))
                && matches!(chars.get(next), Some('"' | '\''));
            if prefix {
                let end = crate::brackets::skip_python_string(&chars, next);
                out.push((TokenKind::String, chars[i..end].iter().collect()));
                i = end;
                continue;
            }
            let kind = if let Some(kind) = name_next.take() {
                kind
            } else if KEYWORDS.contains(&token.as_str()) {
                name_next = match token.as_str() {
                    "def" => Some(TokenKind::Function),
                    "class" => Some(TokenKind::Type),
                    _ => None,
                };
                TokenKind::Keyword
            } else if token.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                TokenKind::Type
            } else {
                TokenKind::Text
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        if ch.is_ascii_digit() {
            let (token, next) = take_while(&chars, i, |c| {
                c.is_ascii_alphanumeric() || c == '_' || c == '.'
            });
            out.push((TokenKind::Number, token));
            i = next;
            continue;
        }
        if !ch.is_whitespace() {
            name_next = None;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
    out
}

fn take_string(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut token = String::from("\"");
    while i < chars.len() {
        let ch = chars[i];
        token.push(ch);
        if ch == '\\' && i + 1 < chars.len() {
            token.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if ch == '"' {
            return (token, i + 1);
        }
        i += 1;
    }
    (token, i)
}

fn take_while(chars: &[char], start: usize, pred: impl Fn(char) -> bool) -> (String, usize) {
    let mut i = start;
    let mut token = String::new();
    while i < chars.len() && pred(chars[i]) {
        token.push(chars[i]);
        i += 1;
    }
    (token, i)
}

fn skip_ws(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn starts_prefix(chars: &[char], i: usize, prefix: &str) -> bool {
    let w: Vec<char> = prefix.chars().collect();
    i + w.len() <= chars.len() && chars[i..i + w.len()] == w[..]
}

fn starts_with(chars: &[char], i: usize, word: &str) -> bool {
    let w: Vec<char> = word.chars().collect();
    if i + w.len() > chars.len() {
        return false;
    }
    chars[i..i + w.len()] == w[..]
        && (i + w.len() == chars.len() || !chars[i + w.len()].is_ascii_alphanumeric())
}

fn tokenize_markdown(source: &str) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut fence_char = '`';
    let mut fence_len = 0usize;
    for line in source.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, true),
            None => (line, false),
        };
        if in_fence {
            if md_closes_fence(body, fence_char, fence_len) {
                in_fence = false;
                color_fence_edge(&mut out, body);
            } else {
                push_token(&mut out, TokenKind::Text, body);
            }
        } else if let Some((marker, len)) = md_opening_fence(body) {
            in_fence = true;
            fence_char = marker;
            fence_len = len;
            color_fence_edge(&mut out, body);
        } else if md_heading(body) {
            color_heading(&mut out, body);
        } else if md_rule(body) {
            push_token(&mut out, TokenKind::Punct, body);
        } else {
            color_markdown_line(&mut out, body);
        }
        if newline {
            push_token(&mut out, TokenKind::Text, "\n");
        }
    }
    out
}

fn push_token(out: &mut Vec<(TokenKind, String)>, kind: TokenKind, text: &str) {
    if !text.is_empty() {
        out.push((kind, text.to_string()));
    }
}

fn color_fence_edge(out: &mut Vec<(TokenKind, String)>, line: &str) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() && chars[index] == ' ' {
        index += 1;
    }
    push_slice(out, TokenKind::Text, &chars, 0, index);
    let marker = chars.get(index).copied().unwrap_or('`');
    let mut end = index;
    while end < chars.len() && chars[end] == marker {
        end += 1;
    }
    push_slice(out, TokenKind::Punct, &chars, index, end);
    let mut lang = end;
    while lang < chars.len() && chars[lang].is_whitespace() {
        lang += 1;
    }
    push_slice(out, TokenKind::Text, &chars, end, lang);
    push_slice(out, TokenKind::Type, &chars, lang, chars.len());
}

fn color_heading(out: &mut Vec<(TokenKind, String)>, line: &str) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() && chars[index] == ' ' {
        index += 1;
    }
    push_slice(out, TokenKind::Text, &chars, 0, index);
    let mut hashes = index;
    while hashes < chars.len() && chars[hashes] == '#' {
        hashes += 1;
    }
    push_slice(out, TokenKind::Punct, &chars, index, hashes);
    let mut text = hashes;
    while text < chars.len() && chars[text].is_whitespace() {
        text += 1;
    }
    push_slice(out, TokenKind::Text, &chars, hashes, text);
    push_slice(out, TokenKind::Keyword, &chars, text, chars.len());
}

fn color_markdown_line(out: &mut Vec<(TokenKind, String)>, line: &str) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() && chars[index] == ' ' {
        index += 1;
    }
    push_slice(out, TokenKind::Text, &chars, 0, index);
    let quote = take_quote_prefix(&chars, index);
    push_slice(out, TokenKind::Punct, &chars, index, quote);
    index = quote;
    if let Some(end) = list_marker_end(&chars, index) {
        push_slice(out, TokenKind::Punct, &chars, index, end);
        index = end;
    }
    let pipes = line.matches('|').count() >= 2;
    color_inlines(out, &chars, index, pipes);
}

fn color_inlines(
    out: &mut Vec<(TokenKind, String)>,
    chars: &[char],
    mut index: usize,
    pipes: bool,
) {
    while index < chars.len() {
        if chars[index] == '\\' && index + 1 < chars.len() {
            push_slice(out, TokenKind::Text, chars, index, index + 2);
            index += 2;
            continue;
        }
        if chars[index] == '`'
            && let Some(end) = inline_code_end(chars, index)
        {
            push_slice(out, TokenKind::String, chars, index, end);
            index = end;
            continue;
        }
        if chars[index] == '!'
            && index + 1 < chars.len()
            && chars[index + 1] == '['
            && let Some(end) = color_link(out, chars, index)
        {
            index = end;
            continue;
        }
        if chars[index] == '['
            && let Some(end) = color_link(out, chars, index)
        {
            index = end;
            continue;
        }
        if let Some((marker, inner, end)) = match_emphasis(chars, index) {
            push_slice(out, TokenKind::Punct, chars, index, index + marker);
            let kind = if marker == 2 {
                TokenKind::Keyword
            } else {
                TokenKind::Type
            };
            push_slice(out, kind, chars, index + marker, inner);
            push_slice(out, TokenKind::Punct, chars, inner, end);
            index = end;
            continue;
        }
        if pipes && chars[index] == '|' {
            push_slice(out, TokenKind::Punct, chars, index, index + 1);
            index += 1;
            continue;
        }
        push_slice(out, TokenKind::Text, chars, index, index + 1);
        index += 1;
    }
}

fn color_link(out: &mut Vec<(TokenKind, String)>, chars: &[char], start: usize) -> Option<usize> {
    let bang = chars[start] == '!';
    let bracket = if bang { start + 1 } else { start };
    if bracket >= chars.len() || chars[bracket] != '[' {
        return None;
    }
    let label = bracket + 1;
    let mut end = label;
    while end < chars.len() && chars[end] != ']' && chars[end] != '\n' && chars[end] != '[' {
        end += 1;
    }
    if end >= chars.len() || chars[end] != ']' || end == label {
        return None;
    }
    let paren = end + 1;
    if paren >= chars.len() || chars[paren] != '(' {
        return None;
    }
    let url_start = paren + 1;
    let mut url = url_start;
    while url < chars.len() && chars[url] != ')' && chars[url] != '\n' {
        url += 1;
    }
    if url >= chars.len() || chars[url] != ')' || url == url_start {
        return None;
    }
    if bang {
        push_slice(out, TokenKind::Punct, chars, start, start + 1);
    }
    push_slice(out, TokenKind::Punct, chars, bracket, bracket + 1);
    push_slice(out, TokenKind::Key, chars, label, end);
    push_slice(out, TokenKind::Punct, chars, end, url_start);
    push_slice(out, TokenKind::String, chars, url_start, url);
    push_slice(out, TokenKind::Punct, chars, url, url + 1);
    Some(url + 1)
}

fn inline_code_end(chars: &[char], start: usize) -> Option<usize> {
    let mut len = 0;
    while start + len < chars.len() && chars[start + len] == '`' {
        len += 1;
    }
    if len == 0 {
        return None;
    }
    let mut index = start + len;
    while index < chars.len() {
        if chars[index] == '\n' {
            return None;
        }
        if chars[index] == '`' {
            let mut close = 0;
            while index + close < chars.len() && chars[index + close] == '`' {
                close += 1;
            }
            if close == len {
                return Some(index + close);
            }
            index += close;
            continue;
        }
        index += 1;
    }
    None
}

fn match_emphasis(chars: &[char], index: usize) -> Option<(usize, usize, usize)> {
    let marker = chars[index];
    if marker != '*' && marker != '_' {
        return None;
    }
    if marker == '_' && index > 0 && chars[index - 1].is_ascii_alphanumeric() {
        return None;
    }
    let doubled = index + 1 < chars.len() && chars[index + 1] == marker;
    let len = if doubled { 2 } else { 1 };
    let inner = index + len;
    if inner >= chars.len() || chars[inner].is_whitespace() {
        return None;
    }
    let mut cursor = inner;
    while cursor < chars.len() && chars[cursor] != '\n' {
        if chars[cursor] == '`' {
            return None;
        }
        if chars[cursor] == marker {
            let close_doubled = cursor + 1 < chars.len() && chars[cursor + 1] == marker;
            if doubled == close_doubled && cursor > inner && !chars[cursor - 1].is_whitespace() {
                let close = if doubled { 2 } else { 1 };
                let after = cursor + close;
                if marker == '_' && after < chars.len() && chars[after].is_ascii_alphanumeric() {
                    return None;
                }
                return Some((len, cursor, after));
            }
        }
        cursor += 1;
    }
    None
}

fn take_quote_prefix(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index] == '>' {
        index += 1;
        if index < chars.len() && chars[index] == ' ' {
            index += 1;
        }
    }
    index
}

fn list_marker_end(chars: &[char], index: usize) -> Option<usize> {
    if index >= chars.len() {
        return None;
    }
    if matches!(chars[index], '-' | '*' | '+') && index + 1 < chars.len() && chars[index + 1] == ' '
    {
        return Some(index + 2);
    }
    let mut cursor = index;
    while cursor < chars.len() && chars[cursor].is_ascii_digit() {
        cursor += 1;
    }
    if cursor == index || cursor - index > 9 || cursor + 1 >= chars.len() {
        return None;
    }
    if (chars[cursor] == '.' || chars[cursor] == ')') && chars[cursor + 1] == ' ' {
        return Some(cursor + 2);
    }
    None
}

fn push_slice(
    out: &mut Vec<(TokenKind, String)>,
    kind: TokenKind,
    chars: &[char],
    from: usize,
    to: usize,
) {
    if from < to && to <= chars.len() {
        out.push((kind, chars[from..to].iter().collect()));
    }
}

fn md_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return false;
    }
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    (1..=6).contains(&hashes)
        && trimmed[hashes..].starts_with(' ')
        && !trimmed[hashes..].trim().is_empty()
}

fn md_rule(line: &str) -> bool {
    let marks: Vec<char> = line
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    marks.len() >= 3
        && marks.iter().all(|ch| *ch == marks[0])
        && matches!(marks[0], '-' | '*' | '_')
}

fn md_opening_fence(line: &str) -> Option<(char, usize)> {
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

fn md_closes_fence(line: &str, marker: char, len: usize) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == marker) && trimmed.chars().count() >= len
}

include!("highlight_extra.rs");

#[cfg(test)]
mod tests {
    use super::{TokenKind, tokens};
    use crate::format::FormatKind;

    #[test]
    fn python_marks_keywords_strings_comments_decorators_and_numbers() {
        let src = "@app.route(\"/\")\ndef index(n=0x1F):  # home\n    msg = f\"hi {n}\"\n    doc = \"\"\"a\n# not a comment\"\"\"\n    return None if n else 1_000.5\nclass Page(Base):\n    pass\n";
        let toks = tokens(src, FormatKind::Python);
        let has = |kind: TokenKind, text: &str| toks.iter().any(|(k, t)| *k == kind && t == text);
        assert!(has(TokenKind::Macro, "@app.route"));
        assert!(has(TokenKind::Keyword, "def"));
        assert!(has(TokenKind::Function, "index"));
        assert!(has(TokenKind::Number, "0x1F"));
        assert!(has(TokenKind::Comment, "# home"));
        assert!(has(TokenKind::String, "f\"hi {n}\""));
        assert!(has(TokenKind::String, "\"\"\"a\n# not a comment\"\"\""));
        assert!(has(TokenKind::Keyword, "None"));
        assert!(has(TokenKind::Number, "1_000.5"));
        assert!(has(TokenKind::Type, "Page"));
        assert!(has(TokenKind::Keyword, "pass"));
        let painted: String = toks.into_iter().map(|(_, text)| text).collect();
        assert_eq!(painted, src);
    }

    #[test]
    fn json_marks_keys_and_numbers() {
        let toks = tokens("{\"name\":1}", FormatKind::Json);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v.contains("name"))
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v == "1")
        );
    }

    #[test]
    fn rust_marks_fn_name_and_macro() {
        let toks = tokens("fn main() { eprintln!(\"x\"); }", FormatKind::Rust);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Function && v == "main")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Macro && v == "eprintln!")
        );
    }

    #[test]
    fn xml_marks_tags_and_attrs() {
        let toks = tokens("<root id=\"1\"><!--x--></root>", FormatKind::Xml);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Keyword && v == "root")
        );
        assert!(toks.iter().any(|(k, v)| *k == TokenKind::Key && v == "id"));
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::String && v.contains('1'))
        );
        assert!(toks.iter().any(|(k, _)| *k == TokenKind::Comment));
    }

    #[test]
    fn yaml_marks_keys_values_and_numbers() {
        let src = "name: copycraft\ncount: 2\nactive: true\n";
        let toks = tokens(src, FormatKind::Yaml);
        let joined: String = toks.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, src);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v == "name")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::String && v == "copycraft")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v == "2")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Keyword && v == "true")
        );
    }

    #[test]
    fn csv_marks_header_numbers_quotes_and_delimiters() {
        let src = "name,age,city\nalice,30,\"Den Haag\"\n";
        let toks = tokens(src, FormatKind::Csv);
        let joined: String = toks.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, src);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v == "name")
        );
        assert!(toks.iter().any(|(k, v)| *k == TokenKind::Key && v == "age"));
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v == "30")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::String && v == "\"Den Haag\"")
        );
        assert!(toks.iter().any(|(k, v)| *k == TokenKind::Punct && v == ","));
    }

    #[test]
    fn semicolon_table_marks_salary_and_keeps_a_phone_as_text() {
        let src = "Id;Naam;Telefoon;Salaris\n1;Jan;06-12345678;3450\n";
        let toks = tokens(src, FormatKind::Csv);
        let joined: String = toks.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, src);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v == "Salaris")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::String && v == "06-12345678")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v == "3450")
        );
        assert!(toks.iter().any(|(k, v)| *k == TokenKind::Punct && v == ";"));
    }

    #[test]
    fn aligned_tsv_uses_the_middle_dot_as_its_delimiter() {
        let src = "Naam  · Salaris\nJan   · 3450\n";
        let toks = tokens(src, FormatKind::Tsv);
        let joined: String = toks.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, src);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v == "Naam")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::String && v == "Jan")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v == "3450")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Punct && v == "\u{00b7}")
        );
    }

    #[test]
    fn dataframe_marks_shape_and_numbers() {
        let src =
            "shape: (2, 2)\n\u{2502} Id \u{2502} n \u{2502}\n\u{2502} 1 \u{2502} 9 \u{2502}\n";
        let toks = tokens(src, FormatKind::Dataframe);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Keyword && v.starts_with("shape"))
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Number && v.trim() == "1")
        );
    }

    #[test]
    fn markdown_marks_heading_code_and_link() {
        let src = "# Title\n\nSee [docs](https://example.com) and `code` plus **bold**.\n";
        let toks = tokens(src, FormatKind::Markdown);
        let joined: String = toks.iter().map(|(_, text)| text.as_str()).collect();
        assert_eq!(joined, src);
        assert!(
            toks.iter()
                .any(|(kind, text)| *kind == TokenKind::Keyword && text == "Title")
        );
        assert!(
            toks.iter()
                .any(|(kind, text)| *kind == TokenKind::Key && text == "docs")
        );
        assert!(
            toks.iter()
                .any(|(kind, text)| *kind == TokenKind::String && text.contains("example.com"))
        );
        assert!(
            toks.iter()
                .any(|(kind, text)| *kind == TokenKind::String && text == "`code`")
        );
        assert!(
            toks.iter()
                .any(|(kind, text)| *kind == TokenKind::Keyword && text == "bold")
        );
    }

    #[test]
    fn redacted_marks_placeholders() {
        let toks = tokens("Naam: [PERSON]\nmail: [EMAIL_ADDRESS]\n", FormatKind::Plain);
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Keyword && v == "[PERSON]")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Keyword && v == "[EMAIL_ADDRESS]")
        );
        assert!(
            toks.iter()
                .any(|(k, v)| *k == TokenKind::Key && v == "Naam")
        );
    }
}
