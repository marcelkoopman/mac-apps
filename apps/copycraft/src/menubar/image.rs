//! Image actions: Image ▾ steps and their jobs. A step's result is a new copy, like a Text ▾
//! step's: on the clipboard and in history, shown on the card; ⌘Z shows the copy it was taken
//! on ([`CopySteps`]).

#[cfg(target_os = "macos")]
use super::source::{LateCheck, late_check};
use super::*;

impl App {
    /// The pasteboard's current change is not read: history is off, paused, ignore-next, or an
    /// ignored app. CopyCraft's own write is never suppressed while history is on.
    pub(crate) fn pasteboard_suppressed(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            // History off: the card never shows the pasteboard, only what was pasted or dropped.
            if !crate::macos_pasteboard::background_read_allowed() {
                return true;
            }
            if crate::macos_pasteboard::current_marks().own {
                return false;
            }
            let change = crate::macos_pasteboard::change_count();
            matches!(ClipboardView::from_os(), ClipboardView::Held)
                || crate::capture::blocks(change)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// The card shows a picture (copied or dropped).
    pub(crate) fn shows_image(&self) -> bool {
        match &self.opened {
            Some(opened) => opened.image.is_some(),
            None if self.browsing.is_some() => self
                .browsed_index()
                .is_some_and(|at| self.history.image(at).is_some()),
            None if self.pasteboard_suppressed() => {
                self.history.image(self.history_cursor).is_some()
            }
            None => ClipboardView::from_os().is_image(),
        }
    }

    /// The picture on the card: the dropped or chosen picture, a history entry browsed, or the
    /// clipboard's when history holds it.
    pub(crate) fn image_source(&self) -> Option<SecretBytes> {
        if let Some(opened) = &self.opened {
            return opened.image.as_ref().map(|image| image.bytes.clone());
        }
        if self.browsing.is_some() {
            return self.browsed_index().and_then(|at| self.history.image(at));
        }
        #[cfg(target_os = "macos")]
        {
            let suppressed = self.pasteboard_suppressed();
            clipboard_card_picture(CardPictureGate {
                suppressed,
                entry: self.history.image(self.history_cursor).as_ref(),
                is_image: !suppressed && ClipboardView::from_os().is_image(),
                change: crate::macos_pasteboard::change_count(),
                in_history: self.image_on_pasteboard.as_ref(),
                read: self.card_picture.as_ref(),
            })
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// Read the clipboard's picture for the card, once per copy, when history does not hold it
    /// ([`App::card_picture`]): Image ▾ is then offered at once, with history off for the
    /// clipboard (not shown then), held, or not recorded yet. The pasteboard is read here, on
    /// the main thread, under the same rules as the card's scan; the bytes are made on a
    /// worker, and a quick picture is back within `QUICK`, so the first card built has Image ▾.
    pub(crate) fn ensure_card_picture(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let suppressed = self.pasteboard_suppressed();
            let change = crate::macos_pasteboard::change_count();
            let in_history = self.history_image_on_pasteboard(change).is_some();
            let read = self.card_picture.as_ref().map(|(at, _)| *at);
            if !card_picture_due(
                suppressed,
                change,
                in_history,
                read,
                self.card_picture_asked,
            ) {
                return;
            }
            self.card_picture_asked = Some(change);
            self.drop_card_picture();
            let Some(input) = crate::macos_pasteboard::card_image_input() else {
                return;
            };
            let ready = self.start_work(
                work::Lane::CardPicture,
                work::QUICK,
                move || {
                    crate::macos_pasteboard::image_input_bytes(input).and_then(SecretBytes::new)
                },
                move |app: &mut App, bytes| {
                    if app.keep_card_picture(change, bytes) && launcher::is_open() {
                        app.refresh_popup();
                    }
                },
            );
            if let Some(bytes) = ready {
                self.keep_card_picture(change, bytes);
            }
        }
    }

    /// The clipboard's picture read for the card at `change` is in. `false` (zeroized) when
    /// the clipboard changed meanwhile or nothing could be read.
    #[cfg(target_os = "macos")]
    fn keep_card_picture(&mut self, change: isize, bytes: Option<SecretBytes>) -> bool {
        let Some(bytes) = bytes else {
            return false;
        };
        if crate::macos_pasteboard::change_count() != change
            || self.card_picture_asked != Some(change)
        {
            bytes.zeroize();
            return false;
        }
        self.drop_card_picture();
        self.card_picture = Some((change, bytes));
        true
    }

    /// The text of the clipboard's picture at `change` is in: the card's scan, or (`own`) the
    /// copy's own recognition. History took that picture before it was checked, by a step
    /// ([`App::unchecked_source`]) or the poll ([`App::polled_unchecked`]): a token, key or URL
    /// password takes it out again, with its strip tile, as it would have kept it out. A poll's
    /// copy is settled by its own recognition (the card's scan can only take it out).
    #[cfg(target_os = "macos")]
    pub(crate) fn check_unchecked_source(
        &mut self,
        change: isize,
        recognised: Option<&str>,
        own: bool,
    ) {
        let check = late_check(Some(recognised));
        if let Some((at, source)) = self.unchecked_source.take_if(|(at, _)| *at == change)
            && check == LateCheck::Retract
            && self.history.holds_image(&source)
        {
            self.retract_image(Some(at), &source);
        }
        let mut retracted = false;
        let mut index = 0;
        while index < self.polled_unchecked.len() {
            let (at, _) = &self.polled_unchecked[index];
            if *at != change || (check != LateCheck::Retract && !own) {
                index += 1;
                continue;
            }
            let (at, copy) = self.polled_unchecked.remove(index);
            if check == LateCheck::Retract && self.history.holds_image(&copy) {
                self.retract_image(Some(at), &copy);
                retracted = true;
            }
        }
        if cfg!(debug_assertions) && own {
            crate::diag::log(crate::diag::LogEvent::ImageChecked { retracted });
        }
    }

    /// Forget (and zeroize) the clipboard's picture read for the card.
    pub(crate) fn drop_card_picture(&mut self) {
        if let Some((_, bytes)) = self.card_picture.take() {
            bytes.zeroize();
        }
    }

    /// Debug builds only: log what decides a copied picture card's chips when it changes
    /// ([`crate::diag::LogEvent::ImageGate`]). Only yes or no, never content.
    #[cfg(target_os = "macos")]
    fn log_image_gate(&mut self) {
        if !cfg!(debug_assertions) || self.opened.is_some() || self.browsing.is_some() {
            return;
        }
        let suppressed = self.pasteboard_suppressed();
        let change = crate::macos_pasteboard::change_count();
        let event = crate::diag::LogEvent::ImageGate {
            kept_history: crate::settings::clipboard_history(),
            suppressed,
            image: !suppressed && ClipboardView::from_os().is_image(),
            in_history: self.history_image_on_pasteboard(change).is_some(),
            read: self
                .card_picture
                .as_ref()
                .is_some_and(|(at, _)| *at == change),
            asked: self.card_picture_asked == Some(change),
            scanned: self.image_scan_change == Some(change),
            refused: self.skip_image_change == Some(change),
        };
        if self.image_gate_logged != Some(event) {
            self.image_gate_logged = Some(event);
            crate::diag::log(event);
        }
    }

    /// The history id of the picture `bytes`, or [`NOT_IN_HISTORY`].
    fn image_id(&self, bytes: &SecretBytes) -> u64 {
        self.history.image_id(bytes).unwrap_or(NOT_IN_HISTORY)
    }

    /// Run `op` on the picture shown; its result becomes a new copy when it is done
    /// ([`Self::image_stepped`]).
    pub(crate) fn image_step(&mut self, op: ImageOp) {
        // Picture steps only while the picture is shown (Image ▾ is off on the recognized
        // text); no switch to the picture.
        if !commands::image_steps_allowed(self.card_view) {
            crate::diag::log(crate::diag::LogEvent::ImageStepRefused);
            return;
        }
        let Some(bytes) = self.image_source() else {
            return;
        };
        self.image_note = None;
        self.reset_view();
        self.run_image_job(crate::image_edit::Job::new(op, &bytes));
        self.refresh_popup();
    }

    /// Resize › Custom…: ask for a width or a height, then resize to it.
    pub(crate) fn custom_resize(&mut self) {
        if !commands::image_steps_allowed(self.card_view) {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let mut message = crate::locale::t("resize_prompt").to_string();
            loop {
                let answer =
                    mac_ui::dialog::prompt_text(mtm, crate::locale::t("resize_to"), &message, "");
                launcher::order_front();
                let Some(answer) = answer.map(Zeroizing::new) else {
                    return;
                };
                if answer.trim().is_empty() {
                    return;
                }
                match crate::image_edit::parse_custom_resize(&answer) {
                    Some(to) => {
                        self.image_step(ImageOp::Resize(to));
                        return;
                    }
                    None => {
                        message = crate::locale::t("resize_invalid").to_string();
                    }
                }
            }
        }
    }

    /// ⌘Z and ⇧⌘Z on the picture card: show the copy `to` picks among the ones Image ▾ steps
    /// made, as after a Text ▾ step ([`App::browse_step`]): only shown, the clipboard is left
    /// alone; Copy puts it there.
    pub(crate) fn image_goto(&mut self, to: impl FnOnce(usize) -> Option<usize>) {
        let Some(bytes) = self.image_source() else {
            return;
        };
        let id = self.image_id(&bytes);
        if !self.browse_step(StepsOf::Image, id, to) {
            return;
        }
        // The step running on the picture left is stale now.
        self.cancel_image_job();
        self.image_note = None;
        self.reset_view();
        self.refresh_popup();
    }

    /// Start `job` on a thread, cancelling the one running. Only the step itself runs there:
    /// the new picture is copied as soon as it is back, and scanned afterwards
    /// ([`Self::image_stepped`]).
    pub(crate) fn run_image_job(&mut self, job: crate::image_edit::Job) {
        self.cancel_image_job();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let generation = job.generation;
        let original = job.original();
        let spawned = std::thread::Builder::new()
            .name("copycraft-image".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let result = job.run(&flag, &crate::macos_image_edit::ImageIoCodec);
                #[cfg(not(target_os = "macos"))]
                let result: Result<crate::image_edit::JobDone, _> = {
                    drop((job, &flag));
                    Err(crate::image_edit::ImageError::Failed(
                        "Needs macOS".to_string(),
                    ))
                };
                launcher::emit(UserEvent::ImageDone(Box::new(launcher::ImageDone {
                    generation,
                    result,
                })));
            });
        match spawned {
            Ok(_) => {
                self.image_job = Some(ImageRun {
                    started: Background::now(),
                    generation,
                    cancel,
                    original,
                });
            }
            Err(e) => self.work_failed(crate::diag::Work::ImageJob, &e),
        }
    }

    pub(crate) fn cancel_image_job(&mut self) {
        if let Some(run) = self.image_job.take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        self.stop_spinner_when_idle();
    }

    /// A finished Image ▾ job is back on the main thread. Dropped (and wiped) when stale: a
    /// newer job started, or the card shows another picture by now, as a Text ▾ result is.
    pub(crate) fn finish_image_job(&mut self, done: launcher::ImageDone) {
        let run = self
            .image_job
            .take_if(|run| run.generation == done.generation);
        if let Some(run) = &run {
            crate::diag::log(crate::diag::LogEvent::ImageJobDone {
                ms: run.started.elapsed_ms() as u64,
            });
        }
        self.stop_spinner_when_idle();
        // The step's picture is still on the card. History may have recorded the clipboard's
        // copy while a step ran on the picture read for the card: that picture still counts.
        let source = run.as_ref().and_then(|run| {
            let matches = |bytes: &SecretBytes| bytes.allocation_id() == run.source();
            self.image_source().filter(matches).or_else(|| {
                self.card_picture
                    .as_ref()
                    .map(|(_, bytes)| bytes.clone())
                    .filter(matches)
            })
        });
        let current = run.filter(|_| source.is_some());
        match (current, done.result) {
            (Some(_), Ok(finished)) => {
                if let Some(source) = source {
                    self.image_stepped(&source, finished);
                }
            }
            (None, Ok(mut finished)) => finished.wipe(),
            (_, Err(crate::image_edit::ImageError::Cancelled)) => {}
            // A step that changes nothing ("Already that size or smaller"): a note, no copy.
            (Some(run), Err(e)) => self.image_note = Some((run.source(), e.to_string())),
            (None, Err(_)) => {}
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Copy a step's result (`done`) like a Text ▾ step's: at once, on the clipboard as
    /// copycraft's own copy and into history as a new entry, which the card then shows. It
    /// makes the copy after `source`'s entry for ⌘Z. A step adds no text, so the copy is
    /// concealed or kept out of history on what is known of `source`'s text; the copy's own
    /// recognition then runs on a thread and can only tighten that
    /// ([`Self::image_step_scanned`]). History held, or a token or key in the picture: on the
    /// clipboard only, with a note.
    fn image_stepped(&mut self, source: &SecretBytes, mut done: crate::image_edit::JobDone) {
        #[cfg(target_os = "macos")]
        {
            let mut id = self.image_id(source);
            let known = self.known_image_scan(source);
            let scanned = known.is_some();
            let verdict = crate::sensitivity::picture_verdict(
                known.as_ref().and_then(|scan| scan.ocr.as_deref()),
            );
            if let Some(mut known) = known {
                known.wipe();
            }
            // The clipboard's picture the step was taken on, read for the card but not in
            // history (not recorded yet, or held): it goes in first, so the step's copy has a
            // copy to go back to with ⌘Z and the strip shows both. Its own bytes: the card's
            // are zeroized with the next copy.
            let source_change = crate::macos_pasteboard::change_count();
            let paused = crate::capture::history_paused();
            let held = self.history.held();
            let clipboard_copy = self
                .card_picture
                .as_ref()
                .is_some_and(|(at, bytes)| *at == source_change && bytes.same_allocation(source));
            let in_history = id != NOT_IN_HISTORY;
            let history_on = crate::settings::clipboard_history();
            let source_kept = keep_step_source(
                history_on,
                in_history,
                clipboard_copy,
                held,
                verdict.admitted,
            ) && source
                .with(|bytes| SecretBytes::new(bytes.to_vec()))
                .and_then(|copy| self.history.record_secret_image(copy))
                .map(|kept| {
                    id = self.image_id(&kept);
                    // Its own recording (waiting for its text) is not needed any more; its
                    // text, when in, can still take it out again (`check_unchecked_source`).
                    self.polled_image_wait = None;
                    self.recorded_image_change = Some(source_change);
                    self.unchecked_source = (!scanned).then_some((source_change, kept));
                })
                .is_some();
            let keep = step_keep(paused, verdict.admitted);
            let png = Zeroizing::new(done.picture.with(<[u8]>::to_vec));
            if let Err(e) = self.write_own_image(&png, verdict.concealed) {
                done.wipe();
                crate::diag::log(crate::diag::LogEvent::CopyFailed(
                    crate::diag::ErrorKind::of(&e),
                ));
                return;
            }
            let change = crate::macos_pasteboard::change_count();
            self.last_copy = Some(Instant::now());
            // The new copy is the clipboard's: the card follows it again.
            self.browsing = None;
            // The user asked for this copy: an ignored front app does not hold it, Pause does.
            let recorded = (keep == StepKeep::Keep)
                .then(|| {
                    self.history
                        .record_image_unless(done.picture.clone(), false)
                })
                .flatten();
            if cfg!(debug_assertions) {
                crate::diag::log(crate::diag::LogEvent::ImageStepKept {
                    source_in_history: in_history,
                    clipboard_copy,
                    source_kept,
                    source_checked: scanned,
                    admitted: verdict.admitted,
                    paused,
                    held,
                    kept: recorded.is_some(),
                });
            }
            self.scan_step_copy(change, png, verdict, recorded.clone());
            let kept = recorded.is_some();
            if let Some(recorded) = recorded {
                self.recorded_image_change = Some(change);
                self.image_on_pasteboard = Some((change, recorded.clone()));
                self.current_image = Some(recorded);
            }
            // A dropped or chosen picture: the card leaves it for the new copy, kept or not.
            let leave_file = self.opened.is_some();
            let label = std::mem::take(&mut done.label);
            self.commit_step(
                StepsOf::Image,
                id,
                leave_file,
                source_kept,
                kept.then_some(Some(label)),
            );
            self.image_note = if kept {
                None
            } else {
                // Not kept: the card shows the clipboard's copy, these very bytes (the
                // pasteboard has its own), as if read for it; the note goes on it.
                self.drop_card_picture();
                self.card_picture_asked = Some(change);
                self.card_picture = Some((change, done.picture));
                let shown = self
                    .image_source()
                    .map_or(source.allocation_id(), |shown| shown.allocation_id());
                Some((shown, step_note(keep, None).to_string()))
            };
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = source;
            done.wipe();
        }
    }

    /// What a scan already found in `source` (the dropped picture's, its entry's, or the
    /// clipboard's), without scanning it again.
    #[cfg(target_os = "macos")]
    fn known_image_scan(&self, source: &SecretBytes) -> Option<commands::ImageScan> {
        if let Some(image) = self.opened.as_ref().and_then(|file| file.image.as_ref())
            && image.bytes.same_allocation(source)
        {
            return image.scan.clone();
        }
        if let Some(scan) = self.history.image_scan(source) {
            return Some(scan);
        }
        let change = crate::macos_pasteboard::change_count();
        // The clipboard's copy, from history or as read for the card: the card's scan of it.
        let on_card = |slot: &Option<(isize, SecretBytes)>| {
            slot.as_ref()
                .is_some_and(|(at, bytes)| *at == change && bytes.same_allocation(source))
        };
        (on_card(&self.image_on_pasteboard) || on_card(&self.card_picture))
            .then_some(())
            .and(self.image_scan_change)
            .filter(|at| *at == change)
            .and(self.image_scan.clone())
    }

    /// Scan a step's copy (`png`, on the clipboard as `change`, made with `verdict`; in history
    /// as `recorded`) on a thread, as the card would; [`Self::image_step_scanned`] takes it.
    #[cfg(target_os = "macos")]
    pub(crate) fn scan_step_copy(
        &mut self,
        change: isize,
        png: Zeroizing<Vec<u8>>,
        verdict: crate::sensitivity::PictureVerdict,
        recorded: Option<SecretBytes>,
    ) {
        // The card's own scan of this change waits for this one.
        self.image_scan_for = Some(change);
        let started = self.run_detached(
            "copycraft-image-step-scan",
            move || crate::macos_pasteboard::scan_image_bytes_within(png),
            move |app: &mut App, scan| app.image_step_scanned(change, verdict, recorded, scan),
        );
        if !started && self.image_scan_for == Some(change) {
            self.image_scan_for = None;
        }
    }

    /// A step's copy is scanned. The scan is kept with its entry (and the card's, while the
    /// clipboard still holds the copy). Its text can only tighten what the copy went in with
    /// ([`crate::sensitivity::picture_recheck`]): sensitive text conceals the clipboard's copy
    /// (when it is still there); a token or key takes the entry out of history again, as a
    /// text with one never goes in.
    #[cfg(target_os = "macos")]
    pub(crate) fn image_step_scanned(
        &mut self,
        change: isize,
        verdict: crate::sensitivity::PictureVerdict,
        recorded: Option<SecretBytes>,
        mut scan: Option<commands::ImageScan>,
    ) {
        if self.image_scan_for == Some(change) {
            self.image_scan_for = None;
        }
        let recheck = crate::sensitivity::picture_recheck(
            verdict,
            scan.as_ref().and_then(|scan| scan.ocr.as_deref()),
        );
        let on_clipboard = crate::macos_pasteboard::change_count() == change;
        if recheck.conceal && on_clipboard {
            crate::macos_pasteboard::add_concealed();
            self.clear_sensitive_later(change);
        }
        // Still in history (not wiped, trimmed or deleted meanwhile).
        let kept = recorded.filter(|bytes| self.history.holds_image(bytes));
        if recheck.retract {
            // A screenshot's thumbnail goes too: no drag of it to another app.
            if self.shot_change.take_if(|shot| *shot == change).is_some() {
                mac_ui::screenshot::close_thumbnail();
            }
            if let Some(bytes) = kept {
                self.retract_image(Some(change), &bytes);
            }
            // The picture it was taken on went in unchecked with it: out again too.
            if let Some((at, source)) = self.unchecked_source.take()
                && self.history.holds_image(&source)
            {
                self.retract_image(Some(at), &source);
            }
            if let Some(mut scan) = scan.take() {
                scan.wipe();
            }
        } else if let Some(found) = scan.take() {
            if let Some(bytes) = kept.as_ref() {
                self.history.remember_image_scan(bytes, &found);
            }
            if on_clipboard {
                if let Some(mut old) = self.image_scan.replace(found) {
                    old.wipe();
                }
                self.image_scan_change = Some(change);
            } else {
                let mut found = found;
                found.wipe();
            }
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Take a picture history took before its text was checked (`bytes`; a step's copy or a
    /// copied picture at clipboard change `change`, or a drop: `None`) out of history again: it
    /// shows a token or key. Gone for good (zeroized, not kept for ⌘Z), its strip tile with
    /// it; the clipboard keeps its copy, which history skips.
    pub(crate) fn retract_image(&mut self, change: Option<isize>, bytes: &SecretBytes) {
        let Some(id) = self.history.image_id(bytes) else {
            return;
        };
        let shown = self
            .current_image
            .as_ref()
            .is_some_and(|current| current.same_allocation(bytes));
        if shown {
            self.current_image = None;
        }
        if self
            .image_on_pasteboard
            .as_ref()
            .is_some_and(|(_, on)| on.same_allocation(bytes))
        {
            self.image_on_pasteboard = None;
        }
        if let Some(change) = change {
            if self.recorded_image_change == Some(change) {
                self.recorded_image_change = None;
            }
            self.skip_image_change = Some(change);
        }
        // The card stays on the entry it was on; the copy's own place goes to the next one.
        let at = |history: &clipboard::ClipboardHistory, index: usize| {
            history.id_at(index).filter(|kept| *kept != id)
        };
        let cursor = at(&self.history, self.history_cursor);
        let clipboard_at = self
            .clipboard_cursor
            .and_then(|index| at(&self.history, index));
        if self.browsing == Some(id) {
            self.browsing = None;
        }
        self.history.discard_id(id);
        self.history_cursor = cursor
            .and_then(|kept| self.history.index_of(kept))
            .unwrap_or(0);
        self.clipboard_cursor = clipboard_at.and_then(|kept| self.history.index_of(kept));
        self.history_emptied = self.history.is_empty();
        crate::diag::log(crate::diag::LogEvent::CopySkipped(
            crate::diag::Skip::Refused,
        ));
        self.after_history_edit();
    }

    /// Image ▾ for the picture card: whether a step runs on it, its note, and where ⌘Z goes.
    pub(crate) fn attach_image(&mut self, data: &mut LaunchData) {
        if data.subject_kind != SubjectKind::Image {
            return;
        }
        #[cfg(target_os = "macos")]
        self.log_image_gate();
        let Some(bytes) = self.image_source() else {
            return;
        };
        let entry = bytes.allocation_id();
        let working = self
            .image_job
            .as_ref()
            .is_some_and(|run| run.source() == entry);
        let note = self
            .image_note
            .as_ref()
            .filter(|(at, _)| *at == entry)
            .map(|(_, note)| note.clone());
        let (dropped, location) = self.dropped_picture_file(&bytes);
        let id = self.image_id(&bytes);
        let steps = (id != NOT_IN_HISTORY)
            .then(|| CopySteps::showing(self.image_steps.as_ref(), id))
            .flatten();
        data.image_edit = Some(commands::ImageShown {
            entry: entry as u64,
            label: steps
                .filter(|steps| steps.cursor > 0)
                .and_then(|steps| steps.labels.get(steps.cursor).cloned().flatten()),
            // Only to a copy history still has (a dropped file's picture may not be in it).
            can_undo: steps.is_some_and(|steps| {
                steps.cursor > 0 && self.history.contains_id(steps.ids[steps.cursor - 1])
            }),
            can_redo: steps.is_some_and(|steps| {
                steps
                    .ids
                    .get(steps.cursor + 1)
                    .is_some_and(|&next| self.history.contains_id(next))
            }),
            working,
            note,
            dropped,
            location,
        });
    }

    /// Whether `bytes` on the card are a dropped or chosen picture file (JPEG, HEIC, PNG or
    /// TIFF; not a PDF page, a document's picture or a picture CopyCraft made), and whether it
    /// carries a location. Looked at once per picture ([`App::picture_probe`]); only yes or no
    /// is kept, never logged.
    fn dropped_picture_file(&mut self, bytes: &SecretBytes) -> (bool, bool) {
        let Some(image) = self.opened.as_ref().and_then(|file| {
            file.image
                .as_ref()
                .filter(|image| file.pdf.is_none() && image.pdf_text.is_none())
        }) else {
            return (false, false);
        };
        if !image.bytes.same_allocation(bytes) {
            return (false, false);
        }
        let at = bytes.allocation_id();
        if let Some((probed, found)) = self.picture_probe
            && probed == at
        {
            return found;
        }
        let found = bytes.with(|raw| {
            let file = strippable_picture(raw);
            #[cfg(target_os = "macos")]
            let location = file && crate::macos_image_edit::has_location(raw);
            #[cfg(not(target_os = "macos"))]
            let location = false;
            (file, location)
        });
        self.picture_probe = Some((at, found));
        found
    }

    /// A picture CopyCraft made (a QR code) is on the card: it has no metadata to remove.
    pub(crate) fn mark_made_picture(&mut self) {
        if let Some(image) = self.opened.as_ref().and_then(|file| file.image.as_ref()) {
            self.picture_probe = Some((image.bytes.allocation_id(), (false, false)));
        }
    }
}

/// Whether `bytes` are a picture file whose metadata *Remove metadata* takes off: JPEG, HEIC
/// (or HEIF), PNG or TIFF, by their first bytes. Pure.
pub(crate) fn strippable_picture(bytes: &[u8]) -> bool {
    matches!(
        infer::get(bytes).map(|kind| kind.mime_type()),
        Some("image/jpeg" | "image/png" | "image/tiff" | "image/heif" | "image/heic")
    )
}

/// What Image ▾ looks at for a card that shows no opened or browsed copy.
pub(crate) struct CardPictureGate<'a, T> {
    /// History off, held, or this copy blocked: the card shows history's entry at the cursor
    /// (`entry`), never the clipboard.
    pub suppressed: bool,
    pub entry: Option<&'a T>,
    /// The clipboard holds a picture (only asked when not suppressed).
    pub is_image: bool,
    /// The clipboard's change count now.
    pub change: isize,
    /// The history picture the clipboard holds, at its change count ([`App::image_on_pasteboard`]).
    pub in_history: Option<&'a (isize, T)>,
    /// The clipboard's picture read for the card, at its change count ([`App::card_picture`]).
    pub read: Option<&'a (isize, T)>,
}

/// The picture Image ▾ works on. Suppressed: history's entry at the cursor. Otherwise the
/// clipboard's copy when it is a picture: history's entry for it when history holds this copy
/// (⌘Z then goes back through history), else the picture read for the card. Neither history
/// nor text recognition is waited for. Pure.
pub(crate) fn clipboard_card_picture<T: Clone>(gate: CardPictureGate<'_, T>) -> Option<T> {
    if gate.suppressed {
        return gate.entry.cloned();
    }
    if !gate.is_image {
        return None;
    }
    let at = |slot: Option<&(isize, T)>| {
        slot.filter(|(at, _)| *at == gate.change)
            .map(|(_, value)| value.clone())
    };
    at(gate.in_history).or_else(|| at(gate.read))
}

/// Whether the clipboard's picture at `change` is read for the card now: once per copy (`read`
/// and `asked` are the change counts read and asked for), not when history already holds this
/// copy, and never while the card is suppressed (history off: the card does not show it). Pure.
pub(crate) fn card_picture_due(
    suppressed: bool,
    change: isize,
    in_history: bool,
    read: Option<isize>,
    asked: Option<isize>,
) -> bool {
    !suppressed && !in_history && read != Some(change) && asked != Some(change)
}

/// Whether an Image ▾ step's copy goes into history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepKeep {
    Keep,
    /// Pause is on: the copy is on the clipboard only. An ignored app in front does not hold a
    /// step's copy, text or picture ([`step_keep`]).
    Paused,
    /// The picture's text holds a token, key or URL password.
    Secret,
}

/// [`StepKeep`] for a step's copy (Image ▾ and Text ▾ alike). `paused` is Pause alone: the
/// user asked for the copy, so an ignored front app does not hold it (the copy is CopyCraft's
/// own); the copy it was taken on stays out then ([`keep_step_source`]). Too large is decided
/// by history itself. Pure.
pub(crate) fn step_keep(paused: bool, admitted: bool) -> StepKeep {
    if !admitted {
        StepKeep::Secret
    } else if paused {
        StepKeep::Paused
    } else {
        StepKeep::Keep
    }
}

/// Why a step's copy is not in history, for the meta line: `keep` held it back, or (it was to
/// be kept) history refused it: `refused` says why (a picture's only reason is its size, so
/// `None` reads as too large).
pub(crate) fn step_note(
    keep: StepKeep,
    refused: Option<crate::clipboard::Refused>,
) -> &'static str {
    use crate::clipboard::Refused;
    match (keep, refused) {
        (StepKeep::Paused, _) | (StepKeep::Keep, Some(Refused::Held)) => {
            crate::locale::t("text_not_kept")
        }
        (StepKeep::Secret, _) => crate::locale::t("text_not_kept_secret"),
        (StepKeep::Keep, Some(Refused::Blank)) => crate::locale::t("text_not_kept_blank"),
        (StepKeep::Keep, Some(Refused::TooLarge) | None) => {
            crate::locale::t("image_not_kept_large")
        }
    }
}

/// Whether the copy a step was taken on goes into history before the step's copy: the
/// clipboard's copy read for the card (`clipboard_copy`) that history does not hold yet, under
/// the rules of any copy (not `held` by Pause or an ignored front app, `admitted` on what is
/// known of its text). Only while history is kept (`history_on`); a dropped or chosen picture
/// has its own way in. Pure.
pub(crate) fn keep_step_source(
    history_on: bool,
    in_history: bool,
    clipboard_copy: bool,
    held: bool,
    admitted: bool,
) -> bool {
    history_on && !in_history && clipboard_copy && !held && admitted
}

#[cfg(test)]
mod step_tests {
    use super::*;

    #[test]
    fn a_step_on_a_launch_picture_keeps_the_picture_and_the_copy() {
        // Not in history, read for the card: both go in, the picture first.
        assert!(keep_step_source(true, false, true, false, true));
        assert_eq!(step_keep(false, true), StepKeep::Keep);
    }

    #[test]
    fn a_step_on_a_picture_in_history_adds_only_its_copy() {
        assert!(!keep_step_source(true, true, true, false, true));
        assert!(!keep_step_source(true, true, false, false, true));
        assert_eq!(step_keep(false, true), StepKeep::Keep);
    }

    #[test]
    fn an_ignored_front_app_holds_the_picture_but_not_the_step_copy() {
        // `held` without Pause: the front app is ignored.
        assert!(!keep_step_source(true, false, true, true, true));
        assert_eq!(step_keep(false, true), StepKeep::Keep);
    }

    #[test]
    fn pause_and_a_secret_keep_both_out() {
        assert!(!keep_step_source(true, false, true, true, true));
        assert_eq!(step_keep(true, true), StepKeep::Paused);
        assert!(!keep_step_source(true, false, true, false, false));
        assert_eq!(step_keep(false, false), StepKeep::Secret);
        assert_eq!(step_keep(true, false), StepKeep::Secret);
    }

    #[test]
    fn a_dropped_picture_is_not_kept_as_a_source_here() {
        assert!(!keep_step_source(true, false, false, false, true));
    }

    #[test]
    fn a_step_copy_left_out_says_why() {
        assert_eq!(
            step_note(StepKeep::Paused, None),
            crate::locale::t("text_not_kept")
        );
        assert_eq!(
            step_note(StepKeep::Secret, None),
            crate::locale::t("text_not_kept_secret")
        );
        // Meant to be kept, refused by history: too large, or (a text) blank.
        use crate::clipboard::Refused;
        for refused in [None, Some(Refused::TooLarge)] {
            assert_eq!(
                step_note(StepKeep::Keep, refused),
                crate::locale::t("image_not_kept_large")
            );
        }
        assert_eq!(
            step_note(StepKeep::Keep, Some(Refused::Blank)),
            crate::locale::t("text_not_kept_blank")
        );
        assert_ne!(
            crate::locale::t("text_not_kept_blank"),
            crate::locale::t("image_not_kept_large")
        );
    }

    #[test]
    fn text_and_image_steps_treat_an_ignored_app_alike() {
        // Both pass Pause alone to `step_keep`: an ignored front app (`held`, not paused)
        // keeps the copy the step was taken on out, the step's own copy in.
        let (paused, held) = (false, true);
        assert_eq!(step_keep(paused, true), StepKeep::Keep);
        assert!(!keep_step_source(true, false, true, held, true));
        // Pause keeps both out, for text and pictures.
        assert_eq!(step_keep(true, true), StepKeep::Paused);
        // A copy still being checked for a token is not let in before the check.
        assert!(!keep_step_source(true, false, true, false, false));
    }

    #[test]
    fn strip_metadata_is_for_jpeg_heic_png_and_tiff_files() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0];
        let tiff = b"MM\0\x2a\0\0\0\x08\0\x01".to_vec();
        let mut heic = vec![0, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
        for (name, bytes) in [
            ("png", &png[..]),
            ("jpeg", &jpeg),
            ("tiff", &tiff),
            ("heic", &heic),
        ] {
            assert!(strippable_picture(bytes), "{name}");
        }
        assert!(!strippable_picture(b"GIF89a\x01\0\x01\0"));
        assert!(!strippable_picture(b"%PDF-1.7"));
        assert!(!strippable_picture(b"plain words"));
    }

    #[test]
    fn without_history_the_picture_is_not_kept_as_a_source() {
        assert!(!keep_step_source(false, false, true, false, true));
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn gate<'a>(
        suppressed: bool,
        entry: Option<&'a u8>,
        in_history: Option<&'a (isize, u8)>,
        read: Option<&'a (isize, u8)>,
    ) -> CardPictureGate<'a, u8> {
        CardPictureGate {
            suppressed,
            entry,
            is_image: !suppressed,
            change: 5,
            in_history,
            read,
        }
    }

    #[test]
    fn a_launch_picture_with_history_on_gets_image_steps_before_it_is_recorded() {
        // On the clipboard at launch, not in history yet (recording waits for its text).
        assert!(card_picture_due(false, 5, false, None, None));
        assert_eq!(clipboard_card_picture(gate(false, None, None, None)), None);
        // Read for the card: Image ▾ works on it, history or not.
        assert_eq!(
            clipboard_card_picture(gate(false, None, None, Some(&(5, 1)))),
            Some(1)
        );
        assert!(!card_picture_due(false, 5, false, Some(5), Some(5)));
        // Recorded later: history's entry, for ⌘Z.
        assert_eq!(
            clipboard_card_picture(gate(false, None, Some(&(5, 2)), Some(&(5, 1)))),
            Some(2)
        );
        assert!(!card_picture_due(false, 5, true, None, None));
        // A read of an earlier copy is not this one.
        assert_eq!(
            clipboard_card_picture(gate(false, None, Some(&(4, 2)), Some(&(4, 1)))),
            None
        );
        assert!(card_picture_due(false, 5, false, Some(4), Some(4)));
    }

    #[test]
    fn a_launch_picture_with_history_off_is_not_read_and_the_card_shows_history() {
        // History off: the card shows history (a paste or a drop), never the clipboard, so the
        // clipboard's picture is not read and gives no steps.
        assert!(!card_picture_due(true, 5, false, None, None));
        assert_eq!(
            clipboard_card_picture(gate(true, None, None, Some(&(5, 1)))),
            None
        );
        // A pasted picture in history gets its steps.
        assert_eq!(
            clipboard_card_picture(gate(true, Some(&3), None, None)),
            Some(3)
        );
    }

    #[test]
    fn image_steps_do_not_wait_for_a_scan_still_pending() {
        use crate::commands::{CommandId, ImageScan, image_chips};
        // Scan still pending: Info and Image ▾ at once, no text toggle yet.
        let ids: Vec<_> = image_chips(None, true, false)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec![CommandId::Info, CommandId::ImageMenu]);
        // No picture to work on: only Info.
        let ids: Vec<_> = image_chips(None, false, false)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec![CommandId::Info]);
        // The scan found text: the toggle joins with a card refresh.
        let scan = ImageScan {
            info: String::new(),
            data_url: None,
            ocr: Some("x".into()),
            qr: None,
        };
        let ids: Vec<_> = image_chips(Some(&scan), true, false)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert!(ids.contains(&CommandId::ImageMenu));
        assert!(ids.len() == 3, "Info, text toggle and Image ▾");
    }
}
