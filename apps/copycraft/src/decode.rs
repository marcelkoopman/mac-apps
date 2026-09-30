use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use serde_json::Value as JsonValue;

const MIN_BASE64_LEN: usize = 8;

pub fn try_decode(text: &str) -> Option<String> {
    let peeled = peel(text);
    if peeled.is_empty() {
        return None;
    }
    let decoded = try_jwt(peeled)
        .or_else(|| try_data_uri(peeled))
        .or_else(|| try_base64(peeled))
        .or_else(|| try_percent(text.trim()))?;
    if decoded == peeled || decoded == text.trim() {
        return None;
    }
    Some(decoded)
}

fn peel(text: &str) -> &str {
    let trimmed = text.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    }
}

fn try_jwt(text: &str) -> Option<String> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    if !looks_like_b64url(parts[0]) || !looks_like_b64url(parts[1]) {
        return None;
    }
    let header = decode_b64_to_json(parts[0])?;
    let payload = decode_b64_to_json(parts[1])?;
    let mut root = serde_json::Map::new();
    root.insert("header".into(), header);
    root.insert("payload".into(), payload);
    serde_json::to_string_pretty(&JsonValue::Object(root)).ok()
}

fn looks_like_b64url(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
}

fn decode_b64_to_json(part: &str) -> Option<JsonValue> {
    serde_json::from_slice(&decode_b64_bytes(part)?).ok()
}

fn try_data_uri(text: &str) -> Option<String> {
    let marker = ";base64,";
    let lower = text.to_ascii_lowercase();
    if !lower.starts_with("data:") {
        return None;
    }
    let idx = lower.find(marker)?;
    try_base64(text.get(idx + marker.len()..)?)
}

fn try_base64(text: &str) -> Option<String> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < MIN_BASE64_LEN || compact.len() % 4 == 1 || !is_base64_alphabet(&compact) {
        return None;
    }
    bytes_to_text(&decode_b64_bytes(&compact)?)
}

fn decode_b64_bytes(input: &str) -> Option<Vec<u8>> {
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .into_iter()
        .find_map(|engine| engine.decode(input).ok())
}

fn is_base64_alphabet(text: &str) -> bool {
    let mut padding = 0usize;
    for c in text.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' | '-' | '_' => {
                if padding > 0 {
                    return false;
                }
            }
            '=' => {
                padding += 1;
                if padding > 2 {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn bytes_to_text(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    if text.is_empty() || !is_mostly_printable(text) {
        return None;
    }
    Some(text.to_string())
}

fn is_mostly_printable(text: &str) -> bool {
    if text.contains('\0') {
        return false;
    }
    let total = text.chars().count();
    if total == 0 {
        return false;
    }
    let printable = text
        .chars()
        .filter(|c| *c == '\n' || *c == '\r' || *c == '\t' || !c.is_control())
        .count();
    printable * 20 >= total * 19
}

fn try_percent(text: &str) -> Option<String> {
    if text.is_empty() || !has_percent_escape(text) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = from_hex(bytes[i + 1])?;
            let lo = from_hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    bytes_to_text(&out).filter(|decoded| decoded.as_str() != text)
}

fn has_percent_escape(text: &str) -> bool {
    text.as_bytes().windows(3).any(|window| {
        window[0] == b'%' && window[1].is_ascii_hexdigit() && window[2].is_ascii_hexdigit()
    })
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::try_decode;

    #[test]
    fn decodes_standard_base64() {
        assert_eq!(
            try_decode("SGVsbG8gV29ybGQ=").as_deref(),
            Some("Hello World")
        );
    }

    #[test]
    fn decodes_base64_with_whitespace() {
        let src = "SGVs\nbG8g\nV29y\nbGQ=";
        assert_eq!(try_decode(src).as_deref(), Some("Hello World"));
    }

    #[test]
    fn decodes_quoted_base64() {
        assert_eq!(
            try_decode("\"SGVsbG8gV29ybGQ=\"").as_deref(),
            Some("Hello World")
        );
    }

    #[test]
    fn decodes_url_safe_base64() {
        assert_eq!(
            try_decode("aGVsbG8td29ybGQ_").as_deref(),
            Some("hello-world?")
        );
    }

    #[test]
    fn decodes_json_from_base64() {
        let out = try_decode("eyJuYW1lIjoiY29weWNyYWZ0In0=").expect("json");
        assert!(out.contains("copycraft"));
        assert!(out.contains("name"));
    }

    #[test]
    fn decodes_data_uri() {
        let src = "data:text/plain;base64,SGVsbG8gV29ybGQ=";
        assert_eq!(try_decode(src).as_deref(), Some("Hello World"));
    }

    #[test]
    fn decodes_percent_encoding() {
        assert_eq!(try_decode("hello%20world").as_deref(), Some("hello world"));
        assert_eq!(
            try_decode("https://example.com/q?x=hello%2Fworld").as_deref(),
            Some("https://example.com/q?x=hello/world")
        );
    }

    #[test]
    fn decodes_utf8_percent_encoding() {
        assert_eq!(try_decode("caf%C3%A9").as_deref(), Some("café"));
    }

    #[test]
    fn decodes_jwt_header_and_payload() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = try_decode(token).expect("jwt");
        assert!(out.contains("\"alg\": \"HS256\""));
        assert!(out.contains("John Doe"));
        assert!(out.contains("header"));
        assert!(out.contains("payload"));
    }

    #[test]
    fn rejects_plain_text() {
        assert_eq!(try_decode("hello world"), None);
        assert_eq!(try_decode("copycraft"), None);
        assert_eq!(try_decode("{\"name\":\"copycraft\"}"), None);
        assert_eq!(try_decode("https://example.com/path"), None);
        assert_eq!(try_decode("100% done"), None);
        assert_eq!(try_decode(""), None);
        assert_eq!(try_decode("YQ=="), None);
    }
}
