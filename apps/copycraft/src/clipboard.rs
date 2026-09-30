use std::sync::{Arc, Mutex, MutexGuard};

use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use zeroize::{Zeroize, Zeroizing};

use crate::format;

const MAX_HISTORY: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Clipboard pixel size. `width` and `height` can be a smaller preview.
    pub full_width: usize,
    pub full_height: usize,
}

impl ClipboardImage {
    pub fn new(width: usize, height: usize, rgba: Vec<u8>) -> Option<Self> {
        let pixels = width.checked_mul(height)?.checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != pixels {
            return None;
        }
        Some(Self {
            width,
            height,
            rgba,
            full_width: width,
            full_height: height,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn from_arboard(data: arboard::ImageData<'_>) -> Option<Self> {
        Self::new(data.width, data.height, data.bytes.into_owned())
    }

    pub fn png_bytes(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        PngEncoder::new(&mut buf)
            .write_image(
                &self.rgba,
                self.width as u32,
                self.height as u32,
                ExtendedColorType::Rgba8,
            )
            .map_err(|e| e.to_string())?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardView {
    Empty,
    NoText,
    /// Owned clipboard text. Dropping the view overwrites that allocation.
    Text(Zeroizing<String>),
    /// An image is on the clipboard. Pixels are not loaded here.
    Image,
}

impl ClipboardView {
    pub fn from_os() -> Self {
        #[cfg(target_os = "macos")]
        {
            // The menu refreshes several times a second. Reading pixels here
            // decodes a full image every time and stalls the process.
            crate::macos_pasteboard::current_view()
        }
        #[cfg(not(target_os = "macos"))]
        {
            from_os_fallback()
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Empty | Self::NoText | Self::Image => None,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_previewable(&self) -> bool {
        matches!(self, Self::Text(_) | Self::Image)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn type_mark(&self) -> Option<&'static str> {
        match self {
            Self::Text(text) => Some(menu_mark(text.as_str())),
            Self::Image => Some(format::FormatKind::Image.menu_symbol()),
            Self::Empty | Self::NoText => None,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn label(&self) -> String {
        match self {
            Self::Empty => "(clipboard is empty)".to_string(),
            Self::NoText => "(clipboard has no text)".to_string(),
            Self::Text(text) => one_line(text.as_str()),
            Self::Image => format::FormatKind::Image.menu_symbol().to_string(),
        }
    }
}

/// Image bytes shared by history and the current-item handle.
/// `zeroize` overwrites the one allocation every clone points at.
#[derive(Clone)]
pub struct SecretBytes {
    inner: Arc<Mutex<Zeroizing<Vec<u8>>>>,
}

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        Some(Self {
            inner: Arc::new(Mutex::new(Zeroizing::new(bytes))),
        })
    }

    pub fn zeroize(&self) {
        self.lock().zeroize();
    }

    pub fn with<R>(&self, read: impl FnOnce(&[u8]) -> R) -> R {
        read(self.lock().as_slice())
    }

    pub fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    fn lock(&self) -> MutexGuard<'_, Zeroizing<Vec<u8>>> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    #[cfg(test)]
    fn prefix_is_zero(&self, len: usize) -> bool {
        let guard = self.lock();
        if !guard.is_empty() || guard.capacity() < len {
            return false;
        }
        let ptr = guard.as_ptr();
        // SAFETY: zeroize keeps this allocation and writes zeros across its
        // capacity. `len` is inside that capacity, and `guard` still owns it.
        let wiped = unsafe { std::slice::from_raw_parts(ptr, len) };
        wiped.iter().all(|byte| *byte == 0)
    }
}

impl PartialEq for SecretBytes {
    fn eq(&self, other: &Self) -> bool {
        if self.same_allocation(other) {
            return true;
        }
        let left = self.lock();
        let right = other.lock();
        left.as_slice() == right.as_slice()
    }
}

impl Eq for SecretBytes {}

#[derive(Clone)]
enum HistoryEntry {
    Text(Zeroizing<String>),
    Image(SecretBytes),
}

impl Drop for HistoryEntry {
    fn drop(&mut self) {
        match self {
            Self::Text(text) => text.zeroize(),
            Self::Image(bytes) => bytes.zeroize(),
        }
    }
}

impl Drop for ClipboardImage {
    fn drop(&mut self) {
        self.rgba.zeroize();
    }
}

#[derive(Default, Clone)]
pub struct ClipboardHistory {
    entries: Vec<HistoryEntry>,
}

impl ClipboardHistory {
    pub fn record(&mut self, mut text: String) {
        if text.trim().is_empty() {
            text.zeroize();
            return;
        }
        self.entries.retain(|existing| match existing {
            HistoryEntry::Text(existing) => existing.as_str() != text,
            HistoryEntry::Image(_) => true,
        });
        self.entries
            .insert(0, HistoryEntry::Text(Zeroizing::new(text)));
        self.entries.truncate(MAX_HISTORY);
    }

    pub fn record_image(&mut self, bytes: Vec<u8>) -> Option<SecretBytes> {
        let bytes = SecretBytes::new(bytes)?;
        self.entries.retain(|existing| match existing {
            HistoryEntry::Image(existing) => existing != &bytes,
            HistoryEntry::Text(_) => true,
        });
        self.entries.insert(0, HistoryEntry::Image(bytes.clone()));
        self.entries.truncate(MAX_HISTORY);
        Some(bytes)
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        match self.entries.get(index)? {
            HistoryEntry::Text(text) => Some(text.as_str()),
            HistoryEntry::Image(_) => None,
        }
    }

    pub fn image(&self, index: usize) -> Option<SecretBytes> {
        match self.entries.get(index)? {
            HistoryEntry::Image(bytes) => Some(bytes.clone()),
            HistoryEntry::Text(_) => None,
        }
    }

    pub fn mark(&self, index: usize) -> Option<&'static str> {
        match self.entries.get(index)? {
            HistoryEntry::Text(text) => Some(menu_mark(text.as_str())),
            HistoryEntry::Image(_) => Some(format::FormatKind::Image.menu_symbol()),
        }
    }

    pub fn byte_len(&self, index: usize) -> Option<usize> {
        match self.entries.get(index)? {
            HistoryEntry::Text(text) => Some(text.len()),
            HistoryEntry::Image(bytes) => Some(bytes.with(<[u8]>::len)),
        }
    }

    /// The current clipboard item stays on the Current row. Older images stay listed.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_in_history(
        &self,
        index: usize,
        current_text: Option<&str>,
        current_image: Option<&SecretBytes>,
    ) -> bool {
        match self.entries.get(index) {
            Some(HistoryEntry::Text(text)) => current_text != Some(text.as_str()),
            Some(HistoryEntry::Image(bytes)) => {
                !current_image.is_some_and(|current| current.same_allocation(bytes))
            }
            None => false,
        }
    }

    pub fn labels(&self) -> Vec<(usize, String)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let label = match entry {
                    HistoryEntry::Text(text) => one_line(text.as_str()),
                    HistoryEntry::Image(_) => format::FormatKind::Image.menu_symbol().to_string(),
                };
                (i, label)
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn write_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

pub fn clear_clipboard() -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.clear().map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
fn from_os_fallback() -> ClipboardView {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let text = cb.get_text().ok();
            match text {
                Some(text) if !text.trim().is_empty() => ClipboardView::Text(Zeroizing::new(text)),
                other => match cb.get_image().ok().and_then(ClipboardImage::from_arboard) {
                    Some(_) => ClipboardView::Image,
                    None if other.as_deref().is_some_and(|text| text.trim().is_empty()) => {
                        ClipboardView::Empty
                    }
                    None => ClipboardView::NoText,
                },
            }
        }
        Err(_) => ClipboardView::NoText,
    }
}

pub fn try_format_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    if !value.is_object() && !value.is_array() {
        return None;
    }
    serde_json::to_string_pretty(&value).ok()
}

pub fn formatted(text: &str) -> String {
    format::format_text(text)
}

pub fn menu_mark(text: &str) -> &'static str {
    format::detect(text).menu_symbol()
}

pub fn one_line(text: &str) -> String {
    menu_mark(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardHistory, ClipboardImage, ClipboardView, Zeroizing, formatted, one_line,
        try_format_json,
    };
    use zeroize::Zeroize;

    #[test]
    fn image_rejects_invalid_dimensions() {
        assert!(ClipboardImage::new(0, 1, vec![0, 0, 0, 255]).is_none());
        assert!(ClipboardImage::new(1, 1, vec![0, 0, 0]).is_none());
        assert!(ClipboardImage::new(2, 1, vec![255, 0, 0, 255]).is_none());
    }

    #[test]
    fn image_encodes_png() {
        let image = ClipboardImage::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).expect("rgba");
        let png = image.png_bytes().expect("png");
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        let decoded = image::load_from_memory(&png).expect("decode");
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 1);
    }

    #[test]
    fn image_view_is_previewable() {
        let view = ClipboardView::Image;
        assert!(view.is_previewable());
        assert!(view.is_image());
        assert!(view.text().is_none());
        assert_eq!(view.label(), "img");
        assert_eq!(view.type_mark(), Some("img"));
        assert!(!ClipboardView::Empty.is_previewable());
        assert!(!ClipboardView::NoText.is_previewable());
        assert!(ClipboardView::Text(Zeroizing::new("hello".into())).is_previewable());
    }

    #[test]
    fn one_line_uses_symbol_for_plain_text() {
        assert_eq!(one_line("\n  Hello world  \nmore"), "¶");
    }

    #[test]
    fn one_line_marks_json() {
        assert_eq!(one_line("{\"name\":\"copycraft\"}"), "{}");
    }

    #[test]
    fn one_line_marks_xml() {
        assert_eq!(one_line("<root><item/></root>"), "</>");
    }

    #[test]
    fn menu_mark_hides_content() {
        let mark = super::menu_mark("Naam: Jan de Vries\nE-mailadres: jan@x.nl");
        assert_eq!(mark, "¶");
        assert!(!mark.contains("Jan"));
        assert!(!mark.contains("@"));
    }

    #[test]
    fn menu_mark_by_type() {
        assert_eq!(super::menu_mark("{\"a\":1}"), "{}");
        assert_eq!(
            super::menu_mark("name: copycraft\nitems:\n  - one\n"),
            "---"
        );
        assert_eq!(super::menu_mark("<root><item/></root>"), "</>");
        assert_eq!(super::menu_mark("https://example.com/x"), "://");
        assert_eq!(super::menu_mark("fn main() {}"), "fn");
        assert_eq!(super::menu_mark("plain"), "Aa");
        assert_eq!(super::menu_mark("name,age\nalice,30\nbob,40"), "csv");
        assert_eq!(super::menu_mark("name\tage\nalice\t30\nbob\t40"), "tsv");
    }

    #[test]
    fn formats_json_when_valid() {
        let pretty = try_format_json("{\"name\":\"copycraft\"}").expect("json");
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("copycraft"));
    }

    #[test]
    fn ignores_non_json() {
        assert_eq!(try_format_json("hello"), None);
        assert_eq!(try_format_json("123"), None);
    }

    #[test]
    fn formatted_pretty_prints_json() {
        let pretty = formatted("{\"a\":1}");
        assert!(pretty.contains('\n'));
        assert!(pretty.contains('{'));
    }

    #[test]
    fn formatted_pretty_prints_xml() {
        let pretty = formatted("<a><b>x</b></a>");
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("    <b>"));
    }

    #[test]
    fn one_line_is_short_symbol() {
        let label = one_line(&"a".repeat(80));
        assert_eq!(label, "Aa");
    }

    #[test]
    fn history_dedupes_and_moves_to_front() {
        let mut history = ClipboardHistory::default();
        history.record("one".into());
        history.record("two".into());
        history.record("one".into());
        assert_eq!(history.get(0), Some("one"));
        assert_eq!(history.get(1), Some("two"));
        assert_eq!(history.labels().len(), 2);
    }

    #[test]
    fn history_keeps_an_image_after_later_text() {
        let mut history = ClipboardHistory::default();
        let image = history
            .record_image(vec![137, 80, 78, 71, 13, 10, 26, 10])
            .expect("image");
        history.record("hello".into());
        assert_eq!(history.get(0), Some("hello"));
        assert!(history.image(1).is_some());
        assert!(!history.shows_in_history(0, Some("hello"), None));
        assert!(history.shows_in_history(1, Some("hello"), None));
        assert!(!history.shows_in_history(1, None, Some(&image)));
        assert_eq!(history.mark(1), Some("img"));
        assert_eq!(history.labels()[1].1, "img");
        assert_eq!(history.byte_len(0), Some("hello".len()));
        assert_eq!(history.byte_len(1), Some(8));
        assert_eq!(history.byte_len(2), None);
    }

    #[test]
    fn history_moves_duplicate_image_to_front() {
        let mut history = ClipboardHistory::default();
        history.record_image(vec![1, 2, 3, 4]).expect("image");
        history.record("hello".into());
        history.record_image(vec![1, 2, 3, 4]).expect("image");
        assert!(history.image(0).is_some());
        assert_eq!(history.get(1), Some("hello"));
        assert_eq!(history.labels().len(), 2);
    }

    #[test]
    fn history_clear_empties_selectable_items() {
        let mut history = ClipboardHistory::default();
        history.record("one".into());
        history.record("two".into());
        history.clear();
        assert!(history.is_empty());
        assert!(history.labels().is_empty());
        assert_eq!(history.get(0), None);
    }

    #[test]
    fn clear_zeroizes_shared_image_bytes() {
        let mut history = ClipboardHistory::default();
        history.record("secret-token".into());
        let image = history
            .record_image(b"secret-bytes".to_vec())
            .expect("image");
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.get(0), None);
        assert!(image.prefix_is_zero(b"secret-bytes".len()));
    }

    #[test]
    fn zeroizing_text_overwrites_its_allocation() {
        let mut text = Zeroizing::new("secret-token".to_string());
        let ptr = text.as_ptr();
        let len = text.len();
        let cap = text.capacity();
        text.zeroize();
        assert!(text.is_empty());
        assert_eq!(text.capacity(), cap);
        // SAFETY: zeroize keeps this allocation and writes zeros across its
        // capacity. `text` still owns the buffer, and `len` is inside it.
        let wiped = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert!(wiped.iter().all(|byte| *byte == 0));
    }
}
