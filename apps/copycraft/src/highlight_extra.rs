fn looks_redacted(source: &str) -> bool {
    source.contains('[') && source.contains(']') && has_redact_tag(source)
}

fn has_redact_tag(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && redact_tag_chars(&chars, i).is_some() {
            return true;
        }
        i += 1;
    }
    false
}

fn tokenize_yaml(source: &str) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut line_start = true;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            out.push((TokenKind::Text, "\n".into()));
            i += 1;
            line_start = true;
            continue;
        }
        if line_start && starts_at(&chars, i, "---") {
            out.push((TokenKind::Punct, "---".into()));
            i += 3;
            line_start = false;
            continue;
        }
        if ch == '#' {
            let (token, next) = take_while(&chars, i, |c| c != '\n');
            out.push((TokenKind::Comment, token));
            i = next;
            continue;
        }
        if ch == '"' {
            let (token, next) = take_string(&chars, i);
            out.push((TokenKind::String, token));
            i = next;
            line_start = false;
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let (token, next) =
                take_while(&chars, i, |c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'));
            out.push((TokenKind::Number, token));
            i = next;
            line_start = false;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let (token, next) =
                take_while(&chars, i, |c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
            let after = skip_ws_extra(&chars, next);
            let kind = if after < chars.len() && chars[after] == ':' {
                TokenKind::Key
            } else if matches!(token.as_str(), "true" | "false" | "null" | "yes" | "no") {
                TokenKind::Keyword
            } else {
                TokenKind::String
            };
            out.push((kind, token));
            i = next;
            line_start = false;
            continue;
        }
        if matches!(ch, ':' | '-' | '[' | ']' | '{' | '}' | ',') {
            out.push((TokenKind::Punct, ch.to_string()));
            i += 1;
            line_start = false;
            continue;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
        if !ch.is_whitespace() {
            line_start = false;
        }
    }
    out
}

fn tokenize_dataframe(source: &str) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    for (idx, line) in source.split_inclusive('\n').enumerate() {
        tokenize_df_line(&mut out, line, idx == 0);
    }
    if out.is_empty() {
        out.push((TokenKind::Text, source.to_string()));
    }
    out
}

fn tokenize_df_line(out: &mut Vec<(TokenKind, String)>, line: &str, first: bool) {
    if first && line.starts_with("shape:") {
        out.push((TokenKind::Keyword, "shape:".into()));
        out.push((TokenKind::Number, line["shape:".len()..].to_string()));
        return;
    }
    let headerish =
        line.contains('\u{2500}') || line.contains('\u{2502}') || line.contains('\u{253c}');
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if is_box(ch) {
            out.push((TokenKind::Punct, ch.to_string()));
            i += 1;
            continue;
        }
        if ch == '"' {
            let (token, next) = take_string(&chars, i);
            out.push((TokenKind::String, token));
            i = next;
            continue;
        }
        if ch.is_ascii_digit()
            || (ch == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let (token, next) =
                take_while(&chars, i, |c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'));
            out.push((TokenKind::Number, token));
            i = next;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let (token, next) =
                take_while(&chars, i, |c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':'));
            let kind = if matches!(
                token.as_str(),
                "str" | "i64" | "u64" | "i32" | "f64" | "f32" | "bool" | "date" | "datetime" | "null"
            ) {
                TokenKind::Type
            } else if headerish {
                TokenKind::Key
            } else {
                TokenKind::String
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        out.push((TokenKind::Text, ch.to_string()));
        i += 1;
    }
}

fn is_box(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{257F}' | '|')
}

/// Display mark used when a TSV is aligned. A real tab does not sit on a character column.
const TSV_MARK: char = '\u{00b7}';

fn tokenize_csv(source: &str) -> Vec<(TokenKind, String)> {
    tokenize_table(source, csv_separator(source))
}

fn tokenize_tsv(source: &str) -> Vec<(TokenKind, String)> {
    let sep = if source.contains('\t') { '\t' } else { TSV_MARK };
    tokenize_table(source, sep)
}

fn csv_separator(source: &str) -> char {
    let header = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(source);
    let semis = header.matches(';').count();
    let commas = header.matches(',').count();
    if semis >= 1 && semis >= commas {
        ';'
    } else {
        ','
    }
}

fn tokenize_table(source: &str, sep: char) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    let mut header = true;
    let mut rest = source;
    while !rest.is_empty() {
        let (line, ending, next) = take_table_line(rest);
        if line.trim().is_empty() {
            if !line.is_empty() {
                out.push((TokenKind::Text, line.to_string()));
            }
        } else {
            tokenize_table_line(&mut out, line, sep, header);
            header = false;
        }
        if !ending.is_empty() {
            out.push((TokenKind::Text, ending.to_string()));
        }
        rest = next;
    }
    if out.is_empty() {
        out.push((TokenKind::Text, source.to_string()));
    }
    out
}

fn take_table_line(source: &str) -> (&str, &str, &str) {
    match source.find('\n') {
        Some(i) => {
            let ending_start = if i > 0 && source.as_bytes()[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            (
                &source[..ending_start],
                &source[ending_start..=i],
                &source[i + 1..],
            )
        }
        None => match source.strip_suffix('\r') {
            Some(line) => (line, &source[line.len()..], ""),
            None => (source, "", ""),
        },
    }
}

fn tokenize_table_line(out: &mut Vec<(TokenKind, String)>, line: &str, sep: char, header: bool) {
    let mut cell_start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek().is_some_and(|(_, next)| *next == '"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if ch == sep && !in_quotes {
            emit_table_cell(out, &line[cell_start..idx], header);
            out.push((TokenKind::Punct, ch.to_string()));
            cell_start = idx + ch.len_utf8();
        }
    }
    emit_table_cell(out, &line[cell_start..], header);
}

fn emit_table_cell(out: &mut Vec<(TokenKind, String)>, cell: &str, header: bool) {
    if cell.is_empty() {
        return;
    }
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        out.push((TokenKind::Text, cell.to_string()));
        return;
    }
    let lead = cell.len() - cell.trim_start().len();
    let core_end = lead + trimmed.len();
    if lead > 0 {
        out.push((TokenKind::Text, cell[..lead].to_string()));
    }
    out.push((table_cell_kind(trimmed, header), trimmed.to_string()));
    if core_end < cell.len() {
        out.push((TokenKind::Text, cell[core_end..].to_string()));
    }
}

fn table_cell_kind(cell: &str, header: bool) -> TokenKind {
    if header {
        return TokenKind::Key;
    }
    if is_quoted_cell(cell) {
        return TokenKind::String;
    }
    if is_plain_number(cell) {
        TokenKind::Number
    } else {
        TokenKind::String
    }
}

fn is_quoted_cell(cell: &str) -> bool {
    let bytes = cell.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'
}

fn is_plain_number(cell: &str) -> bool {
    let mut chars = cell.chars().peekable();
    if matches!(chars.peek(), Some('+' | '-')) {
        chars.next();
    }
    let mut digits = 0usize;
    let mut separator = false;
    for ch in chars {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        if (ch == '.' || ch == ',') && !separator {
            separator = true;
            continue;
        }
        return false;
    }
    digits > 0
}

fn tokenize_redacted(source: &str) -> Vec<(TokenKind, String)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some((tag, end)) = redact_tag_chars(&chars, i) {
                out.push((TokenKind::Keyword, tag));
                i = end;
                continue;
            }
        if chars[i] == '"' {
            let (token, next) = take_string(&chars, i);
            out.push((TokenKind::String, token));
            i = next;
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let (token, next) = take_while(&chars, i, |c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
            });
            let after = skip_ws_extra(&chars, next);
            let kind = if after < chars.len() && chars[after] == ':' {
                TokenKind::Key
            } else {
                TokenKind::Text
            };
            out.push((kind, token));
            i = next;
            continue;
        }
        if chars[i].is_ascii_digit() {
            let (token, next) = take_while(&chars, i, |c| c.is_ascii_digit() || c == '.');
            out.push((TokenKind::Number, token));
            i = next;
            continue;
        }
        out.push((TokenKind::Text, chars[i].to_string()));
        i += 1;
    }
    out
}

fn redact_tag_chars(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() && chars[j] != ']' {
        if !(chars[j].is_ascii_uppercase() || chars[j] == '_') {
            return None;
        }
        j += 1;
    }
    if j >= chars.len() || j == i + 1 {
        return None;
    }
    Some((chars[i..=j].iter().collect(), j + 1))
}

fn starts_at(chars: &[char], i: usize, s: &str) -> bool {
    let w: Vec<char> = s.chars().collect();
    i + w.len() <= chars.len() && chars[i..i + w.len()] == w[..]
}

fn skip_ws_extra(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}
