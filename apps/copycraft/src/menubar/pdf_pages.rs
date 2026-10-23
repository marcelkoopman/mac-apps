//! Turning the pages of a PDF in the Beeld view. The card shows the first page from the start;
//! the others are drawn when asked for, on a worker thread, and kept in memory only, at most
//! [`crate::pdf::PAGE_CACHE`] of them besides the first. They go with the file
//! ([`SecretBytes`] zeroizes when its last owner lets go). The Tekst view and the AI input are
//! the PDF's text, whatever page the picture shows.

use super::*;

impl App {
    /// The PDF on the card, when it is shown as its pages (Beeld view, more than one page).
    fn pdf_on_card(&mut self) -> Option<(&mut crate::open_file::OpenedImage, usize)> {
        if self.card_view != commands::CardView::Original {
            return None;
        }
        let file = self.opened.as_mut()?;
        let pages = file.pdf?.pages;
        let image = file
            .image
            .as_mut()
            .filter(|image| image.pdf_source.is_some())?;
        Some((image, pages))
    }

    /// Turn `delta` pages (negative: back). Beeps nothing here; the launcher checks the ends.
    pub(crate) fn pdf_turn(&mut self, delta: isize) {
        let Some((image, pages)) = self.pdf_on_card() else {
            return;
        };
        let Some(target) = crate::pdf::step_page(image.want, delta, pages) else {
            return;
        };
        image.want = target;
        let ready = target == 0 || image.pages.contains_key(&target);
        let source = image.pdf_source.clone();
        if ready {
            image.page = target;
            self.pdf_page_changed();
            return;
        }
        // Drawn on a worker (the PDF framework hands its main-thread step back); the card keeps
        // the page it has until then.
        #[cfg(target_os = "macos")]
        if let Some(source) = source {
            let spawned = std::thread::Builder::new()
                .name("copycraft-pdf-page".into())
                .spawn(move || {
                    let page = crate::macos_pdf::render_page(&source, target, true)
                        .and_then(|(mut png, _)| SecretBytes::new(std::mem::take(&mut *png)));
                    launcher::emit(UserEvent::PdfPageRendered {
                        source,
                        index: target,
                        page,
                    });
                });
            if let Err(e) = spawned {
                crate::diag::log(crate::diag::LogEvent::DroppedImageScanFailed(
                    crate::diag::ErrorKind::io(&e),
                ));
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = source;
    }

    /// Page `index` is drawn on its thread.
    pub(crate) fn finish_pdf_page(
        &mut self,
        source: &SecretBytes,
        index: usize,
        page: Option<SecretBytes>,
    ) {
        let Some(image) = self
            .opened
            .as_mut()
            .and_then(|file| file.image.as_mut())
            .filter(|image| {
                image
                    .pdf_source
                    .as_ref()
                    .is_some_and(|own| own.same_allocation(source))
            })
        else {
            return;
        };
        let Some(page) = page else {
            // It cannot be drawn: back to the page that shows.
            if image.want == index {
                image.want = image.page;
            }
            return;
        };
        image.pages.insert(index, page);
        let cached: Vec<usize> = image.pages.keys().copied().collect();
        for old in crate::pdf::pages_to_evict(&cached, image.want) {
            image.pages.remove(&old);
        }
        if image.want == index {
            image.page = index;
            self.pdf_page_changed();
        }
    }

    /// ⌘+ ⌘− ⌘0 or a pinch: the zoom asked for. Above fit the page is drawn again at that zoom
    /// on a worker; the card keeps what it shows until then.
    pub(crate) fn pdf_zoom(&mut self, step: crate::pdf::ZoomStep) {
        let Some((image, _)) = self.pdf_on_card() else {
            return;
        };
        let target = crate::pdf::zoom_step(image.want_zoom, step);
        if target == image.want_zoom {
            return;
        }
        image.want_zoom = target;
        self.pdf_apply_zoom();
    }

    /// Make what the card shows follow the zoom asked for, for the page shown.
    fn pdf_apply_zoom(&mut self) {
        let Some((image, _)) = self.pdf_on_card() else {
            return;
        };
        let (page, want) = (image.page, image.want_zoom);
        let ready = want <= crate::pdf::ZOOM_FIT
            || image
                .zoomed
                .as_ref()
                .is_some_and(|(shown, zoom, _)| *shown == page && *zoom == want);
        if ready {
            let changed = image.zoom != want;
            image.zoom = want;
            if changed && launcher::is_open() {
                self.refresh_popup();
            }
            return;
        }
        // Only the page's picture and its zoom: the old drawing goes now, not when the next
        // one arrives (zoomed pages are big).
        if image
            .zoomed
            .as_ref()
            .is_some_and(|(shown, _, _)| *shown != page)
        {
            image.zoomed = None;
        }
        #[cfg(target_os = "macos")]
        if let Some(source) = image.pdf_source.clone() {
            let spawned = std::thread::Builder::new()
                .name("copycraft-pdf-zoom".into())
                .spawn(move || {
                    let drawn = crate::macos_pdf::render_page_zoomed(&source, page, true, want)
                        .and_then(|(mut png, _)| SecretBytes::new(std::mem::take(&mut *png)));
                    launcher::emit(UserEvent::PdfZoomRendered {
                        source,
                        index: page,
                        zoom: want,
                        page: drawn,
                    });
                });
            if let Err(e) = spawned {
                crate::diag::log(crate::diag::LogEvent::DroppedImageScanFailed(
                    crate::diag::ErrorKind::io(&e),
                ));
            }
        }
    }

    /// Page `index` is drawn at `zoom` on its thread.
    pub(crate) fn finish_pdf_zoom(
        &mut self,
        source: &SecretBytes,
        index: usize,
        zoom: u32,
        page: Option<SecretBytes>,
    ) {
        let Some(image) = self
            .opened
            .as_mut()
            .and_then(|file| file.image.as_mut())
            .filter(|image| {
                image
                    .pdf_source
                    .as_ref()
                    .is_some_and(|own| own.same_allocation(source))
            })
        else {
            return;
        };
        // Asked for again, or turned to another page, meanwhile: this drawing is not wanted.
        if image.want_zoom != zoom || image.page != index {
            return;
        }
        let Some(page) = page else {
            // It cannot be drawn (too big, say): back to what shows.
            image.want_zoom = image.shown_zoom();
            image.zoom = image.want_zoom;
            return;
        };
        image.zoomed = Some((index, zoom, page));
        image.zoom = zoom;
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    fn pdf_page_changed(&mut self) {
        // The zoom stays; the new page is drawn at it.
        self.pdf_apply_zoom();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }
}
