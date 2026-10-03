use std::path::Path;

use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;
use crate::commands::{ImageFacts, ImageScan};

/// Pasted files larger than this stay off the card.
pub const MAX_FILE_BYTES: u64 = 8_000_000;
/// Dropped pictures larger than this stay off the card. Higher than for text: a photo is
/// easily over 8 MB, and a picture dragged from a web page often comes as uncompressed TIFF.
pub const MAX_IMAGE_BYTES: u64 = 32_000_000;

const NOT_TEXT: &str = "This file is not text";
const TOO_LARGE: &str = "File is larger than 8 MB";
const UNREADABLE: &str = "Can't read this file";
const TEXT_TOO_LARGE: &str = "Text is larger than 8 MB";
const EMPTY_TEXT: &str = "The dropped text is empty";
const IMAGE_TOO_LARGE: &str = "Image is larger than 32 MB";
/// For a dropped picture the system cannot draw.
pub const UNREADABLE_IMAGE: &str = "Can't read this image";

/// A file the card is showing instead of the clipboard.
#[derive(Clone)]
pub struct OpenedFile {
    pub name: String,
    pub text: Option<Zeroizing<String>>,
    pub note: Option<String>,
    /// A dropped picture, shown like a copied one.
    pub image: Option<OpenedImage>,
}

/// A dropped picture: its encoded bytes (shared with its history entry), and what the image
/// card shows about it once the app has looked.
#[derive(Clone)]
pub struct OpenedImage {
    pub bytes: SecretBytes,
    pub facts: Option<ImageFacts>,
    pub scan: Option<ImageScan>,
}

/// A dropped file: a picture when its contents are one (see [`is_image_file`]), else read as
/// text like [`load`]. A binary file that is not text either is tried as a picture too: the
/// drop only lets text and image files through, and the system draws formats the sniffer does
/// not know (camera RAW, …). Whether it can is up to the card ([`UNREADABLE_IMAGE`]).
pub fn load_dropped(path: &Path) -> OpenedFile {
    if is_image_file(path) {
        return load_image(path);
    }
    let opened = load(path);
    if opened.note.as_deref() == Some(NOT_TEXT) {
        return load_image(path);
    }
    opened
}

fn load_image(path: &Path) -> OpenedFile {
    let name = file_name(path);
    let Ok(meta) = std::fs::metadata(path) else {
        return noted(name, UNREADABLE);
    };
    if !meta.is_file() {
        return noted(name, UNREADABLE);
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return noted(name, IMAGE_TOO_LARGE);
    }
    match std::fs::read(path) {
        Ok(bytes) => from_image_bytes(name, bytes),
        Err(_) => noted(name, UNREADABLE),
    }
}

/// Image data dropped on the card, shown like a file called `name`.
pub fn from_image_data(name: &str, mut bytes: Zeroizing<Vec<u8>>) -> OpenedFile {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return noted(name.to_string(), IMAGE_TOO_LARGE);
    }
    // Moved, not copied, into the zeroizing history buffer.
    from_image_bytes(name.to_string(), std::mem::take(&mut *bytes))
}

fn from_image_bytes(name: String, bytes: Vec<u8>) -> OpenedFile {
    match SecretBytes::new(bytes) {
        Some(bytes) => OpenedFile {
            name,
            text: None,
            note: None,
            image: Some(OpenedImage {
                bytes,
                facts: None,
                scan: None,
            }),
        },
        None => noted(name, UNREADABLE_IMAGE),
    }
}

/// The file starts like a picture (PNG, JPEG, HEIC, GIF, WebP, TIFF, BMP, …).
fn is_image_file(path: &Path) -> bool {
    matches!(
        infer::get_from_path(path),
        Ok(Some(kind)) if kind.matcher_type() == infer::MatcherType::Image
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

pub fn load(path: &Path) -> OpenedFile {
    let name = file_name(path);
    let Ok(meta) = std::fs::metadata(path) else {
        return noted(name, UNREADABLE);
    };
    if !meta.is_file() {
        return noted(name, UNREADABLE);
    }
    if meta.len() > MAX_FILE_BYTES {
        return noted(name, TOO_LARGE);
    }
    // Zeroizing from the first byte, so the file contents are overwritten on every path out.
    let bytes = match std::fs::read(path) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(_) => return noted(name, UNREADABLE),
    };
    match classify(&bytes) {
        Classified::Text(text) => OpenedFile {
            name,
            text: Some(text),
            note: None,
            image: None,
        },
        Classified::NotText => noted(name, NOT_TEXT),
        Classified::TooLarge => noted(name, TOO_LARGE),
    }
}

/// Dropped text, shown on the card like a file called `name`. It has the file size limit.
pub fn from_text(name: &str, text: Zeroizing<String>) -> OpenedFile {
    let name = name.to_string();
    if text.len() as u64 > MAX_FILE_BYTES {
        return noted(name, TEXT_TOO_LARGE);
    }
    if text.trim().is_empty() {
        return noted(name, EMPTY_TEXT);
    }
    OpenedFile {
        name,
        text: Some(text),
        note: None,
        image: None,
    }
}

enum Classified {
    Text(Zeroizing<String>),
    NotText,
    TooLarge,
}

fn classify(bytes: &[u8]) -> Classified {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Classified::TooLarge;
    }
    match decode(bytes) {
        Some(text) => Classified::Text(text),
        None => Classified::NotText,
    }
}

pub fn noted(name: String, note: &str) -> OpenedFile {
    OpenedFile {
        name,
        text: None,
        note: Some(note.to_string()),
        image: None,
    }
}

/// The file's text in one exact-size allocation, so no copy of it is left behind unzeroized.
fn decode(bytes: &[u8]) -> Option<Zeroizing<String>> {
    if let Some(text) = decode_utf16(bytes) {
        return Some(text);
    }
    if bytes.contains(&0) {
        return None;
    }
    // Validate in place: `String::from_utf8(bytes.to_vec())` made a second copy that was
    // dropped without being zeroized.
    let text = std::str::from_utf8(bytes).ok()?;
    Some(Zeroizing::new(
        text.trim_start_matches('\u{feff}').to_string(),
    ))
}

fn decode_utf16(bytes: &[u8]) -> Option<Zeroizing<String>> {
    let (rest, be) = if bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], false)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], true)
    } else {
        return None;
    };
    if !rest.len().is_multiple_of(2) {
        return None;
    }
    let (pairs, _) = rest.as_chunks::<2>();
    let units = pairs.iter().map(|pair| {
        if be {
            u16::from_be_bytes(*pair)
        } else {
            u16::from_le_bytes(*pair)
        }
    });
    // Measure first, then fill one buffer of that size: growing a String while decoding
    // reallocates and frees the old buffers without zeroizing them.
    let mut len = 0;
    for unit in char::decode_utf16(units.clone()) {
        len += unit.ok()?.len_utf8();
    }
    let mut text = Zeroizing::new(String::with_capacity(len));
    for unit in char::decode_utf16(units) {
        text.push(unit.ok()?);
    }
    Some(text)
}

/// Save panel name: the stem of `name` (its own extension dropped: `people.csv`,
/// `a.b.CSV`) and exactly one `extension`, lower case (`people.parquet`). A name without an
/// extension keeps all of it; so does a last part that is no extension (`Report v1.2`).
/// Empty `extension`: the stem alone.
pub fn save_name(name: &str, extension: &str) -> String {
    let name = name.trim();
    let stem = name_stem(name).trim_end_matches('.');
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let stem = if stem.is_empty() { "clipboard" } else { stem };
    if extension.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{extension}")
    }
}

/// `name` without its file extension: the part after the last dot when it looks like one
/// (1 to 10 letters or digits, at least one letter) and something is left before it.
fn name_stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && (1..=10).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric())
                && extension.chars().any(|c| c.is_ascii_alphabetic()) =>
        {
            stem
        }
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::{
        Classified, MAX_FILE_BYTES, MAX_IMAGE_BYTES, classify, decode, from_image_data, from_text,
        load, load_dropped, save_name,
    };

    /// The 8-byte PNG signature and an IHDR chunk start: enough for the type sniffer.
    const PNG_START: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";

    #[test]
    fn a_dropped_picture_file_is_an_image() {
        let path = std::env::temp_dir().join(format!("copycraft-drop-{}.png", std::process::id()));
        std::fs::write(&path, PNG_START).unwrap();
        let opened = load_dropped(&path);
        let _ = std::fs::remove_file(&path);
        let image = opened.image.expect("image");
        assert_eq!(image.bytes.with(<[u8]>::len), PNG_START.len());
        assert!(opened.text.is_none());
        assert!(opened.note.is_none());
    }

    #[test]
    fn a_dropped_text_file_is_still_text() {
        let path = std::env::temp_dir().join(format!("copycraft-drop-{}.txt", std::process::id()));
        std::fs::write(&path, "plain\n").unwrap();
        let opened = load_dropped(&path);
        let _ = std::fs::remove_file(&path);
        assert!(opened.image.is_none());
        assert_eq!(opened.text.unwrap().as_str(), "plain\n");
    }

    #[test]
    fn a_dropped_binary_file_is_tried_as_a_picture() {
        let path = std::env::temp_dir().join(format!("copycraft-drop-{}.raw", std::process::id()));
        std::fs::write(&path, [0, 1, 2, 3]).unwrap();
        let opened = load_dropped(&path);
        let _ = std::fs::remove_file(&path);
        assert!(opened.image.is_some());
        assert!(opened.note.is_none());
    }

    #[test]
    fn dropped_image_data_has_the_image_limit() {
        let opened = from_image_data("Dropped image", Zeroizing::new(PNG_START.to_vec()));
        assert_eq!(opened.name, "Dropped image");
        assert!(opened.image.is_some());

        let limit = usize::try_from(MAX_IMAGE_BYTES).unwrap();
        let over = from_image_data("Dropped image", Zeroizing::new(vec![0; limit + 1]));
        assert!(over.image.is_none());
        assert_eq!(over.note.as_deref(), Some("Image is larger than 32 MB"));
        let empty = from_image_data("Dropped image", Zeroizing::new(Vec::new()));
        assert_eq!(empty.note.as_deref(), Some("Can't read this image"));
    }

    #[test]
    fn reads_utf8_and_a_bom() {
        match classify(b"Naam\tSalaris\nJan\t1\n") {
            Classified::Text(text) => assert!(text.contains("Jan")),
            other => panic!(
                "expected text, got {}",
                matches!(other, Classified::Text(_))
            ),
        }
        let text = decode(b"\xEF\xBB\xBFhello").unwrap();
        assert_eq!(text.as_str(), "hello");
    }

    #[test]
    fn reads_utf16_and_rejects_binary() {
        let text = decode(&[0xFF, 0xFE, b'A', 0, b'B', 0]).unwrap();
        assert_eq!(text.as_str(), "AB");
        assert!(decode(b"\x89PNG\x00\x00").is_none());
        assert!(matches!(classify(&[0, 1, 2, 3]), Classified::NotText));
    }

    #[test]
    fn decoded_text_fills_one_exact_buffer() {
        // An exact-size buffer was never grown, so no reallocated copy was freed unzeroized.
        let text = decode("héllo wörld".as_bytes()).unwrap();
        assert_eq!(text.as_str(), "héllo wörld");
        assert_eq!(text.capacity(), text.len());
        // UTF-16 BE with a BMP char (3 UTF-8 bytes) and a surrogate pair (4 UTF-8 bytes).
        let utf16: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("a€😀".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        let text = decode(&utf16).unwrap();
        assert_eq!(text.as_str(), "a€😀");
        assert_eq!(text.capacity(), text.len());
    }

    #[test]
    fn unpaired_utf16_surrogate_is_not_text() {
        assert!(decode(&[0xFF, 0xFE, 0x00, 0xD8]).is_none());
    }

    #[test]
    fn load_reads_a_text_file() {
        let path = std::env::temp_dir().join(format!("copycraft-open-{}.tsv", std::process::id()));
        std::fs::write(&path, "hello file\n").unwrap();
        let opened = load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(opened.name, path.file_name().unwrap().to_str().unwrap());
        assert_eq!(opened.text.unwrap().as_str(), "hello file\n");
        assert!(opened.note.is_none());
    }

    #[test]
    fn dropped_text_is_shown_up_to_the_file_limit() {
        let opened = from_text("Dropped text", Zeroizing::new("{\"a\": 1}".to_string()));
        assert_eq!(opened.name, "Dropped text");
        assert_eq!(opened.text.unwrap().as_str(), "{\"a\": 1}");
        assert!(opened.note.is_none());

        let limit = usize::try_from(MAX_FILE_BYTES).unwrap();
        let at_limit = from_text("Dropped text", Zeroizing::new("a".repeat(limit)));
        assert!(at_limit.text.is_some());
        let over = from_text("Dropped text", Zeroizing::new("a".repeat(limit + 1)));
        assert!(over.text.is_none());
        assert_eq!(over.note.as_deref(), Some("Text is larger than 8 MB"));
    }

    #[test]
    fn blank_dropped_text_gets_a_note() {
        let opened = from_text("Dropped text", Zeroizing::new(" \n\t".to_string()));
        assert!(opened.text.is_none());
        assert_eq!(opened.note.as_deref(), Some("The dropped text is empty"));
    }

    #[test]
    fn save_name_keeps_the_stem() {
        assert_eq!(save_name("people.tsv", "txt"), "people.txt");
        assert_eq!(save_name("people.tsv", "tsv"), "people.tsv");
        let anker = "Thuis_Energiegegevens_2_Oct_2025_to_2_Oct_2026";
        assert_eq!(
            save_name(&format!("{anker}.csv"), "parquet"),
            format!("{anker}.parquet")
        );
        // No extension; a dotted name loses only its last extension.
        assert_eq!(save_name("people", "csv"), "people.csv");
        assert_eq!(save_name("a.b.csv", "parquet"), "a.b.parquet");
        // Already the target extension, in any case: still one.
        assert_eq!(save_name("t.parquet", "parquet"), "t.parquet");
        assert_eq!(save_name("t.PARQUET", "parquet"), "t.parquet");
        assert_eq!(save_name("T.CSV", "parquet"), "T.parquet");
        assert_eq!(save_name("t.csv", "CSV"), "t.csv");
        assert_eq!(save_name("t.csv", ".parquet"), "t.parquet");
        // Saving twice through the helper adds nothing.
        let once = save_name("t.csv", "parquet");
        assert_eq!(save_name(&once, "parquet"), once);
        // Not extensions: a version number, a dot file. A trailing dot goes.
        assert_eq!(save_name("Report v1.2", "txt"), "Report v1.2.txt");
        assert_eq!(save_name("notes.", "txt"), "notes.txt");
        assert_eq!(save_name(".csv", "parquet"), ".csv.parquet");
        assert_eq!(save_name("", "png"), "clipboard.png");
        assert_eq!(save_name("people.tsv", ""), "people");
    }
}
