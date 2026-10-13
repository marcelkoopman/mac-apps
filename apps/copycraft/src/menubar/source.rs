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
        let base = self.clipboard_cursor.unwrap_or(self.history_cursor);
        let clipboard_at = if let Some(image) = opened.image.as_ref() {
            Some(
                self.history
                    .record_image_tracking(image.bytes.clone(), base),
            )
        } else if let Some(text) = opened.text.as_deref() {
            self.history.record_tracking(text.to_string(), base)
        } else {
            None
        };
        if let Some(clipboard_at) = clipboard_at {
            self.last_copy = Some(Instant::now());
            // The clipboard is not recorded again (back to the front) until it changes. A
            // copied picture is recorded once per pasteboard change anyway.
            if let Some(current) = ClipboardView::from_os().text() {
                self.skip_record = Some(Zeroizing::new(current.to_string()));
            }
            self.clipboard_cursor = Some(clipboard_at);
            self.history_cursor = 0;
            self.refresh_status_menu();
        }
        if let Some(image) = opened.image.as_ref() {
            self.scan_dropped_image(image.bytes.clone());
        }
        self.show_source(opened);
        launcher::order_front();
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
    pub(crate) fn scan_dropped_image(&self, image: SecretBytes) {
        #[cfg(target_os = "macos")]
        {
            let spawned = std::thread::Builder::new()
                .name("copycraft-drop-scan".into())
                .spawn(move || {
                    // Copied out first: the scan (text recognition) takes a while, and the card
                    // reads the same bytes to draw the picture.
                    let bytes = Zeroizing::new(image.with(<[u8]>::to_vec));
                    let scan = crate::macos_pasteboard::scan_image_bytes(&bytes);
                    launcher::emit(UserEvent::DroppedImageScanned { image, scan });
                });
            if let Err(e) = spawned {
                crate::diag::log(crate::diag::LogEvent::DroppedImageScanFailed(
                    crate::diag::ErrorKind::io(&e),
                ));
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = image;
    }

    pub(crate) fn finish_dropped_scan(
        &mut self,
        image: &SecretBytes,
        scan: Option<commands::ImageScan>,
    ) {
        // The card moved on (the clipboard, another drop, Wipe) while the thread ran.
        let Some(shown) = self
            .opened
            .as_mut()
            .and_then(|file| file.image.as_mut())
            .filter(|shown| shown.bytes.same_allocation(image))
        else {
            return;
        };
        shown.scan = scan;
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
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
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
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
        self.reset_view();
        self.refresh_popup();
    }

    pub(crate) fn launch_data(&mut self, view: &ClipboardView) -> LaunchData {
        if view.is_image() {
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
        data.image = None;
        data.image_scan = None;
        if let Some(image) = image {
            // Drawn like a copied picture, from its own bytes instead of the clipboard's.
            data.subject_kind = SubjectKind::Image;
            data.image = image.facts;
            data.image_scan = image.scan;
            data.picture = Some(image.bytes);
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
            notice: self
                .pin_refused
                .then(|| crate::locale::t("pin_list_full").to_string()),
            history_nav: commands::history_nav(self.history.len(), self.history_cursor),
            theme: appearance::load(),
            settings: crate::settings::load(),
            view: self.card_view,
            image_scan: self.image_scan.clone(),
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
