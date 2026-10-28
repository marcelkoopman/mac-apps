//! Screenshots (phase 1): *Take Screenshot…* (status menu, search, the screenshot hotkey)
//! asks for Screen Recording when needed, shows `mac_ui`'s selection overlay, takes the
//! picture through ScreenCaptureKit and puts it on the clipboard as a new copy, in history like
//! an Image ▾ step's (Pause holds it back), then shows the thumbnail. In memory only: Wipe and
//! the lock screen drop a result still on its way, the overlay and the thumbnail
//! ([`App::forget_screenshot`]). The log says only that one was taken, or the kind of failure.

use super::*;

use crate::screenshot::Failure;

impl App {
    /// *Take Screenshot…*: the permission first ([`crate::screenshot::next`]), then the
    /// selection.
    pub(crate) fn take_screenshot(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            if !crate::screenshot::available() {
                crate::diag::log(crate::diag::LogEvent::ScreenshotFailed(
                    Failure::Unavailable,
                ));
                // From the hotkey too (the command is greyed out): say so, not silence.
                mac_ui::panel::activate_app(mtm);
                mac_ui::dialog::alert(mtm, crate::locale::t("screenshot_unavailable"), "");
                return;
            }
            use crate::screenshot::Next;
            let next = crate::screenshot::next(
                mac_ui::screenshot::allowed(),
                crate::settings::screenshot_asked(),
            );
            match next {
                Next::Select => self.select_screenshot(mtm),
                Next::Explain => {
                    explain(mtm);
                    crate::settings::set_screenshot_asked();
                    let granted = mac_ui::screenshot::ask();
                    if crate::screenshot::after_question(granted) == Some(Next::Select) {
                        self.select_screenshot(mtm);
                    }
                }
                Next::Denied => denied(mtm),
            }
        }
    }

    /// Show the overlay; what is picked is captured off the main thread and comes back as
    /// [`UserEvent::ScreenshotTaken`].
    #[cfg(target_os = "macos")]
    fn select_screenshot(&mut self, mtm: mac_ui::objc2::MainThreadMarker) {
        self.shot_generation = self.shot_generation.wrapping_add(1);
        let generation = self.shot_generation;
        let include_own = crate::settings::screenshot_includes_own();
        // CopyCraft's own windows stay out unless the user lets them in (Settings › Privacy);
        // that only lets in the card and the table window: Settings, About and Welcome stay out
        // always. By window id too, as ScreenCaptureKit ignores `sharingType` since macOS 15.
        let leave_out = mac_ui::screenshot::LeaveOut {
            pid: (!include_own).then(mac_ui::screenshot::own_pid),
            windows: Vec::new(),
        }
        .with(kept_out_windows(mtm, include_own));
        launcher::set_screenshot_includes(include_own);
        let picker = leave_out.clone();
        mac_ui::screenshot::select(mtm, &picker, move |pick| {
            let Some(pick) = pick else {
                launcher::emit(UserEvent::ScreenshotTaken {
                    generation,
                    result: Err(None),
                });
                return;
            };
            mac_ui::screenshot::capture(pick.into(), leave_out, move |result| {
                let result = match result {
                    Ok(mut png) => {
                        SecretBytes::new(std::mem::take(&mut *png)).ok_or(Some(Failure::NoPicture))
                    }
                    Err(failed) => Err(Some(failure_of(failed))),
                };
                launcher::emit(UserEvent::ScreenshotTaken { generation, result });
            });
        });
    }

    /// A screenshot is back: on the clipboard, into history unless Pause holds it, then the
    /// thumbnail. A stale one (Wipe, the lock screen in between) is zeroized instead.
    pub(crate) fn screenshot_taken(
        &mut self,
        generation: u64,
        result: Result<SecretBytes, Option<Failure>>,
    ) {
        if generation != self.shot_generation {
            if let Ok(png) = result {
                png.zeroize();
            }
            return;
        }
        launcher::set_screenshot_includes(false);
        let png = match result {
            Ok(png) => png,
            Err(None) => return,
            Err(Some(why)) => {
                crate::diag::log(crate::diag::LogEvent::ScreenshotFailed(why));
                #[cfg(target_os = "macos")]
                if why == Failure::NotAllowed
                    && let Some(mtm) = mac_ui::objc2::MainThreadMarker::new()
                {
                    denied(mtm);
                }
                return;
            }
        };
        #[cfg(target_os = "macos")]
        self.keep_screenshot(png);
        #[cfg(not(target_os = "macos"))]
        png.zeroize();
    }

    /// Its text first: recognised on a thread, waited for at most
    /// [`crate::screenshot::RECOGNITION_WAIT`], so a token or key on the screen never reaches
    /// the clipboard unconcealed or history at all ([`Self::write_screenshot`]).
    #[cfg(target_os = "macos")]
    fn keep_screenshot(&mut self, png: SecretBytes) {
        let generation = self.shot_generation;
        let source = png.clone();
        let started = self.run_detached(
            "copycraft-shot-text",
            move || {
                let bytes = source.with(|bytes| Zeroizing::new(bytes.to_vec()));
                crate::macos_pasteboard::scan_image_bytes_waiting(
                    bytes,
                    crate::screenshot::RECOGNITION_WAIT,
                )
            },
            move |app: &mut App, scan| app.write_screenshot(generation, png, scan),
        );
        if !started {
            crate::diag::log(crate::diag::LogEvent::ScreenshotFailed(Failure::Clipboard));
        }
    }

    /// The screenshot's text is in (`Some`), or was not within the wait (`None`): the clipboard
    /// (concealed with sensitive or unknown text), then history unless paused or it shows a
    /// token or key, then the thumbnail. With the text not in yet the full scan checks it
    /// again, as for an Image ▾ step's copy, and can conceal it or take it out of history.
    #[cfg(target_os = "macos")]
    fn write_screenshot(
        &mut self,
        generation: u64,
        png: SecretBytes,
        scan: Option<Option<commands::ImageScan>>,
    ) {
        if generation != self.shot_generation {
            png.zeroize();
            if let Some(Some(mut scan)) = scan {
                scan.wipe();
            }
            return;
        }
        let shot = crate::screenshot::verdict(
            scan.as_ref()
                .map(|scan| scan.as_ref().and_then(|scan| scan.ocr.as_deref())),
        );
        if cfg!(debug_assertions) {
            crate::diag::log(crate::diag::LogEvent::ScreenshotChecked {
                text_in: scan.is_some(),
                concealed: shot.mark_concealed,
                admitted: shot.verdict.admitted,
            });
        }
        let keep = crate::screenshot::keep(crate::capture::history_paused(), shot);
        // Straight from the one copy; another only for the thumbnail and a later check.
        if png
            .with(|bytes| self.write_own_image(bytes, shot.mark_concealed))
            .is_err()
        {
            png.zeroize();
            if let Some(Some(mut scan)) = scan {
                scan.wipe();
            }
            crate::diag::log(crate::diag::LogEvent::ScreenshotFailed(Failure::Clipboard));
            return;
        }
        // Concealed only until its text is known: cleared after a minute only for sensitive
        // text (the full scan starts that timer when it finds some).
        if !shot.verdict.concealed {
            self.sensitive_clear = None;
        }
        let change = crate::macos_pasteboard::change_count();
        self.shot_change = Some(change);
        self.last_copy = Some(Instant::now());
        // The new copy is the clipboard's: the card follows it again.
        self.browsing = None;
        let recorded = (keep == StepKeep::Keep)
            .then(|| self.history.record_image_unless(png.clone(), false))
            .flatten();
        if let Some(recorded) = &recorded {
            self.recorded_image_change = Some(change);
            self.image_on_pasteboard = Some((change, recorded.clone()));
            self.current_image = Some(recorded.clone());
        }
        // None with a token or key on it: nothing to drag to another app.
        let thumbnail = shot
            .verdict
            .admitted
            .then(|| Zeroizing::new(png.with(<[u8]>::to_vec)));
        match scan {
            // Known: kept with its entry and for the card, nothing to check again.
            Some(scan) => self.image_step_scanned(change, shot.verdict, recorded, scan),
            None => {
                let bytes = Zeroizing::new(png.with(<[u8]>::to_vec));
                self.scan_step_copy(change, bytes, shot.verdict, recorded);
            }
        }
        // With no entry (paused, a token or key) the bytes are zeroized with this handle, now.
        drop(png);
        crate::diag::log(crate::diag::LogEvent::ScreenshotCaptured);
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
        if let Some(thumbnail) = thumbnail
            && let Some(mtm) = mac_ui::objc2::MainThreadMarker::new()
        {
            let label = crate::locale::t("screenshot_thumbnail_a11y");
            mac_ui::screenshot::show_thumbnail(mtm, thumbnail, label, || {
                launcher::emit(UserEvent::ScreenshotOpened);
            });
        }
    }

    /// The thumbnail was clicked: the card, on the newest copy (the screenshot).
    pub(crate) fn open_screenshot(&mut self) {
        self.history_cursor = 0;
        self.browsing = None;
        if launcher::is_open() {
            self.refresh_popup();
        } else {
            self.summon_popup();
        }
    }

    /// Wipe, the lock screen, the session ending: a screenshot on its way is dropped, the
    /// overlay and the thumbnail (and its copy of the picture) go.
    pub(crate) fn forget_screenshot(&mut self) {
        self.shot_generation = self.shot_generation.wrapping_add(1);
        self.shot_change = None;
        launcher::set_screenshot_includes(false);
        #[cfg(target_os = "macos")]
        {
            mac_ui::screenshot::cancel_selection();
            mac_ui::screenshot::close_thumbnail();
        }
    }
}

#[cfg(target_os = "macos")]
fn failure_of(failed: mac_ui::screenshot::Failed) -> Failure {
    match failed {
        mac_ui::screenshot::Failed::Unavailable => Failure::Unavailable,
        mac_ui::screenshot::Failed::NotAllowed => Failure::NotAllowed,
        mac_ui::screenshot::Failed::NoPicture => Failure::NoPicture,
    }
}

/// Why CopyCraft asks for Screen Recording, before the system's question. One button,
/// *Continue*: the choice is made in the system's question that follows.
#[cfg(target_os = "macos")]
fn explain(mtm: mac_ui::objc2::MainThreadMarker) {
    mac_ui::panel::activate_app(mtm);
    mac_ui::dialog::buttons(
        mtm,
        crate::locale::t("screenshot_ask_title"),
        crate::locale::t("screenshot_ask_message"),
        &[crate::locale::t("notify_ask_ok")],
    );
}

/// Screen Recording is off: say where to turn it on, with a button that opens that pane.
/// Outside an .app bundle (`cargo run`) macOS counts the app that started CopyCraft (a
/// terminal or editor), so the text names that app instead.
#[cfg(target_os = "macos")]
fn denied(mtm: mac_ui::objc2::MainThreadMarker) {
    mac_ui::panel::activate_app(mtm);
    let (title, message) = if mac_ui::notify::is_bundled() {
        (
            crate::locale::t("screenshot_denied_title"),
            crate::locale::t("screenshot_denied_message"),
        )
    } else {
        (
            crate::locale::t("screenshot_denied_unbundled_title"),
            crate::locale::t("screenshot_denied_unbundled_message"),
        )
    };
    let answer = mac_ui::dialog::buttons(
        mtm,
        title,
        message,
        &[
            crate::locale::t("screenshot_open_privacy"),
            crate::locale::t("cancel"),
        ],
    );
    if answer == Some(0) {
        crate::system_settings::open(crate::system_settings::Pane::ScreenRecordingPrivacy);
    }
}

/// The window numbers of CopyCraft's windows a screenshot leaves out: all of them, except the
/// card and the table window when `include_own` lets those in.
#[cfg(target_os = "macos")]
fn kept_out_windows(mtm: mac_ui::objc2::MainThreadMarker, include_own: bool) -> Vec<u32> {
    let let_in: Vec<isize> = if include_own {
        [
            crate::macos_launcher::window_number(),
            crate::macos_table_window::window_number(),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else {
        Vec::new()
    };
    mac_ui::objc2_app_kit::NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .map(|window| window.windowNumber())
        .filter(|number| !let_in.contains(number))
        .filter_map(|number| u32::try_from(number).ok())
        .collect()
}
