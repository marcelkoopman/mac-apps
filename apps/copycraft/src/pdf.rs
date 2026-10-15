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
        if chars >= MAX_TEXT_CHARS {
            break;
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

/// A pixel counts as paper up to this value in every channel.
const PAPER_MIN: u8 = 245;

/// The part of a rendered RGBA page that has content, as `(x, y, width, height)`: the box around
/// the non-white pixels plus a margin (2 % of the width, at least 12 px). `None` when the page is
/// blank or the crop would not remove anything noticeable. Only for showing the page: the text
/// and the recognition never see the crop.
pub fn content_crop(
    rgba: &[u8],
    width: usize,
    height: usize,
) -> Option<(usize, usize, usize, usize)> {
    if width == 0 || height == 0 || rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0usize, 0usize);
    for (y, row) in rgba.chunks_exact(width * 4).enumerate() {
        let ink = |px: &[u8; 4]| px[0] < PAPER_MIN || px[1] < PAPER_MIN || px[2] < PAPER_MIN;
        let Some(first) = row.as_chunks::<4>().0.iter().position(ink) else {
            continue;
        };
        let last = row
            .as_chunks::<4>()
            .0
            .iter()
            .rposition(ink)
            .unwrap_or(first);
        left = left.min(first);
        right = right.max(last);
        top = top.min(y);
        bottom = y;
    }
    if right < left || bottom < top {
        return None;
    }
    let margin = (width / 50).max(12);
    let x0 = left.saturating_sub(margin);
    let y0 = top.saturating_sub(margin);
    let x1 = (right + 1 + margin).min(width);
    let y1 = (bottom + 1 + margin).min(height);
    let (w, h) = (x1 - x0, y1 - y0);
    // Not worth it when less than a tenth of the area goes.
    (w * h * 10 < width * height * 9).then_some((x0, y0, w, h))
}

/// The `crop` rectangle of an RGBA image, in one zeroizing allocation.
pub fn crop_rgba(
    rgba: &[u8],
    width: usize,
    crop: (usize, usize, usize, usize),
) -> Zeroizing<Vec<u8>> {
    let (x, y, w, h) = crop;
    let mut out = Zeroizing::new(Vec::with_capacity(w * h * 4));
    for row in y..y + h {
        let start = (row * width + x) * 4;
        out.extend_from_slice(&rgba[start..start + w * 4]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width: usize, height: usize, ink: &[(usize, usize)]) -> Vec<u8> {
        let mut rgba = vec![255u8; width * height * 4];
        for &(x, y) in ink {
            let at = (y * width + x) * 4;
            rgba[at..at + 3].copy_from_slice(&[0, 0, 0]);
        }
        rgba
    }

    #[test]
    fn the_crop_is_the_ink_plus_a_margin() {
        let rgba = page(400, 600, &[(150, 200), (250, 400)]);
        let (x, y, w, h) = content_crop(&rgba, 400, 600).expect("crop");
        assert_eq!((x, y), (150 - 12, 200 - 12));
        assert_eq!((w, h), (101 + 24, 201 + 24));
        let cropped = crop_rgba(&rgba, 400, (x, y, w, h));
        assert_eq!(cropped.len(), w * h * 4);
        // The first ink pixel sits at the margin.
        assert_eq!(cropped[(12 * w + 12) * 4], 0);
    }

    #[test]
    fn a_blank_or_full_page_is_not_cropped() {
        assert!(content_crop(&page(100, 100, &[]), 100, 100).is_none());
        assert!(content_crop(&page(100, 100, &[(0, 0), (99, 99)]), 100, 100).is_none());
        assert!(content_crop(&[0; 3], 100, 100).is_none());
    }

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
