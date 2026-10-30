//! Screenshots through ScreenCaptureKit, in memory only: the Screen Recording permission
//! ([`allowed`], [`ask`]), the selection overlay ([`select`]: an area, a window or a whole
//! screen; Esc cancels), the capture itself ([`capture`], PNG bytes) and the thumbnail that
//! shows the result for a few seconds ([`show_thumbnail`]: a click opens it, a drag hands it to
//! another app).
//!
//! No file is written and nothing about the picture is kept here beyond the thumbnail's own
//! copy, which [`close_thumbnail`] zeroizes. Window ids and frames are read to find the window
//! under the pointer; names and titles never are.
//!
//! ScreenCaptureKit is reached through objc2-screen-capture-kit's typed bindings; the crate does
//! not link the framework, so it is linked below. [`available`] is `false` when the classes are
//! missing (before macOS 14), so a caller can grey out its command instead of failing, and
//! [`capture`] checks again before it touches them.

mod capture;
mod select;
mod thumbnail;

pub use crate::shot_area::{LeaveOut, Rect};
pub use capture::{Failed, Target, capture};
pub use select::{Pick, cancel_selection, dismiss_selection, select};
pub use thumbnail::{close_thumbnail, show_thumbnail};

use objc2::runtime::AnyClass;

impl From<Pick> for Target {
    fn from(pick: Pick) -> Self {
        match pick {
            Pick::Area { display, rect } => Target::Area { display, rect },
            Pick::Window { id } => Target::Window { id },
            Pick::Screen { display } => Target::Screen { display },
        }
    }
}

#[link(name = "ScreenCaptureKit", kind = "framework")]
unsafe extern "C" {}

/// Whether ScreenCaptureKit's screenshot API is there (macOS 14 and later).
pub fn available() -> bool {
    AnyClass::get(c"SCScreenshotManager").is_some()
        && AnyClass::get(c"SCShareableContent").is_some()
}

/// Whether macOS allows this app to capture the screen now. Asks nothing.
pub fn allowed() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

/// Ask macOS for Screen Recording: the system's own question the first time (it can point to
/// System Settings itself), nothing after that. `true` when allowed.
pub fn ask() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

/// This process's id, for leaving its own windows out.
pub fn own_pid() -> i32 {
    i32::try_from(std::process::id()).unwrap_or(-1)
}
