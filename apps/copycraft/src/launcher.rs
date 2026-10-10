use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use mac_ui::winit::event_loop::EventLoopProxy;

use crate::commands::{CommandId, LaunchData};

#[derive(Debug, Clone)]
pub enum UserEvent {
    Run(CommandId),
    ImageScanned {
        change: isize,
        scan: Option<crate::commands::ImageScan>,
    },
    /// The save thread is done: `Err` holds the message to log.
    SaveFinished(Result<(), String>),
    /// The "Show all" thread built the whole-text card.
    FullCardReady(Box<FullCard>),
    /// A file was dropped on the card.
    DroppedFile(std::path::PathBuf),
    /// A promised file (from Photos, Mail, …) was dropped and written to a temporary folder,
    /// which goes once it is read (`mac_ui::drop::discard_promised`).
    DroppedPromisedFile(std::path::PathBuf),
    /// Image data was dropped on the card. Zeroized when the event is dropped.
    DroppedImage(zeroize::Zeroizing<Vec<u8>>),
    /// Text was dropped on the card. Zeroized when the event is dropped.
    DroppedText(zeroize::Zeroizing<String>),
    /// The background checker has the sensitivity labels of a long copy.
    LabelsChecked,
    /// The screen locked, the Mac is going to sleep, or the user switched away.
    SessionEnded,
    /// A table job ([`crate::table::Job`]) is done, or failed.
    TableDone(Box<TableDone>),
    /// An Image ▾ job ([`crate::image_edit::Job`]) is done, or failed.
    ImageDone(Box<ImageDone>),
    /// The scan of the picture version `picture` (an Image ▾ step's) is done.
    VersionScanned {
        picture: crate::clipboard::SecretBytes,
        scan: Option<crate::commands::ImageScan>,
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
}

/// What the table window asks for: a command on its table (a step, undo, redo, a version,
/// another reading), or it was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableWindowEvent {
    Run(CommandId),
    Closed,
}

/// Show `view` in the table window, opening it the first time.
pub fn show_table_window(view: crate::commands::TableWindowView) {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::show(view);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = view;
    }
}

/// Close the table window and wipe what it shows (Wipe, lock, its entry gone).
pub fn close_table_window() {
    #[cfg(target_os = "macos")]
    crate::macos_table_window::close();
}

/// Choose columns… in the table window (Table ▾).
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

/// Close the Settings window.
pub fn close_settings() {
    #[cfg(target_os = "macos")]
    crate::macos_settings::close();
}

pub fn show_about() {
    #[cfg(target_os = "macos")]
    crate::macos_about::show();
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

/// What an Image ▾ job left: the generation it ran for and its version, or why it stopped.
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

pub fn summon(data: LaunchData) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::summon(data);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = data;
    }
}

/// The menu bar icon's click: close an open card, or open it under the icon.
pub fn toggle_under_icon(data: LaunchData, tray: &mac_ui::tray_icon::TrayIcon) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::toggle_under_icon(data, tray);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (data, tray);
    }
}

pub fn reveal(data: LaunchData) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::reveal(data);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = data;
    }
}

pub fn sync(data: LaunchData) {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::sync(data);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = data;
    }
}

/// Like [`sync`], with the card already built (off the main thread, or earlier for the same
/// history entry).
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

pub fn order_front() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::order_front();
}

pub fn wipe_shown() {
    #[cfg(target_os = "macos")]
    crate::macos_launcher::wipe_shown();
}

/// "Allow screenshots" in the menu bar menu (for testing): off at every start, never stored.
/// The card and the table window read it when they are made, and follow it when it changes.
static ALLOW_SCREENSHOTS: AtomicBool = AtomicBool::new(false);

/// How the card and the table window take part in screenshots, recordings and screen sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    /// Left out (NSWindowSharingNone), the default.
    Excluded,
    /// Captured like other windows (NSWindowSharingReadOnly), while "Allow screenshots" is on.
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
    capture_for(allows_screenshots())
}

pub fn allows_screenshots() -> bool {
    ALLOW_SCREENSHOTS.load(Ordering::Relaxed)
}

/// Turn "Allow screenshots" on or off; the open windows follow at once.
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
