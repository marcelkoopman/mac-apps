//! Shared look & feel and UI frameworks for the mac-apps menubar apps.
//!
//! The framework crates are re-exported so apps can reach them as `mac_ui::tray_icon::...`,
//! `mac_ui::objc2_app_kit::...` and so on, with versions and features pinned in one place.
//!
//! # Features
//!
//! Modules beyond the always-on ones are opt-in, each enabling only the AppKit/Foundation
//! features it needs:
//!
//! - `theme`: `theme`, the appearance switch stored in the user defaults.
//! - `widgets`: `widgets` and `fonts`, AppKit control constructors and font lookup.
//! - `dialog`: `dialog`, modal `NSAlert` message, confirm, choice, prompt and list pick.
//! - `panel`: `panel`, borderless floating panel creation, activation and placement.
//! - `notify`: `notify`, user notifications.
//! - `file_panel`: `file_panel`, modal `NSOpenPanel` (choose a file) and `NSSavePanel`
//!   (choose a save path).
//! - `progress`: `progress`, an indeterminate spinning `NSProgressIndicator` (busy wheel).
//! - `appkit-full`: all objc2-app-kit and objc2-foundation default features.
//!
//! The AppKit modules exist on macOS only.

pub use tray_icon;
pub use winit;

#[cfg(target_os = "macos")]
pub use objc2;
#[cfg(target_os = "macos")]
pub use objc2_app_kit;
#[cfg(target_os = "macos")]
pub use objc2_foundation;

#[cfg(all(target_os = "macos", any(feature = "panel", feature = "dialog")))]
mod activation;
#[cfg(all(target_os = "macos", feature = "dialog"))]
pub mod dialog;
#[cfg(all(target_os = "macos", feature = "file_panel"))]
pub mod file_panel;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod fonts;
#[cfg(target_os = "macos")]
pub mod glass;
pub mod icon;
#[cfg(target_os = "macos")]
pub mod layer;
#[cfg(all(target_os = "macos", feature = "notify"))]
pub mod notify;
#[cfg(all(target_os = "macos", feature = "panel"))]
pub mod panel;
#[cfg(all(target_os = "macos", feature = "progress"))]
pub mod progress;
#[cfg(feature = "theme")]
pub mod theme;
pub mod tray;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod widgets;
