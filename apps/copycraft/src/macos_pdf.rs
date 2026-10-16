#![cfg(target_os = "macos")]
//! PDFs for the PDF card: the text layer through PDFKit, and pages drawn with CoreGraphics for
//! the Vision text recognition when there is no text layer. Everything stays in memory: no file
//! is written, nothing is cached, nothing goes over the network, and no content is logged.
//!
//! PDFKit objects are used on the main thread only: the worker that opens a file hands each
//! step to the main queue ([`mac_ui::actions::run_on_main`]) and waits for the answer. The
//! document bytes live in a [`SecretBytes`] (zeroized when dropped). PDFKit and Foundation keep
//! their own copies while a step runs; those are released when it ends, and are out of our reach.

use std::sync::mpsc;
use std::time::Duration;

use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{AnyThread, MainThreadMarker};
use mac_ui::objc2_foundation::NSData;
use objc2_core_foundation::{CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGContext, CGPDFBox, CGPDFPage};
use objc2_pdf_kit::PDFDocument;
use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;
use crate::commands::ImageScan;
use crate::pdf::{MAX_PAGES, OCR_MAX_PAGES};

/// A main-thread step that takes longer than this is given up on.
const MAIN_WAIT: Duration = Duration::from_secs(120);

/// What reading a PDF's text layer found.
pub(crate) enum Read {
    /// Not a PDF the system can open.
    Unreadable,
    /// Needs a password.
    Locked,
    /// `pages` in the document; the text of the first `read` of them, `None` without a text
    /// layer.
    Pages {
        pages: usize,
        read: usize,
        text: Option<Zeroizing<String>>,
    },
}

/// Run `step` on the main thread and wait for its answer. Directly when already there.
fn on_main<T: Send + 'static>(step: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    if MainThreadMarker::new().is_some() {
        return Some(step());
    }
    let (send, receive) = mpsc::channel();
    mac_ui::actions::run_on_main(move || {
        let _ = send.send(step());
    });
    receive.recv_timeout(MAIN_WAIT).ok()
}

fn document(source: &SecretBytes) -> Option<Retained<PDFDocument>> {
    let data = source.with(NSData::with_bytes);
    // SAFETY: `initWithData:` on a freshly allocated PDFDocument, with live data.
    unsafe { PDFDocument::initWithData(PDFDocument::alloc(), &data) }
}

/// The text layer of `source`: the first [`MAX_PAGES`] pages.
pub(crate) fn read(source: &SecretBytes) -> Read {
    let source = source.clone();
    on_main(move || read_on_main(&source)).unwrap_or(Read::Unreadable)
}

fn read_on_main(source: &SecretBytes) -> Read {
    let Some(document) = document(source) else {
        return Read::Unreadable;
    };
    // SAFETY: plain property reads of a PDFDocument on the main thread.
    let (locked, pages) = unsafe { (document.isLocked(), document.pageCount()) };
    if locked {
        return Read::Locked;
    }
    let read = pages.min(MAX_PAGES);
    let texts = (0..read).filter_map(|index| {
        // SAFETY: `index` is below the page count; the page and its string are read at once,
        // on the main thread.
        let text = unsafe { document.pageAtIndex(index)?.string() }?;
        Some(Zeroizing::new(text.to_string()))
    });
    Read::Pages {
        pages,
        read,
        text: crate::pdf::join_pages(texts),
    }
}

/// Page `index` as PNG, and the pages in the document. `None` when it cannot be drawn. `crop`:
/// without the empty white margin (for showing the page; see [`crate::pdf::content_crop`]).
pub(crate) fn render_page(
    source: &SecretBytes,
    index: usize,
    crop: bool,
) -> Option<(Zeroizing<Vec<u8>>, usize)> {
    render_page_zoomed(source, index, crop, crate::pdf::ZOOM_FIT)
}

/// [`render_page`] at `zoom` percent: more pixels, within [`crate::pdf::ZOOM_MAX_PIXELS`], so the
/// zoomed page is drawn again from the PDF instead of a bitmap stretched.
pub(crate) fn render_page_zoomed(
    source: &SecretBytes,
    index: usize,
    crop: bool,
    zoom: u32,
) -> Option<(Zeroizing<Vec<u8>>, usize)> {
    let source = source.clone();
    on_main(move || render_on_main(&source, index, crop, zoom)).flatten()
}

fn render_on_main(
    source: &SecretBytes,
    index: usize,
    crop: bool,
    zoom: u32,
) -> Option<(Zeroizing<Vec<u8>>, usize)> {
    let document = document(source)?;
    // SAFETY: plain property reads of a PDFDocument on the main thread.
    let (locked, pages) = unsafe { (document.isLocked(), document.pageCount()) };
    if locked || index >= pages {
        return None;
    }
    // SAFETY: `index` is below the page count; reads on the main thread.
    let page_ref = unsafe { document.pageAtIndex(index)?.pageRef() }?;
    Some((draw_png(&page_ref, crop, zoom)?, pages))
}

fn draw_png(page: &CGPDFPage, crop: bool, zoom: u32) -> Option<Zeroizing<Vec<u8>>> {
    let area = CGPDFBox::CropBox;
    let bounds = CGPDFPage::box_rect(Some(page), area);
    let (mut width, mut height) = (bounds.size.width, bounds.size.height);
    if CGPDFPage::rotation_angle(Some(page)).rem_euclid(180) == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    // Three times the page size (216 dpi, readable text) at fit, more when zoomed in, within the
    // edge and pixel limits ([`crate::pdf::render_size`]).
    let (px_w, px_h) = crate::pdf::render_size(width, height, zoom)?;
    let mut rgba = Zeroizing::new(vec![0u8; px_w * px_h * 4]);
    let target = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: px_w as f64,
            height: px_h as f64,
        },
    };
    crate::macos_image_io::with_bitmap(&mut rgba, px_w, px_h, |context| {
        // A white sheet, then the page fitted onto it (rotation applied).
        CGContext::set_rgb_fill_color(Some(context), 1.0, 1.0, 1.0, 1.0);
        CGContext::fill_rect(Some(context), target);
        let fit: CGAffineTransform =
            CGPDFPage::drawing_transform(Some(page), area, target, 0, true);
        CGContext::save_g_state(Some(context));
        CGContext::concat_ctm(Some(context), fit);
        CGContext::draw_pdf_page(Some(context), Some(page));
        CGContext::restore_g_state(Some(context));
    })?;
    let (mut cropped, mut out_w, mut out_h) = (None, px_w, px_h);
    if crop && let Some(rect) = crate::pdf::content_crop(&rgba, px_w, px_h) {
        cropped = Some(crate::pdf::crop_rgba(&rgba, px_w, rect));
        (out_w, out_h) = (rect.2, rect.3);
    }
    let pixels = cropped.as_deref().map_or(rgba.as_slice(), Vec::as_slice);
    let mut png = Zeroizing::new(Vec::new());
    PngEncoder::new(&mut *png)
        .write_image(pixels, out_w as u32, out_h as u32, ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
}

/// The recognition for a scanned PDF, like a dropped picture's scan but over the first
/// [`OCR_MAX_PAGES`] pages: the picture info, data URL and barcodes are page 1's, the text is
/// every page's, a blank line between pages. Runs on a background thread (Vision is slow);
/// only the page drawing goes to the main thread.
pub(crate) fn scan(source: &SecretBytes, text: Option<Zeroizing<String>>) -> Option<ImageScan> {
    let (first, pages) = render_page(source, 0, true)?;
    let mut scan = crate::macos_pasteboard::scan_image_bytes(&first)?;
    drop(first);
    // A PDF with a text layer: that text is the *Tekst* view, not a recognition of the pages.
    if let Some(text) = text {
        drop(scan.ocr.take().map(Zeroizing::new));
        scan.ocr = Some(text.to_string());
        return Some(scan);
    }
    let mut texts: Vec<Zeroizing<String>> =
        scan.ocr.take().map(Zeroizing::new).into_iter().collect();
    for index in 1..pages.min(OCR_MAX_PAGES) {
        let Some((png, _)) = render_page(source, index, false) else {
            continue;
        };
        if let Some(text) = crate::macos_vision::scan_png(&png).0 {
            texts.push(Zeroizing::new(text));
        }
    }
    scan.ocr = crate::pdf::join_pages(texts).map(|text| text.to_string());
    Some(scan)
}
