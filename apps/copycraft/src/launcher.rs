use std::sync::Mutex;

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
    /// The scan (info, data URL, text, barcodes) of the dropped picture `image` is done.
    DroppedImageScanned {
        image: crate::clipboard::SecretBytes,
        scan: Option<crate::commands::ImageScan>,
    },
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
