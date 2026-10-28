//! Command dispatch, the diff view and the hotkey binding.

use super::*;

impl App {
    pub(crate) fn run_command(&mut self, event_loop: &ActiveEventLoop, id: CommandId) {
        // ⌘Z puts back only the copy removed just now: anything else done on the card first
        // lets it go (zeroized). Looking around history keeps it.
        if !matches!(
            id,
            CommandId::RemoveHistory(_)
                | CommandId::RestoreHistory
                | CommandId::DeleteHistory
                | CommandId::HistoryView(_)
                | CommandId::HistoryOlder
                | CommandId::HistoryNewer
        ) {
            self.history.forget_removed();
        }
        match id {
            CommandId::Visit => self.visit_current(),
            CommandId::OpenLink => self.open_link_in_text(),
            CommandId::CopyCleanLink => self.copy_link_part(false),
            CommandId::CopyDomain => self.copy_link_part(true),
            CommandId::Copy => {
                if let Err(e) = self.copy_current() {
                    crate::diag::log(crate::diag::LogEvent::CopyFailed(
                        crate::diag::ErrorKind::of(&e),
                    ));
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
                // The active view's chip clicked again goes back to the card's own view.
                let view = CardView::from_command(&id).unwrap_or_default();
                if view == self.card_view && view != CardView::Original {
                    self.reset_view();
                } else {
                    self.card_view = view;
                }
                self.refresh_popup();
            }
            CommandId::History(index) => {
                if let Err(e) = self.restore_history(index) {
                    crate::diag::log(crate::diag::LogEvent::RestoreHistoryFailed(
                        crate::diag::ErrorKind::of(&e),
                    ));
                }
            }
            CommandId::HistoryView(index) => {
                // A normal click on the strip ends a comparison of two of its cards.
                if self.diff_sel.is_empty() {
                    self.view_history(index);
                } else {
                    self.clear_diff();
                    if index == self.history_cursor {
                        self.refresh_popup();
                    } else {
                        self.view_history(index);
                    }
                }
            }
            CommandId::StripCompare(index) => self.compare_tiles(index),
            CommandId::StripPin(index) => self.toggle_pin_at(index),
            CommandId::EndCompare => {
                self.clear_diff();
                self.refresh_popup();
            }
            CommandId::RemoveHistory(index) => self.remove_from_history(index),
            CommandId::RestoreHistory => self.restore_removed(),
            CommandId::ToggleStrip => {
                commands::toggle_strip();
                self.refresh_popup();
            }
            CommandId::HistoryOlder => self.step_history(true),
            CommandId::HistoryNewer => self.step_history(false),
            CommandId::ClearClipboard => {
                if let Err(e) = self.clear_clipboard() {
                    crate::diag::log(crate::diag::LogEvent::ClearClipboardFailed(
                        crate::diag::ErrorKind::of(&e),
                    ));
                }
            }
            CommandId::ClearHistory => self.clear_history(),
            CommandId::Clear => self.clear_secrets(),
            CommandId::ChooseFile => self.choose_file(),
            CommandId::Generate(generator) => self.generate(generator),
            CommandId::UseClipboard => {
                self.leave_opened();
                self.reset_view();
                self.image_scan = None;
                self.image_scan_change = None;
                self.refresh_popup();
            }
            CommandId::Appearance(theme) => {
                appearance::save(theme);
                self.refresh_popup();
            }
            CommandId::CardTheme(theme) => {
                crate::settings::set_card_theme(theme);
                // As the Settings popup does; an open Settings window follows.
                #[cfg(target_os = "macos")]
                {
                    crate::macos_settings::card_theme_changed();
                    crate::macos_launcher::apply_card_theme();
                }
                self.refresh_popup();
            }
            CommandId::ClearSensitive => {
                let on = !crate::settings::load().clear_sensitive;
                crate::settings::set_clear_sensitive(on);
                if !on {
                    self.sensitive_clear = None;
                }
                // Settings › Privacy shows the same setting.
                #[cfg(target_os = "macos")]
                crate::macos_settings::refresh_history_controls();
                self.refresh_popup();
            }
            CommandId::PdfPage(delta) => self.pdf_turn(delta),
            CommandId::PdfZoom(step) => self.pdf_zoom(step),
            CommandId::WindowSize(size) => {
                crate::settings::set_window_size(size);
                self.refresh_popup();
            }
            CommandId::KeepHistory(minutes) => {
                crate::settings::set_history_minutes(minutes);
                self.history_minutes = minutes;
                #[cfg(target_os = "macos")]
                crate::macos_settings::refresh_history_controls();
                self.refresh_popup();
            }
            CommandId::TableStep(op) => self.table_step(TableTarget::Card, op),
            CommandId::TableOpenWindow => self.open_table_window(),
            // On a text Text ▾ worked on, ⌘Z and ⇧⌘Z go between the copies its steps made.
            CommandId::StepBack if self.text_steps_shown() => {
                self.text_goto(|cursor| cursor.checked_sub(1));
            }
            CommandId::StepForward if self.text_steps_shown() => {
                self.text_goto(|cursor| Some(cursor + 1));
            }
            CommandId::ToggleLineNumbers => {
                commands::toggle_line_numbers();
                // Kept cards were built with the old choice.
                self.history.forget_cards();
                self.full_card = None;
                self.refresh_popup();
            }
            CommandId::TextHead => self.text_lines_prompt(false),
            CommandId::TextTail => self.text_lines_prompt(true),
            CommandId::TextReverse => self.run_text(crate::text_ops::TextOp::Reverse),
            CommandId::TextColumn => self.text_column_prompt(),
            CommandId::TextFilter => self.text_filter_prompt(),
            CommandId::TextCleanUrl => self.text_clean_url_prompt(),
            CommandId::TextJson => self.text_json_prompt(),
            CommandId::TextCase(style) => self.text_case_prompt(style),
            CommandId::TextEncode(codec) => self.run_text(crate::text_ops::TextOp::Encode(codec)),
            CommandId::TextHash(kind) => self.run_text(crate::text_ops::TextOp::Hash(kind)),
            CommandId::Ai(action) => self.ai_start(action, None),
            // The dimmed "and N more" line does nothing; the whole text is `AI_FN_WHOLE`.
            CommandId::AiFn(_, commands::AI_FN_MORE) => {}
            CommandId::AiFn(action, commands::AI_FN_WHOLE) => self.ai_start(action, None),
            CommandId::AiFn(action, index) => self.ai_start(action, Some(index)),
            CommandId::AiApply => self.ai_apply(),
            CommandId::AiCancel => self.ai_stop(),
            CommandId::AiMinimize => self.ai_minimize(),
            CommandId::AiUnavailable | CommandId::AiOnDevice => {}
            // On a picture card, ⌘Z and ⇧⌘Z go between the copies Image ▾ steps made (there is
            // no version capsule), as on a text Text ▾ stepped.
            CommandId::StepBack if self.shows_image() => {
                self.image_goto(|cursor| cursor.checked_sub(1));
            }
            CommandId::StepForward if self.shows_image() => {
                self.image_goto(|cursor| Some(cursor + 1))
            }
            CommandId::StepBack => {
                self.table_goto(TableTarget::Card, |cursor| cursor.checked_sub(1));
            }
            CommandId::StepForward => self.table_goto(TableTarget::Card, |cursor| Some(cursor + 1)),
            CommandId::TableVersion(index) => self.table_goto(TableTarget::Card, |_| Some(index)),
            CommandId::ImageStep(op) => self.image_step(op),
            CommandId::ImageResizeCustom => self.custom_resize(),
            // The card pops the menu itself.
            CommandId::TableMenu
            | CommandId::ImageMenu
            | CommandId::TextMenu
            | CommandId::AiMenu
            | CommandId::TableChooseColumns => {}
            // The table window's grid menu; they come in as the window's commands.
            CommandId::TableCopyCell(..)
            | CommandId::TableCopyRow(_)
            | CommandId::TableCopyColumn(_) => {}
            CommandId::TableDateOrder(month_first) => {
                self.table_reread(TableTarget::Card, |options| {
                    options.month_first = month_first;
                });
            }
            CommandId::Settings => launcher::show_settings(),
            CommandId::About => launcher::show_about(),
            CommandId::SuggestFeature => launcher::show_feedback(crate::mailto::Kind::Feature),
            CommandId::ReportProblem => launcher::show_feedback(crate::mailto::Kind::Problem),
            CommandId::TextComplexity => launcher::show_complexity(),
            CommandId::AgentCommands => self.run_text(crate::text_ops::TextOp::AgentCommands),
            CommandId::AgentOutline => launcher::show_agent_outline(),
            CommandId::AgentCheck => launcher::show_agent_check(),
            CommandId::GotoLine(line) => launcher::scroll_card_to_line(line),
            CommandId::ExportDiagnostics => self.export_diagnostics(),
            CommandId::CheckAi => self.ai_check_now(),
            CommandId::Welcome => launcher::show_welcome(),
            CommandId::Diff => {
                if self.diff_sel.pair().is_some() {
                    self.card_view = CardView::Diff;
                    if self.diff_outcome.is_none() && self.diffing.is_none() && !self.kick_diff() {
                        self.reset_view();
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
            // Greyed out while history is off; a stray call (search, a stale menu) does nothing.
            CommandId::Pause | CommandId::IgnoreNext | CommandId::IgnoreFrontApp
                if !crate::history_mode::capture_controls_active(
                    crate::settings::clipboard_history(),
                ) => {}
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
            CommandId::TakeScreenshot => self.take_screenshot(),
            CommandId::Pin => self.pin_shown(true),
            CommandId::Unpin => self.pin_shown(false),
            CommandId::DeleteHistory => self.remove_from_history(self.history_cursor),
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
                self.reset_view();
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
                self.reset_view();
            }
        }
        self.refresh_popup();
    }

    /// ⌘-click on strip card `index`: compare that copy with the one on screen (the older of the
    /// two is A). Both must be text; the same card, or a copy that is not text, does nothing.
    /// A second ⌘-click on a card already in the diff takes it out, as *Remove from Diff*.
    pub(crate) fn compare_tiles(&mut self, index: usize) {
        if self.opened.is_some() {
            return;
        }
        let shown = self.history_cursor;
        let (Some(shown_id), Some(other_id)) =
            (self.history.id_at(shown), self.history.id_at(index))
        else {
            return;
        };
        if self.diff_sel.contains(other_id) {
            self.toggle_diff(other_id);
            return;
        }
        if !self.history.is_text_id(shown_id) || !self.history.is_text_id(other_id) {
            return;
        }
        if !self
            .diff_sel
            .compare_tiles((shown, shown_id), (index, other_id))
        {
            return;
        }
        if !self.strip_compare_used {
            self.strip_compare_used = true;
            crate::settings::set_strip_compare_used();
        }
        self.invalidate_diff();
        self.card_view = CardView::Diff;
        if !self.kick_diff() {
            self.reset_view();
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
            self.reset_view();
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
                self.diffing = None;
                self.work_failed(crate::diag::Work::Diff, &err);
                false
            }
        }
    }

    pub(crate) fn finish_diff(&mut self, generation: u64, outcome: crate::diff::Outcome) {
        if generation != self.diff_generation || self.diff_sel.pair().is_none() {
            return;
        }
        if let Some(run) = self.diffing.take() {
            crate::diag::log(crate::diag::LogEvent::DiffBuilt {
                ms: run.elapsed_ms() as u64,
            });
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
            self.reset_view();
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
            self.reset_view();
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

    /// Unregister the old chords and register the ones stored in Settings.
    pub(crate) fn rebind_hotkey(&mut self) {
        self.rebind_card_hotkey();
        self.rebind_screenshot_hotkey();
    }

    /// The screenshot hotkey: the stored chord, or none when it cannot be registered (taken
    /// by another app or macOS; Settings says so); the card's hotkey is not touched.
    pub(crate) fn rebind_screenshot_hotkey(&mut self) {
        let next = crate::settings::screenshot_hotkey();
        if self.screenshot_hotkey == Some(next) {
            return;
        }
        if let Some(old) = self.screenshot_hotkey.take()
            && let Err(e) = self.hotkeys.unregister(old)
        {
            crate::diag::log(crate::diag::LogEvent::HotkeyUnregisterFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
        }
        self.screenshot_hotkey = register_screenshot_hotkey(&self.hotkeys);
        self.refresh_status_menu();
        #[cfg(target_os = "macos")]
        crate::macos_settings::refresh_hotkeys();
    }

    /// The card's hotkey: the stored chord, or none when it cannot be registered (Settings
    /// says it is taken, and the menu still opens the card).
    fn rebind_card_hotkey(&mut self) {
        let next = crate::settings::hotkey();
        if self.format_hotkey == Some(next) {
            return;
        }
        if let Some(old) = self.format_hotkey.take()
            && let Err(e) = self.hotkeys.unregister(old)
        {
            crate::diag::log(crate::diag::LogEvent::HotkeyUnregisterFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
        }
        self.format_hotkey = register_card_chord(&self.hotkeys, next);
        self.refresh_status_menu();
        #[cfg(target_os = "macos")]
        crate::macos_settings::refresh_hotkeys();
    }
}
