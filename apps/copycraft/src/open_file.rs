use std::path::Path;

use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;
use crate::commands::{ImageFacts, ImageScan};

/// Pasted files larger than this stay off the card.
pub const MAX_FILE_BYTES: u64 = 8_000_000;
/// Dropped pictures larger than this stay off the card. Higher than for text: a photo is
/// easily over 8 MB, and a picture dragged from a web page often comes as uncompressed TIFF.
pub const MAX_IMAGE_BYTES: u64 = 32_000_000;

fn not_text() -> &'static str {
    crate::locale::t("file_not_text")
}
fn too_large() -> &'static str {
    crate::locale::t("file_too_large")
}
pub fn unreadable() -> &'static str {
    crate::locale::t("file_unreadable")
}
fn text_too_large() -> &'static str {
    crate::locale::t("text_too_large")
}
fn empty_text() -> &'static str {
    crate::locale::t("dropped_text_empty")
}
fn image_too_large() -> &'static str {
    crate::locale::t("image_too_large")
}
/// For a dropped picture the system cannot draw.
pub fn unreadable_image() -> &'static str {
    crate::locale::t("image_unreadable")
}

/// A file the card is showing instead of the clipboard.
#[derive(Clone)]
pub struct OpenedFile {
    pub name: String,
    pub text: Option<Zeroizing<String>>,
    pub note: Option<String>,
    /// A picture from a drop or the open panel, shown like a copied one.
    pub image: Option<OpenedImage>,
    /// A PDF: its text (or, without a text layer, the picture of its first page) is above.
    pub pdf: Option<crate::pdf::PdfFacts>,
    /// A zip archive: the card's text is the list of names in it ([`crate::zip_archive`]).
    pub zip: Option<ZipFile>,
}

/// A dropped or chosen zip: where it is (to extract it later) and what the card says about
/// it. Only counts; the names are the card's text.
#[derive(Clone)]
pub struct ZipFile {
    pub path: std::path::PathBuf,
    pub facts: crate::zip_archive::ZipFacts,
    /// Keeps a promised file (dragged from Mail, …) on disk while the card shows the zip, so
    /// it can still be extracted; it goes when the card moves on.
    pub keep: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

/// A picture from a drop or the open panel: its encoded bytes (shared with its history entry
/// when a drop recorded it), and what the image card shows about it once the app has looked.
#[derive(Clone)]
pub struct OpenedImage {
    pub bytes: SecretBytes,
    pub facts: Option<ImageFacts>,
    pub scan: Option<ImageScan>,
    /// The PDF this is the first page of. The scan reads all its pages: text recognition for a
    /// scan, the text layer's own text for a PDF that has one.
    pub pdf_source: Option<SecretBytes>,
    /// The text layer of that PDF, when it has one (the *Tekst* view and the AI input).
    pub pdf_text: Option<Zeroizing<String>>,
    /// The page of that PDF the card shows (0-based) and the one asked for, which shows as soon
    /// as it is drawn. The first page is `bytes`.
    pub page: usize,
    pub want: usize,
    /// Pages drawn so far, in memory only ([`crate::pdf::PAGE_CACHE`] at most; wiped when the
    /// file leaves the card).
    pub pages: std::collections::HashMap<usize, SecretBytes>,
    /// The zoom (percent, [`crate::pdf::ZOOM_FIT`] = fit) the card shows and the one asked for,
    /// which shows once the page is drawn at it. Another file starts at fit.
    pub zoom: u32,
    pub want_zoom: u32,
    /// The page `.0` drawn again for zoom `.1` (above fit), in memory only like `pages`.
    pub zoomed: Option<(usize, u32, SecretBytes)>,
}

impl OpenedImage {
    /// The page drawn again for the zoom shown, when there is one (above fit).
    pub fn zoomed_bytes(&self) -> Option<SecretBytes> {
        self.zoomed
            .as_ref()
            .filter(|(page, zoom, _)| {
                *page == self.page && *zoom == self.zoom && self.zoom > crate::pdf::ZOOM_FIT
            })
            .map(|(_, _, bytes)| bytes.clone())
    }

    /// The zoom the card shows: the one asked for once its picture is there (above fit it is
    /// drawn again for it), else fit.
    pub fn shown_zoom(&self) -> u32 {
        if self.zoom > crate::pdf::ZOOM_FIT && self.zoomed_bytes().is_none() {
            crate::pdf::ZOOM_FIT
        } else {
            self.zoom
        }
    }

    /// The picture the card draws: the page shown, else the first.
    pub fn shown_bytes(&self) -> SecretBytes {
        self.pages
            .get(&self.page)
            .filter(|_| self.page > 0)
            .unwrap_or(&self.bytes)
            .clone()
    }
}

/// A dropped or chosen file: a picture when its contents are one (see [`is_image_file`]), else
/// read as text like [`load`]. A binary file that is not text either is tried as a picture too:
/// the system draws formats the sniffer does not know (camera RAW, …). Whether it can is up to
/// the card ([`unreadable_image`]). Kies bestand uses this too; the app then records it in
/// history the same way as a drop.
pub fn load_dropped(path: &Path) -> OpenedFile {
    if crate::zip_archive::is_zip_file(path) {
        return load_zip(path);
    }
    if is_pdf_file(path) {
        return load_pdf(path, true);
    }
    // An SVG is text (the SVG card draws it and has its code a toggle away), never a picture.
    if !is_svg_name(path) && is_image_file(path) {
        return load_image(path);
    }
    let opened = load_file(path, true);
    if opened.note.as_deref() == Some(not_text()) {
        return load_image(path);
    }
    opened
}

/// A zip (`.zip`): the names in it as the card's text, read from its central directory only
/// (nothing is unpacked). A damaged, encrypted-only or empty zip is a note.
fn load_zip(path: &Path) -> OpenedFile {
    let name = file_name(path);
    let Ok(mut file) = std::fs::File::open(path) else {
        return noted(name, unreadable());
    };
    let Ok(len) = file.metadata().map(|meta| meta.len()) else {
        return noted(name, unreadable());
    };
    match crate::zip_archive::list(&mut file, len) {
        Ok(listing) => OpenedFile {
            name,
            text: Some(listing.text(|more| crate::locale::tf("zip_more", &[&more]))),
            note: None,
            image: None,
            pdf: None,
            zip: Some(ZipFile {
                path: path.to_path_buf(),
                facts: listing.facts(),
                keep: None,
            }),
        },
        Err(error) => noted(name, error.message()),
    }
}

fn load_image(path: &Path) -> OpenedFile {
    let name = file_name(path);
    let Ok(meta) = std::fs::metadata(path) else {
        return noted(name, unreadable());
    };
    if !meta.is_file() {
        return noted(name, unreadable());
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return noted(name, image_too_large());
    }
    match std::fs::read(path) {
        Ok(bytes) => from_image_bytes(name, bytes),
        Err(_) => noted(name, unreadable()),
    }
}

/// Image data dropped on the card, shown like a file called `name`.
pub fn from_image_data(name: &str, mut bytes: Zeroizing<Vec<u8>>) -> OpenedFile {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return noted(name.to_string(), image_too_large());
    }
    // Moved, not copied, into the zeroizing history buffer.
    from_image_bytes(name.to_string(), std::mem::take(&mut *bytes))
}

fn from_image_bytes(name: String, mut bytes: Vec<u8>) -> OpenedFile {
    // SVG markup handed over as image data (a drag from Finder or a browser): the SVG card.
    if let Some(text) = svg_text(&bytes) {
        zeroize::Zeroize::zeroize(&mut bytes);
        return from_text(&name, text);
    }
    match SecretBytes::new(bytes) {
        Some(bytes) => OpenedFile {
            name,
            text: None,
            note: None,
            image: Some(OpenedImage {
                bytes,
                facts: None,
                scan: None,
                pdf_source: None,
                pdf_text: None,
                page: 0,
                want: 0,
                pages: Default::default(),
                zoom: crate::pdf::ZOOM_FIT,
                want_zoom: crate::pdf::ZOOM_FIT,
                zoomed: None,
            }),
            pdf: None,
            zip: None,
        },
        None => noted(name, unreadable_image()),
    }
}

fn is_svg_name(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// The bytes as text when they are an SVG document (UTF-8 or UTF-16, root `<svg>`).
fn svg_text(bytes: &[u8]) -> Option<Zeroizing<String>> {
    let text = decode(bytes)?;
    crate::format::svg::is_svg(&text).then_some(text)
}

/// The file starts like a picture (PNG, JPEG, HEIC, GIF, WebP, TIFF, BMP, …).
fn is_image_file(path: &Path) -> bool {
    matches!(
        infer::get_from_path(path),
        Ok(Some(kind)) if kind.matcher_type() == infer::MatcherType::Image
    )
}

/// The file starts like a PDF.
fn is_pdf_file(path: &Path) -> bool {
    matches!(
        infer::get_from_path(path),
        Ok(Some(kind)) if kind.mime_type() == "application/pdf"
    )
}

/// A PDF: its text layer as the card's text. Without one, `ocr` (a drop or Kies bestand) makes
/// it a picture of the first page whose text comes from text recognition after the card is
/// up; a copied file has no such card and says there is no text. The file is read into
/// zeroizing memory and nothing is written anywhere.
#[cfg(target_os = "macos")]
fn load_pdf(path: &Path, ocr: bool) -> OpenedFile {
    use crate::macos_pdf::Read;
    let name = file_name(path);
    let Ok(meta) = std::fs::metadata(path) else {
        return noted(name, unreadable());
    };
    if !meta.is_file() {
        return noted(name, unreadable());
    }
    if meta.len() > crate::pdf::MAX_PDF_BYTES {
        return noted(name, crate::locale::t("pdf_too_large"));
    }
    let Ok(mut bytes) = std::fs::read(path).map(Zeroizing::new) else {
        return noted(name, unreadable());
    };
    if !crate::pdf::is_pdf(&bytes) {
        return noted(name, unreadable());
    }
    let Some(source) = SecretBytes::new(std::mem::take(&mut *bytes)) else {
        return noted(name, unreadable());
    };
    drop(bytes);
    match crate::macos_pdf::read(&source) {
        Read::Unreadable => noted(name, unreadable()),
        Read::Locked => noted(name, crate::locale::t("pdf_locked")),
        Read::Pages {
            pages,
            read,
            text: Some(text),
        } => {
            // A drop or Kies bestand shows the page (the PDF itself) with its text behind the
            // *Tekst* button; a copy has no such card and shows the text.
            if ocr
                && let Some((mut png, _)) = crate::macos_pdf::render_page(&source, 0, true)
                && let Some(picture) = SecretBytes::new(std::mem::take(&mut *png))
            {
                let scan = ImageScan {
                    info: String::new(),
                    data_url: None,
                    ocr: Some(text.to_string()),
                    qr: None,
                };
                return OpenedFile {
                    name,
                    text: None,
                    note: None,
                    image: Some(OpenedImage {
                        bytes: picture,
                        facts: None,
                        scan: Some(scan),
                        pdf_source: Some(source),
                        pdf_text: Some(text),
                        page: 0,
                        want: 0,
                        pages: Default::default(),
                        zoom: crate::pdf::ZOOM_FIT,
                        want_zoom: crate::pdf::ZOOM_FIT,
                        zoomed: None,
                    }),
                    pdf: Some(crate::pdf::PdfFacts {
                        pages,
                        read,
                        scanned: false,
                    }),
                    zip: None,
                };
            }
            crate::format::hint_by_name(&name, &text);
            OpenedFile {
                name,
                text: Some(text),
                note: None,
                image: None,
                pdf: Some(crate::pdf::PdfFacts {
                    pages,
                    read,
                    scanned: false,
                }),
                zip: None,
            }
        }
        Read::Pages {
            pages, text: None, ..
        } => {
            if !ocr {
                return noted(name, crate::locale::t("pdf_no_text"));
            }
            let Some((mut png, _)) = crate::macos_pdf::render_page(&source, 0, true) else {
                return noted(name, unreadable());
            };
            let Some(picture) = SecretBytes::new(std::mem::take(&mut *png)) else {
                return noted(name, unreadable());
            };
            OpenedFile {
                name,
                text: None,
                note: None,
                image: Some(OpenedImage {
                    bytes: picture,
                    facts: None,
                    scan: None,
                    pdf_source: Some(source),
                    pdf_text: None,
                    page: 0,
                    want: 0,
                    pages: Default::default(),
                    zoom: crate::pdf::ZOOM_FIT,
                    want_zoom: crate::pdf::ZOOM_FIT,
                    zoomed: None,
                }),
                pdf: Some(crate::pdf::PdfFacts {
                    pages,
                    read: pages.min(crate::pdf::OCR_MAX_PAGES),
                    scanned: true,
                }),
                zip: None,
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn load_pdf(path: &Path, _ocr: bool) -> OpenedFile {
    noted(file_name(path), not_text())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

pub fn load(path: &Path) -> OpenedFile {
    load_file(path, false)
}

/// [`load`]; with `preview` (a drop or Kies bestand) a Word document also gets the picture of
/// its first page, when Quick Look makes one.
fn load_file(path: &Path, preview: bool) -> OpenedFile {
    if is_pdf_file(path) {
        return load_pdf(path, false);
    }
    let name = file_name(path);
    let Ok(meta) = std::fs::metadata(path) else {
        return noted(name, unreadable());
    };
    if !meta.is_file() {
        return noted(name, unreadable());
    }
    if let Some(opened) = load_docx(path, &name, meta.len(), preview) {
        return opened;
    }
    if meta.len() > MAX_FILE_BYTES {
        return noted(name, too_large());
    }
    // Zeroizing from the first byte, so the file contents are overwritten on every path out.
    let bytes = match std::fs::read(path) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(_) => return noted(name, unreadable()),
    };
    match classify(&bytes) {
        Classified::Text(text) => {
            crate::format::hint_by_name(&name, &text);
            crate::format::hint_by_path(path, &text);
            OpenedFile {
                name,
                text: Some(text),
                note: None,
                image: None,
                pdf: None,
                zip: None,
            }
        }
        Classified::NotText => noted(name, not_text()),
        Classified::TooLarge => noted(name, too_large()),
    }
}

/// A Word document (.docx): its text as the card's text. `None` when the file is not one (no
/// zip, or a zip without `word/document.xml`), so the caller goes on with it as any other file.
/// Read into zeroizing memory and unpacked in memory only; nothing is written anywhere.
fn load_docx(path: &Path, name: &str, len: u64, preview: bool) -> Option<OpenedFile> {
    use crate::docx::{DocxError, MAX_DOCX_BYTES};
    let named = is_docx_name(path);
    // A zip, or (for a .docx name) an encrypted Office container: both start with their magic.
    let mut magic = [0u8; 8];
    std::io::Read::read_exact(&mut std::fs::File::open(path).ok()?, &mut magic).ok()?;
    let zip = crate::docx::has_zip_magic(&magic);
    if !zip && !(named && crate::docx::has_ole_magic(&magic)) {
        return None;
    }
    if len > MAX_DOCX_BYTES {
        // Only a file named .docx gets the Word note; any other big zip stays "not text".
        return named.then(|| noted(name.to_string(), crate::locale::t("docx_too_large")));
    }
    let bytes = crate::docx::read_file(path, MAX_DOCX_BYTES)?;
    if zip && !crate::docx::is_docx(&bytes) {
        return None;
    }
    let name = name.to_string();
    Some(match crate::docx::extract(&bytes) {
        Ok(text) => {
            // The picture of the first page (Quick Look), with the text behind the *Tekst*
            // button. Without one (it failed or timed out): the text card.
            if preview && let Some(picture) = first_page_picture(path) {
                return Some(document_with_picture(name, text, picture));
            }
            OpenedFile {
                name,
                text: Some(text),
                note: None,
                image: None,
                pdf: None,
                zip: None,
            }
        }
        Err(DocxError::NotDocx) => return None,
        Err(DocxError::Locked) => noted(name, crate::locale::t("docx_locked")),
        Err(DocxError::Bomb) => noted(name, crate::locale::t("docx_bomb")),
        Err(DocxError::TooLarge) => noted(name, text_too_large()),
        Err(DocxError::NoText) => noted(name, crate::locale::t("docx_no_text")),
        Err(DocxError::Unreadable) => noted(name, unreadable()),
    })
}

/// A document's text with the picture of its first page: an image card whose *Tekst* view is
/// the text (like a PDF page and its text layer; `pdf_text` without `pdf_source` marks it).
fn document_with_picture(
    name: String,
    text: Zeroizing<String>,
    picture: SecretBytes,
) -> OpenedFile {
    let scan = ImageScan {
        info: String::new(),
        data_url: None,
        ocr: Some(text.to_string()),
        qr: None,
    };
    OpenedFile {
        name,
        text: None,
        note: None,
        image: Some(OpenedImage {
            bytes: picture,
            facts: None,
            scan: Some(scan),
            pdf_source: None,
            pdf_text: Some(text),
            page: 0,
            want: 0,
            pages: Default::default(),
            zoom: crate::pdf::ZOOM_FIT,
            want_zoom: crate::pdf::ZOOM_FIT,
            zoomed: None,
        }),
        pdf: None,
        zip: None,
    }
}

/// Width and height (points) and scale of the Quick Look page: 1280 × 1800 pixels at most.
#[cfg(target_os = "macos")]
const PREVIEW_POINTS: (f64, f64) = (640.0, 900.0);
#[cfg(target_os = "macos")]
const PREVIEW_SCALE: f64 = 2.0;
/// Quick Look gets this long; the loader thread waits, never the main thread.
#[cfg(target_os = "macos")]
const PREVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The first page of the document as PNG, drawn by Quick Look in memory (nothing is written
/// to disk by the app). `None` when it cannot or takes too long.
#[cfg(target_os = "macos")]
fn first_page_picture(path: &Path) -> Option<SecretBytes> {
    let mut png = Zeroizing::new(mac_ui::quicklook::first_page_png(
        path,
        PREVIEW_POINTS.0,
        PREVIEW_POINTS.1,
        PREVIEW_SCALE,
        PREVIEW_TIMEOUT,
    )?);
    SecretBytes::new(std::mem::take(&mut *png))
}

#[cfg(not(target_os = "macos"))]
fn first_page_picture(_path: &Path) -> Option<SecretBytes> {
    None
}

fn is_docx_name(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
}

/// Dropped text, shown on the card like a file called `name`. It has the file size limit.
pub fn from_text(name: &str, text: Zeroizing<String>) -> OpenedFile {
    let name = name.to_string();
    if text.len() as u64 > MAX_FILE_BYTES {
        return noted(name, text_too_large());
    }
    if text.trim().is_empty() {
        return noted(name, empty_text());
    }
    crate::format::hint_by_name(&name, &text);
    OpenedFile {
        name,
        text: Some(text),
        note: None,
        image: None,
        pdf: None,
        zip: None,
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
        pdf: None,
        zip: None,
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
/// `a.b.CSV`) and exactly one `extension`, lower case (`people.tsv`). A name without an
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
    fn a_dropped_svg_is_text_for_the_svg_card() {
        let svg = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\"/></svg>\n";
        // As image data (Finder or a browser hands the markup over as a picture).
        let opened = from_image_data("logo.svg", Zeroizing::new(svg.as_bytes().to_vec()));
        assert!(opened.image.is_none());
        assert_eq!(opened.text.as_deref().map(String::as_str), Some(svg));
        assert_eq!(crate::format::detect(svg), crate::format::FormatKind::Svg);
        // As a file, also when its name is the only hint.
        let path = std::env::temp_dir().join(format!("copycraft-drop-{}.svg", std::process::id()));
        std::fs::write(&path, svg).unwrap();
        let opened = load_dropped(&path);
        let _ = std::fs::remove_file(&path);
        assert!(opened.image.is_none());
        assert!(opened.text.is_some());
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

    fn temp_file(tag: &str, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("copycraft-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn load_reads_a_docx_as_text() {
        let zipped = crate::docx::tests::docx("<w:p><w:r><w:t>Hallo docx</w:t></w:r></w:p>", true);
        let path = temp_file("docx", "brief.docx", &zipped);
        let opened = load(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(opened.name, "brief.docx");
        assert_eq!(opened.text.unwrap().as_str(), "Hallo docx");
        assert!(opened.note.is_none() && opened.image.is_none());
    }

    #[test]
    fn a_docx_without_text_or_a_locked_one_gets_a_note() {
        let empty = crate::docx::tests::docx("<w:p/>", false);
        let path = temp_file("docx-empty", "leeg.docx", &empty);
        let opened = load(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert!(opened.text.is_none());
        assert_eq!(
            opened.note.as_deref(),
            Some("This Word document has no text")
        );

        let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        ole.extend_from_slice(&[0; 64]);
        let path = temp_file("docx-locked", "geheim.docx", &ole);
        let opened = load(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert!(opened.text.is_none());
        assert!(opened.note.is_some());
    }

    #[test]
    fn a_document_picture_keeps_its_text_behind_the_tekst_button() {
        let picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]).unwrap();
        let opened = super::document_with_picture(
            "brief.docx".to_string(),
            Zeroizing::new("Hallo".to_string()),
            picture,
        );
        assert!(opened.text.is_none() && opened.note.is_none() && opened.pdf.is_none());
        let image = opened.image.unwrap();
        assert!(image.pdf_source.is_none());
        assert_eq!(image.pdf_text.as_deref().map(String::as_str), Some("Hallo"));
        assert_eq!(image.scan.unwrap().ocr.as_deref(), Some("Hallo"));
    }

    #[test]
    fn a_dropped_docx_without_a_preview_is_the_text_card() {
        // Linux has no Quick Look; on a Mac the preview may or may not come: the text is there
        // either way (as the card's text or behind the picture).
        let zipped = crate::docx::tests::docx("<w:p><w:r><w:t>Hallo</w:t></w:r></w:p>", true);
        let path = temp_file("docx-drop", "brief.docx", &zipped);
        let opened = load_dropped(&path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let text = opened
            .text
            .as_ref()
            .map(|text| text.to_string())
            .or_else(|| {
                opened
                    .image
                    .as_ref()
                    .and_then(|image| image.pdf_text.as_ref().map(|text| text.to_string()))
            });
        assert_eq!(text.as_deref(), Some("Hallo"));
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
            save_name(&format!("{anker}.csv"), "tsv"),
            format!("{anker}.tsv")
        );
        // No extension; a dotted name loses only its last extension.
        assert_eq!(save_name("people", "csv"), "people.csv");
        assert_eq!(save_name("a.b.csv", "tsv"), "a.b.tsv");
        // Already the target extension, in any case: still one.
        assert_eq!(save_name("t.tsv", "tsv"), "t.tsv");
        assert_eq!(save_name("t.TSV", "tsv"), "t.tsv");
        assert_eq!(save_name("T.CSV", "tsv"), "T.tsv");
        assert_eq!(save_name("t.csv", "CSV"), "t.csv");
        assert_eq!(save_name("t.csv", ".tsv"), "t.tsv");
        // Saving twice through the helper adds nothing.
        let once = save_name("t.csv", "tsv");
        assert_eq!(save_name(&once, "tsv"), once);
        // Not extensions: a version number, a dot file. A trailing dot goes.
        assert_eq!(save_name("Report v1.2", "txt"), "Report v1.2.txt");
        assert_eq!(save_name("notes.", "txt"), "notes.txt");
        assert_eq!(save_name(".csv", "tsv"), ".csv.tsv");
        assert_eq!(save_name("", "png"), "clipboard.png");
        assert_eq!(save_name("people.tsv", ""), "people");
    }
}
