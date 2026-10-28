//! History actions, pins, sensitive clearing and recording copies.

#[cfg(target_os = "macos")]
use super::source::{LateCheck, late_check};
use super::*;

impl App {
    pub(crate) fn format_link_in_place(&mut self) -> bool {
        let Some(text) = self.source_text() else {
            return false;
        };
        let link = crate::link_card::is_link(&text);
        if !link {
            return false;
        }
        let formatted = crate::format::format_text(text.trim());
        if formatted != text.trim() {
            if self.opened.is_some() {
                if let Some(file) = self.opened.as_mut() {
                    file.text = Some(Zeroizing::new(formatted));
                }
                self.refresh_popup();
            } else if let Err(e) = self.write_own(&formatted) {
                crate::diag::log(crate::diag::LogEvent::FormatLinkFailed(
                    crate::diag::ErrorKind::of(&e),
                ));
            } else {
                self.record_own_copy(&formatted);
                self.refresh_popup();
            }
        }
        true
    }

    pub(crate) fn clear_history(&mut self) {
        self.forget_entries(false);
    }

    /// `full` drops every entry (Wipe's history half, lock, sleep, user switch). Otherwise
    /// only unpinned entries go; pins stay and diff picks whose entry remains stay too.
    pub(crate) fn forget_entries(&mut self, full: bool) {
        discard_preview_files();
        self.forget_decoded_picture();
        let view = ClipboardView::from_os();
        self.skip_record = view.text().map(|text| Zeroizing::new(text.to_string()));
        if view.is_image() {
            #[cfg(target_os = "macos")]
            {
                self.skip_image_change = Some(crate::macos_pasteboard::change_count());
            }
        }
        self.pin_refused = false;
        self.history_emptied = false;
        // A drop still waiting for its check stays out, like what was cleared.
        self.dropped_text_wait.clear();
        self.dropped_image_wait.clear();
        self.polled_image_wait = None;
        self.polled_unchecked.clear();
        if full {
            self.clear_diff();
            self.current_image = None;
            self.image_on_pasteboard = None;
            self.drop_card_picture();
            self.unchecked_source = None;
            self.cancel_image_job();
            self.image_note = None;
            self.image_steps = None;
            self.text_steps = None;
            self.ai_stop();
            self.text_generation = self.text_generation.wrapping_add(1);
            self.text_note = None;
            self.cancel_table_job();
            self.table_error = None;
            self.close_table_window();
            self.history.clear();
        } else {
            self.history.clear_unpinned();
            if self
                .current_image
                .as_ref()
                .is_some_and(|bytes| !self.history.holds_image(bytes))
            {
                self.current_image = None;
                self.image_on_pasteboard = None;
                self.cancel_image_job();
                self.image_note = None;
            }
            self.sync_diff_picks();
            self.refresh_table_window();
            if self.table_window.is_none() {
                self.cancel_table_job();
                self.table_error = None;
            }
        }
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    pub(crate) fn pin_shown(&mut self, pin: bool) {
        self.pin_at(self.history_cursor, pin);
    }

    /// Right-click on strip card `index` › Pin / Unpin: the other of what it is now.
    pub(crate) fn toggle_pin_at(&mut self, index: usize) {
        let pinned = self.history.is_pinned(index);
        self.pin_at(index, !pinned);
    }

    /// Pin (or unpin) history entry `index`. A full pin list says so on the meta line.
    fn pin_at(&mut self, index: usize, pin: bool) {
        let Some(id) = self.history.id_at(index) else {
            return;
        };
        if self.opened.is_some() {
            return;
        }
        if pin {
            self.pin_refused = !self.history.pin(id);
            if self.pin_refused {
                self.notify(
                    crate::notifications::Kind::PinListFull,
                    false,
                    Duration::ZERO,
                    None,
                );
            }
        } else {
            self.pin_refused = false;
            if !self.history.unpin(id) && index == self.history_cursor {
                self.history_cursor = 0;
            }
            self.settle_cursor();
            self.sync_diff_picks();
            self.refresh_table_window();
        }
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Take history entry `index` out: the ✕ on its strip card, ⌫, or ⋯ › Remove from History.
    /// Only history changes; the clipboard keeps what it holds. The card shows the copy that
    /// took its place, or the paste hint when history is empty now. ⌘Z puts it back
    /// ([`Self::restore_removed`]).
    pub(crate) fn remove_from_history(&mut self, index: usize) {
        if self.opened.is_some() {
            return;
        }
        let Some(id) = self.history.id_at(index) else {
            return;
        };
        // The newest entry is usually what the clipboard holds: the next poll must not record
        // it again.
        if index == 0 {
            if let Some(text) = self.history.get(0) {
                self.skip_record = Some(Zeroizing::new(text.to_string()));
                #[cfg(target_os = "macos")]
                {
                    self.removal_skip_change = Some(crate::macos_pasteboard::change_count());
                }
            } else {
                #[cfg(target_os = "macos")]
                {
                    self.skip_image_change = Some(crate::macos_pasteboard::change_count());
                }
            }
        }
        let shown = self.history_cursor;
        let Some(removed) = self.history.take_id(id) else {
            return;
        };
        self.pin_refused = false;
        let len = self.history.len();
        self.history_cursor = clipboard::cursor_after_remove(removed, shown, len);
        if let Some(cursor) = self.clipboard_cursor {
            self.clipboard_cursor =
                (len > 0).then(|| clipboard::cursor_after_remove(removed, cursor, len));
        }
        if removed == shown {
            self.browsing = self.history.id_at(self.history_cursor);
            self.reset_view();
        }
        self.history_emptied = self.history.is_empty();
        self.after_history_edit();
    }

    /// ⌘Z: the copy [`Self::remove_from_history`] took out goes back where it was, and the
    /// card shows it.
    pub(crate) fn restore_removed(&mut self) {
        if self.opened.is_some() {
            return;
        }
        let Some(index) = self.history.restore_removed() else {
            return;
        };
        if let Some(text) = self.history.get(index)
            && self
                .skip_record
                .as_ref()
                .is_some_and(|skipped| skipped.as_str() == text)
        {
            self.skip_record = None;
        }
        self.removal_skip_change = None;
        self.history_emptied = false;
        if let Some(cursor) = self.clipboard_cursor
            && cursor >= index
        {
            self.clipboard_cursor = Some(cursor + 1);
        }
        self.history_cursor = index;
        self.browsing = self.history.id_at(index);
        self.reset_view();
        self.after_history_edit();
    }

    pub(super) fn after_history_edit(&mut self) {
        self.sync_diff_picks();
        self.refresh_table_window();
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    pub(crate) fn clear_pins(&mut self) {
        let kept = self.history.id_at(self.history_cursor);
        self.history.clear_pins();
        self.pin_refused = false;
        self.history_cursor = kept.and_then(|id| self.history.index_of(id)).unwrap_or(0);
        self.clipboard_cursor = None;
        self.sync_diff_picks();
        self.refresh_table_window();
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    pub(crate) fn settle_cursor(&mut self) {
        if self.history.is_empty() {
            self.history_cursor = 0;
        } else if self.history_cursor >= self.history.len() {
            self.history_cursor = self.history.len() - 1;
        }
    }

    /// Forget history once [`Self::history_minutes`] have passed since the last copy. Returns
    /// when the event loop should look again.
    pub(crate) fn forget_history_when_due(&mut self, now: Instant) -> Option<Instant> {
        let due = history_due(
            self.last_copy,
            self.history_minutes,
            history_forgettable(&self.history),
        )?;
        if now < due {
            return Some(due);
        }
        self.last_copy = None;
        self.clear_history();
        // clear_history drops it too; explicit, since only it may have set the timer.
        self.history.forget_removed();
        crate::diag::log(crate::diag::LogEvent::HistoryForgottenIdle {
            minutes: self.history_minutes,
        });
        None
    }

    /// The screen locked, the Mac is going to sleep or another user took over: forget history
    /// whatever the setting says.
    pub(crate) fn session_ended(&mut self) {
        crate::capture::on_session_end();
        #[cfg(target_os = "macos")]
        crate::macos_settings::refresh_ignored_field();
        self.last_copy = None;
        // What Wipe forgets besides the pasteboard, also with history empty: the card's text,
        // file and picture, the steps, the AI run and the table window (pins included).
        self.forget_held_state();
        let had_entries = end_session_history(&mut self.history);
        // History (now or already empty) and the copy on the clipboard is not recorded again.
        self.forget_entries(true);
        if had_entries {
            crate::diag::log(crate::diag::LogEvent::HistoryForgottenLock);
        }
    }

    /// Empty the OS pasteboard. History and the open card stay.
    pub(crate) fn clear_clipboard(&mut self) -> anyhow::Result<()> {
        clipboard::clear_clipboard().map_err(anyhow::Error::msg)?;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        self.skip_record = Some(Zeroizing::new(String::new()));
        Ok(())
    }

    /// Everything Wipe and a lock forget besides history itself: the card's text, file and
    /// picture, what is waiting for a check, the steps (⌘Z), their notes and running jobs (a
    /// result still on its way is dropped), the AI run, the table window and what the card
    /// showed. The one routine for both, so neither can miss a part.
    pub(crate) fn forget_held_state(&mut self) {
        discard_preview_files();
        self.clear_diff();
        self.forget_decoded_picture();
        self.close_table_window();
        self.opened = None;
        self.dropped_text_wait.clear();
        self.dropped_image_wait.clear();
        self.polled_image_wait = None;
        self.polled_unchecked.clear();
        self.opened_table = crate::settings::preferred_table();
        self.card_text = None;
        self.browsing = None;
        self.loose_table = crate::settings::preferred_table();
        self.loose_text = None;
        self.cancel_image_job();
        self.image_note = None;
        self.image_steps = None;
        self.text_steps = None;
        self.text_note = None;
        // A Text ▾ result still on its way is for a copy that is gone.
        self.text_generation = self.text_generation.wrapping_add(1);
        self.forget_screenshot();
        self.ai_stop();
        self.cancel_table_job();
        self.table_error = None;
        self.full_card = None;
        self.current_image = None;
        self.image_on_pasteboard = None;
        self.drop_card_picture();
        self.card_picture_asked = None;
        self.unchecked_source = None;
        self.reset_view();
        self.image_scan = None;
        self.image_scan_change = None;
        self.image_scan_for = None;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        crate::memo::forget_all();
        launcher::wipe_shown();
    }

    /// Overwrite clipboard text and image bytes still held in this process,
    /// then empty the pasteboard.
    pub(crate) fn clear_secrets(&mut self) {
        self.forget_held_state();
        self.history.clear();
        self.pin_refused = false;
        self.history_emptied = false;
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.recorded_image_change = None;
        self.skip_image_change = None;
        self.skip_record = None;
        if let Err(e) = clipboard::clear_clipboard() {
            crate::diag::log(crate::diag::LogEvent::ClearClipboardFailed(
                crate::diag::ErrorKind::from_text(&e),
            ));
        }
        self.signature = ClipSig::default();
        self.polled_change = None;
        let view = ClipboardView::from_os();
        self.sync_tooltip(&view);
        self.refresh_status_menu();
        if launcher::is_open() {
            self.record_current(&view);
            let data = self.launch_data(&view);
            self.sync_card(data);
        }
    }

    pub(crate) fn should_record(&self, text: &str) -> bool {
        self.skip_record
            .as_ref()
            .is_none_or(|skipped| skipped.as_str() != text)
    }

    /// A card of the strip was clicked: show that copy. Only shown, like stepping.
    pub(crate) fn view_history(&mut self, index: usize) {
        if index == self.history_cursor || index >= self.history.len() {
            return;
        }
        if self.ai_hide_for_history() {
            return;
        }
        let previous = self.history_cursor;
        self.history_cursor = index;
        self.browsing = None;
        if self.show_history(index) {
            self.clipboard_cursor = None;
        } else {
            self.history_cursor = previous;
        }
    }

    pub(crate) fn step_history(&mut self, older: bool) {
        if self.ai_hide_for_history() {
            return;
        }
        let Some(next) = commands::step_history(self.history.len(), self.history_cursor, older)
        else {
            return;
        };
        // Needed in every build: the cursor goes back when the entry cannot be shown.
        let previous = self.history_cursor;
        self.history_cursor = next;
        self.browsing = None;
        if self.show_history(next) {
            self.clipboard_cursor = None;
        } else {
            self.history_cursor = previous;
        }
    }

    pub(crate) fn clip_signature(&self, view: &ClipboardView) -> ClipSig {
        #[cfg(target_os = "macos")]
        let image_change = crate::macos_pasteboard::change_count();
        #[cfg(not(target_os = "macos"))]
        let image_change = 0;
        ClipSig {
            text_hash: hash_text(view.text().unwrap_or("")),
            image: view.is_image(),
            image_change,
            history_len: self.history.len(),
        }
    }

    /// Show history entry `index` on the card. Only shown: the clipboard is left alone (no
    /// Copied toast) and a poll does not take the card back to the clipboard's item until a
    /// new copy arrives. Copy puts it on the clipboard. `false`: there is no entry there.
    pub(crate) fn show_history(&mut self, index: usize) -> bool {
        let Some(id) = self.history.id_at(index) else {
            return false;
        };
        self.browsing = Some(id);
        self.reset_view();
        self.opened = None;
        if launcher::is_open() {
            self.refresh_popup();
        }
        true
    }

    pub(crate) fn restore_history(&mut self, index: usize) -> anyhow::Result<()> {
        self.reset_view();
        self.clipboard_cursor = None;
        if let Some(bytes) = self.history.image(index) {
            #[cfg(target_os = "macos")]
            bytes
                .with(crate::macos_pasteboard::write_history_image)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            #[cfg(not(target_os = "macos"))]
            {
                let _ = bytes;
                return Ok(());
            }
            // Chosen from the menu: the entry moves to the front, as a new copy of it would.
            #[cfg(target_os = "macos")]
            {
                self.history.record_image_tracking(bytes.clone(), 0);
                self.image_on_pasteboard =
                    Some((crate::macos_pasteboard::change_count(), bytes.clone()));
                self.current_image = Some(bytes);
                self.history_cursor = 0;
                self.refresh_status_menu();
                self.opened = None;
                self.show_restored();
                return Ok(());
            }
        }
        let Some(text) = self
            .history
            .get(index)
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            return Ok(());
        };
        self.write_own(text.as_str())?;
        self.record_own_copy(text.as_str());
        self.opened = None;
        self.show_restored();
        Ok(())
    }

    /// Put text copycraft made or kept on the clipboard. Text labelled sensitive (credential,
    /// PII, financial) goes on with nspasteboard.org's Concealed type, and with that setting on
    /// it is cleared after [`SENSITIVE_CLEAR_AFTER`] unless something else was copied by then.
    /// A long copy whose labels are still being checked goes on plain; [`Self::labels_checked`]
    /// conceals it once they are in, if the pasteboard still holds it.
    pub(crate) fn write_own(&mut self, text: &str) -> anyhow::Result<()> {
        self.write_own_as(text, false)
    }

    /// [`Self::write_own`]; `conceal` marks it concealed whatever it holds (the hash of a
    /// sensitive card).
    pub(crate) fn write_own_as(&mut self, text: &str, conceal: bool) -> anyhow::Result<()> {
        let labeling = crate::sensitivity::labeling(text);
        let concealed = conceal
            || matches!(&labeling, crate::sensitivity::Labeling::Known(found) if !found.labels.is_empty());
        clipboard::write_clipboard(text, concealed).map_err(anyhow::Error::msg)?;
        self.sensitive_clear = None;
        self.conceal_when_checked = None;
        #[cfg(target_os = "macos")]
        {
            let change = crate::macos_pasteboard::change_count();
            if concealed {
                self.clear_sensitive_later(change);
            } else if labeling == crate::sensitivity::Labeling::Checking {
                self.conceal_when_checked = Some(change);
            }
            if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
                mac_ui::toast::show(
                    mtm,
                    crate::locale::t("copied_toast"),
                    crate::settings::announce_confirmations(),
                );
            }
        }
        Ok(())
    }

    /// Put a picture copycraft made on the clipboard (an Image ▾ step's), as [`Self::write_own`]
    /// does text: `concealed` (its recognised text is sensitive) marks it Concealed and, with
    /// that setting on, clears it after [`SENSITIVE_CLEAR_AFTER`]. The bytes are never logged.
    #[cfg(target_os = "macos")]
    pub(crate) fn write_own_image(&mut self, png: &[u8], concealed: bool) -> anyhow::Result<()> {
        crate::macos_pasteboard::write_image(png, concealed).map_err(anyhow::Error::msg)?;
        self.sensitive_clear = None;
        self.conceal_when_checked = None;
        if concealed {
            self.clear_sensitive_later(crate::macos_pasteboard::change_count());
        }
        if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
            mac_ui::toast::show(
                mtm,
                crate::locale::t("copied_toast"),
                crate::settings::announce_confirmations(),
            );
        }
        Ok(())
    }

    pub(crate) fn clear_sensitive_later(&mut self, change: isize) {
        if crate::settings::load().clear_sensitive {
            self.sensitive_clear = Some(SensitiveClear {
                change,
                due: Instant::now() + SENSITIVE_CLEAR_AFTER,
            });
        }
    }

    /// The checker has labels for a long copy: rebuild the open card (its meta line said
    /// "Checking…"), and conceal copycraft's own write that was waiting for them.
    pub(crate) fn labels_checked(&mut self) {
        self.secret_labels_checked();
        // A long dropped or pasted text waited for its labels too.
        self.dropped_text_checked();
        // A long copy from another app waits for its labels before it goes into history
        // ([`crate::sensitivity::Admission::Wait`]): record it now, unless it holds a credential.
        let view = ClipboardView::from_os();
        self.record_current(&view);
        #[cfg(target_os = "macos")]
        if let Some(change) = self.conceal_when_checked
            && crate::macos_pasteboard::change_count() == change
        {
            let view = ClipboardView::from_os();
            match view.text().map(crate::sensitivity::labeling) {
                Some(crate::sensitivity::Labeling::Checking) => {}
                Some(crate::sensitivity::Labeling::Known(found)) if !found.labels.is_empty() => {
                    self.conceal_when_checked = None;
                    crate::macos_pasteboard::add_concealed();
                    self.clear_sensitive_later(crate::macos_pasteboard::change_count());
                }
                _ => self.conceal_when_checked = None,
            }
        }
        if self
            .full_card
            .as_ref()
            .is_some_and(|full| commands::is_checking(&full.card))
        {
            self.full_card = None;
            self.refresh_popup();
            self.show_all();
            return;
        }
        self.refresh_popup();
    }

    /// Empty the pasteboard once the sensitive copy's minute is up, if it still holds that copy.
    /// Returns when the event loop should look again.
    pub(crate) fn clear_sensitive_when_due(&mut self, now: Instant) -> Option<Instant> {
        let pending = self.sensitive_clear?;
        if now < pending.due {
            return Some(pending.due);
        }
        self.sensitive_clear = None;
        #[cfg(not(target_os = "macos"))]
        let _ = pending.change;
        #[cfg(target_os = "macos")]
        if crate::macos_pasteboard::change_count() == pending.change {
            if let Err(e) = self.clear_clipboard() {
                crate::diag::log(crate::diag::LogEvent::ClearSensitiveFailed(
                    crate::diag::ErrorKind::of(&e),
                ));
            } else {
                crate::diag::log(crate::diag::LogEvent::SensitiveCleared);
            }
        }
        None
    }

    /// Text copycraft put on the clipboard (Copy, Format in place, a history entry chosen from
    /// the menu): the newest copy in history. The poller skips copycraft's own writes, so this
    /// is where they are recorded.
    /// Put copycraft's own copy into history. Returns `false` when history is held (recording
    /// paused, or an ignored app in front), when it holds a credential, or when it is long and
    /// its labels are still being checked (recorded later): the clipboard has it, history does
    /// not (yet).
    pub(crate) fn record_own_copy(&mut self, text: &str) -> bool {
        self.last_copy = Some(Instant::now());
        // CopyCraft's own copy of a token, key or URL password stays out of history too. A long
        // copy whose labels are not in yet waits for them, as a long drop does: `labels_checked`
        // records it then, unless it holds a credential.
        use crate::sensitivity::Record;
        match crate::sensitivity::record_when(crate::sensitivity::admission(
            &crate::sensitivity::labeling(text),
        )) {
            Record::Now => {}
            Record::AfterCheck => {
                self.dropped_text_wait
                    .push(Zeroizing::new(text.to_string()));
                return false;
            }
            Record::Never => return false,
        }
        if !self.history.record(text.to_string()) {
            return false;
        }
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
        true
    }

    /// The history row stays a type mark. The card shows the copy that was chosen.
    pub(crate) fn show_restored(&mut self) {
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::set_card_shown(true);
        let view = ClipboardView::from_os();
        self.record_current(&view);
        let mut data = self.launch_data(&view);
        self.attach_image(&mut data);
        self.attach_text(&mut data);
        if launcher::is_open() {
            self.sync_card(data);
        } else {
            self.reveal_card(data);
        }
    }

    /// A copied picture's bytes, made on a worker for pasteboard `change`. It goes into history
    /// at once, so the strip shows it without waiting; its text is recognised next, on a thread
    /// of its own, and a token, key or URL password in it takes it out again
    /// ([`Self::finish_polled_image`]). A newer copy meanwhile, a step that recorded it already,
    /// held history or a picture too large for history: the bytes are zeroized.
    #[cfg(target_os = "macos")]
    pub(crate) fn finish_image_record(&mut self, change: isize, bytes: Option<SecretBytes>) {
        let Some(bytes) = bytes else {
            return;
        };
        if self.polled_image_wait != Some(change)
            || crate::macos_pasteboard::change_count() != change
            || self.history.held()
            || bytes.byte_len() > clipboard::MAX_IMAGE_BYTES
        {
            bytes.zeroize();
            return;
        }
        self.polled_image_wait = None;
        // The card's own scan of this copy may be in already: its text decides now.
        let known = (self.image_scan_change == Some(change))
            .then(|| self.image_scan.as_ref().map(|scan| scan.ocr.clone()))
            .flatten();
        let check = late_check(known.as_ref().map(|ocr| ocr.as_deref()));
        if check == LateCheck::Retract {
            bytes.zeroize();
            self.skip_image_change = Some(change);
            crate::diag::log(crate::diag::LogEvent::CopySkipped(
                crate::diag::Skip::Refused,
            ));
            return;
        }
        let copy = bytes.clone();
        self.last_copy = Some(Instant::now());
        self.current_image = self.history.record_secret_image(bytes);
        let Some(recorded) = self.current_image.clone() else {
            copy.zeroize();
            return;
        };
        self.image_on_pasteboard = Some((change, recorded.clone()));
        if self.image_scan_change == Some(change)
            && let Some(scan) = self.image_scan.as_ref()
        {
            self.history.remember_image_scan(&recorded, scan);
        }
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.browsing = None;
        if cfg!(debug_assertions) {
            crate::diag::log(crate::diag::LogEvent::ImageAdmitted {
                checked: check == LateCheck::Admit,
            });
        }
        if check == LateCheck::Wait {
            self.polled_unchecked.push((change, recorded));
            let started = self.run_detached(
                "copycraft-image-check",
                move || {
                    // Copied out: the recognition reads its own bytes, history keeps `bytes`.
                    let pixels = Zeroizing::new(copy.with(<[u8]>::to_vec));
                    crate::macos_pasteboard::recognized_text_within(pixels)
                },
                move |app: &mut App, text| app.finish_polled_image(change, text),
            );
            if !started {
                // No recognition: kept, as when it fails or takes too long.
                self.finish_polled_image(change, None);
            }
        }
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The text recognised in the copied picture of pasteboard `change` is in. History took the
    /// picture already ([`Self::finish_image_record`]): a token, key or URL password in that
    /// text ([`crate::sensitivity::picture_admission`]) takes it out again, with its strip
    /// tile, also when a newer copy came meanwhile. The text is never logged.
    #[cfg(target_os = "macos")]
    pub(crate) fn finish_polled_image(&mut self, change: isize, text: Option<Zeroizing<String>>) {
        // A step put this picture into history before its text was in; or the poll did.
        self.check_unchecked_source(change, text.as_deref().map(String::as_str), true);
    }

    pub(crate) fn record_current(&mut self, view: &ClipboardView) {
        if matches!(view, ClipboardView::Held) {
            return;
        }
        // A symbol copy is plain text without copycraft's own pasteboard type. The change count
        // is what keeps it out of history, and out of a scan: callers that already read the
        // pasteboard still must not record it.
        #[cfg(target_os = "macos")]
        if crate::symbols::gate(
            crate::symbols::ignored_change(),
            crate::macos_pasteboard::change_count(),
        )
        .ignore()
        {
            return;
        }
        // CopyCraft's own write: what belongs in history was recorded when it was written (see
        // `record_own_copy`), and a history step stays where it is.
        #[cfg(target_os = "macos")]
        if crate::macos_pasteboard::current_marks().own {
            return;
        }
        #[cfg(target_os = "macos")]
        if view.is_image() {
            let change = crate::macos_pasteboard::change_count();
            if self.skip_image_change == Some(change) || self.recorded_image_change == Some(change)
            {
                return;
            }
            self.recorded_image_change = Some(change);
            // The pasteboard is read here, on the main thread (a privacy alert must not come
            // from another thread). Reading a copied file, hashing and wrapping the bytes run on
            // a worker; a quick picture is back within `QUICK` and recorded right here.
            if let Some(input) = crate::macos_pasteboard::history_image_input() {
                self.polled_image_wait = Some(change);
                let ready = self.start_work(
                    work::Lane::Image,
                    work::QUICK,
                    move || {
                        crate::macos_pasteboard::image_input_bytes(input).and_then(SecretBytes::new)
                    },
                    move |app: &mut App, bytes| app.finish_image_record(change, bytes),
                );
                if let Some(bytes) = ready {
                    self.finish_image_record(change, bytes);
                }
            }
            return;
        }
        self.current_image = None;
        // No picture on the clipboard now: the one read for the card goes.
        self.drop_card_picture();
        // The clipboard's text is already the newest entry (every poll and card refresh reads
        // it again): not a new copy, so the card stays on the entry being browsed.
        if let Some(text) = view.text()
            && self.should_record(text)
            && is_new_copy(self.history.get(0), text)
        {
            match admit_text(&mut self.history, text) {
                TextAdmit::Kept => {}
                // A token, key or URL password stays out of history. The card still shows the
                // clipboard; the copy is not looked at again until the clipboard changes.
                TextAdmit::Refused => {
                    self.skip_record = Some(Zeroizing::new(text.to_string()));
                    self.browsing = None;
                    crate::diag::log(crate::diag::LogEvent::CopySkipped(
                        crate::diag::Skip::Refused,
                    ));
                    return;
                }
                // A long copy still being checked: `labels_checked` looks again.
                TextAdmit::Waiting => return,
            }
            self.skip_record = None;
            self.last_copy = Some(Instant::now());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
            // A new copy: the card follows the clipboard again.
            self.browsing = None;
        }
    }
}
