//! Image actions: Image ▾ steps and their jobs. A step's result is a new copy, like a Text ▾
//! step's: on the clipboard and in history, shown on the card; ⌘Z shows the copy it was taken
//! on ([`CopySteps`]).

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
            if self.pasteboard_suppressed() {
                return self.history.image(self.history_cursor);
            }
            if !ClipboardView::from_os().is_image() {
                return None;
            }
            self.history_image_on_pasteboard(crate::macos_pasteboard::change_count())
        }
        #[cfg(not(target_os = "macos"))]
        None
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
        let source = job.source();
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
                    source,
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
        let source = self.image_source();
        let current = run.filter(|run| {
            source
                .as_ref()
                .is_some_and(|bytes| bytes.allocation_id() == run.source)
        });
        match (current, done.result) {
            (Some(_), Ok(finished)) => {
                if let Some(source) = source {
                    self.image_stepped(&source, finished);
                }
            }
            (None, Ok(mut finished)) => finished.wipe(),
            (_, Err(crate::image_edit::ImageError::Cancelled)) => {}
            // A step that changes nothing ("Already that size or smaller"): a note, no copy.
            (Some(run), Err(e)) => self.image_note = Some((run.source, e.to_string())),
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
            let id = self.image_id(source);
            let known = self.known_image_scan(source);
            let verdict = crate::sensitivity::picture_verdict(
                known.as_ref().and_then(|scan| scan.ocr.as_deref()),
            );
            if let Some(mut known) = known {
                known.wipe();
            }
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
            let held = self.history.held();
            let recorded = if verdict.admitted {
                self.history.record_secret_image(done.picture.clone())
            } else {
                None
            };
            self.scan_step_copy(change, png, verdict, recorded.clone());
            let Some(recorded) = recorded else {
                // Not kept: the pasteboard has its own copy of the picture.
                done.wipe();
                let note = if held {
                    crate::locale::t("text_not_kept")
                } else if verdict.admitted {
                    crate::locale::t("image_not_kept_large")
                } else {
                    crate::locale::t("text_not_kept_secret")
                };
                self.image_note = Some((source.allocation_id(), note.to_string()));
                return;
            };
            // A dropped or chosen picture: the card leaves it for the new copy.
            if self.opened.is_some() {
                self.leave_opened();
            }
            self.recorded_image_change = Some(change);
            self.image_on_pasteboard = Some((change, recorded.clone()));
            self.current_image = Some(recorded);
            self.history_cursor = 0;
            self.clipboard_cursor = None;
            self.refresh_status_menu();
            if let Some(new_id) = self.history.id_at(0) {
                self.image_steps = Some(CopySteps::stepped(
                    self.image_steps.take(),
                    id,
                    new_id,
                    done.label,
                ));
            }
            self.image_note = None;
            self.reset_view();
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
        self.image_on_pasteboard
            .as_ref()
            .filter(|(at, bytes)| *at == change && bytes.same_allocation(source))
            .and(self.image_scan_change)
            .filter(|at| *at == change)
            .and(self.image_scan.clone())
    }

    /// Scan a step's copy (`png`, on the clipboard as `change`, made with `verdict`; in history
    /// as `recorded`) on a thread, as the card would; [`Self::image_step_scanned`] takes it.
    #[cfg(target_os = "macos")]
    fn scan_step_copy(
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
            if let Some(bytes) = kept {
                self.retract_step_copy(change, &bytes);
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

    /// Take a step's copy (`bytes`, clipboard change `change`) out of history again: it shows a
    /// token or key. Gone for good (zeroized, not kept for ⌘Z); the clipboard keeps its
    /// concealed copy, which history skips.
    #[cfg(target_os = "macos")]
    fn retract_step_copy(&mut self, change: isize, bytes: &SecretBytes) {
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
        if self.recorded_image_change == Some(change) {
            self.recorded_image_change = None;
        }
        self.skip_image_change = Some(change);
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
            crate::diag::Skip::Credential,
        ));
        self.after_history_edit();
    }

    /// Image ▾ for the picture card: whether a step runs on it, its note, and where ⌘Z goes.
    pub(crate) fn attach_image(&mut self, data: &mut LaunchData) {
        if data.subject_kind != SubjectKind::Image {
            return;
        }
        let Some(bytes) = self.image_source() else {
            return;
        };
        let entry = bytes.allocation_id();
        let working = self
            .image_job
            .as_ref()
            .is_some_and(|run| run.source == entry);
        let note = self
            .image_note
            .as_ref()
            .filter(|(at, _)| *at == entry)
            .map(|(_, note)| note.clone());
        let id = self.image_id(&bytes);
        let steps = (id != NOT_IN_HISTORY)
            .then(|| CopySteps::showing(self.image_steps.as_ref(), id))
            .flatten();
        data.image_edit = Some(commands::ImageShown {
            entry: entry as u64,
            label: steps
                .filter(|steps| steps.cursor > 0)
                .and_then(|steps| steps.labels.get(steps.cursor).cloned()),
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
        });
    }
}
