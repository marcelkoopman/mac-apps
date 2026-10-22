//! Encoding… in Text ▾ › Convert: the whole text to and from Base64 (standard, RFC 4648 §4,
//! and URL-safe, §5), percent-encoding (RFC 3986) and hex.
//!
//! Encoding takes the text's UTF-8 bytes exactly as copied. Decoding ignores space and line
//! breaks around and inside the code (wrapped Base64 and hex dumps) and must give UTF-8 text:
//! the card holds text. Invalid input gives an [`Invalid`] reason, never the input itself.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_PAD_INDIFFERENT};
use base64::engine::general_purpose::{URL_SAFE_NO_PAD, URL_SAFE_NO_PAD_INDIFFERENT};

/// One step of the Encoding submenu, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Base64Encode,
    Base64Decode,
    Base64UrlEncode,
    Base64UrlDecode,
    UrlEncode,
    UrlDecode,
    HexEncode,
    HexDecode,
}

/// Why a decode gave nothing. Names the problem, never the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The input is not this code (a stray character, a bad length, a broken `%` escape).
    NotEncoded,
    /// It decodes, but the bytes are not UTF-8 text.
    NotText,
}

impl Codec {
    pub const ALL: [Codec; 8] = [
        Codec::Base64Encode,
        Codec::Base64Decode,
        Codec::Base64UrlEncode,
        Codec::Base64UrlDecode,
        Codec::UrlEncode,
        Codec::UrlDecode,
        Codec::HexEncode,
        Codec::HexDecode,
    ];

    /// Its menu item.
    pub fn title(self) -> &'static str {
        match self {
            Codec::Base64Encode => crate::locale::t("encode_base64"),
            Codec::Base64Decode => crate::locale::t("decode_base64"),
            Codec::Base64UrlEncode => crate::locale::t("encode_base64_url"),
            Codec::Base64UrlDecode => crate::locale::t("decode_base64_url"),
            Codec::UrlEncode => crate::locale::t("encode_url"),
            Codec::UrlDecode => crate::locale::t("decode_url"),
            Codec::HexEncode => crate::locale::t("encode_hex"),
            Codec::HexDecode => crate::locale::t("decode_hex"),
        }
    }

    /// The code's name in a message: "Base64", "URL encoding", "hex".
    pub fn code_name(self) -> &'static str {
        match self {
            Codec::Base64Encode | Codec::Base64Decode => "Base64",
            Codec::Base64UrlEncode | Codec::Base64UrlDecode => "Base64url",
            Codec::UrlEncode | Codec::UrlDecode => crate::locale::t("encoding_url_name"),
            Codec::HexEncode | Codec::HexDecode => crate::locale::t("encoding_hex_name"),
        }
    }

    /// Encode, not decode.
    pub fn encodes(self) -> bool {
        matches!(
            self,
            Codec::Base64Encode | Codec::Base64UrlEncode | Codec::UrlEncode | Codec::HexEncode
        )
    }

    /// The version's name in the version capsule: "Base64 encoded", "Hex decoded".
    pub fn label(self) -> String {
        if self.encodes() {
            crate::locale::tf("encoding_label_encoded", &[&self.code_name()])
        } else {
            crate::locale::tf("encoding_label_decoded", &[&self.code_name()])
        }
    }

    /// The note when `reason` stopped a decode. Content-free.
    pub fn invalid_note(self, reason: Invalid) -> String {
        match reason {
            Invalid::NotEncoded => crate::locale::tf("encoding_invalid", &[&self.code_name()]),
            Invalid::NotText => crate::locale::t("encoding_not_text").to_string(),
        }
    }

    /// `text` encoded or decoded.
    pub fn apply(self, text: &str) -> Result<String, Invalid> {
        match self {
            Codec::Base64Encode => Ok(STANDARD.encode(text)),
            Codec::Base64UrlEncode => Ok(URL_SAFE_NO_PAD.encode(text)),
            Codec::Base64Decode => utf8(
                STANDARD_PAD_INDIFFERENT
                    .decode(squeezed(text))
                    .map_err(|_| Invalid::NotEncoded)?,
            ),
            Codec::Base64UrlDecode => utf8(
                URL_SAFE_NO_PAD_INDIFFERENT
                    .decode(squeezed(text))
                    .map_err(|_| Invalid::NotEncoded)?,
            ),
            Codec::UrlEncode => Ok(percent_encode(text)),
            Codec::UrlDecode => percent_decode(text.trim()),
            Codec::HexEncode => Ok(hex_encode(text.as_bytes())),
            Codec::HexDecode => utf8(hex_decode(text)?),
        }
    }
}

/// The text without any whitespace (wrapped Base64 and spaced hex dumps).
fn squeezed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn utf8(bytes: Vec<u8>) -> Result<String, Invalid> {
    String::from_utf8(bytes).map_err(|_| Invalid::NotText)
}

/// Every byte except the RFC 3986 unreserved characters (`A–Z a–z 0–9 - . _ ~`) as `%XX`.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4, true));
            out.push(hex_digit(byte & 0x0f, true));
        }
    }
    out
}

/// `%XX` to bytes; `+` stays a plus (RFC 3986, not a form). A `%` without two hex digits
/// after it is invalid.
fn percent_decode(text: &str) -> Result<String, Invalid> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let high = bytes.get(at + 1).and_then(|b| hex_value(*b));
            let low = bytes.get(at + 2).and_then(|b| hex_value(*b));
            let (Some(high), Some(low)) = (high, low) else {
                return Err(Invalid::NotEncoded);
            };
            out.push(high << 4 | low);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    utf8(out)
}

fn hex_digit(nibble: u8, upper: bool) -> char {
    let digits: &[u8; 16] = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    digits[usize::from(nibble & 0x0f)] as char
}

fn hex_value(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|value| value as u8)
}

/// Lowercase hex, two digits a byte, nothing between.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_digit(byte >> 4, false));
        out.push(hex_digit(byte & 0x0f, false));
    }
    out
}

/// Hex digits to bytes: whitespace anywhere and one leading `0x` are allowed, either case.
fn hex_decode(text: &str) -> Result<Vec<u8>, Invalid> {
    let squeezed = squeezed(text);
    let digits = squeezed
        .strip_prefix("0x")
        .or_else(|| squeezed.strip_prefix("0X"))
        .unwrap_or(&squeezed)
        .as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return Err(Invalid::NotEncoded);
    }
    digits
        .chunks(2)
        .map(|pair| match (hex_value(pair[0]), hex_value(pair[1])) {
            (Some(high), Some(low)) => Ok(high << 4 | low),
            _ => Err(Invalid::NotEncoded),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Codec, Invalid};

    /// RFC 4648 §10 test vectors.
    const RFC4648: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn base64_matches_the_rfc_4648_vectors_both_ways() {
        for (plain, coded) in RFC4648 {
            assert_eq!(Codec::Base64Encode.apply(plain).as_deref(), Ok(coded));
            assert_eq!(Codec::Base64Decode.apply(coded).as_deref(), Ok(plain));
            let url = coded.trim_end_matches('=');
            assert_eq!(Codec::Base64UrlEncode.apply(plain).as_deref(), Ok(url));
            assert_eq!(Codec::Base64UrlDecode.apply(url).as_deref(), Ok(plain));
            // Padding is optional on decode.
            assert_eq!(Codec::Base64UrlDecode.apply(coded).as_deref(), Ok(plain));
            assert_eq!(Codec::Base64Decode.apply(url).as_deref(), Ok(plain));
        }
    }

    #[test]
    fn rfc_4648_hex_vectors() {
        for (plain, hex) in [("", ""), ("f", "66"), ("foobar", "666f6f626172")] {
            assert_eq!(Codec::HexEncode.apply(plain).as_deref(), Ok(hex));
            if !plain.is_empty() {
                assert_eq!(Codec::HexDecode.apply(hex).as_deref(), Ok(plain));
                let upper = hex.to_uppercase();
                assert_eq!(Codec::HexDecode.apply(&upper).as_deref(), Ok(plain));
            }
        }
        assert_eq!(Codec::HexDecode.apply("0x66 6f\n6f").as_deref(), Ok("foo"));
    }

    #[test]
    fn url_safe_base64_uses_minus_and_underscore() {
        // 0xfb 0xff: "+/8=" in the standard alphabet.
        let text = "\u{fbff}";
        let standard = Codec::Base64Encode.apply(text).expect("encodes");
        let url = Codec::Base64UrlEncode.apply(text).expect("encodes");
        assert!(
            standard.contains('+') || standard.contains('/'),
            "{standard}"
        );
        assert!(!url.contains('+') && !url.contains('/') && !url.contains('='));
        assert_eq!(Codec::Base64UrlDecode.apply(&url).as_deref(), Ok(text));
        assert_eq!(
            Codec::Base64Decode.apply(&url),
            Err(Invalid::NotEncoded),
            "the standard alphabet refuses - and _"
        );
    }

    #[test]
    fn wrapped_base64_decodes() {
        assert_eq!(
            Codec::Base64Decode.apply("  Zm9v\nYmFy\r\n").as_deref(),
            Ok("foobar")
        );
    }

    #[test]
    fn url_encoding_keeps_only_the_unreserved_characters() {
        let encoded = Codec::UrlEncode.apply("a b&c=d/é~_.-").expect("encodes");
        assert_eq!(encoded, "a%20b%26c%3Dd%2F%C3%A9~_.-");
        assert_eq!(
            Codec::UrlDecode.apply(&encoded).as_deref(),
            Ok("a b&c=d/é~_.-")
        );
        assert_eq!(Codec::UrlDecode.apply("a+b%2B").as_deref(), Ok("a+b+"));
        assert_eq!(Codec::UrlDecode.apply("100%"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::UrlDecode.apply("%zz"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::UrlDecode.apply("%ff"), Err(Invalid::NotText));
    }

    #[test]
    fn invalid_input_gives_a_reason_and_a_note_without_the_input() {
        assert_eq!(Codec::Base64Decode.apply("abc$"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::Base64Decode.apply("Zg=a"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::HexDecode.apply("abc"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::HexDecode.apply("zz"), Err(Invalid::NotEncoded));
        assert_eq!(Codec::HexDecode.apply(""), Err(Invalid::NotEncoded));
        // 0xff is no UTF-8.
        assert_eq!(Codec::HexDecode.apply("ff"), Err(Invalid::NotText));
        assert_eq!(Codec::Base64Decode.apply("/w=="), Err(Invalid::NotText));
        let secret = "s3cr3t-$$$";
        for codec in Codec::ALL {
            for reason in [Invalid::NotEncoded, Invalid::NotText] {
                let note = codec.invalid_note(reason);
                assert!(!note.is_empty() && !note.contains(secret), "{codec:?}");
            }
        }
    }

    #[test]
    fn every_codec_has_a_title_and_a_label() {
        for codec in Codec::ALL {
            assert_ne!(codec.title(), "?", "{codec:?}");
            assert!(!codec.label().is_empty());
        }
        assert_eq!(Codec::ALL.iter().filter(|codec| codec.encodes()).count(), 4);
    }
}
