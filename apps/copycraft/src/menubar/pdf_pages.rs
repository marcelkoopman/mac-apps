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

    fn pdf_page_changed(&mut self) {
        // Edits of the picture belonged to the page they were made on.
        self.opened_image = ImageVersions::default();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }
}
