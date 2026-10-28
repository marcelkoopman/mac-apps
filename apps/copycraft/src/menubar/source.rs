//! Opened files, dropped content and the launch data they produce.

use super::*;

impl App {
    pub(crate) fn choose_file(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use mac_ui::objc2::MainThreadMarker;
            use mac_ui::objc2_app_kit::NSApplication;
            use mac_ui::panel;
            // The panel steals key. Remember who had it, and keep a revealed card or table
            // sharp until the panel is gone. The wheel runs while the panel is still loading.
            let previous = MainThreadMarker::new()
                .and_then(|mtm| NSApplication::sharedApplication(mtm).keyWindow());
            let _wait = crate::macos_launcher::PanelWait::show();
            let path = crate::macos_launcher::holding_reveal(|| {
                crate::macos_table_window::holding_reveal(crate::macos_open::choose_path)
            });
            if let Some(path) = path {
                // Layout during the panel may have covered the wheel. Put it back for the read.
                crate::macos_launcher::raise_panel_wait();
                // Same as a drop: history, the well, and a picture scan. The clipboard stays.
                self.open_path(path, false);
            } else if let Some(window) = previous.as_ref() {
                if let Some(mtm) = MainThreadMarker::new() {
                    panel::activate_app(mtm);
                }
                panel::bring_to_front(window);
            } else {
                launcher::order_front();
            }
        }
    }

    /// Read a dropped or chosen file on a thread (a large file, a picture), then show it. A
    /// promised file (`promised`) is removed after the read.
    pub(crate) fn open_path(&mut self, path: std::path::PathBuf, promised: bool) {
        let opened = self.start_work(
            work::Lane::File,
            work::QUICK,
            move || {
                let opened = crate::open_file::load_dropped(&path);
                if promised {
                    mac_ui::drop::discard_promised(&path);
                }
                opened
            },
            |app, opened| app.show_dropped(opened),
        );
        if let Some(opened) = opened {
            self.show_dropped(opened);
        }
    }

    /// A dropped or chosen file, text or picture: into history like a copy (newest first, the
    /// same limit, zeroized when it falls off or on Wipe), then onto the card. The clipboard is
    /// left as it is.
    pub(crate) fn show_dropped(&mut self, opened: crate::open_file::OpenedFile) {
        let opened = self.look_at_dropped_image(opened);
        // The clipboard's copy goes into history first, by the usual rules (paused, private,
        // a token or key, history off), if it is not there yet: the drop takes the card, not
        // its place in history.
        self.record_current(&ClipboardView::from_os());
        let base = self.clipboard_cursor.unwrap_or(self.history_cursor);
        let clipboard_at = if let Some(image) = opened.image.as_ref() {
            // A picture goes into history at once, so the strip shows it; one whose recognised
            // text shows a token or key goes out again ([`Self::finish_dropped_scan`]).
            let moved_to = self
                .history
                .record_image_tracking(image.bytes.clone(), base);
            if waits_for_recognition(image) && self.history.holds_image(&image.bytes) {
                self.dropped_image_wait.push(image.bytes.clone());
            }
            Some(moved_to)
        } else if let Some(text) = opened.text.as_deref() {
            // A file that holds a token, key or URL password is shown, not kept in history. A
            // long one is checked in the background first: `labels_checked` decides then.
            use crate::sensitivity::Record;
            match crate::sensitivity::record_when(crate::sensitivity::admission(
                &crate::sensitivity::labeling(text),
            )) {
                Record::Now => self.history.record_tracking(text.to_string(), base),
                Record::AfterCheck => {
                    self.dropped_text_wait
                        .push(Zeroizing::new(text.to_string()));
                    None
                }
                Record::Never => None,
            }
        } else {
            None
        };
        if let Some(clipboard_at) = clipboard_at {
            self.dropped_recorded(clipboard_at);
        }
        // A document's picture (Quick Look) has its text already; recognizing the picture would
        // only replace it.
        if let Some(image) = opened
            .image
            .as_ref()
            .filter(|image| image.pdf_text.is_none() || image.pdf_source.is_some())
        {
            self.scan_dropped_image(
                image.bytes.clone(),
                image.pdf_source.clone(),
                image.pdf_text.clone(),
            );
        }
        self.show_source(opened);
        launcher::order_front();
    }

    /// A drop is in history now, at index 0; the entry the card or the clipboard was on is at
    /// `moved_to`. On the card: the clipboard's entry is remembered for when the card goes back
    /// to it. Recorded after the card moved on (a late check): the card stays on its entry.
    pub(crate) fn dropped_recorded(&mut self, moved_to: usize) {
        self.last_copy = Some(Instant::now());
        // The clipboard is not recorded again (back to the front) until it changes, when
        // history has it. A copy not in history yet (one still being checked) is left to be
        // recorded. A copied picture is recorded once per pasteboard change anyway.
        if let Some(skip) = skip_after_drop(&self.history, ClipboardView::from_os().text()) {
            self.skip_record = Some(skip);
        }
        if self.opened.is_some() {
            self.clipboard_cursor = Some(moved_to);
            self.history_cursor = 0;
        } else {
            self.history_cursor = moved_to;
        }
        self.refresh_status_menu();
    }

    /// The checker has labels: the long dropped texts that waited for them go into history
    /// now, unless they hold a credential. Also when the card moved on: they were dropped.
    pub(crate) fn dropped_text_checked(&mut self) {
        use crate::sensitivity::Record;
        for text in std::mem::take(&mut self.dropped_text_wait) {
            match crate::sensitivity::record_when(crate::sensitivity::admission(
                &crate::sensitivity::labeling(&text),
            )) {
                Record::Now => {
                    let base = self.clipboard_cursor.unwrap_or(self.history_cursor);
                    if let Some(moved_to) = self.history.record_tracking(text.to_string(), base) {
                        self.dropped_recorded(moved_to);
                    }
                }
                Record::AfterCheck => self.dropped_text_wait.push(text),
                Record::Never => crate::diag::log(crate::diag::LogEvent::CopySkipped(
                    crate::diag::Skip::Refused,
                )),
            }
        }
    }

    /// ⋯ › New: the value goes on the clipboard and onto a new card, like text pasted into the
    /// card (so also with history off). It is the clipboard's copy too, so the card follows
    /// it. The value is never logged.
    pub(crate) fn generate(&mut self, generator: crate::generate::Generator) {
        self.new_value(generator.title(), Zeroizing::new(generator.make()));
    }

    /// A value made in CopyCraft (⋯ › New) goes on the clipboard and onto a new card titled
    /// `title`. Never logged.
    pub(crate) fn new_value(&mut self, title: &str, value: Zeroizing<String>) {
        if let Err(e) = self.write_own(&value) {
            crate::diag::log(crate::diag::LogEvent::TextStepFailed(
                crate::diag::ErrorKind::of(&e),
            ));
            return;
        }
        let newest = self.history.id_at(0);
        self.show_dropped(crate::open_file::from_text(title, value));
        // Recorded: the new entry at index 0 is also what the clipboard holds now.
        if self.history.id_at(0) != newest {
            self.clipboard_cursor = Some(0);
        }
    }

    /// A picture made in CopyCraft (Text ▾ › QR Code) goes on the clipboard and onto a new card
    /// titled `title`, like [`Self::new_value`]. Never logged.
    #[cfg(target_os = "macos")]
    pub(crate) fn new_image(&mut self, title: &str, png: Zeroizing<Vec<u8>>) {
        if let Err(e) = self.write_own_image(&png, false) {
            crate::diag::log(crate::diag::LogEvent::TextStepFailed(
                crate::diag::ErrorKind::of(&e),
            ));
            return;
        }
        let newest = self.history.id_at(0);
        self.show_dropped(crate::open_file::from_image_data(title, png));
        if self.history.id_at(0) != newest {
            self.clipboard_cursor = Some(0);
        }
    }

    /// ⌘V in the card: what the clipboard held goes in like a drop. Text passes
    /// [`crate::sensitivity::admission`] before it is kept in history (in memory only, as a
    /// drop); the card and all its tools work on it. Also with history off: the user asked.
    pub(crate) fn show_pasted(&mut self, pasted: crate::history_mode::Pasted) {
        use crate::history_mode::Pasted;
        let opened = match pasted {
            Pasted::Text(text) => crate::open_file::from_text(pasted_text(), text),
            Pasted::Image(bytes) => crate::open_file::from_image_data(pasted_image(), bytes),
            Pasted::Hidden => crate::open_file::noted(
                clipboard::hidden_content().to_string(),
                crate::locale::t("private_copy_note"),
            ),
            Pasted::Nothing => crate::open_file::noted(
                pasted_text().to_string(),
                crate::locale::t("paste_nothing"),
            ),
        };
        self.show_dropped(opened);
    }

    /// "Keep clipboard history" changed. Off: what was read from the pasteboard is wiped from
    /// the reader's cache, Pause and Ignore Next Copy are reset (they are greyed out), and the
    /// poller looks again from scratch when history is turned on.
    pub(crate) fn clipboard_history_changed(&mut self) {
        let on = crate::settings::clipboard_history();
        self.polled_change = None;
        self.poll_again = false;
        self.paste_explaining = false;
        if !on {
            crate::capture::set_paused(false);
            crate::capture::set_ignore_next(false);
            #[cfg(target_os = "macos")]
            crate::macos_pasteboard::zeroize_caches();
            self.drop_card_picture();
            self.card_picture_asked = None;
            self.sync_tooltip(&ClipboardView::Empty);
        }
        self.refresh_status_menu();
        #[cfg(target_os = "macos")]
        {
            crate::macos_settings::refresh_history_controls();
            crate::macos_welcome::refresh_history_box();
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The format and size of a dropped picture, which the image card shows. One the system
    /// cannot read gets a note instead, and stays out of history.
    pub(crate) fn look_at_dropped_image(
        &self,
        mut opened: crate::open_file::OpenedFile,
    ) -> crate::open_file::OpenedFile {
        let Some(image) = opened.image.as_mut() else {
            return opened;
        };
        #[cfg(target_os = "macos")]
        let facts = image.bytes.with(crate::macos_pasteboard::image_bytes_facts);
        #[cfg(not(target_os = "macos"))]
        let facts = None;
        if facts.is_none() {
            return crate::open_file::noted(
                std::mem::take(&mut opened.name),
                crate::open_file::unreadable_image(),
            );
        }
        image.facts = facts;
        opened
    }

    /// Info text, data URL, text and barcodes of a dropped picture, on a background thread
    /// like a copied one's ([`UserEvent::DroppedImageScanned`]).
    pub(crate) fn scan_dropped_image(
        &mut self,
        image: SecretBytes,
        pdf: Option<SecretBytes>,
        pdf_text: Option<Zeroizing<String>>,
    ) {
        #[cfg(target_os = "macos")]
        {
            // Only a PDF's recognition can take long enough for a notification.
            self.notices.pdf_scan_started = pdf.is_some().then(Instant::now);
            let id = image.allocation_id();
            let spawned = std::thread::Builder::new()
                .name("copycraft-drop-scan".into())
                .spawn(move || {
                    // A scanned PDF: its pages are drawn and recognized (the image is page 1).
                    if let Some(pdf) = pdf {
                        let scan = crate::macos_pdf::scan(&pdf, pdf_text);
                        launcher::emit(UserEvent::DroppedImageScanned { image, scan });
                        return;
                    }
                    // Copied out first: the scan (text recognition) takes a while, and the card
                    // reads the same bytes to draw the picture. A scan that takes too long
                    // counts as failed: the picture then goes into history without it.
                    let bytes = Zeroizing::new(image.with(<[u8]>::to_vec));
                    let scan = crate::macos_pasteboard::scan_image_bytes_within(bytes);
                    launcher::emit(UserEvent::DroppedImageScanned { image, scan });
                });
            if let Err(e) = spawned {
                crate::diag::log(crate::diag::LogEvent::DroppedImageScanFailed(
                    crate::diag::ErrorKind::io(&e),
                ));
                // No scan: a picture waiting for it goes into history as before.
                if let Some(waiting) = self
                    .dropped_image_wait
                    .iter()
                    .find(|waiting| waiting.allocation_id() == id)
                {
                    launcher::emit(UserEvent::DroppedImageScanned {
                        image: waiting.clone(),
                        scan: None,
                    });
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (image, pdf, pdf_text);
    }

    pub(crate) fn finish_dropped_scan(
        &mut self,
        image: &SecretBytes,
        scan: Option<commands::ImageScan>,
    ) {
        // History took the picture before its text was in: a token, key or URL password in
        // that text ([`crate::sensitivity::picture_admission`]) takes it out again, with its
        // strip tile, also when the card moved on by now. Never logged.
        let waited = self
            .dropped_image_wait
            .iter()
            .position(|waiting| waiting.same_allocation(image));
        if let Some(at) = waited {
            self.dropped_image_wait.remove(at);
            let check = late_check(Some(scan.as_ref().and_then(|scan| scan.ocr.as_deref())));
            if check == LateCheck::Retract && self.history.holds_image(image) {
                self.retract_image(None, image);
            }
        }
        // The card moved on (the clipboard, another drop, Wipe) while the thread ran.
        let Some(shown) = self
            .opened
            .as_mut()
            .and_then(|file| file.image.as_mut())
            .filter(|shown| shown.bytes.same_allocation(image))
        else {
            // Kept with its entry, so the card has it when it shows the entry.
            if let Some(mut scan) = scan {
                if self.history.holds_image(image) {
                    self.history.remember_image_scan(image, &scan);
                }
                scan.wipe();
            }
            if launcher::is_open() {
                self.refresh_popup();
            }
            return;
        };
        // A PDF keeps the text of its text layer when the scan fails.
        let scanned = scan.is_some();
        if scan.is_some() || shown.pdf_text.is_none() {
            shown.scan = scan;
        }
        // A slow recognition of a PDF: a notification when the card is hidden (a failed scan
        // says nothing).
        let is_pdf = shown.pdf_source.is_some();
        if let Some(started) = self.notices.pdf_scan_started.take()
            && is_pdf
        {
            self.notify(
                crate::notifications::Kind::OcrDone,
                !scanned,
                started.elapsed(),
                None,
            );
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The card follows the clipboard again. After a drop, the history position goes back to
    /// the clipboard's entry.
    pub(crate) fn leave_opened(&mut self) {
        if self
            .table_window
            .as_ref()
            .is_some_and(|window| window.opened)
        {
            self.close_table_window();
        }
        self.opened = None;
        self.opened_superseded = false;
        self.opened_table = crate::settings::preferred_table();
        if let Some(cursor) = self.clipboard_cursor.take() {
            self.history_cursor = cursor;
        }
    }

    /// Show `opened` on the card instead of the clipboard, from its original view. Every way a
    /// file gets onto the card goes through here, and the card labels and blurs it like the
    /// clipboard. A drop and Kies bestand are recorded in history first ([`Self::show_dropped`]).
    pub(crate) fn show_source(&mut self, opened: crate::open_file::OpenedFile) {
        if self
            .table_window
            .as_ref()
            .is_some_and(|window| window.opened)
        {
            self.close_table_window();
        }
        self.opened = Some(opened);
        self.opened_superseded = false;
        self.opened_table = crate::settings::preferred_table();
        self.reset_view();
        self.refresh_popup();
    }

    pub(crate) fn launch_data(&mut self, view: &ClipboardView) -> LaunchData {
        if view.is_image() {
            // Image ▾ first: it waits for neither history nor the scan's text recognition.
            self.ensure_card_picture();
            self.ensure_image_scan();
        }
        let (subject_kind, subject_text) = match view {
            ClipboardView::Empty => (SubjectKind::Empty, None),
            ClipboardView::NoText => (SubjectKind::NoText, None),
            ClipboardView::Denied => (SubjectKind::Denied, None),
            ClipboardView::Pending if self.paste_explaining => (SubjectKind::PasteAsk, None),
            ClipboardView::Pending => (SubjectKind::Pending, None),
            ClipboardView::Hidden => (SubjectKind::Hidden, None),
            ClipboardView::Image => (SubjectKind::Image, None),
            ClipboardView::Held => (SubjectKind::Empty, None),
            ClipboardView::Text(text) => {
                self.card_view = commands::presented_view(text.as_str(), self.card_view);
                (SubjectKind::Text, Some(text.as_str().to_string()))
            }
        };
        let mut data = self.launch_shell();
        data.subject_kind = subject_kind;
        data.subject_text = subject_text;
        data.image = image_facts(view);
        data
    }

    pub(crate) fn launch_from_opened(&mut self) -> LaunchData {
        let name = self
            .opened
            .as_ref()
            .map(|file| file.name.clone())
            .unwrap_or_else(|| crate::locale::t("title_file").to_string());
        let text = self.opened_text();
        let note = self.opened.as_ref().and_then(|file| file.note.clone());
        let image = self.opened.as_ref().and_then(|file| file.image.clone());
        if let Some(text) = text.as_deref() {
            self.card_view = commands::presented_view(text, self.card_view);
        }
        let mut data = self.launch_shell();
        data.source_name = Some(name);
        data.pdf = self.opened.as_ref().and_then(|file| file.pdf);
        data.pdf_page = image.as_ref().map_or(0, |image| image.page);
        if let Some(image) = image.as_ref() {
            data.pdf_zoom = image.shown_zoom();
            data.pdf_zoom_picture = image.zoomed_bytes();
        }
        data.image = None;
        data.image_scan = None;
        data.document_preview = image
            .as_ref()
            .is_some_and(|image| image.pdf_text.is_some() && image.pdf_source.is_none());
        if let Some(image) = image {
            // Drawn like a copied picture, from its own bytes instead of the clipboard's.
            data.subject_kind = SubjectKind::Image;
            data.picture = Some(image.shown_bytes());
            data.image = image.facts;
            data.image_scan = image.scan;
        } else if let Some(text) = text {
            data.subject_kind = SubjectKind::Text;
            data.subject_text = Some(text);
        } else {
            data.subject_kind = SubjectKind::NoText;
            data.source_note =
                Some(note.unwrap_or_else(|| crate::open_file::unreadable().to_string()));
        }
        data
    }

    /// The history strip's small cards: a few short lines of text, or a light picture. Empty with
    /// fewer than two copies or when ⌘Y turned the strip off.
    fn strip_thumbs(&self) -> Vec<commands::Thumb> {
        if !commands::strip_shown(self.history.len(), commands::strip_turned_off()) {
            return Vec::new();
        }
        commands::strip_range(self.history.len(), self.history_cursor)
            .map(|index| {
                let picture = self.history.image(index).filter(|bytes| {
                    let _ = bytes;
                    self.history
                        .byte_len(index)
                        .is_some_and(|len| len <= commands::THUMB_PICTURE_MAX_BYTES)
                });
                // A link: the Link card's symbol and the domain, not `://` and the URL's start.
                let link = (self.history.kind(index) == Some(crate::format::FormatKind::Url))
                    .then(|| self.history.get(index).and_then(commands::link_thumb))
                    .flatten();
                let (mark, symbol, lines) = match link {
                    Some((symbol, lines)) => (
                        crate::locale::t("title_link").to_string(),
                        Some(symbol),
                        lines,
                    ),
                    None => (
                        self.history.mark(index).unwrap_or("").to_string(),
                        None,
                        self.history
                            .get(index)
                            .map(commands::thumb_lines)
                            .unwrap_or_default(),
                    ),
                };
                commands::Thumb {
                    index,
                    id: self.history.id_at(index),
                    // The pin is a symbol on the card ([`commands::pin_frame`]), not a mark.
                    mark,
                    symbol,
                    pinned: self.history.is_pinned(index),
                    lines,
                    picture,
                    side: self
                        .history
                        .id_at(index)
                        .and_then(|id| self.diff_sel.label(id)),
                    kind: self
                        .history
                        .kind(index)
                        .map(commands::thumb_kind_name)
                        .unwrap_or(""),
                    copied: self.history.copied_at(index),
                    menu: commands::thumb_menu(
                        index,
                        self.history.id_at(index),
                        self.history.is_pinned(index),
                        self.history
                            .id_at(index)
                            .is_some_and(|id| self.history.is_text_id(id)),
                        self.diff_sel.ids(),
                    ),
                }
            })
            .collect()
    }

    pub(crate) fn launch_shell(&self) -> LaunchData {
        let history = self
            .history
            .labels()
            .into_iter()
            .map(|(index, _)| Hist {
                index,
                title: history_title(&self.history, index),
                mark: self.history.mark(index).unwrap_or("").to_string(),
                id: self.history.id_at(index).unwrap_or(0),
                text: self.history.get(index).is_some(),
                search: self.history.search_text(index),
                pinned: self.history.is_pinned(index),
                snippet: self.history.snippet_name(index).map(str::to_string),
            })
            .collect();
        LaunchData {
            subject_kind: SubjectKind::Empty,
            subject_text: None,
            image: None,
            history,
            can_clear_history: self.history.has_unpinned(),
            can_clear_pins: self.history.pin_count() > 0,
            paused: crate::capture::paused(),
            ignore_next: crate::capture::ignore_next(),
            can_ignore_app: crate::capture::last_foreign().is_some(),
            can_screenshot: crate::screenshot::available(),
            notice: self
                .pin_refused
                .then(|| crate::locale::t("pin_list_full").to_string()),
            history_nav: commands::history_nav(self.history.len(), self.history_cursor),
            strip: self.strip_thumbs(),
            strip_cursor: self.history_cursor,
            can_restore_history: self.history.can_restore(),
            strip_hint: commands::strip_hint(
                commands::strip_shown(self.history.len(), commands::strip_turned_off()),
                self.strip_hint_row,
                self.strip_compare_used,
            ),
            theme: appearance::load(),
            settings: crate::settings::load(),
            view: self.card_view,
            image_scan: self.image_scan.clone(),
            pdf: None,
            pdf_page: 0,
            pdf_zoom: crate::pdf::ZOOM_FIT,
            pdf_zoom_picture: None,
            document_preview: false,
            source_name: None,
            source_note: None,
            full: false,
            picture: None,
            table: None,
            image_edit: None,
            text_edit: None,
            shown_id: None,
            shown_mark: None,
            diff_picks: Vec::new(),
            diff: None,
        }
    }

    /// The entry on screen, when the pasteboard itself is not being read.
    pub(crate) fn launch_from_history(&mut self) -> LaunchData {
        let mut data = self.launch_shell();
        if let Some(text) = self.history.get(self.history_cursor) {
            let text = text.to_string();
            self.card_view = commands::presented_view(&text, self.card_view);
            data.view = self.card_view;
            data.subject_kind = SubjectKind::Text;
            data.subject_text = Some(text);
        } else if let Some(bytes) = self.history.image(self.history_cursor) {
            data.subject_kind = SubjectKind::Image;
            data.picture = Some(bytes.clone());
            data.image_scan = self.history.image_scan(&bytes);
            #[cfg(target_os = "macos")]
            {
                data.image = bytes.with(crate::macos_pasteboard::image_bytes_facts);
            }
        } else {
            data.subject_kind = SubjectKind::Empty;
        }
        data
    }

    /// Text the card is showing: the chosen file, or the clipboard.
    pub(crate) fn opened_text(&self) -> Option<String> {
        self.opened
            .as_ref()
            .and_then(|file| file.text.as_ref().map(|value| value.as_str().to_string()))
    }

    /// Index of the version the card browses (see [`App::browsing`]).
    pub(crate) fn browsed_index(&self) -> Option<usize> {
        browsed_entry(
            self.browsing.and_then(|id| self.history.index_of(id)),
            self.opened.is_some(),
        )
    }

    pub(crate) fn source_text(&self) -> Option<String> {
        if self.opened.is_some() {
            self.opened_text()
        } else if let Some(at) = self.browsed_index() {
            self.history.get(at).map(str::to_string)
        } else if self.pasteboard_suppressed() {
            self.history.get(self.history_cursor).map(str::to_string)
        } else {
            ClipboardView::from_os().text().map(str::to_string)
        }
    }

    pub(crate) fn ensure_image_scan(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.pasteboard_suppressed() {
                if let Some(bytes) = self.history.image(self.history_cursor) {
                    self.image_scan = self.history.image_scan(&bytes);
                }
                return;
            }
            let change = crate::macos_pasteboard::change_count();
            if self.image_scan_change == Some(change) || self.image_scan_for == Some(change) {
                return;
            }
            self.image_scan = None;
            self.image_scan_change = None;
            if let Some(scan) = self
                .history_image_on_pasteboard(change)
                .and_then(|bytes| self.history.image_scan(&bytes))
            {
                self.image_scan_change = Some(change);
                self.image_scan = Some(scan);
                return;
            }
            self.image_scan_for = Some(change);
            // The picture is fetched here, on the main thread (a pasteboard read can show the
            // privacy alert, which is modal); the thread only decodes and scans it.
            let input = crate::macos_pasteboard::card_image_input();
            std::thread::spawn(move || {
                let scan = input.and_then(crate::macos_pasteboard::scan_card_input);
                launcher::emit(UserEvent::ImageScanned { change, scan });
            });
        }
    }

    pub(crate) fn finish_image_scan(&mut self, change: isize, scan: Option<commands::ImageScan>) {
        // Also when a step's copy replaced this picture meanwhile.
        #[cfg(target_os = "macos")]
        self.check_unchecked_source(
            change,
            scan.as_ref().and_then(|scan| scan.ocr.as_deref()),
            false,
        );
        #[cfg(target_os = "macos")]
        let current = crate::macos_pasteboard::change_count();
        #[cfg(not(target_os = "macos"))]
        let current = change;
        if change != current {
            return;
        }
        self.image_scan_for = None;
        self.image_scan_change = Some(change);
        if let (Some(bytes), Some(scan)) = (self.history_image_on_pasteboard(change), scan.as_ref())
        {
            self.history.remember_image_scan(&bytes, scan);
        }
        self.image_scan = scan;
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The history picture on the pasteboard at `change`, if copycraft knows which one it is.
    pub(crate) fn history_image_on_pasteboard(&self, change: isize) -> Option<SecretBytes> {
        self.image_on_pasteboard
            .as_ref()
            .filter(|(at, _)| *at == change)
            .map(|(_, bytes)| bytes.clone())
    }
}

/// What a drop leaves the clipboard's text `current` as: skipped (not moved back to the front
/// by the next poll) only when history already has it. A copy not in history yet (still being
/// checked, or never read) must still be recorded: a skip would lose it for good. Pure.
pub(super) fn skip_after_drop(
    history: &clipboard::ClipboardHistory,
    current: Option<&str>,
) -> Option<Zeroizing<String>> {
    current
        .filter(|text| history.holds_text(text))
        .map(|text| Zeroizing::new(text.to_string()))
}

/// What the recognised text of a picture history took before it was checked says, `known`
/// being that text once a scan is in (`None` while none is). Pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LateCheck {
    /// No text yet: it stays in history until its recognition is in.
    Wait,
    /// Checked: it stays.
    Admit,
    /// A token, key or URL password: out of history again.
    Retract,
}

pub(crate) fn late_check(known: Option<Option<&str>>) -> LateCheck {
    match known {
        None => LateCheck::Wait,
        Some(text) => {
            if crate::sensitivity::picture_admission(text) == crate::sensitivity::Admission::Keep {
                LateCheck::Admit
            } else {
                LateCheck::Retract
            }
        }
    }
}

/// A dropped, pasted or chosen picture that goes into history at once and is checked after its
/// text recognition ([`App::finish_dropped_scan`]), which can take it out again. A document's
/// picture (Quick Look) is not recognised, and a PDF's first page needs no check; nor does a
/// picture too large for history.
pub(crate) fn waits_for_recognition(image: &crate::open_file::OpenedImage) -> bool {
    cfg!(target_os = "macos")
        && image.pdf_source.is_none()
        && image.pdf_text.is_none()
        && image.bytes.byte_len() <= clipboard::MAX_IMAGE_BYTES
}
