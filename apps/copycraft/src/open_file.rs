use std::path::Path;

use zeroize::Zeroizing;

/// Pasted files larger than this stay off the card.
pub const MAX_FILE_BYTES: u64 = 8_000_000;

const NOT_TEXT: &str = "This file is not text";
const TOO_LARGE: &str = "File is larger than 8 MB";
const UNREADABLE: &str = "Can't read this file";
const TEXT_TOO_LARGE: &str = "Text is larger than 8 MB";
const EMPTY_TEXT: &str = "The dropped text is empty";

/// A file the card is showing instead of the clipboard.
#[derive(Clone)]
pub struct OpenedFile {
    pub name: String,
    pub text: Option<Zeroizing<String>>,
    pub note: Option<String>,
}

pub fn load(path: &Path) -> OpenedFile {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
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

fn noted(name: String, note: &str) -> OpenedFile {
    OpenedFile {
        name,
        text: None,
        note: Some(note.to_string()),
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

/// Save panel name. The extension matches the card view, the stem the chosen file.
pub fn save_name(name: &str, extension: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    if extension.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{extension}")
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroizing;

    use super::{Classified, MAX_FILE_BYTES, classify, decode, from_text, load, save_name};

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
    }
}
