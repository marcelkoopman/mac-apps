//! Opening, refreshing and filling the card.

use super::*;

/// A "Show all" text longer than this many lines is paged (when the setting is on).
const PAGE_MIN_LINES: usize = 200;

impl App {
    pub(crate) fn summon_popup(&mut self) {
        if !launcher::is_open() {
            self.reset_view();
            self.leave_opened();
            self.full_card = None;
        }
        let data = self.launch_for_popup();
        if launcher::is_open() {
            launcher::hide();
        } else {
            self.reveal_card(data);
        }
    }

    pub(crate) fn toggle_popup_under_icon(&mut self) {
        let now = Instant::now();
        if !icon_click_is_new(icon_click_at(), now) {
            return;
        }
        note_icon_click(now);
        if !launcher::is_open() {
            self.reset_view();
            self.leave_opened();
            self.full_card = None;
        }
        let data = self.launch_for_popup();
        if launcher::is_open() {
            launcher::hide();
        } else {
            let card = self.history_card(&data);
            launcher::open_under_icon(data, card, &self.tray);
        }
    }

    pub(crate) fn show_status_menu(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // performClick: sends the button action. That must not toggle the card as well.
            crate::macos_status_click::set_menu_action(true);
            self.tray.show_menu();
            crate::macos_status_click::set_menu_action(false);
        }
    }

    /// The language setting changed. Cached cards keep the old words, so they are dropped and
    /// the open surfaces are built again.
    pub(crate) fn language_changed(&mut self) {
        crate::locale::apply();
        self.history.forget_cards();
        self.full_card = None;
        self.refresh_status_menu();
        #[cfg(target_os = "macos")]
        {
            crate::macos_launcher::relabel();
            crate::macos_table_window::relabel();
            crate::macos_settings::reopen();
            crate::macos_about::reopen();
            crate::macos_welcome::reopen();
        }
        self.refresh_popup();
    }

    pub(crate) fn refresh_popup(&mut self) {
        self.refresh_table_window();
        if !launcher::is_open() {
            return;
        }
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::set_card_shown(true);
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        let data = self.current_launch_data();
        self.sync_popup(data);
    }

    /// Update the open card. Keeps the "Show all" card while it belongs to this item and view;
    /// anything else drops it, so the card is back to its preview.
    pub(crate) fn sync_popup(&mut self, mut data: LaunchData) {
        let key = commands::content_key(&data);
        let full = self
            .full_card
            .as_ref()
            .filter(|full| full.key == key && full.view == data.view);
        launcher::set_pages(key, full.and_then(|full| full.pages.clone()));
        let full = full.map(|full| full.card.clone());
        match full {
            Some(card) => {
                data.full = true;
                launcher::sync_with_card(data, card);
            }
            None => {
                self.full_card = None;
                let card = self.history_card(&data);
                launcher::sync_with_card(data, card);
            }
        }
    }

    /// The card for `data`. A copied text's preview card is built once per history entry and
    /// view, and kept with that entry (dropped and zeroized with it), so stepping back and
    /// forth through history does not detect, format and lay out the same copies again.
    pub(crate) fn history_card(&mut self, data: &LaunchData) -> commands::WorkCard {
        let slot = self.card_slot(data);
        if let Some(card) = slot.and_then(|index| self.history.card(index, data.view)) {
            return card.clone();
        }
        // Detection, formatting (JSON, XML, Markdown, code, HTML) and the dataframe views run
        // on a thread. A quick card is back within a frame and shows as before; a slow one
        // shows "Working…" first and is put on the card when it is ready (if the card still
        // shows that item and view).
        let key = commands::content_key(data);
        let view = data.view;
        let job = data.clone();
        let built = self.start_work(
            work::Lane::Card,
            work::QUICK,
            move || commands::work_card(&job),
            move |app, card| app.card_built(key, view, card),
        );
        match built {
            Some(card) => {
                self.keep_card(slot, view, &card);
                card
            }
            None => match &self.work_error {
                Some(error) => commands::failed_card(error),
                None => commands::building_card(),
            },
        }
    }

    fn keep_card(&mut self, slot: Option<usize>, view: CardView, card: &commands::WorkCard) {
        if let Some(index) = slot
            && !commands::is_checking(card)
        {
            self.history.remember_card(index, view, card.clone());
        }
    }

    /// A slow card is ready (main thread).
    fn card_built(&mut self, key: u64, view: CardView, mut card: commands::WorkCard) {
        if !launcher::is_open() {
            card.wipe();
            return;
        }
        let data = self.current_launch_data();
        let full = self
            .full_card
            .as_ref()
            .is_some_and(|full| full.key == key && full.view == view);
        if commands::content_key(&data) != key || data.view != view || full {
            card.wipe();
            return;
        }
        let slot = self.card_slot(&data);
        self.keep_card(slot, view, &card);
        launcher::sync_with_card(data, card);
    }

    /// The history entry whose kept cards `data`'s card belongs with, if it may be kept.
    fn card_slot(&self, data: &LaunchData) -> Option<usize> {
        let entry = data
            .subject_text
            .as_deref()
            .filter(|_| {
                self.opened.is_none()
                    && !data.full
                    && data.subject_kind == SubjectKind::Text
                    && data.source_name.is_none()
            })
            .and_then(|text| self.history.text_index(text, self.history_cursor));
        // A table version's card changes with the version, its job and its errors.
        let versioned = data.view == CardView::Dataframe
            && data
                .table
                .as_ref()
                .is_some_and(|table| table.version > 0 || table.working || table.error.is_some());
        // A decoded picture arrives after the card is first built.
        let picture = data.view == CardView::Decode
            && data
                .subject_text
                .as_deref()
                .is_some_and(crate::decode::picture_candidate);
        // A diff is not kept with the entry: it is not one view of that copy.
        let diff_view = data.view == CardView::Diff;
        entry.filter(|_| !versioned && !picture && !diff_view)
    }

    /// Open the card for `data` (the card is built as in [`App::history_card`]).
    pub(crate) fn reveal_card(&mut self, data: LaunchData) {
        let card = self.history_card(&data);
        // Under the menu bar icon, measured now (not where the card was last).
        launcher::open_under_icon(data, card, &self.tray);
    }

    /// Update the open card with `data`.
    pub(crate) fn sync_card(&mut self, data: LaunchData) {
        if !launcher::is_open() {
            return;
        }
        let card = self.history_card(&data);
        launcher::sync_with_card(data, card);
    }

    /// Build the whole-text card on a background thread; the spinner shows if it is slow.
    pub(crate) fn show_all(&mut self) {
        if self.loading_all.is_some() || !launcher::is_open() {
            return;
        }
        let mut data = self.current_launch_data();
        let key = commands::content_key(&data);
        let view = data.view;
        if self
            .full_card
            .as_ref()
            .is_some_and(|full| full.key == key && full.view == view)
        {
            return;
        }
        data.full = true;
        let spawned = std::thread::Builder::new()
            .name("copycraft-show-all".into())
            .spawn(move || {
                let card = commands::work_card(&data);
                // The paged view's line index is worked out here too, never on the main thread.
                let pages =
                    (data.settings.page_long_text && data.table.is_none() && !card.shows_image)
                        .then(|| crate::text_ops::pager::LineIndex::new(&card.excerpt))
                        .filter(|index| index.lines() > PAGE_MIN_LINES)
                        .map(std::sync::Arc::new);
                launcher::emit(UserEvent::FullCardReady(Box::new(launcher::FullCard {
                    key,
                    view,
                    card,
                    pages,
                })));
            });
        match spawned {
            Ok(_) => self.loading_all = Some(Background::now()),
            Err(e) => self.work_failed("show all", &e),
        }
    }

    pub(crate) fn finish_show_all(&mut self, full: Box<launcher::FullCard>) {
        if let Some(run) = self.loading_all.take() {
            eprintln!("copycraft: show all built in {} ms", run.elapsed_ms());
        }
        self.stop_spinner_when_idle();
        if !launcher::is_open() {
            return;
        }
        let data = self.current_launch_data();
        // The card moved on (another copy or view) while the thread ran.
        if commands::content_key(&data) != full.key || data.view != full.view {
            return;
        }
        self.full_card = Some(full);
        self.sync_popup(data);
    }

    /// Clipboard launch, unless the card is holding a chosen file.
    /// The card's default view (formatted where the copy opens formatted), not Raw.
    pub(crate) fn reset_view(&mut self) {
        self.card_view = CardView::Original;
    }

    pub(crate) fn launch_for_popup(&mut self) -> LaunchData {
        #[cfg(target_os = "macos")]
        self.card_opening();
        // A card opened again starts at the clipboard's item, not at an entry browsed before.
        if self.browsing.take().is_some() {
            self.history_cursor = 0;
        }
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        self.current_launch_data()
    }

    /// The card is about to show: with Default or Ask the copy is read now, in one pass (one
    /// alert). The first time, the card explains that alert instead and the copy is read on the
    /// next tick, with the explanation on screen.
    #[cfg(target_os = "macos")]
    pub(crate) fn card_opening(&mut self) {
        crate::macos_pasteboard::set_card_shown(false);
        let waiting = self.opened.is_none()
            && crate::paste_access::explain_first(
                crate::macos_pasteboard::read_strategy(),
                self.paste_explained,
            )
            && matches!(ClipboardView::from_os(), ClipboardView::Pending);
        if waiting {
            self.paste_explaining = true;
            self.paste_explained = true;
            crate::settings::set_paste_alert_explained();
        } else {
            crate::macos_pasteboard::set_card_shown(true);
        }
    }

    pub(crate) fn current_launch_data(&mut self) -> LaunchData {
        self.sync_diff_picks();
        let mut data = if self.opened.is_some() {
            self.launch_from_opened()
        } else if let Some(at) = self.browsed_index() {
            self.history_cursor = at;
            self.launch_from_history()
        } else {
            let view = ClipboardView::from_os();
            if matches!(view, ClipboardView::Held) {
                self.launch_from_history()
            } else {
                self.launch_data(&view)
            }
        };
        self.card_text = data
            .subject_text
            .as_deref()
            .filter(|_| data.subject_kind == SubjectKind::Text)
            .map(|text| Zeroizing::new(text.to_string()));
        self.attach_table(&mut data);
        self.attach_image(&mut data);
        self.attach_text(&mut data);
        if let Some(error) = &self.work_error {
            data.notice = Some(match data.notice.take() {
                Some(notice) => format!("{notice}  ·  {error}"),
                None => error.clone(),
            });
        }
        self.attach_decoded_picture(&mut data);
        self.fill_diff(&mut data);
        data
    }

    /// The Decode chip on a Base64 picture draws the picture: its bytes are decoded on a thread
    /// (the first time for this copy; the card describes the picture until they arrive) and then
    /// kept for the copy, so the card shows them like a dropped picture's.
    pub(crate) fn attach_decoded_picture(&mut self, data: &mut LaunchData) {
        if data.view != CardView::Decode || data.subject_kind != SubjectKind::Text {
            return;
        }
        let Some(text) = data
            .subject_text
            .as_deref()
            .filter(|text| crate::decode::picture_candidate(text))
        else {
            return;
        };
        let key = hash_text(text);
        if let Some(shown) = self
            .decoded_picture
            .as_ref()
            .filter(|shown| shown.key == key)
        {
            data.picture = shown.bytes.clone();
            return;
        }
        self.forget_decoded_picture();
        // Kept even if the thread cannot start: the card keeps describing the picture.
        self.decoded_picture = Some(DecodedPicture { key, bytes: None });
        let text = Zeroizing::new(text.to_string());
        let spawned = std::thread::Builder::new()
            .name("copycraft-decode-picture".into())
            .spawn(move || {
                let picture = decoded_picture_bytes(&text);
                launcher::emit(UserEvent::DecodedPicture { key, picture });
            });
        if let Err(e) = spawned {
            eprintln!("decode picture skipped: {e}");
        }
    }

    pub(crate) fn finish_decoded_picture(&mut self, key: u64, picture: Option<SecretBytes>) {
        let Some(shown) = self
            .decoded_picture
            .as_mut()
            .filter(|shown| shown.key == key)
        else {
            // The card moved on (another copy, Wipe) while the thread ran.
            if let Some(picture) = picture {
                picture.zeroize();
            }
            return;
        };
        let drawn = picture.is_some();
        shown.bytes = picture;
        if drawn && launcher::is_open() && self.card_view == CardView::Decode {
            self.refresh_popup();
        }
    }

    /// Overwrite the Base64 picture's decoded bytes and forget them.
    pub(crate) fn forget_decoded_picture(&mut self) {
        if let Some(bytes) = self.decoded_picture.take().and_then(|shown| shown.bytes) {
            bytes.zeroize();
        }
    }
}
