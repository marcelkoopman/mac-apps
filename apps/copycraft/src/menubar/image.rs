//! Image actions: image versions, edits and image jobs.

use super::*;

impl App {
    /// The pasteboard's current change is not read: paused, ignore-next, or an ignored app.
    /// Copycraft's own write is never suppressed.
    pub(crate) fn pasteboard_suppressed(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
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
            None if self.pasteboard_suppressed() => {
                self.history.image(self.history_cursor).is_some()
            }
            None => ClipboardView::from_os().is_image(),
        }
    }

    /// The picture on the card, as it came (its versions are worked out from it): the dropped
    /// or chosen picture, or the clipboard's when history holds it.
    pub(crate) fn image_source(&self) -> Option<SecretBytes> {
        if let Some(opened) = &self.opened {
            return opened.image.as_ref().map(|image| image.bytes.clone());
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

    /// The versions of the picture `bytes`: its history entry's, else the chosen file's.
    pub(crate) fn image_versions(&mut self, bytes: &SecretBytes) -> Option<&mut ImageVersions> {
        if self.history.holds_image(bytes) {
            return self.history.image_versions_mut(bytes);
        }
        let opened = self
            .opened
            .as_ref()
            .and_then(|file| file.image.as_ref())
            .is_some_and(|image| image.bytes.same_allocation(bytes));
        opened.then_some(&mut self.opened_image)
    }

    /// The versions whose newest job is `generation`.
    pub(crate) fn image_awaiting(&mut self, generation: u64) -> Option<&mut ImageVersions> {
        if self.opened_image.awaits(generation) {
            return Some(&mut self.opened_image);
        }
        self.history.image_awaiting(generation)
    }

    /// Run `op` on the version shown; the card shows the new version when it is done.
    pub(crate) fn image_step(&mut self, op: ImageOp) {
        let Some(bytes) = self.image_source() else {
            return;
        };
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        match versions.push(op, &bytes) {
            Ok(job) => {
                self.image_note = None;
                self.card_view = CardView::Original;
                self.run_image_job(job);
            }
            Err(e) => self.image_note = Some((bytes.allocation_id(), e.to_string())),
        }
        self.refresh_popup();
    }

    /// Resize › Custom…: ask for a width or a height, then resize to it.
    pub(crate) fn custom_resize(&mut self) {
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

    /// Show the version `to` picks from the one shown (undo, redo, a version from the menu).
    pub(crate) fn image_goto(&mut self, to: impl FnOnce(usize) -> Option<usize>) {
        let Some(bytes) = self.image_source() else {
            return;
        };
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        let Some(index) = to(versions.cursor()) else {
            return;
        };
        let job = versions.goto(index, &bytes);
        self.image_note = None;
        self.card_view = CardView::Original;
        if let Some(job) = job {
            self.run_image_job(job);
        } else {
            // Shown at once (the original, or a version kept): a job still running is stale.
            self.cancel_image_job();
        }
        self.refresh_popup();
    }

    /// Start `job` on a thread, cancelling the one running. The version is scanned (info,
    /// text, barcodes) after it is shown ([`UserEvent::VersionScanned`]).
    pub(crate) fn run_image_job(&mut self, job: crate::image_edit::Job) {
        self.cancel_image_job();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let generation = job.generation;
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
                let picture = result
                    .as_ref()
                    .ok()
                    .map(crate::image_edit::JobDone::picture);
                launcher::emit(UserEvent::ImageDone(Box::new(launcher::ImageDone {
                    generation,
                    result,
                })));
                #[cfg(target_os = "macos")]
                if let Some(picture) = picture
                    && !flag.load(Ordering::Relaxed)
                {
                    let bytes = Zeroizing::new(picture.with(<[u8]>::to_vec));
                    let scan = crate::macos_pasteboard::scan_image_bytes(&bytes);
                    launcher::emit(UserEvent::VersionScanned { picture, scan });
                }
                #[cfg(not(target_os = "macos"))]
                let _ = picture;
            });
        match spawned {
            Ok(_) => {
                self.image_job = Some(ImageRun {
                    started: Background::now(),
                    generation,
                    cancel,
                });
            }
            Err(e) => eprintln!("image job failed to start: {e}"),
        }
    }

    pub(crate) fn cancel_image_job(&mut self) {
        if let Some(run) = self.image_job.take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        self.stop_spinner_when_idle();
    }

    /// Take a finished Image ▾ job into the versions it was made for (if they still wait for
    /// it), and show it.
    pub(crate) fn finish_image_job(&mut self, done: launcher::ImageDone) {
        if let Some(run) = self
            .image_job
            .take_if(|run| run.generation == done.generation)
        {
            eprintln!(
                "copycraft: image job done in {} ms",
                run.started.elapsed_ms()
            );
        }
        self.stop_spinner_when_idle();
        match done.result {
            Ok(finished) => {
                if let Some(versions) = self.image_awaiting(finished.generation()) {
                    versions.finish(finished);
                }
            }
            Err(crate::image_edit::ImageError::Cancelled) => return,
            // A step that changes nothing ("Already that size or smaller"): a note, no version.
            Err(e) => {
                if self.image_awaiting(done.generation).is_none() {
                    return;
                }
                if let Some(bytes) = self.image_source() {
                    self.image_note = Some((bytes.allocation_id(), e.to_string()));
                }
            }
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Keep a version's scan with it, and show it when the card shows that version.
    pub(crate) fn finish_version_scan(
        &mut self,
        picture: &SecretBytes,
        scan: Option<commands::ImageScan>,
    ) {
        let Some(scan) = scan else {
            return;
        };
        let kept = self.opened_image.remember_scan(picture, &scan)
            || self.history.remember_version_scan(picture, &scan);
        if kept && launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The picture versions for the card: `data` is the card for the picture they are of. A
    /// step's version shown replaces the picture, its facts and its scan; one not worked out
    /// (after Wipe of the far entries' pictures) is worked out again.
    pub(crate) fn attach_image(&mut self, data: &mut LaunchData) {
        if data.subject_kind != SubjectKind::Image {
            return;
        }
        let Some(bytes) = self.image_source() else {
            return;
        };
        let running = self.image_job.as_ref().map(|run| run.generation);
        let note = self
            .image_note
            .as_ref()
            .filter(|(entry, _)| *entry == bytes.allocation_id())
            .map(|(_, note)| note.clone());
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        let running_here = running.is_some_and(|generation| versions.awaits(generation));
        let load = if running_here {
            None
        } else {
            versions.load(&bytes)
        };
        let working = load.is_some() || running_here;
        data.image_edit = Some(commands::ImageShown {
            entry: bytes.allocation_id() as u64,
            labels: versions.labels(),
            version: versions.cursor(),
            working,
            note,
        });
        if let Some(shown) = versions.shown() {
            data.picture = Some(shown.picture.clone());
            data.image = Some(shown.facts.clone());
            data.image_scan = shown.scan.clone();
        }
        if let Some(job) = load {
            self.run_image_job(job);
        }
    }

    /// The version of the picture on the card when it is a step's and worked out.
    pub(crate) fn shown_image_version(&mut self) -> Option<crate::image_edit::VersionPicture> {
        let bytes = self.image_source()?;
        self.image_versions(&bytes)?.shown().cloned()
    }
}
