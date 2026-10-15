//! The PDF card, the parts that need no system framework: limits, what the card says about a
//! PDF (the footer), and joining the text of the pages. PDFKit and CoreGraphics live in
//! `macos_pdf.rs`. Nothing here logs content, and nothing is cached on disk.

use zeroize::Zeroizing;

/// PDFs larger than this stay off the card (like a dropped picture).
pub const MAX_PDF_BYTES: u64 = 32_000_000;
/// Pages whose text is read. Later pages are left out and the footer says so.
pub const MAX_PAGES: usize = 100;
/// Pages that are rendered and recognized when the PDF has no text layer. Rendering and
/// recognition are slow and memory hungry; the footer says how many pages were read.
pub const OCR_MAX_PAGES: usize = 10;
/// Extracted text kept on the card, in characters. The AI input is cut to
/// [`mac_ai::MAX_CHARS`] on top of this.
pub const MAX_TEXT_CHARS: usize = 1_000_000;

/// What the card knows about a PDF it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfFacts {
    /// Pages in the document.
    pub pages: usize,
    /// Pages whose text was read (or, for OCR, rendered): at most [`MAX_PAGES`] / [`OCR_MAX_PAGES`].
    pub read: usize,
    /// No text layer: the card is a picture of the first page, its text comes from OCR.
    pub scanned: bool,
}

impl PdfFacts {
    /// "12 pagina's", or "Eerste 100 van 340 pagina's" when pages were left out.
    pub fn label(&self) -> String {
        if self.pages == 1 {
            crate::locale::t("pdf_one_page").to_string()
        } else if self.read < self.pages {
            crate::locale::tf("pdf_first_pages", &[&self.read, &self.pages])
        } else {
            crate::locale::tf("pdf_n_pages", &[&self.pages])
        }
    }
}

/// The bytes start like a PDF (`%PDF-` within the first kilobyte, as the format allows).
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|window| window == b"%PDF-")
}

/// The text of `pages` in one allocation, cut to [`MAX_TEXT_CHARS`], pages separated by a blank
/// line. Page texts are zeroized when dropped. `None` when no page has any text (a scan).
pub fn join_pages(pages: impl IntoIterator<Item = Zeroizing<String>>) -> Option<Zeroizing<String>> {
    let mut out = Zeroizing::new(String::new());
    let mut chars = 0usize;
    for page in pages {
        let page = page.trim();
        if page.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
            chars += 2;
        }
        let room = MAX_TEXT_CHARS.saturating_sub(chars);
        match page.char_indices().nth(room) {
            Some((cut, _)) => {
                out.push_str(&page[..cut]);
                break;
            }
            None => {
                out.push_str(page);
                chars += page.chars().count();
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(texts: &[&str]) -> Vec<Zeroizing<String>> {
        texts
            .iter()
            .map(|text| Zeroizing::new((*text).to_string()))
            .collect()
    }

    #[test]
    fn recognizes_the_pdf_header() {
        assert!(is_pdf(b"%PDF-1.7\n%..."));
        assert!(is_pdf(b"\xEF\xBB\xBF%PDF-1.4"));
        assert!(!is_pdf(b"hello"));
        assert!(!is_pdf(b""));
    }

    #[test]
    fn joins_pages_with_a_blank_line_and_skips_empty_ones() {
        let text = join_pages(pages(&["one ", "  ", "two"])).expect("text");
        assert_eq!(text.as_str(), "one\n\ntwo");
    }

    #[test]
    fn no_text_on_any_page_is_a_scan() {
        assert!(join_pages(pages(&["", " \n "])).is_none());
        assert!(join_pages(Vec::new()).is_none());
    }

    #[test]
    fn the_text_is_cut_at_the_limit_on_a_char_boundary() {
        let long = "é".repeat(MAX_TEXT_CHARS);
        let text = join_pages(pages(&[&long, "more"])).expect("text");
        assert_eq!(text.chars().count(), MAX_TEXT_CHARS);
        let text = join_pages(pages(&["ab", &long])).expect("text");
        assert_eq!(text.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn the_footer_names_the_pages() {
        let all = PdfFacts {
            pages: 12,
            read: 12,
            scanned: false,
        };
        assert!(all.label().contains("12"));
        let cut = PdfFacts {
            pages: 340,
            read: MAX_PAGES,
            scanned: false,
        };
        let label = cut.label();
        assert!(label.contains("100") && label.contains("340"));
        let one = PdfFacts {
            pages: 1,
            read: 1,
            scanned: true,
        };
        assert!(!one.label().contains("pagina's"));
    }
}
