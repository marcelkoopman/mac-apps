//! A small XML parser into [`Node`] trees (elements, attributes, text).

use super::*;

pub(super) struct Parser<'a> {
    pub(super) s: &'a str,
    pub(super) i: usize,
}

pub(super) fn parse_document(text: &str) -> Option<Node> {
    let mut parser = Parser { s: text, i: 0 };
    parser.skip_misc()?;
    let node = parser.parse_element()?;
    parser.skip_misc()?;
    parser.eof().then_some(node)
}

impl<'a> Parser<'a> {
    pub(super) fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    pub(super) fn starts(&self, prefix: &str) -> bool {
        self.s[self.i..].starts_with(prefix)
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let ch = self.s[self.i..].chars().next()?;
        self.i += ch.len_utf8();
        Some(ch)
    }

    pub(super) fn expect(&mut self, want: char) -> Option<()> {
        (self.bump()? == want).then_some(())
    }

    pub(super) fn consume(&mut self, prefix: &str) -> bool {
        if self.starts(prefix) {
            self.i += prefix.len();
            true
        } else {
            false
        }
    }

    pub(super) fn skip_ws(&mut self) {
        while let Some(ch) = self.s[self.i..].chars().next() {
            if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    pub(super) fn skip_misc(&mut self) -> Option<()> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.skip_comment()?;
            } else if self.starts("<?") {
                self.skip_until("?>")?;
            } else if self.starts("<!") {
                self.skip_declaration()?;
            } else {
                break;
            }
        }
        Some(())
    }

    pub(super) fn skip_comment(&mut self) -> Option<()> {
        self.skip_until("-->")
    }

    pub(super) fn skip_until(&mut self, marker: &str) -> Option<()> {
        let rest = self.s[self.i..].find(marker)?;
        self.i += rest + marker.len();
        Some(())
    }

    pub(super) fn skip_declaration(&mut self) -> Option<()> {
        self.expect('<')?;
        let mut brackets = 0i32;
        while let Some(ch) = self.bump() {
            match ch {
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '>' if brackets == 0 => return Some(()),
                _ => {}
            }
        }
        None
    }

    pub(super) fn parse_element(&mut self) -> Option<Node> {
        self.expect('<')?;
        if self.starts("/") || self.starts("!") || self.starts("?") {
            return None;
        }
        let raw = self.read_name()?;
        let mut attrs = Vec::new();
        let closed = loop {
            self.skip_ws();
            if self.consume("/>") {
                break true;
            }
            if self.consume(">") {
                break false;
            }
            let attr_name = self.read_name()?;
            self.skip_ws();
            self.expect('=')?;
            self.skip_ws();
            let value = self.read_quoted()?;
            if is_xmlns(&attr_name) {
                continue;
            }
            let name = local_name(&attr_name);
            if !attrs.iter().any(|(existing, _)| existing == &name) {
                attrs.push((name, value));
            }
        };
        let (children, text) = if closed {
            (Vec::new(), String::new())
        } else {
            self.parse_content(&raw)?
        };
        Some(Node {
            name: local_name(&raw),
            attrs,
            children,
            text,
        })
    }

    pub(super) fn parse_content(&mut self, raw_name: &str) -> Option<(Vec<Node>, String)> {
        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            if self.eof() {
                return None;
            }
            if self.starts("</") {
                self.consume("</");
                let close = self.read_name()?;
                self.skip_ws();
                self.expect('>')?;
                if close != raw_name {
                    return None;
                }
                break;
            }
            if self.starts("<!--") {
                self.skip_comment()?;
            } else if self.starts("<![CDATA[") {
                push_text(&mut text, &self.read_cdata()?);
            } else if self.starts("<?") {
                self.skip_until("?>")?;
            } else if self.starts("<!") {
                self.skip_declaration()?;
            } else if self.starts("<") {
                children.push(self.parse_element()?);
            } else {
                push_text(&mut text, &self.read_text()?);
            }
        }
        Some((children, text))
    }

    pub(super) fn read_name(&mut self) -> Option<String> {
        let start = self.i;
        let first = self.s[self.i..].chars().next()?;
        if !is_name_start(first) {
            return None;
        }
        self.i += first.len_utf8();
        while let Some(ch) = self.s[self.i..].chars().next() {
            if is_name_char(ch) {
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }
        Some(self.s[start..self.i].to_string())
    }

    pub(super) fn read_quoted(&mut self) -> Option<String> {
        let quote = self.bump()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let start = self.i;
        while let Some(ch) = self.s[self.i..].chars().next() {
            if ch == quote {
                let raw = &self.s[start..self.i];
                self.i += ch.len_utf8();
                return decode_entities(raw);
            }
            self.i += ch.len_utf8();
        }
        None
    }

    pub(super) fn read_text(&mut self) -> Option<String> {
        let start = self.i;
        while let Some(ch) = self.s[self.i..].chars().next() {
            if ch == '<' {
                break;
            }
            self.i += ch.len_utf8();
        }
        decode_entities(&self.s[start..self.i])
    }

    pub(super) fn read_cdata(&mut self) -> Option<String> {
        self.consume("<![CDATA[");
        let rest = self.s[self.i..].find("]]>")?;
        let text = self.s[self.i..self.i + rest].to_string();
        self.i += rest + 3;
        Some(text)
    }
}

pub(super) fn push_text(text: &mut String, chunk: &str) {
    let trimmed = chunk.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(&trimmed);
}

pub(super) fn is_xmlns(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

pub(super) fn local_name(qname: &str) -> String {
    qname
        .rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(qname)
        .to_string()
}

pub(super) fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == ':' || (!ch.is_ascii() && ch.is_alphabetic())
}

pub(super) fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit() || ch == '-' || ch == '.'
}

pub(super) fn decode_entities(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let end = rest.find(';')?;
        let name = &rest[..end];
        rest = &rest[end + 1..];
        out.push(decode_entity(name)?);
    }
    out.push_str(rest);
    Some(out)
}
