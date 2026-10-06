//! The decoder chip: one chip next to Original, titled with what the copy decodes as (JWT,
//! UUID, Unix time, URL, Hash, Base64), that shows the decoded text read-only. Detection is string work only and runs
//! with the other chips ([`crate::commands::warm_chips`] off the main thread for a large copy).
//! When several decoders fit, the first in [`DecodeKind`] order wins: one chip, ever.

mod base64;
mod clock;
mod hash;
mod jwt;
mod unix_time;
mod url;
mod uuid;

pub use clock::Env;

/// What a copy decodes as, in precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeKind {
    Jwt,
    Uuid,
    UnixTime,
    Url,
    Hash,
    Base64,
}

impl DecodeKind {
    /// The chip title and the decoded view's card title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Jwt => "JWT",
            Self::Uuid => "UUID",
            Self::UnixTime => crate::locale::t("decode_unix_time"),
            Self::Url => "URL",
            Self::Hash => "Hash",
            Self::Base64 => "Base64",
        }
    }

    /// Search words for the chip.
    pub fn keywords(self) -> &'static str {
        match self {
            Self::Jwt => "decode jwt token",
            Self::Uuid => "decode uuid guid version",
            Self::UnixTime => "decode unix time timestamp epoch date",
            Self::Url => "decode url percent query",
            Self::Hash => "decode hash md5 sha digest",
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
        .or_else(|| found(DecodeKind::Uuid, uuid::decode(peeled, env)))
        .or_else(|| found(DecodeKind::UnixTime, unix_time::decode(peeled, env)))
        .or_else(|| found(DecodeKind::Url, url::decode(peeled, env)))
        .or_else(|| found(DecodeKind::Hash, hash::decode(peeled, env)))
        .or_else(|| found(DecodeKind::Base64, base64::decode(peeled, env)))?;
    if decoded.body == peeled || decoded.body == text.trim() {
        return None;
    }
    Some(decoded)
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

/// Text a person can read: [`is_mostly_printable`], and at least half letters, digits or
/// white space (decoded noise is mostly punctuation and symbols).
fn is_readable(text: &str) -> bool {
    if !is_mostly_printable(text) {
        return false;
    }
    let total = text.chars().count();
    let wordy = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count();
    wordy * 2 >= total
}

pub(crate) fn is_mostly_printable(text: &str) -> bool {
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
        let url = try_decode("https://example.com/q?x=hello%2Fworld").expect("url");
        assert!(url.ends_with("Query parameters\nx = hello/world"), "{url}");
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
    fn negative_samples_get_no_chip() {
        for file in [
            "base64_negative_words.txt",
            "base64_negative_binary.txt",
            "hash_negative.txt",
            "jwt_invalid_no_alg.txt",
            "jwt_invalid_not_json.txt",
            "unix_negative_numbers.txt",
            "url_negative.txt",
            "uuid_negative.txt",
        ] {
            for line in testdata(file).lines() {
                let want = (line == "6f9619ff8b864011b42d00c04fc964ff").then_some(DecodeKind::Hash);
                assert_eq!(detect(line), want, "{file}: {line}");
            }
        }
    }

    #[test]
    fn chip_kind_follows_precedence() {
        assert_eq!(detect(&testdata("jwt_expired.txt")), Some(DecodeKind::Jwt));
        assert_eq!(detect("hello%20world"), Some(DecodeKind::Url));
        assert_eq!(detect("SGVsbG8gV29ybGQ="), Some(DecodeKind::Base64));
        assert_eq!(detect(&testdata("jwt_invalid_no_alg.txt")), None);
        assert_eq!(detect(&testdata("jwt_invalid_not_json.txt")), None);
        assert_eq!(detect("hello"), None);
        // 32 hex digits: a hash (MD5), not a UUID and not Base64; with dashes a UUID.
        assert_eq!(
            detect("6f9619ff8b864011b42d00c04fc964ff"),
            Some(DecodeKind::Hash)
        );
        assert_eq!(
            detect("6f9619ff-8b86-4011-b42d-00c04fc964ff"),
            Some(DecodeKind::Uuid)
        );
        assert_eq!(detect("1700000000"), Some(DecodeKind::UnixTime));
        assert_eq!(detect(&testdata("hash_sha256.txt")), Some(DecodeKind::Hash));
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
