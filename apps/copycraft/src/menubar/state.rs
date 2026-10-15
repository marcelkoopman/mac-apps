//! Clipboard polling, status refresh and small helpers.

use super::*;

impl App {
    pub(crate) fn note_clipboard(&mut self, now: Instant) -> bool {
        // A quiet tick reads only the change count: no copy, hash or record of the text. A
        // promised file can put its picture on the pasteboard without a new change count, so an
        // empty or textless reading is looked at again.
        #[cfg(target_os = "macos")]
        {
            // With Default or Ask a copy is read only while the card is shown.
            let open = launcher::is_open();
            if !open {
                self.paste_explaining = false;
            }
            crate::macos_pasteboard::set_card_shown(open);
            let change = crate::macos_pasteboard::change_count();
            // The frontmost app only matters for a new copy (and for "Add frontmost app",
            // which the status menu refreshes itself): not asked on a quiet tick.
            if self.polled_change != Some(change) {
                crate::capture::observe_front(crate::macos_pasteboard::front_bundle().as_deref());
            }
            // Symbol copy: do not read the pasteboard. A read would classify and label it, and
            // recording it would put the character in history. The card keeps what it showed.
            if crate::symbols::gate(crate::symbols::ignored_change(), change).ignore() {
                self.polled_change = Some(change);
                self.poll_again = false;
                return false;
            }
            if self.polled_change == Some(change)
                && !self.poll_again
                && self.signature.history_len == self.history.len()
            {
                return false;
            }
            // A new copy from another app ends browsing: the card follows it.
            if browse_ends(
                self.polled_change,
                change,
                crate::macos_pasteboard::current_marks().own,
            ) {
                self.browsing = None;
            }
            self.polled_change = Some(change);
        }
        let view = ClipboardView::from_os();
        if matches!(view, ClipboardView::Held) {
            self.poll_again = false;
            self.paste_explaining = false;
            self.sync_tooltip(&view);
            return false;
        }
        // Looked at again every tick, but `current_view` reads a copy at most once
        // (`paste_access::plan`): no reads per tick. A copy waiting for the card is read on the
        // first tick the card is shown.
        self.poll_again = matches!(
            view,
            ClipboardView::Empty
                | ClipboardView::NoText
                | ClipboardView::Denied
                | ClipboardView::Pending
        );
        if !matches!(view, ClipboardView::Pending) {
            self.paste_explaining = false;
        }
        self.record_current(&view);
        let signature = self.clip_signature(&view);
        let image_change = signature.image_change;
        if signature == self.signature {
            return false;
        }
        #[cfg(target_os = "macos")]
        let own = crate::macos_pasteboard::current_marks().own;
        #[cfg(not(target_os = "macos"))]
        let own = false;
        if !own
            && self
                .signature
                .is_new_copy(&signature, matches!(view, ClipboardView::Empty))
        {
            self.blink.start(now);
            // New clipboard content: a file on the card may give way to it when the card closes.
            self.opened_superseded = self.opened.is_some();
        }
        // A chosen file stays on the card while the clipboard keeps its own history.
        if self.opened.is_none() {
            // Copy of a diff is copycraft's own write. Stay on the diff; anything else
            // returns the card to the copy itself.
            if !(own && self.card_view == CardView::Diff) {
                self.reset_view();
            }
            if self
                .image_scan_change
                .is_some_and(|seen| seen != image_change)
            {
                self.image_scan_change = None;
                self.image_scan = None;
            }
        }
        self.signature = signature;
        if let Some(text) = view.text() {
            prewarm(text);
        }
        self.sync_tooltip(&view);
        self.refresh_status_menu();
        true
    }

    pub(crate) fn refresh_status_menu(&self) {
        let entries: Vec<(usize, String)> = self
            .history
            .labels()
            .into_iter()
            .map(|(index, _)| (index, history_title(&self.history, index)))
            .collect();
        self.tray.set_menu(Some(Box::new(status_menu(&entries))));
        // A new menu may reset the status button: aim its click again (next idle).
        self.reinstall_click.set(true);
    }

    /// Swap the menu bar glyph when the blink asks for it. Returns when the blink needs the
    /// event loop again; `None` when it is idle.
    pub(crate) fn show_blink(&mut self, now: Instant) -> Option<Instant> {
        let step = self.blink.tick(now);
        if let Some(glyph) = step.swap
            && let Err(e) = self.icons.show(&self.tray, glyph)
        {
            crate::diag::log(crate::diag::LogEvent::IconBlinkFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
        }
        step.wake
    }

    /// The icon is always the same template (apart from the blink), so the tooltip and the VoiceOver label are where
    /// the detected kind shows.
    pub(crate) fn sync_tooltip(&self, view: &ClipboardView) {
        let tip = if crate::capture::paused() {
            crate::locale::t("tip_paused").to_string()
        } else {
            icon_tip(view)
        };
        if let Err(e) = self.tray.set_tooltip(Some(&tip)) {
            crate::diag::log(crate::diag::LogEvent::TooltipFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
        }
        mac_ui::tray::set_accessibility_label(&self.tray, &tray_label(&tip));
    }
}

pub(crate) fn icon_tip(view: &ClipboardView) -> String {
    match view {
        ClipboardView::Held => {
            if crate::capture::paused() {
                crate::locale::t("tip_paused").to_string()
            } else {
                crate::locale::t("tip_copy_ignored").to_string()
            }
        }
        ClipboardView::Image => crate::locale::t("title_image").to_string(),
        ClipboardView::Empty | ClipboardView::NoText => "Copycraft".to_string(),
        ClipboardView::Denied => crate::paste_access::denied_note().to_string(),
        ClipboardView::Pending => crate::paste_access::pending_note().to_string(),
        ClipboardView::Hidden => clipboard::hidden_content().to_string(),
        ClipboardView::Text(text) => {
            if crate::youtube::video_id(text.as_str()).is_some() {
                crate::locale::t("title_youtube").to_string()
            } else if let Some(host) =
                crate::page_preview::page_url(text.as_str()).and_then(crate::page_preview::host)
            {
                host.to_string()
            } else {
                format::detect(text.as_str()).source_heading().to_string()
            }
        }
    }
}

/// VoiceOver label of the menu bar button: the app name, then the kind from the tooltip.
pub(crate) fn tray_label(tip: &str) -> String {
    if tip == "Copycraft" {
        tip.to_string()
    } else {
        format!("Copycraft, {tip}")
    }
}

pub(crate) fn image_facts(view: &ClipboardView) -> Option<commands::ImageFacts> {
    if !view.is_image() {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_pasteboard::image_facts()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Overwrite and delete pictures handed to Preview (Wipe, lock, sleep, quit, Clear history).
pub(crate) fn discard_preview_files() {
    #[cfg(target_os = "macos")]
    crate::macos_open::discard_previews();
}

/// The clipboard's `text` is a new copy, not the newest history entry read again.
pub(crate) fn is_new_copy(newest: Option<&str>, text: &str) -> bool {
    newest != Some(text)
}

/// Browsing history ends when the pasteboard changed to `change` since `polled` and the change
/// is not copycraft's own write: a new copy.
pub(crate) fn browse_ends(polled: Option<isize>, change: isize, own: bool) -> bool {
    polled.is_some_and(|seen| seen != change) && !own
}

/// A closed card drops a chosen file only once new clipboard content has come in
/// (`superseded`): hiding the card, by the icon, the hotkey or Esc, keeps the file as the last
/// content, and it is there again on the next click. Not while the table window still shows
/// that file: Open table closes the card, and the window stays with the file until it closes.
pub(crate) fn forget_opened_file(
    card_open: bool,
    file_open: bool,
    window_shows_file: bool,
    superseded: bool,
) -> bool {
    file_open && !card_open && !window_shows_file && superseded
}

/// The table window closed: show the card again when that window was opened from a chosen
/// file that is still the card's, and the card is not already open.
pub(crate) fn card_returns_after_window(
    keeps_file: bool,
    file_open: bool,
    card_open: bool,
) -> bool {
    keeps_file && file_open && !card_open
}

/// When unpinned history is due to be forgotten: `minutes` after the last copy. `None` with
/// no time limit (0) or when only pins remain.
pub(crate) fn history_due(
    last_copy: Option<Instant>,
    minutes: u32,
    has_unpinned: bool,
) -> Option<Instant> {
    if minutes == 0 || !has_unpinned {
        return None;
    }
    Some(last_copy? + Duration::from_secs(u64::from(minutes) * 60))
}

/// The next poll: sooner while the card is open and follows the clipboard.
pub(crate) fn poll_interval(card_open: bool) -> Duration {
    if card_open { REFRESH } else { IDLE_REFRESH }
}

/// Classify a large copy on a background thread, so the card finds the format and the
/// sensitive-data labels remembered (see [`crate::memo`]) instead of scanning on the main thread.
pub(crate) fn prewarm(text: &str) {
    if text.len() < PREWARM_LEN {
        return;
    }
    let text = Zeroizing::new(text.to_string());
    // The labels: the background checker. The format and chips here, so the card finds them.
    crate::sensitivity::labeling(&text);
    let spawned = std::thread::Builder::new()
        .name("copycraft-prewarm".into())
        .spawn(move || {
            crate::format::detect(&text);
            crate::commands::warm_chips(&text);
        });
    if let Err(e) = spawned {
        crate::diag::log(crate::diag::LogEvent::PrewarmSkipped(
            crate::diag::ErrorKind::from_text(&e.to_string()),
        ));
    }
}

/// The one place a failed save is reported, whether it failed before or on the save thread.
pub(crate) fn log_save_failure(message: &str) {
    crate::diag::log(crate::diag::LogEvent::SaveFailed(
        crate::diag::ErrorKind::from_text(message),
    ));
    save_feedback(&save_toast(&Err(message.to_string())));
}

/// The confirmation after a save, in the style of Copy's: "Saved", or what went wrong.
pub(crate) fn save_toast(result: &Result<(), String>) -> String {
    match result {
        Ok(()) => crate::locale::t("saved_toast").to_string(),
        Err(message) => {
            let first = message.lines().next().unwrap_or("").trim();
            let short: String = first.chars().take(120).collect();
            if short.is_empty() {
                crate::locale::t("save_failed_toast").to_string()
            } else {
                format!("{}: {short}", crate::locale::t("save_failed_toast"))
            }
        }
    }
}

/// Show `text` with the same toast as Copy's confirmation.
pub(crate) fn save_feedback(text: &str) {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
        mac_ui::toast::show(mtm, text, crate::settings::announce_confirmations());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = text;
}

/// Save job for a dropped picture: the shown scan text, else the picture as it was dropped
/// (its own format, not converted), named after it.
#[cfg(target_os = "macos")]
pub(crate) fn dropped_image_save_job(
    name: &str,
    image: &crate::open_file::OpenedImage,
    view: CardView,
) -> crate::macos_save::SaveJob {
    use crate::macos_save::{SaveContent, SaveJob};
    if let Some(text) = commands::image_view_text(image.scan.as_ref(), view) {
        return SaveJob {
            filename: crate::open_file::save_name(name, "txt"),
            extension: "txt",
            content: SaveContent::Bytes(Zeroizing::new(text.into_bytes())),
        };
    }
    // As it is (its own format, the default), or as PNG, JPEG or HEIC.
    let source = image.bytes.with(crate::macos_pasteboard::image_kind);
    let bytes = Zeroizing::new(image.bytes.with(<[u8]>::to_vec));
    SaveJob::image(
        name,
        bytes,
        false,
        crate::image_edit::image_files(Some(source), true),
    )
}

pub(crate) fn history_title(history: &ClipboardHistory, index: usize) -> String {
    match (history.mark(index), history.byte_len(index)) {
        (Some(mark), Some(len)) => {
            let label = commands::history_label(mark, len);
            if history.is_pinned(index) {
                format!("{}{label}", clipboard::PIN_MARK)
            } else {
                label
            }
        }
        _ => String::new(),
    }
}

/// The bytes of a Base64 picture for the card to draw ([`crate::decode::picture_bytes`]: capped),
/// on the thread. On the Mac the system must also be able to read them, the way it reads a
/// dropped picture ([`crate::macos_pasteboard::image_bytes_facts`]); otherwise the card keeps
/// describing it, and nothing blank is drawn.
pub(crate) fn decoded_picture_bytes(text: &str) -> Option<SecretBytes> {
    use zeroize::Zeroize;
    let mut bytes = crate::decode::picture_bytes(text)?;
    #[cfg(target_os = "macos")]
    if crate::macos_pasteboard::image_bytes_facts(&bytes).is_none() {
        bytes.zeroize();
        return None;
    }
    let picture = SecretBytes::new(std::mem::take(&mut bytes));
    bytes.zeroize();
    picture
}

pub(crate) fn hash_text(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn version_label() -> String {
    tray::version_label("Copycraft", env!("CARGO_PKG_VERSION"))
}

thread_local! {
    static ICON_CLICK_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub(crate) fn icon_click_at() -> Option<Instant> {
    ICON_CLICK_AT.with(Cell::get)
}

pub(crate) fn note_icon_click(now: Instant) {
    ICON_CLICK_AT.with(|slot| slot.set(Some(now)));
}

/// `true` when `now` is a new click, not the second delivery of the click at `last`.
pub(crate) fn icon_click_is_new(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= ICON_CLICK_GAP)
}
