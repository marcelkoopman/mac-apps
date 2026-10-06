//! The decoder chip: one chip next to Original, titled with what the copy decodes as (JWT,
//! Base64, URL), that shows the decoded text read-only. Detection is string work only and runs
//! with the other chips ([`crate::commands::warm_chips`] off the main thread for a large copy).
//! When several decoders fit, the first in [`DecodeKind`] order wins: one chip, ever.

mod clock;
mod jwt;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};

pub use clock::Env;

const MIN_BASE64_LEN: usize = 8;

/// What a copy decodes as, in precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeKind {
    Jwt,
    Url,
    Base64,
}

impl DecodeKind {
    /// The chip title and the decoded view's card title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Jwt => "JWT",
            Self::Url => "URL",
            Self::Base64 => "Base64",
        }
    }

    /// Search words for the chip.
    pub fn keywords(self) -> &'static str {
        match self {
            Self::Jwt => "decode jwt token",
            Self::Url => "decode url percent query",
            Self::Base64 => "decode base64",
        }
    }
}

/// A decoded copy: which decoder, and the read-only text it shows.
pub struct Decoded {
    pub kind: DecodeKind,
    pub body: String,
}

/// The decoder that fits `text`, for the chip. The same checks as [`decode`].
pub fn detect(text: &str) -> Option<DecodeKind> {
    decode_with(text, &neutral_env()).map(|decoded| decoded.kind)
}

/// The decoded view of `text`, with the real clock, language, date order and time zone.
pub fn decode(text: &str) -> Option<Decoded> {
    decode_with(text, &Env::current())
}

/// [`decode`] against a given clock and locale (tests).
pub fn decode_with(text: &str, env: &Env) -> Option<Decoded> {
    let peeled = peel(text);
    if peeled.is_empty() {
        return None;
    }
    let found = |kind: DecodeKind, body: Option<String>| body.map(|body| Decoded { kind, body });
    let decoded = found(DecodeKind::Jwt, jwt::decode(peeled, env))
        .or_else(|| found(DecodeKind::Url, try_percent(text.trim())))
        .or_else(|| {
            found(
                DecodeKind::Base64,
                try_data_uri(peeled)
                    .or_else(|| try_base64(peeled))
                    .map(pretty),
            )
        })?;
    if decoded.body == peeled || decoded.body == text.trim() {
        return None;
    }
    Some(decoded)
}

/// Decoded text that is itself JSON, XML, … is shown formatted, as the Format view would.
fn pretty(text: String) -> String {
    crate::format::format_text(&text)
}

/// Detection does not depend on the clock or the locale: a fixed, pure environment, so the
/// chip check touches no system API.
fn neutral_env() -> Env {
    fn utc(_: i64) -> i64 {
        0
    }
    Env {
        now_ms: 0,
        lang: crate::locale::Lang::En,
        month_first: false,
        offset: utc,
    }
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
    bytes_to_text(&b64_bytes(&compact)?)
}

pub(crate) fn b64_bytes(input: &str) -> Option<Vec<u8>> {
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

/// A file from `testdata/decode/`, without its final newline (as a Notion code block copies).
#[cfg(test)]
pub(crate) fn testdata(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/decode")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    text.strip_suffix('\n').unwrap_or(&text).to_string()
}

#[cfg(test)]
mod tests {
    use super::{DecodeKind, decode_with, detect, testdata};
    use crate::locale::Lang;

    fn try_decode(text: &str) -> Option<String> {
        decode_with(text, &super::clock::tests::env(Lang::En)).map(|decoded| decoded.body)
    }

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
        assert!(out.contains("Header"));
        assert!(out.contains("Payload"));
        assert!(out.contains("Signature not verified"));
    }

    #[test]
    fn chip_kind_follows_precedence() {
        assert_eq!(detect(&testdata("jwt_expired.txt")), Some(DecodeKind::Jwt));
        assert_eq!(detect("hello%20world"), Some(DecodeKind::Url));
        assert_eq!(detect("SGVsbG8gV29ybGQ="), Some(DecodeKind::Base64));
        assert_eq!(detect(&testdata("jwt_invalid_no_alg.txt")), None);
        assert_eq!(detect(&testdata("jwt_invalid_not_json.txt")), None);
        assert_eq!(detect("hello"), None);
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
