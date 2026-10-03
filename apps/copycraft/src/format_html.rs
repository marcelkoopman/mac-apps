//! HTML pretty printing, apart from XML: void elements (`<meta>`, `<br>`, …) do not open a
//! level, the contents of `script`, `style`, `pre` and `textarea` stay exactly as copied, and
//! text with inline elements (`<p>Some <em>words</em>.</p>`) stays on one line.

/// Elements without content or end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Elements whose contents are kept as copied, end tag included.
const RAW: &[&str] = &["script", "style", "pre", "textarea"];

/// Elements that sit in a line of text.
const INLINE: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "button", "cite", "code", "data", "del", "dfn", "em",
    "i", "img", "input", "ins", "kbd", "label", "mark", "q", "s", "samp", "small", "span",
    "strong", "sub", "sup", "time", "u", "var", "wbr",
];

/// Elements that end an open `<p>` (its end tag is optional).
const CLOSES_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

#[derive(Debug)]
enum Token {
    /// `<!DOCTYPE …>`, `<?…?>` and comments.
    Other(String),
    Open(String, String),
    Close(String, String),
    /// A void element, or one written `<x/>`.
    Void(String, String),
    /// A raw-text element from its start tag to its end tag.
    Raw(String, String),
    Text(String),
}

pub fn pretty_html(src: &str) -> String {
    let tokens = tokens(src.trim());
    let mut lines: Vec<String> = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let indent = |open: &Vec<String>| "    ".repeat(open.len());
        match &tokens[i] {
            Token::Open(name, _) | Token::Void(name, _) | Token::Raw(name, _) => {
                close_implied(&mut open, name);
            }
            _ => {}
        }
        if let Some(end) = whole_inline_element(&tokens, i) {
            lines.push(format!("{}{}", indent(&open), inline_line(&tokens[i..end])));
            i = end;
            continue;
        }
        let run = inline_run(&tokens, i);
        if run > i {
            let line = inline_line(&tokens[i..run]);
            if !line.is_empty() {
                lines.push(format!("{}{line}", indent(&open)));
            }
            i = run;
            continue;
        }
        match &tokens[i] {
            Token::Open(name, tag) => {
                lines.push(format!("{}{tag}", indent(&open)));
                open.push(name.clone());
            }
            Token::Close(name, tag) => {
                if let Some(at) = open.iter().rposition(|n| n == name) {
                    open.truncate(at);
                }
                lines.push(format!("{}{tag}", indent(&open)));
            }
            Token::Other(s) | Token::Void(_, s) | Token::Raw(_, s) => {
                lines.push(format!("{}{s}", indent(&open)));
            }
            Token::Text(_) => {}
        }
        i += 1;
    }
    let out = lines.join("\n");
    if out.trim().is_empty() {
        src.to_string()
    } else {
        out
    }
}

/// Ends elements whose end tag HTML lets you leave out, when `name` starts.
fn close_implied(open: &mut Vec<String>, name: &str) {
    while let Some(top) = open.last() {
        let ends = match top.as_str() {
            "p" => CLOSES_P.contains(&name),
            "li" => name == "li",
            "dt" | "dd" => matches!(name, "dt" | "dd"),
            "option" => name == "option",
            "td" | "th" => matches!(name, "td" | "th" | "tr"),
            "tr" => name == "tr",
            _ => false,
        };
        if !ends {
            break;
        }
        open.pop();
    }
}

/// End of the element at `i` when everything in it is text or inline elements.
fn whole_inline_element(tokens: &[Token], i: usize) -> Option<usize> {
    let Token::Open(name, _) = tokens.get(i)? else {
        return None;
    };
    inline_until_close(tokens, i + 1, name)
}

/// After a start tag of `name` at `start - 1`: the index after its end tag, when only text and
/// inline elements come before it.
fn inline_until_close(tokens: &[Token], start: usize, name: &str) -> Option<usize> {
    let mut j = start;
    loop {
        match tokens.get(j)? {
            Token::Close(close, _) if close == name => return Some(j + 1),
            _ => j = inline_item(tokens, j)?,
        }
    }
}

/// The index after an inline item at `j`: text, an inline void element or a whole inline element.
fn inline_item(tokens: &[Token], j: usize) -> Option<usize> {
    match tokens.get(j)? {
        Token::Text(_) => Some(j + 1),
        Token::Void(name, _) if INLINE.contains(&name.as_str()) => Some(j + 1),
        Token::Open(name, _) if INLINE.contains(&name.as_str()) => {
            inline_until_close(tokens, j + 1, name)
        }
        _ => None,
    }
}

/// The index after the run of inline items from `i` (`i` when there is none).
fn inline_run(tokens: &[Token], i: usize) -> usize {
    let mut j = i;
    while let Some(next) = inline_item(tokens, j) {
        j = next;
    }
    j
}

/// Tokens on one line: tags as written, runs of white space in text as one space, none right
/// after a start tag or right before an end tag.
fn inline_line(tokens: &[Token]) -> String {
    let mut line = String::new();
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Text(text) => {
                let mut piece = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if text.starts_with(char::is_whitespace) {
                    piece.insert(0, ' ');
                }
                if text.ends_with(char::is_whitespace) && !text.trim().is_empty() {
                    piece.push(' ');
                }
                if at == 0 || matches!(tokens[at - 1], Token::Open(..)) {
                    piece = piece.trim_start().to_string();
                }
                if matches!(tokens.get(at + 1), None | Some(Token::Close(..))) {
                    piece = piece.trim_end().to_string();
                }
                line.push_str(&piece);
            }
            Token::Open(_, s)
            | Token::Close(_, s)
            | Token::Void(_, s)
            | Token::Other(s)
            | Token::Raw(_, s) => line.push_str(s),
        }
    }
    line.trim().to_string()
}

fn tag_name(tag: &str) -> String {
    tag.trim_start_matches(['<', '/'])
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// The index after the `>` that ends the tag starting at `i`, skipping quoted attribute values.
fn tag_end(chars: &[char], i: usize) -> Option<usize> {
    let mut quote = None;
    for (j, &c) in chars.iter().enumerate().skip(i + 1) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => return Some(j + 1),
            None => {}
        }
    }
    None
}

fn find_from(chars: &[char], from: usize, pattern: &str) -> Option<usize> {
    let pattern: Vec<char> = pattern.chars().collect();
    (from..=chars.len().checked_sub(pattern.len())?).find(|&at| {
        chars[at..at + pattern.len()]
            .iter()
            .zip(&pattern)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn tokens(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < chars.len() {
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        let starts_tag = chars[i] == '<' && (next.is_ascii_alphabetic() || "/!?".contains(next));
        if !starts_tag {
            text.push(chars[i]);
            i += 1;
            continue;
        }
        let end = if find_from(&chars, i, "<!--") == Some(i) {
            find_from(&chars, i + 4, "-->").map(|at| at + 3)
        } else {
            tag_end(&chars, i)
        };
        let Some(end) = end else {
            text.extend(&chars[i..]);
            break;
        };
        if !text.is_empty() {
            tokens.push(Token::Text(std::mem::take(&mut text)));
        }
        let tag: String = chars[i..end].iter().collect();
        let name = tag_name(&tag);
        i = end;
        if tag.starts_with("<!") || tag.starts_with("<?") {
            tokens.push(Token::Other(tag));
        } else if tag.starts_with("</") {
            tokens.push(Token::Close(name, tag));
        } else if VOID.contains(&name.as_str()) || tag.ends_with("/>") {
            tokens.push(Token::Void(name, tag));
        } else if RAW.contains(&name.as_str()) {
            let close = find_from(&chars, i, &format!("</{name}"))
                .and_then(|at| tag_end(&chars, at))
                .unwrap_or(chars.len());
            let whole: String = tag.chars().chain(chars[i..close].iter().copied()).collect();
            tokens.push(Token::Raw(name, whole));
            i = close;
        } else {
            tokens.push(Token::Open(name, tag));
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::pretty_html;

    #[test]
    fn void_elements_do_not_indent() {
        let out = pretty_html(
            r#"<html><head><meta charset="utf-8"><link rel="icon" href="a.png"><title>T</title></head><body><hr><img src="a.png"><div>x</div></body></html>"#,
        );
        assert_eq!(
            out,
            r#"<html>
    <head>
        <meta charset="utf-8">
        <link rel="icon" href="a.png">
        <title>T</title>
    </head>
    <body>
        <hr>
        <img src="a.png">
        <div>x</div>
    </body>
</html>"#
        );
    }

    #[test]
    fn script_style_pre_and_textarea_stay_as_copied() {
        let src = "<body><script>if (a < b && c > d) {\n  go();\n}</script><style>p > b { color: red }</style><pre>  two\n    lines  </pre><textarea>  <b>not a tag</b> </textarea></body>";
        let out = pretty_html(src);
        assert_eq!(
            out,
            "<body>\n    <script>if (a < b && c > d) {\n  go();\n}</script>\n    <style>p > b { color: red }</style>\n    <pre>  two\n    lines  </pre>\n    <textarea>  <b>not a tag</b> </textarea>\n</body>"
        );
        assert_eq!(pretty_html(&out), out);
    }

    #[test]
    fn text_with_inline_elements_stays_on_one_line() {
        let src = "<div>\n  <p>\n    Some   <em>words</em>,\n    a <a href=\"#x\">link</a>.<br>Next.\n  </p>\n</div>";
        assert_eq!(
            pretty_html(src),
            "<div>\n    <p>Some <em>words</em>, a <a href=\"#x\">link</a>.<br>Next.</p>\n</div>"
        );
    }

    #[test]
    fn spaces_around_inline_tags_are_kept() {
        assert_eq!(
            pretty_html("<p>An <img src=\"a.png\"> image, <b> bold </b> text</p>"),
            "<p>An <img src=\"a.png\"> image, <b>bold</b> text</p>"
        );
    }

    #[test]
    fn left_out_end_tags_do_not_drift() {
        let out = pretty_html("<ul><li>One<li>Two <b>bold</b></ul><p>Para<div>Block</div>");
        assert_eq!(
            out,
            "<ul>\n    <li>\n        One\n    <li>\n        Two <b>bold</b>\n</ul>\n<p>\n    Para\n<div>Block</div>"
        );
    }

    #[test]
    fn quoted_angle_brackets_and_comments() {
        let out = pretty_html("<div title=\"a > b\"><!-- note --><span>x</span></div>");
        assert_eq!(
            out,
            "<div title=\"a > b\">\n    <!-- note -->\n    <span>x</span>\n</div>"
        );
    }
}
