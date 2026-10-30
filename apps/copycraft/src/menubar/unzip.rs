//! *Uitpakken*: the zip on the card extracted into a folder the user chooses, on a thread of
//! its own ([`crate::zip_archive::extract`]). The card says "Extracting…" under the wheel once
//! that takes longer than [`commands::OPENING_SPINNER_DELAY_MS`]; a toast says where the files
//! went or what went wrong. Nothing is logged: no names, paths or counts.

use super::*;

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
        // One at a time: a second click while one runs does nothing.
        if self.unzip_started.is_some() {
            return;
        }
        let Some(dest) = choose_folder() else {
            return;
        };
        self.unzip_started = Some(Instant::now());
        self.unzip_shown = false;
        let started = self.run_detached(
            "copycraft-unzip",
            move || {
                // A promised zip stays on disk until this is done.
                let _keep = zip.keep;
                crate::zip_archive::extract(&zip.path, &name, &dest)
            },
            |app, result| app.unzip_done(result),
        );
        if !started {
            self.unzip_started = None;
            super::state::save_feedback(crate::locale::t("work_failed"));
        }
    }

    fn unzip_done(
        &mut self,
        result: Result<crate::zip_archive::Extracted, crate::zip_archive::ZipError>,
    ) {
        self.unzip_started = None;
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
