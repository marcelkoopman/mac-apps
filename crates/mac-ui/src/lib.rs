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
//! - `widgets`: `widgets`, `button`, `fonts` and `text`: AppKit control constructors and wiring
//!   (target/action, text delegates, a scroller Tab skips), the glass push button
//!   ([`button::GlassButton`]), font lookup and attributed-string building ([`text::AttrText`],
//!   byte to UTF-16 ranges). It also adds ⌘-chord and Shift checks on key events to `keys`.
//! - `dialog`: `dialog`, modal `NSAlert` message, confirm, choice, prompt and list pick.
//! - `panel`: `panel`, borderless floating panel creation, activation and placement.
//! - `notify`: `notify`, user notifications.
//! - `file_panel`: `file_panel`, modal `NSOpenPanel` (choose a file) and `NSSavePanel`
//!   (choose a save path).
//! - `progress`: `progress`, an indeterminate spinning `NSProgressIndicator` (busy wheel).
//! - `drop`: `drop`, a view that takes one dropped file or dropped text, judged by type while
//!   the drag moves. Its accept rules (`drop::judge`) build on every platform.
//! - `appkit-full`: all objc2-app-kit and objc2-foundation default features.
//!
//! Always on: `corners` (concentric corner radii), `find` (find in text: matches, counter,
//! steps), `icon`, `keys` (named key codes), `tray` (menu rows, icon set, blink timing) and
//! `wake` (next event-loop deadline). The AppKit modules exist on macOS only.

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
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod button;
pub mod corners;
#[cfg(all(target_os = "macos", feature = "dialog"))]
pub mod dialog;
#[cfg(feature = "drop")]
pub mod drop;
#[cfg(all(target_os = "macos", feature = "file_panel"))]
pub mod file_panel;
pub mod find;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod fonts;
#[cfg(target_os = "macos")]
pub mod glass;
pub mod icon;
pub mod keys;
#[cfg(target_os = "macos")]
pub mod layer;
#[cfg(all(target_os = "macos", feature = "notify"))]
pub mod notify;
#[cfg(all(target_os = "macos", feature = "panel"))]
pub mod panel;
#[cfg(all(target_os = "macos", feature = "progress"))]
pub mod progress;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod text;
#[cfg(feature = "theme")]
pub mod theme;
pub mod tray;
pub mod wake;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod widgets;
