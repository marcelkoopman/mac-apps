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

pub(super) fn tag_balance(text: &str) -> bool {
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

pub(super) fn starts_at(chars: &[char], i: usize, s: &str) -> bool {
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

pub(super) fn inline_leaf(tokens: &[XmlToken], i: usize) -> Option<(String, usize)> {
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

pub(super) fn collapse_xml_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

pub(super) enum XmlToken {
    Decl(String),
    Comment(String),
    Open(String),
    Close(String),
    Empty(String),
    Text(String),
}

pub(super) fn xml_tokens(src: &str) -> Vec<XmlToken> {
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

pub(super) fn push_indent(out: &mut String, indent: i32) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}
