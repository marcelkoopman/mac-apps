use std::path::Path;

use zeroize::{Zeroize, Zeroizing};

/// Pasted files larger than this stay off the card.
pub const MAX_FILE_BYTES: u64 = 8_000_000;

const NOT_TEXT: &str = "This file is not text";
const TOO_LARGE: &str = "File is larger than 8 MB";
const UNREADABLE: &str = "Can't read this file";

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
    let mut bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return noted(name, UNREADABLE),
    };
    let opened = match classify(&bytes) {
        Classified::Text(text) => OpenedFile {
            name,
            text: Some(Zeroizing::new(text)),
            note: None,
        },
        Classified::NotText => noted(name, NOT_TEXT),
        Classified::TooLarge => noted(name, TOO_LARGE),
    };
    bytes.zeroize();
    opened
}

enum Classified {
    Text(String),
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

fn decode(bytes: &[u8]) -> Option<String> {
    if let Some(text) = decode_utf16(bytes) {
        return Some(text);
    }
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes.to_vec()).ok()?;
    Some(text.trim_start_matches('\u{feff}').to_string())
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
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
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| {
            if be {
                u16::from_be_bytes(*pair)
            } else {
                u16::from_le_bytes(*pair)
            }
        })
        .collect();
    String::from_utf16(&units).ok()
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
    use super::{Classified, classify, decode, load, save_name};

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
        assert_eq!(text, "hello");
    }

    #[test]
    fn reads_utf16_and_rejects_binary() {
        let text = decode(&[0xFF, 0xFE, b'A', 0, b'B', 0]).unwrap();
        assert_eq!(text, "AB");
        assert!(decode(b"\x89PNG\x00\x00").is_none());
        assert!(matches!(classify(&[0, 1, 2, 3]), Classified::NotText));
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
    fn save_name_keeps_the_stem() {
        assert_eq!(save_name("people.tsv", "txt"), "people.txt");
        assert_eq!(save_name("people.tsv", "tsv"), "people.tsv");
    }
}
