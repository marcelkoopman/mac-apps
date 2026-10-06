//! Base64 and Base64URL, with or without padding, and `data:…;base64,` URIs. Offered only when
//! the bytes are readable UTF-8 text, JSON (shown pretty) or a picture (PNG, JPEG, GIF, WebP,
//! HEIC: its type, size in pixels where the header has it cheaply, and byte count). Ordinary
//! words are not Base64: at least [`MIN_LEN`] characters of a consistent alphabet and length,
//! and text must read as text.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use serde_json::Value as JsonValue;

use super::clock::Env;

/// Shorter copies are words, not Base64.
const MIN_LEN: usize = 8;

/// The decoded view, or `None` when `text` is not Base64 of something readable.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let payload = data_uri_payload(text).unwrap_or(text);
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if !plausible(&compact) {
        return None;
    }
    let bytes = bytes(&compact)?;
    if let Some(picture) = picture(&bytes) {
        return Some(picture.describe(bytes.len(), env));
    }
    let text = std::str::from_utf8(&bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    if let Ok(json @ (JsonValue::Object(_) | JsonValue::Array(_))) =
        serde_json::from_str::<JsonValue>(text)
    {
        return serde_json::to_string_pretty(&json).ok();
    }
    super::is_readable(text).then(|| text.to_string())
}

/// The Base64 after `data:<type>;base64,`.
fn data_uri_payload(text: &str) -> Option<&str> {
    let head = text.get(..5)?;
    if !head.eq_ignore_ascii_case("data:") {
        return None;
    }
    let comma = text.find(',')?;
    text[..comma]
        .to_ascii_lowercase()
        .ends_with(";base64")
        .then(|| &text[comma + 1..])
}

/// Long enough, one alphabet (standard `+/` or URL-safe `-_`, not both), padding only at the
/// end and only where the length needs it, and not a bare number.
fn plausible(compact: &str) -> bool {
    if compact.len() < MIN_LEN || compact.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let body = compact.trim_end_matches('=');
    let padding = compact.len() - body.len();
    if padding > 2 || (padding > 0 && !compact.len().is_multiple_of(4)) || body.len() % 4 == 1 {
        return false;
    }
    let standard = body.bytes().any(|b| b == b'+' || b == b'/');
    let url_safe = body.bytes().any(|b| b == b'-' || b == b'_');
    !(standard && url_safe)
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'-' | b'_'))
}

/// The bytes under any of the four engines (strict: no stray trailing bits).
pub(crate) fn bytes(input: &str) -> Option<Vec<u8>> {
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .into_iter()
        .find_map(|engine| engine.decode(input).ok())
}

/// A picture recognised by its magic bytes.
#[derive(Debug, PartialEq, Eq)]
struct Picture {
    format: &'static str,
    size: Option<(u32, u32)>,
}

impl Picture {
    fn describe(&self, byte_len: usize, env: &Env) -> String {
        let mut out = env
            .pick(
                &format!("{}-afbeelding", self.format),
                &format!("{} image", self.format),
            )
            .to_string();
        if let Some((width, height)) = self.size {
            out.push_str(&format!("\n{width} × {height} pixels"));
        }
        out.push_str(&format!("\n{byte_len} bytes"));
        out
    }
}

fn picture(bytes: &[u8]) -> Option<Picture> {
    let le16 = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let be32 = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let le24 = |at: usize| {
        let b = bytes.get(at..at + 3)?;
        Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
    };
    let found = |format, size: Option<(u32, u32)>| Some(Picture { format, size });
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let size = (bytes.get(12..16) == Some(b"IHDR"))
            .then(|| Some((be32(16)?, be32(20)?)))
            .flatten();
        return found("PNG", size);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let size = le16(6).zip(le16(8)).map(|(w, h)| (w.into(), h.into()));
        return found("GIF", size);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return found("JPEG", jpeg_size(bytes));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let size = match bytes.get(12..16) {
            Some(b"VP8 ") if bytes.get(23..26) == Some(&[0x9d, 0x01, 0x2a]) => le16(26)
                .zip(le16(28))
                .map(|(w, h)| (u32::from(w & 0x3fff), u32::from(h & 0x3fff))),
            Some(b"VP8L") if bytes.get(20) == Some(&0x2f) => bytes
                .get(21..25)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes)
                .map(|bits| ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)),
            Some(b"VP8X") => le24(24).zip(le24(27)).map(|(w, h)| (w + 1, h + 1)),
            _ => None,
        };
        return found("WebP", size);
    }
    let heif_brands: [&[u8]; 8] = [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1",
    ];
    if bytes.get(4..8) == Some(b"ftyp")
        && bytes
            .get(8..12)
            .is_some_and(|brand| heif_brands.contains(&brand))
    {
        // The first `ispe` (image spatial extent) box: version/flags, then width and height.
        let head = &bytes[..bytes.len().min(64 * 1024)];
        let size = memchr::memmem::find(head, b"ispe")
            .and_then(|at| Some((be32(at + 8)?, be32(at + 12)?)));
        return found("HEIC", size);
    }
    None
}

/// Width and height from the first start-of-frame segment.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD9) {
            at += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let frame = bytes.get(at + 5..at + 9)?;
            let height = u16::from_be_bytes([frame[0], frame[1]]);
            let width = u16::from_be_bytes([frame[2], frame[3]]);
            return Some((width.into(), height.into()));
        }
        at += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    fn en(text: &str) -> Option<String> {
        decode(text, &env(Lang::En))
    }

    #[test]
    fn text_and_json() {
        let out = en(&testdata("base64_text.txt")).expect("text");
        assert_eq!(
            out,
            "Copycraft decodes this synthetic test sentence.\nSecond line: café ✓"
        );
        let json = en(&testdata("base64url_json_no_padding.txt")).expect("json");
        assert!(json.starts_with("{\n  \"user\": \"test-user\""), "{json}");
        assert!(json.contains("\"roles\": [\n"));
        // The same JSON padded and in the standard alphabet.
        assert_eq!(
            en("eyJuYW1lIjoiY29weWNyYWZ0In0=").as_deref(),
            Some("{\n  \"name\": \"copycraft\"\n}")
        );
        assert_eq!(en("SGVs\nbG8g\nV29y\nbGQ=").as_deref(), Some("Hello World"));
        assert_eq!(en("aGVsbG8td29ybGQ_").as_deref(), Some("hello-world?"));
        assert_eq!(
            en("data:text/plain;base64,SGVsbG8gV29ybGQ=").as_deref(),
            Some("Hello World")
        );
    }

    #[test]
    fn pictures_show_type_and_size() {
        let png = en(&testdata("base64_png.txt")).expect("png");
        assert!(png.starts_with("PNG image\n3 × 2 pixels\n"), "{png}");
        let gif = en(&testdata("base64_gif.txt")).expect("gif");
        assert!(gif.starts_with("GIF image\n5 × 4 pixels"), "{gif}");
        let jpeg = en(&testdata("base64_jpeg_data_uri.txt")).expect("jpeg");
        assert!(jpeg.starts_with("JPEG image\n8 × 6 pixels"), "{jpeg}");
        let webp = en(&testdata("base64_webp.txt")).expect("webp");
        assert!(webp.starts_with("WebP image\n7 × 9 pixels"), "{webp}");
        let heic = decode(&testdata("base64_heic_header.txt"), &env(Lang::Nl)).expect("heic");
        assert!(
            heic.starts_with("HEIC-afbeelding\n4032 × 3024 pixels\n44 bytes"),
            "{heic}"
        );
    }

    #[test]
    fn words_numbers_and_binary_are_not_base64() {
        for word in testdata("base64_negative_words.txt").lines() {
            assert_eq!(en(word), None, "{word}");
        }
        assert_eq!(en(&testdata("base64_negative_binary.txt")), None);
        assert_eq!(en("YQ=="), None);
        // Both alphabets at once, padding in the middle.
        assert_eq!(en("SGVsbG8+V29y_GQ="), None);
        assert_eq!(en("SGVsbG8=V29ybGQ="), None);
    }
}
