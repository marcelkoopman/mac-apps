//! Command dispatch, the diff view and the hotkey binding.

use super::*;

impl App {
    pub(crate) fn run_command(&mut self, event_loop: &ActiveEventLoop, id: CommandId) {
        match id {
            CommandId::Visit => self.visit_current(),
            CommandId::Copy => {
                if let Err(e) = self.copy_current() {
                    eprintln!("copy failed: {e:#}");
                }
            }
            CommandId::Save => self.save_current(),
            CommandId::OpenPreview => self.open_preview(),
            CommandId::ShowAll => self.show_all(),
            CommandId::Original
            | CommandId::Format
            | CommandId::Convert
            | CommandId::Decode
            | CommandId::Dataframe
            | CommandId::Schema
            | CommandId::Sample
            | CommandId::Info
            | CommandId::Ocr
            | CommandId::Qr => {
                if matches!(id, CommandId::Format) && self.format_link_in_place() {
                    return;
                }
                self.card_view = CardView::from_command(&id).unwrap_or_default();
                self.refresh_popup();
            }
            CommandId::History(index) => {
                if let Err(e) = self.restore_history(index) {
                    eprintln!("restore history failed: {e:#}");
                }
            }
            CommandId::HistoryOlder => self.step_history(true),
            CommandId::HistoryNewer => self.step_history(false),
            CommandId::ClearClipboard => {
                if let Err(e) = self.clear_clipboard() {
                    eprintln!("clear clipboard failed: {e:#}");
                }
            }
            CommandId::ClearHistory => self.clear_history(),
            CommandId::Clear => self.clear_secrets(),
            CommandId::ChooseFile => self.choose_file(),
            CommandId::UseClipboard => {
                self.leave_opened();
                self.card_view = CardView::Original;
                self.image_scan = None;
                self.image_scan_change = None;
                self.refresh_popup();
            }
            CommandId::Appearance(theme) => {
                appearance::save(theme);
                self.refresh_popup();
            }
            CommandId::ClearSensitive => {
                let on = !crate::settings::load().clear_sensitive;
                crate::settings::set_clear_sensitive(on);
                if !on {
                    self.sensitive_clear = None;
                }
                self.refresh_popup();
            }
            CommandId::KeepHistory(minutes) => {
                crate::settings::set_history_minutes(minutes);
                self.history_minutes = minutes;
                self.refresh_popup();
            }
            CommandId::TableStep(op) => self.table_step(TableTarget::Card, op),
            CommandId::TableOpenWindow => self.open_table_window(),
            // On a picture card, undo, redo and the version capsule are the picture's.
            CommandId::TableUndo if self.shows_image() => {
                self.image_goto(|cursor| cursor.checked_sub(1));
            }
            CommandId::TableRedo if self.shows_image() => {
                self.image_goto(|cursor| Some(cursor + 1))
            }
            CommandId::TableVersion(index) if self.shows_image() => {
                self.image_goto(|_| Some(index))
            }
            CommandId::TableUndo => {
                self.table_goto(TableTarget::Card, |cursor| cursor.checked_sub(1));
            }
            CommandId::TableRedo => self.table_goto(TableTarget::Card, |cursor| Some(cursor + 1)),
            CommandId::TableVersion(index) => self.table_goto(TableTarget::Card, |_| Some(index)),
            CommandId::ImageStep(op) => self.image_step(op),
            CommandId::ImageResizeCustom => self.custom_resize(),
            // The card pops the menu itself.
            CommandId::TableMenu | CommandId::ImageMenu | CommandId::TableChooseColumns => {}
            CommandId::TableDateOrder(month_first) => {
                self.table_reread(TableTarget::Card, |options| {
                    options.month_first = month_first;
                });
            }
            CommandId::Settings => launcher::show_settings(),
            CommandId::About => launcher::show_about(),
            CommandId::Diff => {
                if self.diff_sel.pair().is_some() {
                    self.card_view = CardView::Diff;
                    if self.diff_outcome.is_none() && self.diffing.is_none() && !self.kick_diff() {
                        self.card_view = CardView::Original;
                    }
                    self.refresh_popup();
                }
            }
            CommandId::DiffToggle(id) => self.toggle_diff(id),
            CommandId::DiffShown => {
                if self.opened.is_none()
                    && let Some(id) = self.history.id_at(self.history_cursor)
                {
                    self.toggle_diff(id);
                }
            }
            CommandId::DiffSwap => self.swap_diff(),
            CommandId::Pause => {
                crate::capture::set_paused(!crate::capture::paused());
                self.refresh_status_menu();
                self.sync_tooltip(&ClipboardView::from_os());
                if launcher::is_open() {
                    self.refresh_popup();
                }
            }
            CommandId::IgnoreNext => {
                crate::capture::set_ignore_next(!crate::capture::ignore_next());
                self.refresh_status_menu();
                if launcher::is_open() {
                    self.refresh_popup();
                }
            }
            CommandId::IgnoreFrontApp => {
                if let Some(bundle) = crate::capture::last_foreign() {
                    crate::settings::ignore_app(&bundle);
                    #[cfg(target_os = "macos")]
                    crate::macos_settings::refresh_ignored_field();
                    if launcher::is_open() {
                        self.refresh_popup();
                    }
                }
            }
            CommandId::Pin => self.pin_shown(true),
            CommandId::Unpin => self.pin_shown(false),
            CommandId::DeleteHistory => self.delete_shown(),
            CommandId::ClearPins => self.clear_pins(),
            CommandId::Quit => {
                discard_preview_files();
                event_loop.exit();
            }
        }
    }

    /// Add or remove history entry `id`. Two picks, or a third that replaces the oldest, start
    /// the diff. A non-text entry stays out.
    pub(crate) fn toggle_diff(&mut self, id: u64) {
        if self.diff_sel.contains(id) {
            self.diff_sel.remove(id);
            self.invalidate_diff();
            if self.card_view == CardView::Diff {
                self.card_view = CardView::Original;
            }
            self.refresh_popup();
            return;
        }
        if !self.history.is_text_id(id) {
            return;
        }
        let added = self.diff_sel.add(id);
        if matches!(
            added,
            crate::diff::AddResult::Start | crate::diff::AddResult::Replace
        ) {
            self.card_view = CardView::Diff;
            if !self.kick_diff() {
                self.card_view = CardView::Original;
            }
        }
        self.refresh_popup();
    }

    /// A becomes B and B becomes A. The diff runs again.
    pub(crate) fn swap_diff(&mut self) {
        if !self.diff_sel.swap() {
            return;
        }
        self.card_view = CardView::Diff;
        if !self.kick_diff() {
            self.card_view = CardView::Original;
        }
        self.refresh_popup();
    }

    /// Compare the two chosen texts on a background thread. The texts are copied here, on the
    /// main thread; the worker does not read the pasteboard. `false` when there is no pair, a
    /// side is gone, or the thread cannot start (the caller then leaves the diff view).
    pub(crate) fn kick_diff(&mut self) -> bool {
        let Some((left_id, right_id)) = self.diff_sel.pair() else {
            return false;
        };
        let Some(left) = self.history.text_of(left_id) else {
            return false;
        };
        let Some(right) = self.history.text_of(right_id) else {
            return false;
        };
        self.diff_generation = self.diff_generation.wrapping_add(1);
        let generation = self.diff_generation;
        self.diff_outcome = None;
        self.diffing = Some(Background::now());
        let spawned = std::thread::Builder::new()
            .name("copycraft-diff".into())
            .spawn(move || {
                let outcome = crate::diff::compare(left.as_str(), right.as_str());
                launcher::emit(UserEvent::DiffReady {
                    generation,
                    outcome: Box::new(outcome),
                });
            });
        match spawned {
            Ok(_) => true,
            Err(err) => {
                eprintln!("copycraft: diff skipped: {err}");
                self.diffing = None;
                self.stop_spinner_when_idle();
                false
            }
        }
    }

    pub(crate) fn finish_diff(&mut self, generation: u64, outcome: crate::diff::Outcome) {
        if generation != self.diff_generation || self.diff_sel.pair().is_none() {
            return;
        }
        if let Some(run) = self.diffing.take() {
            eprintln!("copycraft: diff built in {} ms", run.elapsed_ms());
        }
        self.diff_outcome = Some(outcome);
        self.stop_spinner_when_idle();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Drop a running or finished diff. The picks stay.
    pub(crate) fn invalidate_diff(&mut self) {
        self.diff_generation = self.diff_generation.wrapping_add(1);
        self.diff_outcome = None;
        self.diffing = None;
        self.stop_spinner_when_idle();
    }

    /// Wipe, lock and a cleared history drop the picks as well. Nothing is written to disk.
    pub(crate) fn clear_diff(&mut self) {
        self.diff_sel.clear();
        self.invalidate_diff();
        if self.card_view == CardView::Diff {
            self.card_view = CardView::Original;
        }
    }

    /// Drop picks whose entries have fallen off. One side gone leaves the diff view.
    pub(crate) fn sync_diff_picks(&mut self) {
        let dropped = self.diff_sel.retain(|id| self.history.contains_id(id));
        if !dropped {
            return;
        }
        self.invalidate_diff();
        let restarted = self.diff_sel.pair().is_some() && self.kick_diff();
        if !restarted && self.card_view == CardView::Diff {
            self.card_view = CardView::Original;
        }
    }

    pub(crate) fn fill_diff(&self, data: &mut LaunchData) {
        data.diff_picks = self.diff_sel.ids().to_vec();
        data.shown_id = if self.opened.is_some() {
            None
        } else {
            self.history.id_at(self.history_cursor)
        };
        data.shown_mark = data
            .shown_id
            .filter(|_| data.view != CardView::Diff)
            .and_then(|id| self.diff_sel.label(id));
        let Some((left, right)) = self.diff_sel.pair() else {
            data.diff = None;
            return;
        };
        let (sensitive, labels) =
            crate::diff::combine_labels(&self.side_labeling(left), &self.side_labeling(right));
        let phase = match &self.diff_outcome {
            Some(outcome) => crate::diff::Phase::from_outcome(outcome),
            None => crate::diff::Phase::Working,
        };
        data.diff = Some(commands::DiffShown {
            token: crate::diff::token(left, right, phase.tag()),
            sensitive,
            labels,
            phase,
        });
    }

    pub(crate) fn side_labeling(&self, id: u64) -> crate::sensitivity::Labeling {
        match self.history.text_of(id) {
            Some(text) => crate::sensitivity::labeling(text.as_str()),
            None => crate::sensitivity::Labeling::Known(crate::sensitivity::Found::default()),
        }
    }

    /// The diff as text, for Copy and Save. Main thread only.
    pub(crate) fn diff_text(&self) -> Option<Zeroizing<String>> {
        self.diff_outcome
            .as_ref()
            .map(crate::diff::Outcome::copy_text)
    }

    /// Unregister the old chord and register the one stored in Settings.
    pub(crate) fn rebind_hotkey(&mut self) {
        let next = crate::settings::hotkey();
        if next.id() == self.format_hotkey_id {
            return;
        }
        if let Err(e) = self.hotkeys.unregister(self.format_hotkey) {
            eprintln!("copycraft: unregister hotkey: {e}");
        }
        match self.hotkeys.register(next) {
            Ok(()) => {
                self.format_hotkey = next;
                self.format_hotkey_id = next.id();
                self.refresh_status_menu();
                eprintln!("copycraft: hotkey {}", crate::settings::hotkey_label());
            }
            Err(e) => {
                eprintln!("copycraft: register hotkey: {e}");
                // Best effort: put the previous chord back.
                let _ = self.hotkeys.register(self.format_hotkey);
            }
        }
    }
}
