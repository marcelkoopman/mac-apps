//! Base64 and Base64URL, with or without padding, and `data:…;base64,` URIs. Offered only when
//! the bytes are readable UTF-8 text, JSON (shown pretty) or a picture (PNG, JPEG, GIF, WebP,
//! HEIC: its type, size in pixels where the header has it cheaply, and byte count; the card also
//! draws the picture, from [`picture_bytes`], unless it is over the caps below). Ordinary
//! words are not Base64: at least [`MIN_LEN`] characters of a consistent alphabet and length,
//! and text must read as text.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use serde_json::Value as JsonValue;

use super::clock::Env;

/// Shorter copies are words, not Base64.
const MIN_LEN: usize = 8;

/// The card draws a decoded picture of at most this many bytes (20 MiB). A larger one is only
/// described, so a huge payload cannot blow up the card's memory.
pub const MAX_PICTURE_BYTES: usize = 20 * 1024 * 1024;

/// The card draws a picture of at most this many pixels when its header says (50 megapixels, a
/// 48 MP phone photo fits): decoding a picture takes four bytes a pixel.
pub const MAX_PICTURE_PIXELS: u64 = 50_000_000;

/// Base64 characters (12 bytes) that hold the longest magic number [`picture`] looks at.
const MAGIC_CHARS: usize = 16;

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

/// Whether `text` starts like a picture in Base64 (PNG, JPEG, GIF, WebP, HEIC magic bytes), from
/// its first characters alone: cheap, so the card can ask on the main thread before it decodes
/// the rest on a thread ([`picture_bytes`]).
pub fn picture_candidate(text: &str) -> bool {
    let payload = data_uri_payload(text).unwrap_or(text);
    let head: String = payload
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(MAGIC_CHARS)
        .collect();
    head.len() == MAGIC_CHARS && bytes(&head).is_some_and(|bytes| picture(&bytes).is_some())
}

/// The decoded bytes of a Base64 picture the card may draw: a picture by its magic bytes, at
/// most [`MAX_PICTURE_BYTES`], and at most [`MAX_PICTURE_PIXELS`] where the header says. `None`
/// for anything else, and for a picture over the caps (which is still described).
pub fn picture_bytes(text: &str) -> Option<Vec<u8>> {
    let payload = data_uri_payload(text).unwrap_or(text);
    // Room for the line breaks of wrapped Base64, no more: a huge copy is not even scanned.
    if payload.len() > MAX_PICTURE_BYTES / 3 * 4 + MAX_PICTURE_BYTES / 4 {
        return None;
    }
    let compact: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if !plausible(&compact) {
        return None;
    }
    let bytes = bytes(&compact)?;
    let picture = picture(&bytes)?;
    picture.too_big(bytes.len()).is_none().then_some(bytes)
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

/// Why the card does not draw a picture it describes.
#[derive(Debug, PartialEq, Eq)]
enum TooBig {
    Bytes,
    Pixels,
}

impl Picture {
    /// Over a cap of the card ([`MAX_PICTURE_BYTES`], [`MAX_PICTURE_PIXELS`]).
    fn too_big(&self, byte_len: usize) -> Option<TooBig> {
        if byte_len > MAX_PICTURE_BYTES {
            return Some(TooBig::Bytes);
        }
        let (width, height) = self.size?;
        (u64::from(width) * u64::from(height) > MAX_PICTURE_PIXELS).then_some(TooBig::Pixels)
    }

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
        let lang = env.lang;
        match self.too_big(byte_len) {
            Some(TooBig::Bytes) => {
                let mb = MAX_PICTURE_BYTES / (1024 * 1024);
                out.push_str(&format!(
                    "\n{}",
                    crate::locale::tf_in(lang, "decode_picture_over_bytes", &[&mb])
                ));
            }
            Some(TooBig::Pixels) => {
                let mp = MAX_PICTURE_PIXELS / 1_000_000;
                out.push_str(&format!(
                    "\n{}",
                    crate::locale::tf_in(lang, "decode_picture_over_pixels", &[&mp])
                ));
            }
            None => {}
        }
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
    use super::{MAX_PICTURE_BYTES, STANDARD, decode, picture_bytes, picture_candidate};
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;
    use base64::Engine;

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

    /// A PNG signature and IHDR claiming `width` × `height`, then `extra` zero bytes: enough for
    /// the header checks (not a decodable picture).
    fn png_header_base64(width: u32, height: u32, extra: usize) -> String {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 2, 0, 0, 0]);
        bytes.extend(vec![0; extra]);
        STANDARD.encode(bytes)
    }

    #[test]
    fn a_small_picture_is_offered_for_display() {
        for file in [
            "base64_png_image.txt",
            "base64_png.txt",
            "base64_gif.txt",
            "base64_jpeg_data_uri.txt",
            "base64_webp.txt",
            "base64_heic_header.txt",
        ] {
            let text = testdata(file);
            assert!(picture_candidate(&text), "{file}");
            let bytes = picture_bytes(&text).unwrap_or_else(|| panic!("{file}: no bytes"));
            assert!(bytes.len() > 20, "{file}");
        }
        // Quotes, line breaks and the URL-safe alphabet do not matter.
        let png = testdata("base64_png_image.txt");
        let wrapped: Vec<String> = png
            .as_bytes()
            .chunks(20)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        assert!(picture_candidate(&wrapped.join("\n")));
        assert_eq!(
            picture_bytes(&wrapped.join("\n")),
            picture_bytes(&png),
            "wrapped"
        );
    }

    #[test]
    fn the_sample_png_decodes_to_a_real_picture() {
        let bytes = picture_bytes(&testdata("base64_png_image.txt")).expect("png bytes");
        let image = image::load_from_memory(&bytes).expect("a valid PNG");
        assert_eq!((image.width(), image.height()), (4, 4));
        let described = en(&testdata("base64_png_image.txt")).expect("described");
        assert!(
            described.starts_with("PNG image\n4 × 4 pixels\n"),
            "{described}"
        );
        assert!(!described.contains("Not shown"), "{described}");
    }

    #[test]
    fn text_and_noise_are_not_pictures() {
        for file in [
            "base64_text.txt",
            "base64url_json_no_padding.txt",
            "base64_negative_binary.txt",
        ] {
            let text = testdata(file);
            assert!(!picture_candidate(&text), "{file}");
            assert_eq!(picture_bytes(&text), None, "{file}");
        }
        assert!(!picture_candidate(""));
        assert!(!picture_candidate("iVBORw0K"));
        assert_eq!(picture_bytes("iVBORw0KGgoAAAANSUhEU"), None, "bad length");
        assert_eq!(picture_bytes("hello world, this is text"), None);
    }

    #[test]
    fn a_big_picture_is_described_not_drawn() {
        // Over the byte cap: the header looks fine, the payload is too large.
        let big = png_header_base64(10, 10, MAX_PICTURE_BYTES);
        assert!(picture_candidate(&big));
        assert_eq!(picture_bytes(&big), None);
        let described = en(&big).expect("described");
        assert!(
            described.starts_with("PNG image\n10 × 10 pixels\n"),
            "{described}"
        );
        assert!(
            described.ends_with("Not shown: larger than 20 MB"),
            "{described}"
        );
        let dutch = decode(&big, &env(Lang::Nl)).expect("described in Dutch");
        assert!(dutch.ends_with("Niet getoond: groter dan 20 MB"), "{dutch}");
        // Just under the cap is drawn.
        let edge = png_header_base64(10, 10, MAX_PICTURE_BYTES - 64);
        assert_eq!(
            picture_bytes(&edge).map(|bytes| bytes.len()),
            Some(MAX_PICTURE_BYTES - 64 + 29)
        );
        // Over the pixel cap by the header, small in bytes.
        let wide = png_header_base64(10_000, 5_001, 16);
        assert_eq!(picture_bytes(&wide), None);
        let described = en(&wide).expect("described");
        assert!(described.contains("10000 × 5001 pixels"), "{described}");
        assert!(
            described.ends_with("Not shown: more than 50 megapixels"),
            "{described}"
        );
        let dutch = decode(&wide, &env(Lang::Nl)).expect("described in Dutch");
        assert!(
            dutch.ends_with("Niet getoond: meer dan 50 megapixel"),
            "{dutch}"
        );
        // At the pixel cap exactly it is drawn.
        assert!(picture_bytes(&png_header_base64(10_000, 5_000, 16)).is_some());
        // A payload far past the cap is refused before it is decoded.
        let huge = "A".repeat(MAX_PICTURE_BYTES * 2);
        assert_eq!(picture_bytes(&huge), None);
    }
}
