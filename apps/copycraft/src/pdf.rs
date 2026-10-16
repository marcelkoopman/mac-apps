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

/// Rendered pages of a PDF kept in memory besides the first (the Beeld view's page turning);
/// the ones furthest from the page shown are dropped first. Never written anywhere.
pub const PAGE_CACHE: usize = 12;

impl PdfFacts {
    /// Pages the Beeld view can turn to: the first [`MAX_PAGES`].
    pub fn turnable(&self) -> usize {
        self.pages.min(MAX_PAGES)
    }

    /// Footer of the Beeld view: "pagina 2/12", or "pagina 2/100 (van 340)" when pages were
    /// left out. One page: [`Self::label`].
    pub fn page_label(&self, page: usize) -> String {
        let shown = page.min(self.turnable().saturating_sub(1)) + 1;
        if self.pages <= 1 {
            self.label()
        } else if self.pages > MAX_PAGES {
            crate::locale::tf("pdf_page_of_cut", &[&shown, &self.turnable(), &self.pages])
        } else {
            crate::locale::tf("pdf_page_of", &[&shown, &self.pages])
        }
    }
}

/// The page (0-based) after moving `delta` pages from `current` in a PDF of `pages` pages (only
/// the first [`MAX_PAGES`] can be turned to). `None` when nothing moves: at an end, or one page.
pub fn step_page(current: usize, delta: isize, pages: usize) -> Option<usize> {
    let last = pages.min(MAX_PAGES).checked_sub(1)?;
    let current = current.min(last);
    let next = current.saturating_add_signed(delta).min(last);
    (next != current).then_some(next)
}

/// Pages to drop from a cache holding `cached` (other than page 0, which the card owns) so that
/// at most [`PAGE_CACHE`] remain, keeping those nearest `current`.
pub fn pages_to_evict(cached: &[usize], current: usize) -> Vec<usize> {
    let mut by_distance: Vec<usize> = cached.to_vec();
    by_distance.sort_by_key(|page| (page.abs_diff(current), *page));
    by_distance.split_off(by_distance.len().min(PAGE_CACHE))
}

/// Keys that turn pages. ← → only count where they do not step history ([`route_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    Left,
    Right,
    PageUp,
    PageDown,
}

/// What a key does on a PDF card in its Beeld view with more than one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRoute {
    /// Turn pages by this many.
    Turn(isize),
    /// Not the PDF's: the card's own rule (history, chips) applies.
    Card,
}

/// Page Up / Page Down always turn pages. ← → turn pages with ⌥ held, or while the pointer is
/// over the PDF view; elsewhere they keep stepping history. ⌘ is always history.
pub fn route_key(key: PageKey, option: bool, command: bool, pointer_over: bool) -> KeyRoute {
    match key {
        PageKey::PageUp => KeyRoute::Turn(-1),
        PageKey::PageDown => KeyRoute::Turn(1),
        PageKey::Left | PageKey::Right if command => KeyRoute::Card,
        PageKey::Left | PageKey::Right if option || pointer_over => {
            KeyRoute::Turn(if key == PageKey::Left { -1 } else { 1 })
        }
        _ => KeyRoute::Card,
    }
}

/// Scroll distance (points) of a trackpad swipe that turns one page.
pub const SWIPE_STEP: f64 = 60.0;

/// A scroll wheel or swipe over the PDF view: `accumulated` is what earlier events of this
/// gesture left, `delta` the vertical distance of this one (negative: content moves up, the way
/// a reader goes on). Returns the new accumulation and the pages to turn. A wheel notch
/// (`precise` false) turns at once; a trackpad turns after [`SWIPE_STEP`] points.
pub fn scroll_step(accumulated: f64, delta: f64, precise: bool) -> (f64, Option<isize>) {
    if delta == 0.0 {
        return (accumulated, None);
    }
    if !precise {
        return (0.0, Some(if delta < 0.0 { 1 } else { -1 }));
    }
    let total = accumulated + delta;
    if total <= -SWIPE_STEP {
        (0.0, Some(1))
    } else if total >= SWIPE_STEP {
        (0.0, Some(-1))
    } else {
        (total, None)
    }
}

/// Zoom of the PDF page in the Beeld view, in percent. 100 is "fit in the well".
pub const ZOOM_FIT: u32 = 100;
pub const ZOOM_MIN: u32 = 50;
pub const ZOOM_MAX: u32 = 400;
/// One ⌘+ or ⌘− moves the zoom to the next multiple of this.
pub const ZOOM_STEP: u32 = 25;

/// What a zoom shortcut or a pinch asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoomStep {
    In,
    Out,
    /// ⌘0: back to fit.
    Reset,
    /// A pinch ended on this zoom ([`pinch_zoom`]).
    Set(u32),
}

/// The zoom after `step` from `current`, kept within [`ZOOM_MIN`]..=[`ZOOM_MAX`]. In and out go
/// to the next multiple of [`ZOOM_STEP`] (from 110 %: 125 % up, 100 % down).
pub fn zoom_step(current: u32, step: ZoomStep) -> u32 {
    let current = current.clamp(ZOOM_MIN, ZOOM_MAX);
    let next = match step {
        ZoomStep::In => (current / ZOOM_STEP + 1) * ZOOM_STEP,
        ZoomStep::Out => current.div_ceil(ZOOM_STEP).saturating_sub(1) * ZOOM_STEP,
        ZoomStep::Reset => ZOOM_FIT,
        ZoomStep::Set(zoom) => zoom,
    };
    next.clamp(ZOOM_MIN, ZOOM_MAX)
}

/// The zoom after a pinch of `magnification` (the sum of the gesture's magnification values: 0.25
/// is a quarter larger, −0.5 half as large), rounded to 5 % and clamped. A non-number changes
/// nothing. Close to fit it snaps to 100 %.
pub fn pinch_zoom(current: u32, magnification: f64) -> u32 {
    if !magnification.is_finite() {
        return current.clamp(ZOOM_MIN, ZOOM_MAX);
    }
    let factor = (1.0 + magnification).max(0.0);
    let zoom = (f64::from(current.clamp(ZOOM_MIN, ZOOM_MAX)) * factor / 5.0).round() * 5.0;
    let zoom = zoom.clamp(f64::from(ZOOM_MIN), f64::from(ZOOM_MAX)) as u32;
    if zoom.abs_diff(ZOOM_FIT) <= 5 {
        ZOOM_FIT
    } else {
        zoom
    }
}

/// What the footer says about the zoom: "zoom 125%", nothing at 100 %.
pub fn zoom_label(zoom: u32) -> Option<String> {
    (zoom != ZOOM_FIT).then(|| crate::locale::tf("pdf_zoom", &[&zoom]))
}

/// The zoom shortcut for a key press: ⌘= ⌘+ (Shift allowed, as "+" needs it on many layouts),
/// ⌘− ⌘- and ⌘0. `chars` is the key's characters without modifiers. Control and Option make it
/// another chord.
pub fn zoom_key(chars: &str, command: bool, option: bool, control: bool) -> Option<ZoomStep> {
    if !command || option || control {
        return None;
    }
    match chars {
        "=" | "+" => Some(ZoomStep::In),
        "-" | "\u{2212}" | "_" => Some(ZoomStep::Out),
        "0" => Some(ZoomStep::Reset),
        _ => None,
    }
}

/// Scroll distance (points) of a trackpad that zooms one step with ⌘ held.
pub const WHEEL_ZOOM_STEP: f64 = 40.0;

/// ⌘ + scroll wheel or trackpad: `accumulated` is what earlier events of the gesture left,
/// `delta` this event's vertical distance (positive: scrolling up, which zooms in). A wheel notch
/// (`precise` false) is one step; a trackpad adds up to [`WHEEL_ZOOM_STEP`].
pub fn wheel_zoom(accumulated: f64, delta: f64, precise: bool) -> (f64, Option<ZoomStep>) {
    if delta == 0.0 || !delta.is_finite() {
        return (accumulated, None);
    }
    let direction = |up: bool| if up { ZoomStep::In } else { ZoomStep::Out };
    if !precise {
        return (0.0, Some(direction(delta > 0.0)));
    }
    let total = accumulated + delta;
    if total.abs() >= WHEEL_ZOOM_STEP {
        (0.0, Some(direction(total > 0.0)))
    } else {
        (total, None)
    }
}

/// Most pixels and longest edge of a page drawn zoomed in (the fit picture has its own, smaller
/// limits in `macos_pdf.rs`).
pub const ZOOM_MAX_PIXELS: usize = 24_000_000;
pub const ZOOM_MAX_EDGE: f64 = 8192.0;

/// Pixel size to draw a page of `width` × `height` points at `zoom` percent: three times the page
/// size at fit (216 dpi, within a 3000 px edge), growing with the zoom until the edge or the
/// pixel cap ([`ZOOM_MAX_PIXELS`]) holds it. `None` for an unusable page.
pub fn render_size(width: f64, height: f64, zoom: u32) -> Option<(usize, usize)> {
    if !(width >= 1.0 && height >= 1.0 && width.is_finite() && height.is_finite()) {
        return None;
    }
    let (edge, pixels) = if zoom > ZOOM_FIT {
        (ZOOM_MAX_EDGE, ZOOM_MAX_PIXELS as f64)
    } else {
        (3000.0, 12_000_000.0)
    };
    let wanted = 3.0 * f64::from(zoom.max(ZOOM_FIT)) / 100.0;
    let scale = wanted
        .min(edge / width.max(height))
        .min((pixels / (width * height)).sqrt());
    let mut w = ((width * scale).round() as usize).max(1);
    let mut h = ((height * scale).round() as usize).max(1);
    if w.checked_mul(h)? > pixels as usize {
        // Rounding up went over the cap by a hair.
        w = ((width * scale).floor() as usize).max(1);
        h = ((height * scale).floor() as usize).max(1);
    }
    (w.checked_mul(h)? <= pixels as usize).then_some((w, h))
}

/// The size, in points, of the picture in the scroll view at `zoom` percent: the fit size
/// (`fit_w` × `fit_h`, the picture fitted into the well) times the zoom.
pub fn zoomed_extent(fit_w: f64, fit_h: f64, zoom: u32) -> (f64, f64) {
    let factor = f64::from(zoom.clamp(ZOOM_MIN, ZOOM_MAX)) / 100.0;
    (fit_w * factor, fit_h * factor)
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

    #[test]
    fn turning_pages_stays_within_the_first_hundred() {
        assert_eq!(step_page(0, 1, 5), Some(1));
        assert_eq!(step_page(0, -1, 5), None);
        assert_eq!(step_page(4, 1, 5), None);
        assert_eq!(step_page(3, 5, 5), Some(4));
        assert_eq!(step_page(0, 1, 1), None);
        assert_eq!(step_page(0, 1, 0), None);
        // Page 100 is the last one that can be turned to, however long the PDF is.
        assert_eq!(step_page(98, 1, 340), Some(99));
        assert_eq!(step_page(99, 1, 340), None);
        // A stale page past the end is pulled back.
        assert_eq!(step_page(50, -1, 5), Some(3));
    }

    #[test]
    fn the_footer_names_the_page_shown() {
        let facts = |pages| PdfFacts {
            pages,
            read: pages,
            scanned: false,
        };
        assert_eq!(
            facts(12).page_label(1),
            crate::locale::tf("pdf_page_of", &[&2, &12])
        );
        let long = facts(340).page_label(4);
        assert!(
            long.contains('5') && long.contains("100") && long.contains("340"),
            "{long}"
        );
        assert_eq!(facts(1).page_label(0), facts(1).label());
        assert_eq!(facts(340).turnable(), MAX_PAGES);
    }

    #[test]
    fn the_cache_keeps_the_pages_nearest_the_one_shown() {
        let few = [1, 2, 3];
        assert!(pages_to_evict(&few, 2).is_empty());
        let many: Vec<usize> = (1..=20).collect();
        let dropped = pages_to_evict(&many, 3);
        assert_eq!(dropped.len(), 20 - PAGE_CACHE);
        assert!(dropped.contains(&20) && !dropped.contains(&3) && !dropped.contains(&1));
    }

    #[test]
    fn page_keys_turn_pages_only_where_they_do_not_step_history() {
        use KeyRoute::{Card, Turn};
        // Page Up and Page Down are the PDF's anywhere on the card.
        assert_eq!(route_key(PageKey::PageDown, false, false, false), Turn(1));
        assert_eq!(route_key(PageKey::PageUp, false, false, false), Turn(-1));
        // ← → stay history unless ⌥ is held or the pointer is over the PDF.
        assert_eq!(route_key(PageKey::Right, false, false, false), Card);
        assert_eq!(route_key(PageKey::Right, true, false, false), Turn(1));
        assert_eq!(route_key(PageKey::Left, false, false, true), Turn(-1));
        // ⌘← ⌘→ are always history.
        assert_eq!(route_key(PageKey::Left, false, true, true), Card);
        assert_eq!(route_key(PageKey::Right, true, true, true), Card);
    }

    #[test]
    fn a_swipe_or_wheel_turns_pages() {
        // A wheel notch turns one page at once; down (content up) goes on.
        assert_eq!(scroll_step(0.0, -1.0, false), (0.0, Some(1)));
        assert_eq!(scroll_step(0.0, 1.0, false), (0.0, Some(-1)));
        // A trackpad adds up until the step.
        let (acc, turn) = scroll_step(0.0, -25.0, true);
        assert_eq!(turn, None);
        let (acc, turn) = scroll_step(acc, -25.0, true);
        assert_eq!(turn, None);
        assert_eq!(scroll_step(acc, -25.0, true), (0.0, Some(1)));
        assert_eq!(scroll_step(0.0, 70.0, true), (0.0, Some(-1)));
        assert_eq!(scroll_step(5.0, 0.0, true), (5.0, None));
    }

    #[test]
    fn zoom_steps_by_quarters_within_limits() {
        use ZoomStep::*;
        assert_eq!(zoom_step(100, In), 125);
        assert_eq!(zoom_step(100, Out), 75);
        assert_eq!(zoom_step(110, In), 125);
        assert_eq!(zoom_step(110, Out), 100);
        assert_eq!(zoom_step(125, Out), 100);
        assert_eq!(zoom_step(400, In), 400);
        assert_eq!(zoom_step(50, Out), 50);
        assert_eq!(zoom_step(60, Out), 50);
        assert_eq!(zoom_step(250, Reset), 100);
        assert_eq!(zoom_step(100, Set(900)), 400);
        assert_eq!(zoom_step(100, Set(1)), 50);
        // Out of range input is pulled in first.
        assert_eq!(zoom_step(9999, Out), 375);
    }

    #[test]
    fn a_pinch_scales_rounds_snaps_and_clamps() {
        assert_eq!(pinch_zoom(100, 0.5), 150);
        assert_eq!(pinch_zoom(100, -0.5), 50);
        assert_eq!(pinch_zoom(100, 0.02), 100);
        assert_eq!(pinch_zoom(100, 0.07), 100);
        assert_eq!(pinch_zoom(100, 0.12), 110);
        assert_eq!(pinch_zoom(300, 2.0), 400);
        assert_eq!(pinch_zoom(100, -3.0), 50);
        assert_eq!(pinch_zoom(125, f64::NAN), 125);
    }

    #[test]
    fn the_footer_names_the_zoom_only_when_not_fit() {
        assert_eq!(zoom_label(100), None);
        assert_eq!(zoom_label(125).as_deref(), Some("zoom 125%"));
        assert_eq!(zoom_label(50).as_deref(), Some("zoom 50%"));
    }

    #[test]
    fn only_command_plus_minus_zero_zoom() {
        use ZoomStep::*;
        assert_eq!(zoom_key("=", true, false, false), Some(In));
        assert_eq!(zoom_key("+", true, false, false), Some(In));
        assert_eq!(zoom_key("-", true, false, false), Some(Out));
        assert_eq!(zoom_key("\u{2212}", true, false, false), Some(Out));
        assert_eq!(zoom_key("0", true, false, false), Some(Reset));
        // No ⌘: typing a minus or a digit is search text. ⌥ and ⌃ make other chords.
        assert_eq!(zoom_key("-", false, false, false), None);
        assert_eq!(zoom_key("=", true, true, false), None);
        assert_eq!(zoom_key("0", true, false, true), None);
        // Other ⌘ chords keep their meaning.
        for other in ["c", "f", "y", "s", "o", "z", "1"] {
            assert_eq!(zoom_key(other, true, false, false), None);
        }
    }

    #[test]
    fn zoomed_pages_stay_within_the_pixel_cap() {
        // At fit: as before (3x, 3000 px edge).
        assert_eq!(render_size(595.0, 842.0, 100), Some((1785, 2526)));
        assert_eq!(render_size(2000.0, 1000.0, 100), Some((3000, 1500)));
        // 200 %: twice the pixels per side, until the cap.
        assert_eq!(render_size(595.0, 842.0, 200), Some((3570, 5052)));
        let (w, h) = render_size(595.0, 842.0, 400).expect("size");
        assert!(w * h <= ZOOM_MAX_PIXELS && w.max(h) as f64 <= ZOOM_MAX_EDGE);
        // A huge page is scaled down, not refused.
        let (w, h) = render_size(14_400.0, 14_400.0, 400).expect("size");
        assert!(w * h <= ZOOM_MAX_PIXELS);
        assert_eq!(render_size(0.0, 10.0, 100), None);
        assert_eq!(render_size(f64::NAN, 10.0, 100), None);
        // Below fit draws like fit.
        assert_eq!(
            render_size(595.0, 842.0, 50),
            render_size(595.0, 842.0, 100)
        );
    }

    #[test]
    fn the_extent_follows_the_zoom() {
        assert_eq!(zoomed_extent(400.0, 300.0, 200), (800.0, 600.0));
        assert_eq!(zoomed_extent(400.0, 300.0, 9999), (1600.0, 1200.0));
        assert_eq!(zoomed_extent(400.0, 300.0, 50), (200.0, 150.0));
    }

    #[test]
    fn command_scroll_zooms_in_steps() {
        use ZoomStep::*;
        assert_eq!(wheel_zoom(0.0, 1.0, false), (0.0, Some(In)));
        assert_eq!(wheel_zoom(0.0, -1.0, false), (0.0, Some(Out)));
        assert_eq!(wheel_zoom(0.0, 0.0, false), (0.0, None));
        let (acc, step) = wheel_zoom(0.0, 25.0, true);
        assert_eq!(step, None);
        assert_eq!(wheel_zoom(acc, 25.0, true), (0.0, Some(In)));
        assert_eq!(wheel_zoom(0.0, -45.0, true), (0.0, Some(Out)));
        assert_eq!(wheel_zoom(5.0, f64::NAN, true), (5.0, None));
    }
}
