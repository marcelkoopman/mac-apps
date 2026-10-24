//! Text ▾ actions: first and last lines, reverse, column and filter ([`crate::text_ops`]). Each
//! step copies its result as a new copy, a history entry of its own, as Image ▾ does; ⌘Z shows
//! the copy it was taken on (the clipboard is left alone).

use super::image::{StepKeep, keep_step_source, step_keep, step_note};
use super::*;
use crate::text_ops::case_style::CaseStyle;
use crate::text_ops::{self, Separator, TextOp};

/// The entry id Text ▾ and Image ▾ use for a shown copy that is not in history.
pub(crate) const NOT_IN_HISTORY: u64 = u64::MAX;

/// The copies Text ▾ (or Image ▾) steps made: the history entries they are, oldest first (the
/// copy the first step was taken on, then one per step), and which is shown. Only ids and
/// labels: the copies stay in history (and go when it forgets them).
pub(crate) struct CopySteps {
    pub(crate) ids: Vec<u64>,
    pub(crate) labels: Vec<String>,
    pub(crate) cursor: usize,
}

impl CopySteps {
    /// A step on entry `id` made entry `new_id`, labelled `label`: `steps` go on from `id` when
    /// they show it (the copies after it are dropped), else new steps start at `id`.
    pub(crate) fn stepped(steps: Option<Self>, id: u64, new_id: u64, label: String) -> Self {
        let mut steps = match steps {
            Some(steps) if steps.ids.get(steps.cursor) == Some(&id) => steps,
            _ => Self {
                ids: vec![id],
                labels: vec![crate::locale::t("original").to_string()],
                cursor: 0,
            },
        };
        steps.ids.truncate(steps.cursor + 1);
        steps.labels.truncate(steps.cursor + 1);
        steps.ids.push(new_id);
        steps.labels.push(label);
        steps.cursor += 1;
        steps
    }

    /// `steps` when the copy they show is entry `id`.
    pub(crate) fn showing(steps: Option<&Self>, id: u64) -> Option<&Self> {
        steps.filter(|steps| steps.ids.get(steps.cursor) == Some(&id))
    }
}

/// Which [`CopySteps`] [`App::browse_step`] moves in.
#[derive(Clone, Copy)]
pub(crate) enum StepsOf {
    Text,
    Image,
}

/// The id Text ▾ uses for an opened file's text: its step makes a new copy from the file's
/// entry in history, if it has one ([`App::text_stepped`]).
const OPENED_FILE: u64 = u64::MAX - 1;

/// The labels of a Text ▾ step's text, found where the step ran (also for a long one, which
/// the card would hand to the background checker). `None` when the step left no text.
pub(crate) fn step_labels(result: &text_ops::TextResult) -> Option<crate::sensitivity::Found> {
    match result {
        text_ops::TextResult::Step {
            text: Some(text), ..
        } => Some(crate::sensitivity::found(text)),
        _ => None,
    }
}

/// The text a card for `data` shows, for Text ▾: its text subject.
pub(crate) fn card_text_of(data: &LaunchData) -> Option<Zeroizing<String>> {
    data.subject_text
        .as_deref()
        .filter(|_| data.subject_kind == SubjectKind::Text)
        .map(|text| Zeroizing::new(text.to_string()))
}

impl App {
    /// The history entry on the card and its text, when Text ▾ may work on it: a text card that
    /// follows history and is not a table (tables have their own steps).
    pub(crate) fn text_entry(&self) -> Option<(u64, Zeroizing<String>)> {
        if let Some(opened) = &self.opened {
            // An opened text or code file has Text ▾ like a copy; a step makes a new copy.
            return opened
                .text
                .as_ref()
                .filter(|text| commands::offers_text_menu(text))
                .map(|text| (OPENED_FILE, Zeroizing::new(text.to_string())));
        }
        // The copied source the card shows, also when it shows it formatted: the text the
        // card was built from. Reading the pasteboard again could see another state (a read
        // still pending, a newer change) than the card, and leave Text ▾ off the card until
        // it was built again. While recording is paused or the app is ignored (Held) the card
        // shows the history entry: so does Text ▾.
        let view = ClipboardView::from_os();
        let text = if let Some(text) = &self.card_text {
            Some(text.to_string())
        } else if let Some(at) = self.browsed_index() {
            self.history.get(at).map(str::to_string)
        } else if matches!(view, ClipboardView::Held) || self.pasteboard_suppressed() {
            self.history.get(self.history_cursor).map(str::to_string)
        } else {
            view.text().map(str::to_string)
        };
        let text = Zeroizing::new(text?);
        // A link card has no Text ▾, but Clean URL works on it (its chip), and the versions it
        // makes go back and forth like any other step's (the cleaned link has nothing left
        // to clean, so it is let in as one of those versions).
        let link = crate::page_preview::page_url(&text).is_some()
            || crate::youtube::video_id(&text).is_some();
        let offered = commands::offers_text_menu(&text)
            || (link && crate::text_ops::url_clean::has_tracking(&text));
        // A copy that is not in history (not recorded) still gets Text ▾; only ⌘Z back to it
        // has nothing to put back.
        let id = self
            .history
            .text_index(&text, self.history_cursor)
            .and_then(|index| self.history.id_at(index))
            .unwrap_or(NOT_IN_HISTORY);
        let stepped = || {
            link && id != NOT_IN_HISTORY
                && self
                    .text_steps
                    .as_ref()
                    .is_some_and(|steps| steps.ids.contains(&id))
        };
        (offered || stepped()).then_some((id, text))
    }

    /// The text the AI chip works on: the card's text, or the recognized text of a scanned PDF
    /// (the card is its first page; the chip waits for the recognition).
    pub(crate) fn ai_entry(&self) -> Option<(u64, Zeroizing<String>)> {
        self.text_entry().or_else(|| {
            let image = self.opened.as_ref()?.image.as_ref()?;
            // A scanned PDF or a document's picture (its text is behind the picture).
            (image.pdf_source.is_some() || image.pdf_text.is_some()).then_some(())?;
            let text = image.scan.as_ref()?.ocr.as_ref()?;
            Some((OPENED_FILE, Zeroizing::new(text.clone())))
        })
    }

    /// A result that is no step on the card (an applied AI result for a card that moved on,
    /// or for a scanned PDF): a new copy, in history by the rules of CopyCraft's own copies.
    /// `found` holds its labels ([`crate::sensitivity::copy_verdict`]).
    pub(crate) fn copy_ai_result(
        &mut self,
        result: &Zeroizing<String>,
        found: &crate::sensitivity::Found,
    ) {
        let verdict = crate::sensitivity::copy_verdict(found);
        match self.write_own_as(result, verdict.concealed) {
            Ok(()) => {
                self.last_copy = Some(Instant::now());
                if self.record_step_copy(result, verdict.admitted).1 {
                    self.history_cursor = 0;
                    self.clipboard_cursor = None;
                    self.refresh_status_menu();
                }
                save_feedback(crate::locale::t("copied_toast"));
            }
            Err(e) => crate::diag::log(crate::diag::LogEvent::TextStepFailed(
                crate::diag::ErrorKind::of(&e),
            )),
        }
    }

    /// Record a step's copy `result` as an Image ▾ step's: the user asked for it, so an ignored
    /// front app does not hold it; Pause, a token or key (`admitted` false) or its size do.
    /// Returns the verdict and whether history took it.
    fn record_step_copy(&mut self, result: &str, admitted: bool) -> (StepKeep, bool) {
        let keep = step_keep(crate::capture::history_paused(), admitted);
        let recorded =
            keep == StepKeep::Keep && self.history.record_unless(result.to_string(), false);
        (keep, recorded)
    }

    /// The steps whose shown version is entry `id`.
    fn text_steps_of(&self, id: u64) -> Option<&CopySteps> {
        CopySteps::showing(self.text_steps.as_ref(), id)
    }

    /// Undo and redo after a step: show the copy `to` picks
    /// from the one `which` steps show, entry `id`. The card browses history to it; the
    /// clipboard is left alone (no Copied toast), Copy puts it there. `false` when there is no
    /// such copy (also when history let it go: it keeps a limited number of copies).
    pub(crate) fn browse_step(
        &mut self,
        which: StepsOf,
        id: u64,
        to: impl FnOnce(usize) -> Option<usize>,
    ) -> bool {
        let slot = match which {
            StepsOf::Text => &mut self.text_steps,
            StepsOf::Image => &mut self.image_steps,
        };
        let Some(steps) = slot
            .as_mut()
            .filter(|steps| steps.ids.get(steps.cursor) == Some(&id))
        else {
            return false;
        };
        let Some(index) = to(steps.cursor).filter(|&index| index < steps.ids.len()) else {
            return false;
        };
        if index == steps.cursor {
            return false;
        }
        let Some(at) = self.history.index_of(steps.ids[index]) else {
            return false;
        };
        steps.cursor = index;
        self.browsing = Some(steps.ids[index]);
        self.history_cursor = at;
        true
    }

    /// Undo and redo go to the copies Text ▾ steps made of the text on the card.
    pub(crate) fn text_steps_shown(&self) -> bool {
        self.text_entry()
            .is_some_and(|(id, _)| self.text_steps_of(id).is_some_and(|s| s.ids.len() > 1))
    }

    /// Text ▾ for the card: whether ⌘Z and ⇧⌘Z have a copy to go to, and a note for the meta
    /// line.
    pub(crate) fn attach_text(&mut self, data: &mut LaunchData) {
        // Text ▾ works on the text this card is built from, whichever path built it.
        self.card_text = card_text_of(data);
        if data.subject_kind != SubjectKind::Text {
            return;
        }
        let Some((id, text)) = self.text_entry() else {
            return;
        };
        let kind = crate::format::detect(&text);
        let code = crate::complexity::Lang::of(kind).is_some();
        let agent = kind == crate::format::FormatKind::Agent;
        // Only to a copy history still has (it keeps a limited number).
        let steps = (id != OPENED_FILE && id != NOT_IN_HISTORY)
            .then(|| self.text_steps_of(id))
            .flatten();
        let can_undo = steps.is_some_and(|steps| {
            steps.cursor > 0 && self.history.contains_id(steps.ids[steps.cursor - 1])
        });
        let can_redo = steps.is_some_and(|steps| {
            steps
                .ids
                .get(steps.cursor + 1)
                .is_some_and(|&next| self.history.contains_id(next))
        });
        let note = self
            .text_note
            .as_ref()
            .filter(|(entry, _)| *entry == id)
            .map(|(_, note)| note.clone());
        if let Some(note) = &note {
            data.notice = Some(match data.notice.take() {
                Some(notice) => format!("{notice}  ·  {note}"),
                None => note.clone(),
            });
        }
        data.text_edit = Some(commands::TextShown {
            can_undo,
            can_redo,
            note,
            code,
            agent,
            tracking: crate::text_ops::url_clean::has_tracking(&text),
            json: kind == crate::format::FormatKind::Json,
        });
    }

    /// Take step `op` on the text on the card, on a background thread, like the card is built:
    /// the text never blocks the main thread. [`App::finish_text`] takes the result.
    pub(crate) fn run_text(&mut self, op: TextOp) {
        let Some((entry, text)) = self.text_entry() else {
            return;
        };
        self.text_generation = self.text_generation.wrapping_add(1);
        let generation = self.text_generation;
        let source_hash = super::state::hash_text(&text);
        let spawned = std::thread::Builder::new()
            .name("copycraft-text".into())
            .spawn(move || {
                let result = text_ops::TextResult::work(&text, &op);
                // A long result's labels too, here and not on the main thread: a token or key
                // in it keeps it out of history at once ([`App::text_stepped`]).
                let found = step_labels(&result);
                launcher::emit(UserEvent::TextReady {
                    generation,
                    entry,
                    result: Box::new(result),
                    found,
                    source_hash,
                });
            });
        match spawned {
            Ok(_) => self.work_error = None,
            Err(e) => self.work_failed(crate::diag::Work::TextStep, &e),
        }
    }

    /// A Text ▾ result is back on the main thread. Dropped when stale: a newer job started,
    /// history was wiped, or the card shows another entry or another text by now.
    pub(crate) fn finish_text(
        &mut self,
        generation: u64,
        entry: u64,
        result: text_ops::TextResult,
        found: Option<crate::sensitivity::Found>,
        source_hash: u64,
    ) {
        if generation != self.text_generation {
            return;
        }
        // The same entry is not enough: copies outside history (and opened files) share an
        // id, so a newer copy would get the result of the step taken on the one before.
        let Some((id, source)) = self
            .text_entry()
            .filter(|now| super::ai::card_still_shows(entry, source_hash, Some(now)))
        else {
            return;
        };
        match result {
            // A hash of a sensitive card (or file) is copied concealed and stays out of history:
            // the hash can be checked against a guess.
            text_ops::TextResult::Step {
                op: TextOp::Hash(_),
                text: Some(result),
            } if !crate::sensitivity::is_plain(&source) => {
                self.hash_kept_out(id, &result);
            }
            text_ops::TextResult::Step { text: None, .. } => {
                self.text_note = Some((id, crate::locale::t("text_nothing").to_string()));
                self.refresh_popup();
            }
            text_ops::TextResult::Step {
                op,
                text: Some(result),
            } => {
                let found = found.unwrap_or_else(|| crate::sensitivity::found(&result));
                self.text_stepped(id, op.label(), &result, &found);
            }
            // Names the code and the problem; the text itself is never shown or logged.
            text_ops::TextResult::Invalid { codec, reason } => {
                self.text_note = Some((id, codec.invalid_note(reason)));
                self.refresh_popup();
            }
        }
    }

    /// Copy a step's `result` as a new copy (a history entry of its own) taken from entry `id`,
    /// labelled `label`, as an Image ▾ step does: the clipboard holds the result, the card
    /// shows it, ⌘Z shows entry `id` again. A copy that is not in history yet (the clipboard's
    /// at launch) goes in first, by the rules of any copy ([`keep_step_source`]). The result
    /// goes in as an Image ▾ step's ([`Self::record_step_copy`]): the user asked for it, so an
    /// ignored front app does not hold it; Pause, a token or key in it, or its size keep it
    /// out, with a note. On an opened file the card leaves
    /// the file for the new copy. `found` holds the result's labels, also for a long one
    /// ([`crate::sensitivity::copy_verdict`]).
    pub(crate) fn text_stepped(
        &mut self,
        id: u64,
        label: String,
        result: &Zeroizing<String>,
        found: &crate::sensitivity::Found,
    ) {
        let Some((entry, source)) = self.text_entry().filter(|(entry, _)| *entry == id) else {
            // The card moved on (or a scanned PDF, which has no text of its own): the result
            // is a copy like any other.
            self.copy_ai_result(result, found);
            return;
        };
        let from_file = entry == OPENED_FILE;
        // An opened file is in history when it was dropped or chosen (and allowed in).
        let mut id = if from_file {
            self.history
                .text_index(&source, self.history_cursor)
                .and_then(|index| self.history.id_at(index))
                .unwrap_or(NOT_IN_HISTORY)
        } else {
            entry
        };
        let history_on = crate::settings::clipboard_history();
        let in_history = id != NOT_IN_HISTORY;
        let held = self.history.held();
        let paused = crate::capture::history_paused();
        let source_admitted = crate::sensitivity::record_when(crate::sensitivity::admission(
            &crate::sensitivity::labeling(&source),
        )) == crate::sensitivity::Record::Now;
        // The clipboard's own copy, shown on the card (not a file, not a browsed entry).
        let clipboard_copy = !from_file
            && !in_history
            && history_on
            && self.browsed_index().is_none()
            && ClipboardView::from_os().text() == Some(source.as_str());
        let source_kept = keep_step_source(
            history_on,
            in_history,
            clipboard_copy,
            held,
            source_admitted,
        ) && self.history.record(source.to_string());
        if source_kept && let Some(kept) = self.history.id_at(0) {
            id = kept;
        }
        let verdict = crate::sensitivity::copy_verdict(found);
        let admitted = verdict.admitted;
        if let Err(e) = self.write_own_as(result, verdict.concealed) {
            crate::diag::log(crate::diag::LogEvent::TextStepFailed(
                crate::diag::ErrorKind::of(&e),
            ));
            return;
        }
        self.last_copy = Some(Instant::now());
        // The new copy is the clipboard's: the card follows it again.
        self.browsing = None;
        let (keep, recorded) = self.record_step_copy(result, admitted);
        if cfg!(debug_assertions) {
            crate::diag::log(crate::diag::LogEvent::TextStepKept {
                source_in_history: in_history,
                clipboard_copy,
                source_kept,
                admitted,
                paused,
                held,
                kept: recorded,
            });
        }
        if from_file {
            self.leave_opened();
        }
        if source_kept || recorded {
            self.history_cursor = 0;
            self.clipboard_cursor = None;
            self.refresh_status_menu();
        }
        if !recorded {
            // On the card that shows now (the result, or while held the entry history shows).
            let shown = self.text_entry().map_or(id, |(at, _)| at);
            self.text_note = Some((shown, step_note(keep).to_string()));
            self.reset_view();
            self.refresh_popup();
            return;
        }
        if let Some(new_id) = self.history.id_at(0) {
            self.text_steps = Some(CopySteps::stepped(
                self.text_steps.take(),
                id,
                new_id,
                label,
            ));
        }
        self.text_note = None;
        self.reset_view();
        self.refresh_popup();
    }

    /// A hash of sensitive text: on the clipboard, concealed, and nowhere else. No version.
    fn hash_kept_out(&mut self, id: u64, result: &Zeroizing<String>) {
        if let Err(e) = self.write_own_as(result, true) {
            crate::diag::log(crate::diag::LogEvent::TextStepFailed(
                crate::diag::ErrorKind::of(&e),
            ));
            return;
        }
        self.browsing = None;
        self.text_note = Some((id, crate::locale::t("hash_not_kept").to_string()));
        self.refresh_popup();
    }

    /// Show the copy `to` picks from the one shown (undo, redo). Only shown: the clipboard is
    /// left alone (no Copied toast); Copy puts the copy there.
    pub(crate) fn text_goto(&mut self, to: impl FnOnce(usize) -> Option<usize>) {
        let Some((id, _)) = self.text_entry() else {
            return;
        };
        if !self.browse_step(StepsOf::Text, id, to) {
            return;
        }
        self.text_note = None;
        self.refresh_popup();
    }

    /// First Lines… (`last` false) or Last lines…: ask for N (10 to start), then take the step.
    pub(crate) fn text_lines_prompt(&mut self, last: bool) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let title = if last {
                crate::locale::t("text_tail_prompt")
            } else {
                crate::locale::t("text_head_prompt")
            };
            let mut message = crate::locale::t("text_lines_message").to_string();
            loop {
                let answer = mac_ui::dialog::prompt_text(mtm, title, &message, "10");
                launcher::order_front();
                let Some(answer) = answer.map(Zeroizing::new) else {
                    return;
                };
                if answer.trim().is_empty() {
                    return;
                }
                match text_ops::parse_count(&answer) {
                    Some(n) => {
                        self.run_text(if last {
                            TextOp::Tail(n)
                        } else {
                            TextOp::Head(n)
                        });
                        return;
                    }
                    None => message = crate::locale::t("text_count_invalid").to_string(),
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = last;
    }

    /// Column…: the separator (detected, can be changed), the column number (1 to start) and a
    /// live preview of the first lines; then take the step.
    pub(crate) fn text_column_prompt(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((id, text)) = self.text_entry() else {
                return;
            };
            let detected = text_ops::detect_separator(&text);
            let titles: Vec<&str> = Separator::ALL.iter().map(|sep| sep.title()).collect();
            let selected = Separator::ALL
                .iter()
                .position(|sep| *sep == detected)
                .unwrap_or(0);
            let ask = mac_ui::dialog::Form {
                title: crate::locale::t("text_column_title"),
                ok: "",
                message: crate::locale::t("text_column_message"),
                options: &titles,
                selected,
                option_label: crate::locale::t("text_separator"),
                field_label: crate::locale::t("text_column_number"),
                field_default: "1",
                field_width: 60.0,
                checks: &[],
                preview_label: crate::locale::t("text_preview"),
            };
            let text = std::sync::Arc::new(text);
            let answer = mac_ui::dialog::form(
                mtm,
                &ask,
                std::sync::Arc::new(move |option, typed, _| mac_ui::dialog::Preview {
                    text: column_preview(&text, option, typed),
                    ..Default::default()
                }),
            );
            launcher::order_front();
            let Some(answer) = answer else {
                return;
            };
            let typed = Zeroizing::new(answer.text);
            match (
                Separator::ALL.get(answer.option),
                text_ops::parse_count(&typed),
            ) {
                (Some(separator), Some(n)) => self.run_text(TextOp::Column(*separator, n)),
                _ => {
                    self.text_note = Some((id, crate::locale::t("text_count_invalid").to_string()));
                    self.refresh_popup();
                }
            }
        }
    }

    /// Filter (regex)…: a pattern, Invert and Ignore case, with the matching lines (matches
    /// highlighted) worked out live in the background; then take the step.
    pub(crate) fn text_filter_prompt(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((id, text)) = self.text_entry() else {
                return;
            };
            let checks = [
                crate::locale::t("filter_invert"),
                crate::locale::t("filter_ignore_case"),
            ];
            let ask = mac_ui::dialog::Form {
                title: crate::locale::t("filter_title"),
                ok: "",
                message: crate::locale::t("filter_message"),
                options: &[],
                selected: 0,
                option_label: "",
                field_label: crate::locale::t("filter_pattern"),
                field_default: "",
                field_width: 300.0,
                checks: &checks,
                preview_label: crate::locale::t("text_preview"),
            };
            let text = std::sync::Arc::new(text);
            let answer = mac_ui::dialog::form(
                mtm,
                &ask,
                std::sync::Arc::new(move |_, typed, checks| {
                    let flag = |at: usize| checks.get(at).copied().unwrap_or(false);
                    filter_preview(&text, typed, flag(0), flag(1))
                }),
            );
            launcher::order_front();
            let Some(answer) = answer else {
                return;
            };
            let flag = |at: usize| answer.checks.get(at).copied().unwrap_or(false);
            let (invert, ignore_case) = (flag(0), flag(1));
            if let Err(reason) =
                text_ops::filter::LineFilter::new(&answer.text, ignore_case, invert)
            {
                self.text_note = Some((id, reason));
                self.refresh_popup();
                return;
            }
            self.run_text(TextOp::Filter {
                pattern: answer.text,
                ignore_case,
                invert,
            });
        }
    }

    /// Clean URL…: how many tracking parameters go and the first cleaned URLs, worked out in
    /// the background; Apply to Clipboard takes the step (a new version, ⌘Z goes back).
    pub(crate) fn text_clean_url_prompt(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((_, text)) = self.text_entry() else {
                return;
            };
            let ask = preview_only_form(
                crate::locale::t("url_clean_title"),
                crate::locale::t("url_clean_message"),
            );
            let text = std::sync::Arc::new(text);
            let answer = mac_ui::dialog::form(
                mtm,
                &ask,
                std::sync::Arc::new(move |_, _, _| clean_url_preview(&text)),
            );
            launcher::order_front();
            if answer.is_some() {
                self.run_text(TextOp::CleanUrls);
            }
        }
    }
    /// Format JSON…: Format or Compress and Sort keys, with the result (or where the JSON
    /// breaks) worked out in the background; Apply to Clipboard takes the step.
    pub(crate) fn text_json_prompt(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((id, text)) = self.text_entry() else {
                return;
            };
            let layouts = [
                crate::locale::t("json_format"),
                crate::locale::t("json_compress"),
            ];
            let checks = [crate::locale::t("json_sort_keys")];
            let mut ask = preview_only_form(
                crate::locale::t("json_style_title"),
                crate::locale::t("json_style_message"),
            );
            ask.options = &layouts;
            ask.option_label = crate::locale::t("json_style_layout");
            // JSON on several lines most likely wants Compress, one line Format.
            ask.selected = usize::from(text.trim().contains('\n'));
            ask.checks = &checks;
            let text = std::sync::Arc::new(text);
            let shown = std::sync::Arc::clone(&text);
            let answer = mac_ui::dialog::form(
                mtm,
                &ask,
                std::sync::Arc::new(move |option, _, checks| {
                    json_preview(&shown, json_style_of(option, checks))
                }),
            );
            launcher::order_front();
            let Some(answer) = answer else {
                return;
            };
            let style = json_style_of(answer.option, &answer.checks);
            if let Err(err) = text_ops::json_style::restyle(&text, style) {
                self.text_note = Some((id, err.note()));
                self.refresh_popup();
                return;
            }
            self.run_text(TextOp::Json(style));
        }
    }
    /// Case style › a style: the style pop-up (that one picked) and how many lines change with
    /// the first of them, worked out in the background; Apply to Clipboard takes the step.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub(crate) fn text_case_prompt(&mut self, style: CaseStyle) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((_, text)) = self.text_entry() else {
                return;
            };
            let titles: Vec<&str> = CaseStyle::ALL.iter().map(|style| style.title()).collect();
            let mut ask = preview_only_form(
                crate::locale::t("case_menu"),
                crate::locale::t("case_message"),
            );
            ask.options = &titles;
            ask.option_label = crate::locale::t("case_style_label");
            ask.selected = CaseStyle::ALL
                .iter()
                .position(|each| *each == style)
                .unwrap_or(0);
            let text = std::sync::Arc::new(text);
            let answer = mac_ui::dialog::form(
                mtm,
                &ask,
                std::sync::Arc::new(move |option, _, _| {
                    case_preview(&text, CaseStyle::ALL.get(option).copied().unwrap_or(style))
                }),
            );
            launcher::order_front();
            let Some(answer) = answer else {
                return;
            };
            if let Some(style) = CaseStyle::ALL.get(answer.option) {
                self.run_text(TextOp::Case(*style));
            }
        }
    }
}

/// A [`mac_ui::dialog::Form`] with only a preview and Apply to Clipboard (Clean URL…).
#[cfg(target_os = "macos")]
fn preview_only_form<'a>(title: &'a str, message: &'a str) -> mac_ui::dialog::Form<'a> {
    mac_ui::dialog::Form {
        title,
        message,
        ok: crate::locale::t("text_apply"),
        options: &[],
        selected: 0,
        option_label: "",
        field_label: "",
        field_default: "",
        field_width: 0.0,
        checks: &[],
        preview_label: crate::locale::t("text_preview"),
    }
}

/// Lines of a preview shown at most, and characters of each.
#[cfg(target_os = "macos")]
const PREVIEW_LINES: usize = 6;
#[cfg(target_os = "macos")]
const PREVIEW_CHARS: usize = 160;

/// `line` cut to [`PREVIEW_CHARS`] characters with `…`.
#[cfg(target_os = "macos")]
fn preview_line(line: &str) -> String {
    let mut chars = line.chars();
    let mut out: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// The preview under Clean URL…: how many parameters go, then the first cleaned URLs.
#[cfg(target_os = "macos")]
fn clean_url_preview(text: &str) -> mac_ui::dialog::Preview {
    let cleaned = text_ops::url_clean::clean(text);
    if cleaned.removed == 0 {
        return mac_ui::dialog::Preview {
            text: crate::locale::t("url_clean_none").to_string(),
            ..Default::default()
        };
    }
    let mut lines = vec![crate::locale::tf("url_clean_removed", &[&cleaned.removed])];
    lines.extend(
        cleaned
            .changed
            .iter()
            .take(PREVIEW_LINES)
            .map(|url| preview_line(url)),
    );
    mac_ui::dialog::Preview {
        text: lines.join("\n"),
        ..Default::default()
    }
}

/// The Format JSON… answer: the layout pop-up (Format, Compress) and Sort keys.
#[cfg(target_os = "macos")]
fn json_style_of(option: usize, checks: &[bool]) -> text_ops::json_style::JsonStyle {
    text_ops::json_style::JsonStyle {
        compress: option == 1,
        sort: checks.first().copied().unwrap_or(false),
    }
}

/// The preview under Format JSON…: the first lines of the result, or where the JSON breaks.
#[cfg(target_os = "macos")]
fn json_preview(text: &str, style: text_ops::json_style::JsonStyle) -> mac_ui::dialog::Preview {
    match text_ops::json_style::restyle(text, style) {
        Err(err) => mac_ui::dialog::Preview {
            text: err.note(),
            marks: Vec::new(),
            error: true,
        },
        Ok(out) if out == text => mac_ui::dialog::Preview {
            text: crate::locale::t("json_style_same").to_string(),
            ..Default::default()
        },
        Ok(out) => {
            let mut lines: Vec<String> = out
                .lines()
                .take(PREVIEW_LINES + 2)
                .map(preview_line)
                .collect();
            if out.lines().nth(PREVIEW_LINES + 2).is_some() {
                lines.push("…".to_string());
            }
            mac_ui::dialog::Preview {
                text: lines.join("\n"),
                ..Default::default()
            }
        }
    }
}

/// The preview under Case style: how many lines change, then the first of them converted.
#[cfg(target_os = "macos")]
fn case_preview(text: &str, style: CaseStyle) -> mac_ui::dialog::Preview {
    let changed = text_ops::case_style::changed_lines(text, style);
    let mut lines = vec![crate::locale::tf("case_changed", &[&changed])];
    lines.extend(
        text.lines()
            .map(|line| text_ops::case_style::convert_line(line, style))
            .filter(|line| !line.trim().is_empty())
            .take(PREVIEW_LINES)
            .map(|line| preview_line(&line)),
    );
    mac_ui::dialog::Preview {
        text: lines.join("\n"),
        ..Default::default()
    }
}

/// The preview under Column…: the first lines of the column, or why there are none.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn column_preview(text: &str, option: usize, typed: &str) -> String {
    let (Some(separator), Some(n)) = (Separator::ALL.get(option), text_ops::parse_count(typed))
    else {
        return crate::locale::t("text_count_invalid").to_string();
    };
    let preview = text_ops::column_preview(text, *separator, n, 6);
    if preview.is_empty() {
        crate::locale::t("text_preview_empty").to_string()
    } else {
        preview
    }
}

/// The preview under Filter…: how many lines match and the first of them with the matches
/// marked, or why the pattern does not work (in red).
#[cfg(target_os = "macos")]
fn filter_preview(
    text: &str,
    pattern: &str,
    invert: bool,
    ignore_case: bool,
) -> mac_ui::dialog::Preview {
    let filter = match text_ops::filter::LineFilter::new(pattern, ignore_case, invert) {
        Ok(filter) => filter,
        Err(reason) => {
            return mac_ui::dialog::Preview {
                text: reason,
                marks: Vec::new(),
                error: true,
            };
        }
    };
    let shown = text_ops::filter::preview(text, &filter, 8);
    let head = crate::locale::tf("filter_kept", &[&shown.kept]);
    let shift = head.len() + 1;
    mac_ui::dialog::Preview {
        text: format!("{head}\n{}", shown.text),
        marks: shown
            .marks
            .into_iter()
            .map(|range| range.start + shift..range.end + shift)
            .collect(),
        error: false,
    }
}
