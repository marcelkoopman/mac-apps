use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use mac_ui::winit::event_loop::EventLoopProxy;

use crate::commands::{CommandId, LaunchData};

#[derive(Debug, Clone)]
pub enum UserEvent {
    Run(CommandId),
    /// The menu-bar icon was clicked: open the card under it, or close it.
    ToggleUnderIcon,
    /// Right click on the menu-bar icon: open the status menu.
    ShowStatusMenu,
    /// First launch: show the welcome window once the event loop runs.
    ShowWelcome,
    /// CopyCraft was opened again (Finder, Launchpad, Spotlight) while it runs: open the card
    /// in the middle of the screen, the way back when the menu bar icon is hidden.
    Reopen,
    /// The startup check of Apple Intelligence is done (off the main thread).
    AiChecked(mac_ai::Availability),
    ImageScanned {
        change: isize,
        scan: Option<crate::commands::ImageScan>,
    },
    /// The save thread is done: `Err` holds the message to log.
    SaveFinished(Result<(), String>),
    /// The preview thread wrote the picture. `Err` holds the message to log. A stale
    /// `generation` (Wipe, lock, a newer click) is shredded instead of opened.
    PreviewReady {
        generation: u64,
        result: Result<std::path::PathBuf, String>,
    },
    /// Preview answered the open request of click `generation`: `opened` is false when it
    /// could not open the picture.
    PreviewOpened {
        generation: u64,
        opened: bool,
    },
    /// The "Show all" thread built the whole-text card.
    FullCardReady(Box<FullCard>),
    /// A file was dropped on the card.
    DroppedFile(std::path::PathBuf),
    /// Two or more files were dropped on the card at once: two text files are compared.
    DroppedFiles(Vec<std::path::PathBuf>),
    /// A promised file (from Photos, Mail, …) was dropped and written to a temporary folder,
    /// which goes once it is read (`mac_ui::drop::discard_promised`).
    DroppedPromisedFile(std::path::PathBuf),
    /// Image data was dropped on the card. Zeroized when the event is dropped.
    DroppedImage(zeroize::Zeroizing<Vec<u8>>),
    /// Text was dropped on the card. Zeroized when the event is dropped.
    DroppedText(zeroize::Zeroizing<String>),
    /// ⋯ › New › Color from Screen…: the picked colour as `#RRGGBB`. Zeroized when dropped.
    ColorPicked(zeroize::Zeroizing<String>),
    /// ⌘V in the card: what the clipboard held, read once while the key was handled.
    Pasted(crate::history_mode::Pasted),
    /// "Keep clipboard history" was turned on or off (welcome window or Settings).
    ClipboardHistoryChanged,
    /// The background checker has the sensitivity labels of a long copy.
    LabelsChecked,
    /// macOS detected the kind of a copy not read yet (Default or Ask): the icon's tooltip
    /// names it ([`crate::paste_access::PasteHint`]).
    PasteHint,
    /// The screen locked, the Mac is going to sleep, or the user switched away.
    SessionEnded,
    /// A screenshot is done ([`crate::screenshot`]): its PNG bytes, or why not (`Err(None)`:
    /// the user cancelled). One with a stale `generation` (Wipe, the lock screen) is dropped,
    /// its bytes zeroized.
    ScreenshotTaken {
        generation: u64,
        result: Result<crate::clipboard::SecretBytes, Option<crate::screenshot::Failure>>,
    },
    /// The screenshot's thumbnail was clicked: open the card on it.
    ScreenshotOpened,
    /// A table job ([`crate::table::Job`]) is done, or failed.
    TableDone(Box<TableDone>),
    /// An Image ▾ job ([`crate::image_edit::Job`]) is done, or failed.
    ImageDone(Box<ImageDone>),
    /// Page `index` of the PDF `source` is drawn (`None`: it could not be).
    PdfPageRendered {
        source: crate::clipboard::SecretBytes,
        index: usize,
        page: Option<crate::clipboard::SecretBytes>,
    },
    /// Page `index` of the PDF `source` is drawn again for `zoom` percent (`None`: it could not
    /// be).
    PdfZoomRendered {
        source: crate::clipboard::SecretBytes,
        index: usize,
        zoom: u32,
        page: Option<crate::clipboard::SecretBytes>,
    },
    /// The scan (info, data URL, text, barcodes) of the dropped picture `image` is done.
    DroppedImageScanned {
        image: crate::clipboard::SecretBytes,
        scan: Option<crate::commands::ImageScan>,
    },
    /// The Base64 picture of the copy with hash `key` is decoded on its thread (`None`: not one
    /// the card may or can draw).
    DecodedPicture {
        key: u64,
        picture: Option<crate::clipboard::SecretBytes>,
    },
    /// Something in the table window ([`TableWindowEvent`]).
    TableWindow(TableWindowEvent),
    /// Settings changed (blur, date order): refresh the open card.
    SettingsChanged,
    /// The privacy filter was switched in Settings: mask or unmask the open card and table
    /// window now.
    PrivacyFilterChanged,
    /// The global hotkey was changed in Settings: unregister the old one, register the new.
    HotkeyChanged,
    /// The language setting changed: open windows, the menu and shared panels follow it now.
    LanguageChanged,
    /// The diff thread finished. A mismatched `generation` is dropped (Wipe, swap, a newer diff).
    DiffReady {
        generation: u64,
        outcome: Box<crate::diff::Outcome>,
    },
    /// A Text ▾ job finished for history entry `entry`. A mismatched `generation` is dropped
    /// (a newer job, Wipe).
    /// A background job ([`crate::menubar::WorkDone`]): card, table window, Copy, opened file.
    WorkDone(crate::menubar::WorkDone),
    TextReady {
        generation: u64,
        entry: u64,
        result: Box<crate::text_ops::TextResult>,
        /// The labels of the step's text, found on the step's thread ([`crate::menubar`]'s
        /// `step_labels`). Never the text.
        found: Option<crate::sensitivity::Found>,
        /// [`crate::menubar`]'s `hash_text` of the text the step was taken on: a result for
        /// a copy the card no longer shows is dropped.
        source_hash: u64,
    },
    /// Something about notifications ([`crate::notifications`]).
    Notification(NotificationEvent),
}

/// From the notification center (a system queue), handed to the main thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationEvent {
    /// The user clicked a notification of this kind.
    Clicked(crate::notifications::Kind),
    /// The system's answer about the permission (checked, or asked after the explanation).
    Permission(crate::notifications::Answer),
}

/// What the table window asks for: a command on its table (a step, undo, redo, a version,
/// another reading), or it was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableWindowEvent {
    Run(CommandId),
    Closed,
}

/// Show `view` in the table window, opening it the first time.
/// `sheet`: its grid read into columns and laid out on the worker thread, when it reads as one.
pub(crate) fn show_table_window(
    view: crate::commands::TableWindowView,
    sheet: Option<crate::dataframe::PreparedSheet>,
) {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::show(view, sheet);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (view, sheet);
    }
}

/// The table window's font widths (a wide letter, a figure), measured on the main thread, for
/// laying its grid out on a worker ([`crate::dataframe::prepare_sheet`]). `None` off macOS.
pub fn table_grid_metrics() -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        crate::macos_table_window::grid_metrics()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Close the table window and wipe what it shows (Wipe, lock, its entry gone).
/// Put the table window in front (Open table, also when it is already open).
pub fn bring_table_window_forward() {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::bring_forward();
}

pub fn close_table_window() {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::close();
}

/// Choose Columns… in the table window (Table ▾).
pub fn open_table_window_column_picker() {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::open_column_picker();
}

/// The privacy filter setting changed: forget reveals and re-mask or unmask the open card and
/// table window.
pub fn privacy_filter_changed() {
    #[cfg(target_os = "macos")]
    {
        crate::macos_launcher::privacy_filter_changed();
        crate::macos_table_window::privacy_filter_changed();
    }
}

/// Open the Settings window.
pub fn show_settings() {
    #[cfg(target_os = "macos")]
    crate::macos_settings::show();
}

/// Open the Settings window on its Privacy tab.
pub fn show_privacy_settings() {
    #[cfg(target_os = "macos")]
    crate::macos_settings::show_privacy();
}

/// Close the Settings window.
pub fn close_settings() {
    #[cfg(target_os = "macos")]
    crate::macos_settings::close();
}

/// Text ▾ › Complexity: score the code on the card in the background, then pop the list.
pub fn show_complexity() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_complexity();
}

/// Text ▾ › Apple Intelligence: open the preview over the card, working. `notice` says that the
/// text was cut.
pub fn ai_preview_working(title: &str, notice: Option<&str>) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_ai_working(title, notice);
    #[cfg(not(target_os = "macos"))]
    let _ = (title, notice);
}

/// The model's result, into the open preview.
pub fn ai_preview_result(text: &str) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_ai_result(text);
    #[cfg(not(target_os = "macos"))]
    let _ = text;
}

/// The run failed: `message` on the open preview.
pub fn ai_preview_error(message: &str) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_ai_error(message);
    #[cfg(not(target_os = "macos"))]
    let _ = message;
}

/// Bring the card back as it was, with its AI run and preview (not a new clipboard card).
pub fn restore_card(tray: &mac_ui::tray_icon::TrayIcon) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::restore_under_icon(tray);
    #[cfg(not(target_os = "macos"))]
    let _ = tray;
}

/// Open the card in the middle of the main screen with `card` built from `data` (reopen,
/// when the menu bar icon may be hidden).
pub fn open_centred(data: LaunchData, card: crate::commands::WorkCard) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::open_centred(data, card);
    #[cfg(not(target_os = "macos"))]
    let _ = (data, card);
}

/// Bring the card of an AI run or preview back in the middle of the main screen.
pub fn restore_centred() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::restore_centred();
}

/// Hide the card but keep the AI run and its preview, for the icon to bring back.
pub fn minimize_card() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::minimize();
}

/// Close the preview (when open) and wipe its text.
pub fn ai_preview_close() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::close_ai_preview();
}

/// Text ▾ › Heading Outline on agent instructions: the headings, each going to its line.
pub fn show_agent_outline() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_agent_lists(false);
}

/// Text ▾ › Check AGENTS.md on agent instructions: the warnings, worked out in the background.
pub fn show_agent_check() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::show_agent_lists(true);
}

/// Scroll the card to `line` (1-based) of the text it shows.
pub fn scroll_card_to_line(line: usize) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::scroll_card_to_line(line);
    #[cfg(not(target_os = "macos"))]
    let _ = line;
}

pub fn show_about() {
    #[cfg(target_os = "macos")]
    crate::macos_about::show();
}

/// Idee voorstellen… / Probleem melden…: the form that opens a draft mail.
pub fn show_feedback(kind: crate::mailto::Kind) {
    #[cfg(target_os = "macos")]
    {
        crate::macos_feedback::run(kind);
        order_front();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = kind;
}

/// Open the first-run welcome window (also from the status menu).
pub fn show_welcome() {
    #[cfg(target_os = "macos")]
    crate::macos_welcome::show();
}

pub fn close_about() {
    #[cfg(target_os = "macos")]
    crate::macos_about::close();
}

/// What a table job left: the generation it ran for and its frame, or why it stopped.
#[derive(Clone)]
pub struct TableDone {
    pub generation: u64,
    pub result: Result<crate::table::JobDone, crate::table::TableError>,
}

impl std::fmt::Debug for TableDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableDone")
            .field("generation", &self.generation)
            .field("ok", &self.result.is_ok())
            .finish()
    }
}

/// What an Image ▾ job left: the generation it ran for and its new picture, or why it stopped.
#[derive(Clone)]
pub struct ImageDone {
    pub generation: u64,
    pub result: Result<crate::image_edit::JobDone, crate::image_edit::ImageError>,
}

impl std::fmt::Debug for ImageDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageDone")
            .field("generation", &self.generation)
            .field("ok", &self.result.is_ok())
            .finish()
    }
}

/// The whole-text ("Show all") card for one copied item and view.
#[derive(Debug, Clone)]
pub struct FullCard {
    /// [`crate::commands::content_key`] of the item.
    pub key: u64,
    pub view: crate::commands::CardView,
    pub card: crate::commands::WorkCard,
    /// The line index for the paged view, worked out on the show-all thread; `None` when the
    /// text is short, not plain text, or paging is off in Settings.
    pub pages: Option<std::sync::Arc<crate::text_ops::pager::LineIndex>>,
}

static PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);

pub fn install_proxy(proxy: EventLoopProxy<UserEvent>) {
    *PROXY.lock().expect("launcher proxy") = Some(proxy);
}

pub fn emit(event: UserEvent) {
    let guard = PROXY.lock().expect("launcher proxy");
    if let Some(proxy) = guard.as_ref() {
        let _ = proxy.send_event(event);
    }
}

/// Open the card under the menu bar icon of `tray` (at the pointer when the icon has no frame),
/// with `card` already built from `data`.
pub fn open_under_icon(
    data: LaunchData,
    card: crate::commands::WorkCard,
    tray: &mac_ui::tray_icon::TrayIcon,
) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::open_under_icon(data, card, tray);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (data, card, tray);
    }
}

/// Close the card.
/// Close the card without its fade (Open table: the window comes to the front right after).
pub fn hide_at_once() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::hide_at_once();
}

pub fn hide() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::hide();
}

/// Like [`sync`], with the card already built (off the main thread, or earlier for the same
/// history entry).
/// Page the card for content `key` (a long text, after Show all), or stop paging.
pub fn set_pages(key: u64, pages: Option<std::sync::Arc<crate::text_ops::pager::LineIndex>>) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::set_pages(key, pages);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (key, pages);
    }
}

pub fn sync_with_card(data: LaunchData, card: crate::commands::WorkCard) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::sync_with_card(data, card);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (data, card);
    }
}

/// Show (or hide) the busy spinner over the card while a save runs.
pub fn set_busy(busy: bool) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::set_busy(busy);
    #[cfg(not(target_os = "macos"))]
    let _ = busy;
}

/// "Opening file…" with the busy spinner over the card while a dropped or chosen file is on
/// its way ([`crate::commands::FileOpening`]), or take it away.
pub fn set_opening(opening: bool) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::set_opening(opening);
    #[cfg(not(target_os = "macos"))]
    let _ = opening;
}

/// "Extracting…" with the busy spinner over the card while a zip is extracted, or take it away.
pub fn set_unzipping(unzipping: bool) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::set_unzipping(unzipping);
    #[cfg(not(target_os = "macos"))]
    let _ = unzipping;
}

/// The Preview button's spinner (dimmed, no second click) while Preview opens the picture.
pub fn set_preview_busy(busy: bool) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::set_preview_busy(busy);
    #[cfg(not(target_os = "macos"))]
    let _ = busy;
}

pub fn order_front() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::order_front();
}

pub fn wipe_shown() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::wipe_shown();
}

/// "Allow Screenshots" in the menu bar menu (for testing): off at every start, never stored.
/// The card and the table window read it when they are made, and follow it when it changes.
static ALLOW_SCREENSHOTS: AtomicBool = AtomicBool::new(false);

/// How the card and the table window take part in screenshots, recordings and screen sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Left out (NSWindowSharingNone), the default.
    Excluded,
    /// Captured like other windows (NSWindowSharingReadOnly), while "Allow Screenshots" is on.
    ReadOnly,
}

pub fn capture_for(allow_screenshots: bool) -> Capture {
    if allow_screenshots {
        Capture::ReadOnly
    } else {
        Capture::Excluded
    }
}

pub fn capture() -> Capture {
    capture_for(allows_screenshots() || SCREENSHOT_INCLUDES.load(Ordering::Relaxed))
}

/// While a screenshot of CopyCraft's own (Settings › Privacy › *Show CopyCraft in
/// screenshots*) is being picked and taken, the card and the table window can be captured.
static SCREENSHOT_INCLUDES: AtomicBool = AtomicBool::new(false);

/// Let the card and the table window into the screenshot being taken (`true`), or leave them
/// out again (`false`, also on Wipe and the lock screen); the open windows follow at once.
pub fn set_screenshot_includes(on: bool) {
    if SCREENSHOT_INCLUDES.swap(on, Ordering::Relaxed) == on {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_launcher::apply_capture();
        crate::macos_table_window::apply_capture();
    }
}

pub fn allows_screenshots() -> bool {
    ALLOW_SCREENSHOTS.load(Ordering::Relaxed)
}

/// Turn "Allow Screenshots" on or off; the open windows follow at once.
pub fn set_allow_screenshots(on: bool) {
    ALLOW_SCREENSHOTS.store(on, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    {
        crate::macos_launcher::apply_capture();
        crate::macos_table_window::apply_capture();
    }
}

pub fn is_open() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::macos_launcher::is_open()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
