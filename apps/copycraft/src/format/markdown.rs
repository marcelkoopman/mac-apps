pub(super) fn looks_like_markdown(text: &str) -> bool {
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
pub(super) fn yaml_comment_on_a_mapping(text: &str) -> bool {
    if !crate::transform::looks_like_yaml(text) {
        return false;
    }
    let lines: Vec<&str> = text.lines().collect();
    !lines.iter().any(|line| is_toplevel_list_item(line))
        && count_md_links(text) == 0
        && !has_emphasis(text)
        && !lines.iter().any(|line| is_quote_line(line))
}

pub(super) fn heading_then_prose(lines: &[&str]) -> bool {
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

pub(super) fn is_yaml_key(line: &str) -> bool {
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

pub(super) fn hash_comment_file(lines: &[&str]) -> bool {
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

pub(super) fn is_single_hash_comment(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    !rest.starts_with('#') && (rest.is_empty() || rest.starts_with([' ', '\t']))
}

pub(super) fn is_spaceless_token(trimmed: &str) -> bool {
    !trimmed.chars().any(char::is_whitespace)
}

pub(super) fn is_atx_heading(line: &str) -> bool {
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

pub(super) fn is_toplevel_list_item(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t') && is_list_marker(line)
}

pub(super) fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    indent <= 3 && is_list_marker(trimmed)
}

pub(super) fn is_list_marker(trimmed: &str) -> bool {
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

pub(super) fn is_quote_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    indent <= 3 && trimmed.starts_with('>')
}

pub(super) fn has_emphasis(text: &str) -> bool {
    wrapped_marker(text, "**") || wrapped_marker(text, "__")
}

pub(super) fn wrapped_marker(text: &str, marker: &str) -> bool {
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

pub(super) fn count_md_links(text: &str) -> usize {
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

pub(super) fn md_link_end(chars: &[char], start: usize) -> Option<usize> {
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

pub(super) fn wrapped_fence(lines: &[&str]) -> bool {
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

pub(super) fn has_gfm_table(lines: &[&str]) -> bool {
    let separator = lines.iter().any(|line| is_table_separator(line));
    let row = lines
        .iter()
        .any(|line| is_table_row(line) && !is_table_separator(line));
    separator && row
}

pub(super) fn is_table_separator(line: &str) -> bool {
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

pub(super) fn is_table_row(line: &str) -> bool {
    line.matches('|').count() >= 2
}

pub(super) fn opening_fence(line: &str) -> Option<(char, usize)> {
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

pub(super) fn closes_fence(line: &str, marker: char, len: usize) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == marker) && trimmed.chars().count() >= len
}

pub(super) fn format_markdown(src: &str) -> String {
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

pub(super) fn front_matter_end(lines: &[&str]) -> Option<usize> {
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
pub(super) enum MdKind {
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

pub(super) fn classify_md(line: &str, prev: MdKind) -> MdKind {
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

pub(super) fn is_thematic_break(line: &str) -> bool {
    let marks: Vec<char> = line
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    marks.len() >= 3
        && marks.iter().all(|ch| *ch == marks[0])
        && matches!(marks[0], '-' | '*' | '_')
}

pub(super) fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

pub(super) fn push_md(out: &mut Vec<String>, prev: &mut MdKind, kind: MdKind, text: &str) {
    if kind != MdKind::Blank && md_separated(*prev, kind) && *prev != MdKind::Blank {
        out.push(String::new());
    }
    out.push(text.to_string());
    *prev = kind;
}

pub(super) fn md_separated(prev: MdKind, next: MdKind) -> bool {
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

pub(super) fn normalize_heading(line: &str) -> String {
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

pub(super) fn trim_markdown_line(line: &str) -> String {
    if line.ends_with("  ") && !line.trim().is_empty() {
        format!("{}  ", line.trim_end())
    } else {
        line.trim_end().to_string()
    }
}
