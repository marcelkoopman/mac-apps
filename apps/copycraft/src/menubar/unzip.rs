//! *Uitpakken*: the zip on the card extracted into a folder the user chooses, on a thread of
//! its own ([`crate::zip_archive::extract`]). The card says "Extracting…" under the wheel once
//! that takes longer than [`commands::OPENING_SPINNER_DELAY_MS`]; a toast says where the files
//! went or what went wrong. ⌘. or *Uitpakken* again stops it ([`App::unzip_stop`]); nothing
//! is left behind then. Clear History, Wipe and the screen lock stop it too, and Quit stops it
//! and waits a moment for its clean-up ([`App::end_unzip_for_quit`]). Nothing is logged: no
//! names, paths or counts.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

/// How long Quit waits for a stopped extraction to clean up its work folder.
const QUIT_WAIT: Duration = Duration::from_secs(3);

/// A running extraction: its stop flag, and whether it has ended (its work folder gone or
/// moved into place).
#[derive(Default)]
pub(crate) struct UnzipRun {
    pub(crate) stop: AtomicBool,
    ended: Mutex<bool>,
    ended_changed: Condvar,
}

impl UnzipRun {
    fn end(&self) {
        *self.ended.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.ended_changed.notify_all();
    }

    /// Wait until the extraction has ended, at most `limit`. `true` when it has.
    pub(crate) fn wait(&self, limit: Duration) -> bool {
        let ended = self.ended.lock().unwrap_or_else(PoisonError::into_inner);
        let (ended, _) = self
            .ended_changed
            .wait_timeout_while(ended, limit, |ended| !*ended)
            .unwrap_or_else(PoisonError::into_inner);
        *ended
    }
}

impl App {
    /// Ask for a folder, then extract the zip on the card into it.
    pub(crate) fn unzip_opened(&mut self) {
        let Some(file) = self.opened.as_ref() else {
            return;
        };
        let Some(zip) = file.zip.clone() else {
            return;
        };
        let name = file.name.clone();
        // Encrypted, unsafe names, unknown compression: say so before asking for a folder.
        if let Some(blocker) = zip.facts.blocker() {
            super::state::save_feedback(blocker.message());
            return;
        }
        // One at a time: a second click while one runs stops it.
        if self.unzip_started.is_some() {
            self.unzip_stop();
            return;
        }
        let Some(dest) = choose_folder() else {
            return;
        };
        // Free space is checked on the extraction's thread (a slow or sleeping disk never
        // holds up the main thread); too little gives the NoSpace toast, nothing written.
        let run = std::sync::Arc::new(UnzipRun::default());
        self.unzip_stop = Some(run.clone());
        self.unzip_started = Some(Instant::now());
        self.unzip_shown = false;
        let started = self.run_detached(
            "copycraft-unzip",
            move || {
                // A promised zip stays on disk until this is done.
                let _keep = zip.keep;
                let result = crate::zip_archive::extract(&zip.path, &name, &dest, &run.stop);
                run.end();
                result
            },
            |app, result| app.unzip_done(result),
        );
        if !started {
            self.unzip_started = None;
            self.unzip_stop = None;
            super::state::save_feedback(crate::locale::t("work_failed"));
        }
    }

    fn unzip_done(
        &mut self,
        result: Result<crate::zip_archive::Extracted, crate::zip_archive::ZipError>,
    ) {
        self.unzip_started = None;
        self.unzip_stop = None;
        if std::mem::take(&mut self.unzip_shown) {
            launcher::set_unzipping(false);
        }
        let message = match result {
            Ok(extracted) => {
                let folder = extracted
                    .folder
                    .file_name()
                    .map(|name| crate::zip_archive::visible(&name.to_string_lossy()))
                    .unwrap_or_default();
                crate::locale::tf("zip_extracted", &[&folder])
            }
            Err(error) => error.message().to_string(),
        };
        super::state::save_feedback(&message);
    }

    /// Stop the running extraction (⌘., *Uitpakken* again, Clear History, Wipe, the screen
    /// lock). Its result says so; its work folder is removed on its thread.
    pub(crate) fn unzip_stop(&mut self) {
        if let Some(run) = self.unzip_stop.as_ref() {
            run.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Quit: stop a running extraction and wait (at most [`QUIT_WAIT`]) until it has removed
    /// its work folder, so the exit leaves nothing half done behind.
    pub(crate) fn end_unzip_for_quit(&mut self) {
        if let Some(run) = self.unzip_stop.take() {
            run.stop.store(true, Ordering::Relaxed);
            run.wait(QUIT_WAIT);
        }
    }

    /// The card says "Extracting…" once the extraction takes longer than
    /// [`commands::OPENING_SPINNER_DELAY_MS`]. Returns when to look again.
    pub(crate) fn spin_unzip(&mut self, now: Instant) -> Option<Instant> {
        let started = self.unzip_started?;
        if self.unzip_shown {
            return None;
        }
        let due = started + Duration::from_millis(commands::OPENING_SPINNER_DELAY_MS);
        if now >= due {
            self.unzip_shown = true;
            launcher::set_unzipping(true);
            None
        } else {
            Some(due)
        }
    }
}

/// The folder to extract into, from an open panel; `None` when cancelled. The card stays as
/// it was, like after *Kies bestand*.
#[cfg(target_os = "macos")]
fn choose_folder() -> Option<std::path::PathBuf> {
    use mac_ui::objc2::MainThreadMarker;
    use mac_ui::objc2_app_kit::NSApplication;
    let mtm = MainThreadMarker::new()?;
    let previous = NSApplication::sharedApplication(mtm).keyWindow();
    let _wait = crate::macos_launcher::PanelWait::show();
    let chosen = crate::macos_launcher::holding_reveal(|| {
        mac_ui::file_panel::choose_directory(
            mtm,
            crate::locale::t("zip_choose_folder"),
            crate::locale::t("chip_unzip"),
        )
        .unwrap_or_else(|e| {
            crate::diag::log(crate::diag::LogEvent::OpenPanelFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
            None
        })
    });
    match previous.as_ref() {
        Some(window) => {
            mac_ui::panel::activate_app(mtm);
            mac_ui::panel::bring_to_front(window);
        }
        None => launcher::order_front(),
    }
    chosen
}

#[cfg(not(target_os = "macos"))]
fn choose_folder() -> Option<std::path::PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_waits_for_a_stopped_extraction_and_no_longer() {
        let run = std::sync::Arc::new(UnzipRun::default());
        assert!(!run.wait(Duration::from_millis(5)));
        let worker = run.clone();
        let thread = std::thread::spawn(move || {
            while !worker.stop.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            worker.end();
        });
        run.stop.store(true, Ordering::Relaxed);
        assert!(run.wait(Duration::from_secs(10)));
        thread.join().unwrap();
        assert!(run.wait(Duration::ZERO));
    }
}
