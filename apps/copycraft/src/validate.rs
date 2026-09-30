/// Validation of clipboard JSON or XML. The preview text stays unchanged.
pub struct Report {
    pub language: &'static str,
    pub ok: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub detail: String,
}

/// Title-bar label for JSON or XML. Other text has no validation title.
pub fn title(text: &str) -> Option<String> {
    let report = check(text)?;
    Some(format!(
        "{} {}",
        if report.ok { "Valid" } else { "Invalid" },
        report.language
    ))
}

impl Report {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn summary(&self) -> String {
        if self.ok {
            format!("Valid {}", self.language)
        } else {
            format!("Invalid {}\n{}", self.language, self.detail)
        }
    }
}

pub fn check(text: &str) -> Option<Report> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if json_candidate(trimmed) {
        return Some(check_json(trimmed));
    }
    if xml_candidate(trimmed) {
        return Some(check_xml(trimmed));
    }
    None
}

fn json_candidate(text: &str) -> bool {
    let start = text.trim_start();
    start.starts_with('{') || start.starts_with('[')
}

fn xml_candidate(text: &str) -> bool {
    crate::format::looks_like_xml(text) || {
        let start = text.trim_start();
        start.starts_with('<') && start.contains('>')
    }
}

fn check_json(text: &str) -> Report {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) if value.is_object() || value.is_array() => ok("JSON"),
        Ok(_) => invalid("JSON", "expected a JSON object or array".to_string()),
        Err(err) => invalid("JSON", err.to_string()),
    }
}

fn check_xml(text: &str) -> Report {
    match scan_xml(text) {
        Ok(()) => ok("XML"),
        Err(detail) => invalid("XML", detail),
    }
}

fn ok(language: &'static str) -> Report {
    Report {
        language,
        ok: true,
        detail: String::new(),
    }
}

fn invalid(language: &'static str, detail: String) -> Report {
    Report {
        language,
        ok: false,
        detail,
    }
}

struct Scan<'a> {
    text: &'a str,
    i: usize,
    line: usize,
    col: usize,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            i: 0,
            line: 1,
            col: 1,
        }
    }

    fn eof(&self) -> bool {
        self.i >= self.text.len()
    }

    fn peek(&self) -> Option<char> {
        self.text[self.i..].chars().next()
    }

    fn starts(&self, prefix: &str) -> bool {
        self.text[self.i..].starts_with(prefix)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn err(&self, message: &str) -> String {
        format!("{message} at line {}, column {}", self.line, self.col)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
}

fn scan_xml(text: &str) -> Result<(), String> {
    let mut scan = Scan::new(text.trim());
    if scan.starts("\u{feff}") {
        scan.bump();
    }
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut saw_root = false;
    let mut decl_allowed = true;
    while !scan.eof() {
        if scan.peek().is_some_and(char::is_whitespace) {
            if !stack.is_empty() {
                scan.bump();
                continue;
            }
            scan.skip_ws();
            continue;
        }
        if !scan.starts("<") {
            if stack.is_empty() {
                return Err(scan.err("text outside the root element"));
            }
            read_text(&mut scan)?;
            continue;
        }
        if scan.starts("<?") {
            let decl = scan.starts("<?xml") || scan.starts("<?XML");
            if decl && !decl_allowed {
                return Err(scan.err("XML declaration must be the first content"));
            }
            read_pi(&mut scan)?;
            decl_allowed = false;
            continue;
        }
        decl_allowed = false;
        if scan.starts("<!--") {
            read_comment(&mut scan)?;
            continue;
        }
        if scan.starts("<![CDATA[") {
            if stack.is_empty() {
                return Err(scan.err("CDATA outside the root element"));
            }
            read_cdata(&mut scan)?;
            continue;
        }
        if scan.starts("<!") {
            read_declaration(&mut scan)?;
            continue;
        }
        if scan.starts("</") {
            read_close(&mut scan, &mut stack)?;
            continue;
        }
        read_open(&mut scan, &mut stack, &mut saw_root)?;
    }
    if let Some((name, line, col)) = stack.last() {
        return Err(format!(
            "unclosed <{name}> opened at line {line}, column {col}"
        ));
    }
    if !saw_root {
        return Err("document has no root element".to_string());
    }
    Ok(())
}

fn read_open(
    scan: &mut Scan<'_>,
    stack: &mut Vec<(String, usize, usize)>,
    saw_root: &mut bool,
) -> Result<(), String> {
    scan.bump();
    let line = scan.line;
    let col = scan.col;
    let name = read_name(scan)?;
    let mut attrs = Vec::new();
    loop {
        scan.skip_ws();
        if scan.starts("/>") {
            scan.bump();
            scan.bump();
            if stack.is_empty() {
                if *saw_root {
                    return Err(scan.err("document has more than one root element"));
                }
                *saw_root = true;
            }
            return Ok(());
        }
        if scan.peek() == Some('>') {
            scan.bump();
            if stack.is_empty() {
                if *saw_root {
                    return Err(scan.err("document has more than one root element"));
                }
                *saw_root = true;
            }
            stack.push((name, line, col));
            return Ok(());
        }
        if scan.eof() {
            return Err(scan.err("unclosed start tag"));
        }
        let (attr, _value) = read_attr(scan)?;
        if attrs.iter().any(|existing: &String| existing == &attr) {
            return Err(scan.err(&format!("duplicate attribute {attr}")));
        }
        attrs.push(attr);
    }
}

fn read_close(scan: &mut Scan<'_>, stack: &mut Vec<(String, usize, usize)>) -> Result<(), String> {
    scan.bump();
    scan.bump();
    let name = read_name(scan)?;
    scan.skip_ws();
    if scan.peek() != Some('>') {
        return Err(scan.err("expected > after an end tag"));
    }
    scan.bump();
    match stack.pop() {
        Some((open, _, _)) if open == name => Ok(()),
        Some((open, line, col)) => Err(format!(
            "mismatched end tag: expected </{open}>, found </{name}> (opened at line {line}, column {col})"
        )),
        None => Err(scan.err("unexpected end tag")),
    }
}

fn read_attr(scan: &mut Scan<'_>) -> Result<(String, String), String> {
    let name = read_name(scan)?;
    scan.skip_ws();
    if scan.peek() != Some('=') {
        return Err(scan.err("expected = after an attribute name"));
    }
    scan.bump();
    scan.skip_ws();
    let quote = scan.peek();
    if quote != Some('"') && quote != Some('\'') {
        return Err(scan.err("attribute value must be quoted"));
    }
    let quote = scan.bump().unwrap();
    let mut value = String::new();
    loop {
        match scan.peek() {
            None => return Err(scan.err("unclosed attribute value")),
            Some(ch) if ch == quote => {
                scan.bump();
                return Ok((name, value));
            }
            Some('<') => return Err(scan.err("< in an attribute value")),
            Some('&') => {
                read_entity(scan)?;
                value.push('&');
            }
            Some(ch) => {
                value.push(ch);
                scan.bump();
            }
        }
    }
}

fn read_text(scan: &mut Scan<'_>) -> Result<(), String> {
    while let Some(ch) = scan.peek() {
        if ch == '<' {
            return Ok(());
        }
        if scan.starts("]]>") {
            return Err(scan.err("]]> is not allowed in text"));
        }
        if ch == '&' {
            read_entity(scan)?;
        } else {
            scan.bump();
        }
    }
    Ok(())
}

fn read_entity(scan: &mut Scan<'_>) -> Result<(), String> {
    let line = scan.line;
    let col = scan.col;
    scan.bump();
    if scan.starts("#x") || scan.starts("#X") {
        scan.bump();
        scan.bump();
        let digits = read_while(scan, |ch| ch.is_ascii_hexdigit());
        return finish_char_ref(scan, u32::from_str_radix(&digits, 16).ok(), line, col);
    }
    if scan.starts("#") {
        scan.bump();
        let digits = read_while(scan, |ch| ch.is_ascii_digit());
        return finish_char_ref(scan, digits.parse().ok(), line, col);
    }
    let name = read_while(scan, |ch| ch.is_ascii_alphanumeric());
    if scan.peek() != Some(';') || !matches!(name.as_str(), "amp" | "lt" | "gt" | "apos" | "quot") {
        return Err(format!(
            "unknown entity &{name}; at line {line}, column {col}"
        ));
    }
    scan.bump();
    Ok(())
}

fn finish_char_ref(
    scan: &mut Scan<'_>,
    code: Option<u32>,
    line: usize,
    col: usize,
) -> Result<(), String> {
    let Some(code) = code else {
        return Err(format!(
            "invalid character reference at line {line}, column {col}"
        ));
    };
    if scan.peek() != Some(';') || !is_xml_char(code) {
        return Err(format!(
            "invalid character reference at line {line}, column {col}"
        ));
    }
    scan.bump();
    Ok(())
}

fn is_xml_char(code: u32) -> bool {
    matches!(
        char::from_u32(code),
        Some(ch) if !matches!(ch, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}')
    )
}

fn read_comment(scan: &mut Scan<'_>) -> Result<(), String> {
    let line = scan.line;
    let col = scan.col;
    for _ in 0..4 {
        scan.bump();
    }
    while !scan.eof() {
        if scan.starts("--") {
            scan.bump();
            scan.bump();
            if scan.peek() == Some('>') {
                scan.bump();
                return Ok(());
            }
            return Err(format!(
                "comment contains -- at line {}, column {}",
                scan.line, scan.col
            ));
        }
        scan.bump();
    }
    Err(format!("unclosed comment at line {line}, column {col}"))
}

fn read_cdata(scan: &mut Scan<'_>) -> Result<(), String> {
    let line = scan.line;
    let col = scan.col;
    for _ in 0.."<![CDATA[".len() {
        scan.bump();
    }
    while !scan.eof() {
        if scan.starts("]]>") {
            scan.bump();
            scan.bump();
            scan.bump();
            return Ok(());
        }
        scan.bump();
    }
    Err(format!("unclosed CDATA at line {line}, column {col}"))
}

fn read_pi(scan: &mut Scan<'_>) -> Result<(), String> {
    let line = scan.line;
    let col = scan.col;
    scan.bump();
    scan.bump();
    while !scan.eof() {
        if scan.starts("?>") {
            scan.bump();
            scan.bump();
            return Ok(());
        }
        scan.bump();
    }
    Err(format!(
        "unclosed processing instruction at line {line}, column {col}"
    ))
}

fn read_declaration(scan: &mut Scan<'_>) -> Result<(), String> {
    let line = scan.line;
    let col = scan.col;
    scan.bump();
    scan.bump();
    let mut brackets = 0i32;
    let mut quote = None;
    while !scan.eof() {
        let ch = scan.bump().unwrap();
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '>' if brackets == 0 => return Ok(()),
            _ => {}
        }
    }
    Err(format!("unclosed declaration at line {line}, column {col}"))
}

fn read_name(scan: &mut Scan<'_>) -> Result<String, String> {
    let Some(first) = scan.peek() else {
        return Err(scan.err("expected a name"));
    };
    if !is_name_start(first) {
        return Err(scan.err("invalid name"));
    }
    Ok(read_while(scan, is_name_char))
}

fn read_while(scan: &mut Scan<'_>, pred: impl Fn(char) -> bool) -> String {
    let mut out = String::new();
    while let Some(ch) = scan.peek() {
        if !pred(ch) {
            break;
        }
        out.push(ch);
        scan.bump();
    }
    out
}

fn is_name_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || ch == ':'
}

fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit() || matches!(ch, '-' | '.' | '\u{00B7}')
}

#[cfg(test)]
mod tests {
    use super::{check, title};

    #[test]
    fn accepts_json_object_and_array() {
        let object = check(r#"{"name":"copycraft","n":3}"#).expect("json");
        assert!(object.ok);
        assert_eq!(object.summary(), "Valid JSON");
        assert_eq!(
            title(r#"{"name":"copycraft","n":3}"#).as_deref(),
            Some("Valid JSON")
        );
        let array = check("[1, 2, {\"ok\": true}]").expect("array");
        assert!(array.ok);
    }

    #[test]
    fn rejects_broken_json() {
        let report = check(r#"{"name":"copycraft",}"#).expect("json");
        assert!(!report.ok);
        assert!(report.summary().starts_with("Invalid JSON"));
        assert_eq!(
            title(r#"{"name":"copycraft",}"#).as_deref(),
            Some("Invalid JSON")
        );
        assert!(report.detail.contains("trailing comma") || report.detail.contains("comma"));
    }

    #[test]
    fn accepts_well_formed_xml() {
        let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<root id="1">
  <!-- note -->
  <item name="Jan">hi &amp; bye</item>
  <empty/>
  <raw><![CDATA[a < b]]></raw>
</root>"#;
        let report = check(src).expect("xml");
        assert!(report.ok, "{}", report.detail);
        assert_eq!(report.summary(), "Valid XML");
        assert_eq!(title(src).as_deref(), Some("Valid XML"));
    }

    #[test]
    fn title_is_only_for_json_and_xml() {
        assert_eq!(
            title(r#"{"name":"copycraft"}"#).as_deref(),
            Some("Valid JSON")
        );
        assert_eq!(
            title("{\n  \"name\": \"copycraft\"\n}").as_deref(),
            Some("Valid JSON")
        );
        assert_eq!(title("<root><item/></root>").as_deref(), Some("Valid XML"));
        assert_eq!(title("<root><item></root>").as_deref(), Some("Invalid XML"));
        assert_eq!(title(""), None);
        assert_eq!(title("hello world"), None);
        assert_eq!(title("fn main() {}"), None);
        assert_eq!(title("name,age\nalice,30\nbob,40"), None);
        assert_eq!(title("name: copycraft\ncount: 2\n"), None);
    }

    #[test]
    fn rejects_mismatched_and_unclosed_xml() {
        let mismatched = check("<root><person></root></person>").expect("xml");
        assert!(!mismatched.ok);
        assert!(mismatched.detail.contains("mismatched"));
        let unclosed = check("<root><item></item>").expect("xml");
        assert!(!unclosed.ok);
        assert!(unclosed.detail.contains("unclosed"));
        let extra = check("<root/><item/>").expect("xml");
        assert!(!extra.ok);
        assert!(extra.detail.contains("root"));
    }

    #[test]
    fn rejects_bad_attributes_and_entities() {
        let quoted = check(r#"<root id=1></root>"#).expect("xml");
        assert!(!quoted.ok);
        assert!(quoted.detail.contains("quoted"));
        let entity = check("<root>&foo;</root>").expect("xml");
        assert!(!entity.ok);
        assert!(entity.detail.contains("entity"));
        let dup = check(r#"<root a="1" a="2"/>"#).expect("xml");
        assert!(!dup.ok);
        assert!(dup.detail.contains("duplicate"));
    }

    #[test]
    fn ignores_prose_and_code() {
        assert!(check("hello world").is_none());
        assert!(check("fn main() {}").is_none());
        assert!(check("name,age\nalice,30").is_none());
        assert!(check("name: copycraft\ncount: 2\n").is_none());
    }
}
