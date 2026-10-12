//! Copy, save and preview actions, and the spinner for slow work.

use super::*;

impl App {
    pub(crate) fn copy_current(&mut self) -> anyhow::Result<()> {
        if self.card_view == CardView::Diff {
            // The diff stays on the card and out of history. The poller sees copycraft's own
            // pasteboard type and does not record it.
            if let Some(text) = self.diff_text() {
                self.write_own(text.as_str())?;
            }
            return Ok(());
        }
        let from_file = self.opened.is_some();
        let version = self.shown_image_version();
        if let Some(version) = version.as_ref()
            && self.card_view == CardView::Original
        {
            return self.copy_image_version(version);
        }
        if let Some(image) = self.opened.as_ref().and_then(|file| file.image.as_ref()) {
            // The dropped picture stays on the card; only the view's text is copied.
            let scan = match &version {
                Some(version) => version.scan.as_ref(),
                None => image.scan.as_ref(),
            };
            if let Some(text) = commands::image_view_text(scan, self.card_view).map(Zeroizing::new)
            {
                self.write_own(text.as_str())?;
                self.record_own_copy(text.as_str());
            }
            return Ok(());
        }
        if !from_file && self.shows_image() {
            return self.copy_image_view();
        }
        let Some(source) = self.source_text().map(Zeroizing::new) else {
            return Ok(());
        };
        let shown = commands::presented_view(source.as_str(), self.card_view);
        let version = (shown == CardView::Dataframe)
            .then(|| self.shown_table_version())
            .flatten();
        // The conversion (format, decode, the table grid) runs on a thread; the clipboard
        // write stays here, or comes when the slow conversion is done.
        let input = source.clone();
        let late_source = source.clone();
        let body = self.start_work(
            work::Lane::Copy,
            work::QUICK,
            move || {
                match version {
                    Some(frame) => crate::dataframe::frame_grid(&frame),
                    None => commands::transformed_text(input.as_str(), shown),
                }
                .map(Zeroizing::new)
            },
            move |app, body| {
                if let Err(e) = app.copy_converted(body, &late_source, from_file) {
                    eprintln!("copy failed: {e:#}");
                }
            },
        );
        match body {
            Some(body) => self.copy_converted(body, &source, from_file),
            None => Ok(()),
        }
    }

    /// Copy the converted `body` of `source` (main thread).
    fn copy_converted(
        &mut self,
        body: Option<Zeroizing<String>>,
        source: &str,
        from_file: bool,
    ) -> anyhow::Result<()> {
        let Some(body) = body else {
            return Ok(());
        };
        // The clipboard already holds this text. A file does not, so Copy still writes it.
        if !from_file && body.as_str() == source {
            return Ok(());
        }
        self.write_own(body.as_str())?;
        self.record_own_copy(body.as_str());
        if from_file {
            return Ok(());
        }
        self.card_view = CardView::Original;
        self.refresh_popup();
        Ok(())
    }

    /// Copy the table version shown in the table window (full grid, same as Dataframe Copy).
    pub(crate) fn copy_table_window(&mut self) -> anyhow::Result<()> {
        let Some(frame) = self.window_table_frame() else {
            return Ok(());
        };
        let body = self.start_work(
            work::Lane::Copy,
            work::QUICK,
            move || crate::dataframe::frame_grid(&frame).map(Zeroizing::new),
            |app, body| {
                if let Err(e) = app.copy_grid(body) {
                    eprintln!("copy failed: {e:#}");
                }
            },
        );
        match body {
            Some(body) => self.copy_grid(body),
            None => Ok(()),
        }
    }

    fn copy_grid(&mut self, body: Option<Zeroizing<String>>) -> anyhow::Result<()> {
        let Some(body) = body else {
            return Ok(());
        };
        self.write_own(body.as_str())?;
        self.record_own_copy(body.as_str());
        Ok(())
    }

    /// Save the table version shown in the table window (CSV / Parquet panel, as on the card).
    pub(crate) fn save_table_window(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.saving.is_some() {
                return;
            }
            // The wheel runs while the frame is copied and the save panel is still loading.
            let _wait = crate::macos_launcher::PanelWait::show();
            let Some(frame) = self.window_table_frame() else {
                return;
            };
            let mut job = crate::macos_save::SaveJob::table(frame);
            if let Some(opened) = self.opened.as_ref() {
                job.filename = crate::open_file::save_name(&opened.name, job.extension);
            }
            // The save panel steals key, and ordering the card front afterwards would
            // steal it again. Stay on this window: a sharp grid stays sharp.
            let started = crate::macos_table_window::keeping_revealed(|| {
                (|| -> anyhow::Result<()> {
                    let Some(path) = job.choose_path()? else {
                        return Ok(());
                    };
                    let content = job.content;
                    std::thread::Builder::new()
                        .name("copycraft-save".into())
                        .spawn(move || {
                            let result = content.write_to(&path).map_err(|e| format!("{e:#}"));
                            launcher::emit(UserEvent::SaveFinished(result));
                        })
                        .context("cannot start the save thread")?;
                    self.saving = Some(Background::now());
                    Ok(())
                })()
            });
            if let Err(e) = started {
                log_save_failure(&format!("{e:#}"));
            }
        }
    }

    /// The DataFrame of the version the table window shows, when it has a frame.
    pub(crate) fn window_table_frame(&mut self) -> Option<polars::prelude::DataFrame> {
        let (_, home) = self.window_source()?;
        let table = self.target_table(TableTarget::Window, home)?;
        table.frame().cloned()
    }

    pub(crate) fn copy_image_view(&mut self) -> anyhow::Result<()> {
        let version = self.shown_image_version();
        let scan = match &version {
            Some(version) => version.scan.as_ref(),
            None => self.image_scan.as_ref(),
        };
        let Some(text) = commands::image_view_text(scan, self.card_view).map(Zeroizing::new) else {
            return Ok(());
        };
        self.card_view = CardView::Original;
        self.write_own(text.as_str())?;
        self.record_own_copy(text.as_str());
        self.refresh_popup();
        Ok(())
    }

    /// Copy the picture version shown: its PNG goes on the pasteboard (as `public.png`) and
    /// into history as a new copy, with the version's scan.
    pub(crate) fn copy_image_version(
        &mut self,
        version: &crate::image_edit::VersionPicture,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let png = Zeroizing::new(version.picture.with(<[u8]>::to_vec));
            crate::macos_pasteboard::write_history_image(&png)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            self.sensitive_clear = None;
            self.conceal_when_checked = None;
            let change = crate::macos_pasteboard::change_count();
            // A copy of its own: the version's bytes go when it is no longer kept.
            let recorded = self.history.record_image(png.to_vec());
            if let (Some(bytes), Some(scan)) = (recorded.as_ref(), version.scan.as_ref()) {
                self.history.remember_image_scan(bytes, scan);
                self.image_scan = Some(scan.clone());
                self.image_scan_change = Some(change);
            }
            self.recorded_image_change = Some(change);
            self.image_on_pasteboard = recorded.clone().map(|bytes| (change, bytes));
            self.current_image = recorded;
            self.last_copy = Some(Instant::now());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
            self.refresh_status_menu();
            self.refresh_popup();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = version;
        Ok(())
    }

    /// Open the picture on the card in Preview. The card has already closed (it floats above
    /// Preview). The write runs off the main thread; opening Preview stays on it.
    pub(crate) fn open_preview(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(bytes) = self.preview_bytes() else {
                eprintln!("preview: no picture to open");
                return;
            };
            let generation = crate::macos_open::begin_preview();
            let spawned = std::thread::Builder::new()
                .name("copycraft-preview".into())
                .spawn(move || {
                    let result =
                        crate::macos_open::write_preview(bytes).map_err(|err| format!("{err:#}"));
                    launcher::emit(UserEvent::PreviewReady { generation, result });
                });
            if let Err(err) = spawned {
                self.work_failed("preview", &err);
            }
        }
    }

    /// The picture the Preview button opens: the version shown, else the original, else the
    /// pasteboard. `None` for text views and for a version that is not worked out yet.
    #[cfg(target_os = "macos")]
    pub(crate) fn preview_bytes(&mut self) -> Option<crate::macos_open::PreviewBytes> {
        use crate::macos_open::PreviewBytes;
        if self.card_view == CardView::Decode {
            let bytes = self.decoded_picture.as_ref()?.bytes.clone()?;
            return Some(PreviewBytes::Shared(bytes));
        }
        if self.card_view != CardView::Original || !self.shows_image() {
            return None;
        }
        if let Some(source) = self.image_source() {
            let stepped = self
                .image_versions(&source)
                .is_some_and(|versions| versions.cursor() > 0);
            if stepped {
                let picture = self.image_versions(&source)?.shown()?.picture.clone();
                return Some(PreviewBytes::Shared(picture));
            }
            return Some(PreviewBytes::Shared(source));
        }
        if let Some(bytes) = crate::macos_pasteboard::current_image_bytes() {
            return Some(PreviewBytes::Owned(Zeroizing::new(bytes)));
        }
        Some(PreviewBytes::Clipboard(
            crate::macos_pasteboard::card_image_input()?,
        ))
    }

    pub(crate) fn save_current(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.saving.is_some() {
                return;
            }
            // The wheel runs while the save panel is still loading. The panel can also
            // resign the app; a revealed card stays revealed across it.
            let _wait = crate::macos_launcher::PanelWait::show();
            let started = crate::macos_launcher::holding_reveal(|| self.start_save());
            launcher::order_front();
            if let Err(e) = started {
                log_save_failure(&format!("{e:#}"));
            }
        }
    }

    /// Ask for a path on the main thread, then build and write the file on a save thread,
    /// which reports back with [`UserEvent::SaveFinished`]. `Ok` also covers a cancelled save
    /// panel and a card with nothing to save.
    #[cfg(target_os = "macos")]
    pub(crate) fn start_save(&mut self) -> anyhow::Result<()> {
        if self.card_view == CardView::Diff {
            let Some(text) = self.diff_text() else {
                return Ok(());
            };
            return self.spawn_save(crate::macos_save::SaveJob {
                filename: "diff.txt".to_string(),
                extension: "txt",
                content: crate::macos_save::SaveContent::Bytes(Zeroizing::new(
                    text.as_bytes().to_vec(),
                )),
            });
        }
        let version = (self.card_view == CardView::Dataframe)
            .then(|| self.shown_table_version())
            .flatten();
        let image_version = self.shown_image_version();
        let job = match version {
            Some(frame) => {
                let mut job = crate::macos_save::SaveJob::table(frame);
                if let Some(opened) = self.opened.as_ref() {
                    job.filename = crate::open_file::save_name(&opened.name, job.extension);
                }
                Some(job)
            }
            None if image_version.is_some() => image_version
                .as_ref()
                .map(|version| self.image_version_save_job(version)),
            None if self.opened.is_some() => self.opened_save_job(),
            None => self.clipboard_save_job(),
        };
        let Some(job) = job else {
            return Ok(());
        };
        self.spawn_save(job)
    }

    /// Ask for a path, then write `job` on the save thread.
    #[cfg(target_os = "macos")]
    pub(crate) fn spawn_save(&mut self, mut job: crate::macos_save::SaveJob) -> anyhow::Result<()> {
        let Some(path) = job.choose_path()? else {
            return Ok(());
        };
        let content = job.content;
        std::thread::Builder::new()
            .name("copycraft-save".into())
            .spawn(move || {
                let result = content.write_to(&path).map_err(|e| format!("{e:#}"));
                launcher::emit(UserEvent::SaveFinished(result));
            })
            .context("cannot start the save thread")?;
        self.saving = Some(Background::now());
        Ok(())
    }

    /// Show the card spinner once background work (a save, "Show all") runs longer than
    /// [`SPINNER_DELAY`]. Returns when the event loop should wake up next.
    pub(crate) fn spin_slow_work(&mut self, now: Instant) -> Instant {
        let wake = now + poll_interval(launcher::is_open());
        if self.spinner_on {
            return wake;
        }
        let table = self
            .table_job
            .as_ref()
            .filter(|run| !run.quiet)
            .map(|run| run.started);
        let image = self.image_job.as_ref().map(|run| run.started);
        let Some(started) = [self.saving, self.loading_all, table, image, self.diffing]
            .into_iter()
            .flatten()
            .map(|run| run.started)
            .chain(self.lanes.slow_since())
            .min()
        else {
            return wake;
        };
        let due = started + SPINNER_DELAY;
        if now >= due {
            self.spinner_on = true;
            launcher::set_busy(true);
            eprintln!(
                "copycraft: spinner shown after {} ms",
                now.duration_since(started).as_millis()
            );
            wake
        } else {
            wake.min(due)
        }
    }

    /// Take the spinner away once no background work is left.
    pub(crate) fn stop_spinner_when_idle(&mut self) {
        if self.spinner_on
            && self.saving.is_none()
            && self.loading_all.is_none()
            && self.table_job.as_ref().is_none_or(|run| run.quiet)
            && self.image_job.is_none()
            && self.diffing.is_none()
            && self.lanes.slow_since().is_none()
        {
            self.spinner_on = false;
            launcher::set_busy(false);
            eprintln!("copycraft: spinner hidden");
        }
    }

    pub(crate) fn finish_preview(
        &mut self,
        generation: u64,
        result: Result<std::path::PathBuf, String>,
    ) {
        #[cfg(target_os = "macos")]
        {
            if !crate::macos_open::preview_is_current(generation) {
                if let Ok(path) = result {
                    crate::macos_open::shred_preview(&path);
                }
                return;
            }
            match result {
                Ok(path) => {
                    if crate::macos_open::open_in_preview(&path) {
                        crate::macos_open::keep_preview(path);
                    } else {
                        crate::macos_open::shred_preview(&path);
                        eprintln!("preview: could not open Voorvertoning");
                    }
                }
                Err(message) => eprintln!("preview failed: {message}"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (generation, result);
    }

    pub(crate) fn finish_save(&mut self, result: Result<(), String>) {
        if let Some(run) = self.saving.take() {
            let shown = if self.spinner_on {
                "spinner shown"
            } else {
                "under the spinner delay, no spinner"
            };
            eprintln!(
                "copycraft: save finished in {} ms ({shown})",
                run.elapsed_ms()
            );
        }
        self.stop_spinner_when_idle();
        if let Err(message) = result {
            log_save_failure(&message);
        }
    }

    /// Save job for the picture version shown (an Image ▾ step's): the shown scan text, else
    /// the picture as PNG, JPEG or HEIC (the Format popup, the source's format first). Named
    /// after the dropped or chosen file.
    #[cfg(target_os = "macos")]
    pub(crate) fn image_version_save_job(
        &self,
        version: &crate::image_edit::VersionPicture,
    ) -> crate::macos_save::SaveJob {
        use crate::macos_save::{SaveContent, SaveJob};
        let name = self
            .opened
            .as_ref()
            .map_or("clipboard", |opened| opened.name.as_str());
        if let Some(text) = commands::image_view_text(version.scan.as_ref(), self.card_view) {
            return SaveJob {
                filename: crate::open_file::save_name(name, "txt"),
                extension: "txt",
                content: SaveContent::Bytes(Zeroizing::new(text.into_bytes())),
            };
        }
        let source = self
            .image_source()
            .map(|bytes| bytes.with(crate::macos_pasteboard::image_kind));
        SaveJob::image(
            name,
            Zeroizing::new(version.picture.with(<[u8]>::to_vec)),
            true,
            crate::image_edit::image_files(source, false),
        )
    }

    /// Save job for the chosen file's text, or a dropped picture, named after the file.
    #[cfg(target_os = "macos")]
    pub(crate) fn opened_save_job(&self) -> Option<crate::macos_save::SaveJob> {
        let opened = self.opened.as_ref()?;
        if let Some(image) = opened.image.as_ref() {
            return Some(dropped_image_save_job(&opened.name, image, self.card_view));
        }
        let source = opened.text.as_deref()?;
        let view = commands::presented_view(source, self.card_view);
        let mut job = crate::macos_save::SaveJob::text(source, view)?;
        job.filename = crate::open_file::save_name(&opened.name, job.extension);
        Some(job)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn clipboard_save_job(&self) -> Option<crate::macos_save::SaveJob> {
        if self.pasteboard_suppressed() {
            if let Some(text) = self.history.get(self.history_cursor) {
                return crate::macos_save::SaveJob::text(
                    text,
                    commands::presented_view(text, self.card_view),
                );
            }
            if self.history.image(self.history_cursor).is_some() {
                return Some(self.image_save_job());
            }
            return None;
        }
        let view = ClipboardView::from_os();
        if view.is_image() {
            return Some(self.image_save_job());
        }
        let source = view.text()?;
        crate::macos_save::SaveJob::text(source, commands::presented_view(source, self.card_view))
    }

    /// Save job for the image card: the shown scan text, else the history entry's picture (as
    /// it is, or as PNG, JPEG or HEIC), else the pasteboard image as PNG or JPEG, else the
    /// decoded preview encoded as PNG (on the save thread).
    #[cfg(target_os = "macos")]
    pub(crate) fn image_save_job(&self) -> crate::macos_save::SaveJob {
        use crate::macos_save::{SaveContent, SaveJob};
        let job = |filename: &str, extension, content| SaveJob {
            filename: filename.to_string(),
            extension,
            content,
        };
        if let Some(text) = commands::image_view_text(self.image_scan.as_ref(), self.card_view) {
            let bytes = Zeroizing::new(text.into_bytes());
            return job("clipboard.txt", "txt", SaveContent::Bytes(bytes));
        }
        // The history entry's picture: as it is, or another format from the Format popup.
        if let Some(original) = self.image_source() {
            let source = original.with(crate::macos_pasteboard::image_kind);
            let bytes = Zeroizing::new(original.with(<[u8]>::to_vec));
            let files = crate::image_edit::image_files(Some(source), true);
            return SaveJob::image("clipboard", bytes, false, files);
        }
        if let Some(bytes) = crate::macos_pasteboard::current_image_bytes().map(Zeroizing::new) {
            if bytes.starts_with(b"\x89PNG") {
                return job("clipboard.png", "png", SaveContent::Bytes(bytes));
            }
            if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                return job("clipboard.jpg", "jpg", SaveContent::Bytes(bytes));
            }
        }
        // Fetched here, on the main thread; the save thread only decodes it.
        let input = crate::macos_pasteboard::card_image_input();
        job("clipboard.png", "png", SaveContent::ClipboardPng(input))
    }

    pub(crate) fn visit_current(&self) {
        let Some(text) = self.source_text() else {
            return;
        };
        let raw = crate::page_preview::page_url(&text)
            .map(str::to_string)
            .or_else(|| crate::youtube::video_id(&text).map(|_| text.trim().to_string()));
        let Some(raw) = raw else {
            return;
        };
        let url = crate::format::format_text(&raw);
        let url = if crate::page_preview::url_has_userinfo(&url) {
            crate::page_preview::strip_userinfo(&url)
        } else {
            url
        };
        if !crate::url_policy::may_visit(&url) {
            eprintln!("visit refused: only http and https links are opened");
            return;
        }
        #[cfg(target_os = "macos")]
        if !crate::macos_open::open_web_url(&url) {
            eprintln!("visit failed: the system could not open the link");
        }
    }
}
