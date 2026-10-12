//! History actions, pins, sensitive clearing and recording copies.

use super::*;

impl App {
    pub(crate) fn format_link_in_place(&mut self) -> bool {
        let Some(text) = self.source_text() else {
            return false;
        };
        let link = crate::page_preview::page_url(&text).is_some()
            || crate::youtube::video_id(&text).is_some();
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
                eprintln!("format link failed: {e}");
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
        if full {
            self.clear_diff();
            self.current_image = None;
            self.image_on_pasteboard = None;
            self.cancel_image_job();
            self.image_note = None;
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
        let Some(id) = self.history.id_at(self.history_cursor) else {
            return;
        };
        if self.opened.is_some() {
            return;
        }
        if pin {
            self.pin_refused = !self.history.pin(id);
        } else {
            self.pin_refused = false;
            if !self.history.unpin(id) {
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

    pub(crate) fn delete_shown(&mut self) {
        if self.opened.is_some() {
            return;
        }
        let Some(id) = self.history.id_at(self.history_cursor) else {
            return;
        };
        let Some(removed) = self.history.remove_id(id) else {
            return;
        };
        self.pin_refused = false;
        let len = self.history.len();
        self.history_cursor = clipboard::cursor_after_remove(removed, self.history_cursor, len);
        if let Some(cursor) = self.clipboard_cursor {
            self.clipboard_cursor =
                (len > 0).then(|| clipboard::cursor_after_remove(removed, cursor, len));
        }
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
            self.history.has_unpinned(),
        )?;
        if now < due {
            return Some(due);
        }
        self.last_copy = None;
        self.clear_history();
        eprintln!(
            "copycraft: history forgotten {} min after the last copy",
            self.history_minutes
        );
        None
    }

    /// The screen locked, the Mac is going to sleep or another user took over: forget history
    /// whatever the setting says.
    pub(crate) fn session_ended(&mut self) {
        crate::capture::on_session_end();
        #[cfg(target_os = "macos")]
        crate::macos_settings::refresh_ignored_field();
        discard_preview_files();
        self.last_copy = None;
        self.full_card = None;
        // Closed and wiped whatever history holds (a chosen file's window too), pins included.
        self.close_table_window();
        self.clear_diff();
        if !self.history.is_empty() {
            self.forget_entries(true);
            eprintln!("copycraft: history forgotten (lock, sleep or user switch)");
        } else if launcher::is_open() {
            self.refresh_popup();
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

    /// Overwrite clipboard text and image bytes still held in this process,
    /// then empty the pasteboard.
    pub(crate) fn clear_secrets(&mut self) {
        discard_preview_files();
        self.clear_diff();
        self.forget_decoded_picture();
        self.close_table_window();
        self.opened = None;
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
        self.cancel_image_job();
        self.image_note = None;
        self.cancel_table_job();
        self.table_error = None;
        self.full_card = None;
        self.history.clear();
        self.pin_refused = false;
        self.current_image = None;
        self.image_on_pasteboard = None;
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.recorded_image_change = None;
        self.skip_image_change = None;
        self.card_view = CardView::Original;
        self.image_scan = None;
        self.image_scan_change = None;
        self.image_scan_for = None;
        self.skip_record = None;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        crate::memo::forget_all();
        launcher::wipe_shown();
        if let Err(e) = clipboard::clear_clipboard() {
            eprintln!("clear clipboard failed: {e}");
        }
        self.signature = ClipSig::default();
        self.polled_change = None;
        let view = ClipboardView::from_os();
        self.sync_tooltip(&view);
        self.refresh_status_menu();
        if launcher::is_open() {
            self.record_current(&view);
            launcher::sync(self.launch_data(&view));
        }
    }

    pub(crate) fn should_record(&self, text: &str) -> bool {
        self.skip_record
            .as_ref()
            .is_none_or(|skipped| skipped.as_str() != text)
    }

    pub(crate) fn step_history(&mut self, older: bool) {
        let Some(next) = commands::step_history(self.history.len(), self.history_cursor, older)
        else {
            return;
        };
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let previous = self.history_cursor;
        self.history_cursor = next;
        let presented = self.present_history(next).unwrap_or_else(|e| {
            eprintln!("restore history failed: {e:#}");
            false
        });
        if presented {
            // The entry is on the clipboard now, so the card follows the clipboard there.
            self.clipboard_cursor = None;
            self.note_own_write();
        } else {
            self.history_cursor = previous;
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "copycraft: history step to {} in {} µs",
            self.history_cursor,
            started.elapsed().as_micros()
        );
    }

    /// A step put a history entry on the pasteboard, and the card already shows it. Note the
    /// pasteboard as seen, so the next pass of the event loop does not take that change for a
    /// new copy: that rebuilt the whole card a second time, rebuilt the menu bar menu (history
    /// did not change) and blinked the icon. Only the tooltip follows the entry.
    pub(crate) fn note_own_write(&mut self) {
        let view = ClipboardView::from_os();
        self.record_current(&view);
        self.signature = self.clip_signature(&view);
        // As `note_clipboard` does for a change: a scan of another picture no longer applies.
        if self
            .image_scan_change
            .is_some_and(|seen| seen != self.signature.image_change)
        {
            self.image_scan_change = None;
            self.image_scan = None;
        }
        self.sync_tooltip(&view);
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

    // Put a history entry on the clipboard without moving it to the front.
    // `Ok(false)`: there is no entry to show at `index`.
    pub(crate) fn present_history(&mut self, index: usize) -> anyhow::Result<bool> {
        if self.history.image(index).is_some() {
            return self.present_history_image(index);
        }
        let Some(text) = self
            .history
            .get(index)
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            return Ok(false);
        };
        // The write carries copycraft's own pasteboard type, so it is not recorded: that would
        // move this entry to the front, and the other arrow could no longer walk back.
        self.card_view = CardView::Original;
        self.write_own(text.as_str())?;
        self.opened = None;
        if launcher::is_open() {
            self.refresh_popup();
        }
        Ok(true)
    }

    pub(crate) fn present_history_image(&mut self, index: usize) -> anyhow::Result<bool> {
        let Some(bytes) = self.history.image(index) else {
            return Ok(false);
        };
        #[cfg(target_os = "macos")]
        {
            bytes
                .with(crate::macos_pasteboard::write_history_image)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            self.image_on_pasteboard = Some((crate::macos_pasteboard::change_count(), bytes));
            self.card_view = CardView::Original;
            self.opened = None;
            if launcher::is_open() {
                self.refresh_popup();
            }
            Ok(true)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = bytes;
            Ok(false)
        }
    }

    pub(crate) fn restore_history(&mut self, index: usize) -> anyhow::Result<()> {
        self.card_view = CardView::Original;
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
        let labeling = crate::sensitivity::labeling(text);
        let concealed = matches!(&labeling, crate::sensitivity::Labeling::Known(found) if !found.labels.is_empty());
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
                eprintln!("clear sensitive copy failed: {e:#}");
            } else {
                eprintln!("copycraft: sensitive copy cleared after a minute");
            }
        }
        None
    }

    /// Text copycraft put on the clipboard (Copy, Format in place, a history entry chosen from
    /// the menu): the newest copy in history. The poller skips copycraft's own writes, so this
    /// is where they are recorded.
    pub(crate) fn record_own_copy(&mut self, text: &str) {
        self.last_copy = Some(Instant::now());
        self.history.record(text.to_string());
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
    }

    /// The history row stays a type mark. The card shows the copy that was chosen.
    pub(crate) fn show_restored(&mut self) {
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::set_card_shown(true);
        let view = ClipboardView::from_os();
        self.record_current(&view);
        let mut data = self.launch_data(&view);
        self.attach_image(&mut data);
        if launcher::is_open() {
            launcher::sync(data);
        } else {
            launcher::reveal(data);
        }
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
        // Copycraft's own write: what belongs in history was recorded when it was written (see
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
            if let Some(bytes) = crate::macos_pasteboard::current_image_bytes() {
                self.last_copy = Some(Instant::now());
                self.current_image = self.history.record_image(bytes);
                self.image_on_pasteboard = self.current_image.clone().map(|bytes| (change, bytes));
                self.history_cursor = 0;
                self.clipboard_cursor = None;
            }
            return;
        }
        self.current_image = None;
        if let Some(text) = view.text()
            && self.should_record(text)
        {
            self.skip_record = None;
            self.last_copy = Some(Instant::now());
            self.history.record(text.to_string());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
        }
    }
}
