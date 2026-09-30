//! Shared look & feel and UI frameworks for the mac-apps menubar apps.
//!
//! The framework crates are re-exported so apps can reach them as `mac_ui::tray_icon::...`,
//! `mac_ui::objc2_app_kit::...` and so on, with versions and features pinned in one place.

pub use tray_icon;
pub use winit;

#[cfg(target_os = "macos")]
pub use objc2;
#[cfg(target_os = "macos")]
pub use objc2_app_kit;
#[cfg(target_os = "macos")]
pub use objc2_foundation;

#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod fonts;
#[cfg(target_os = "macos")]
pub mod glass;
pub mod icon;
#[cfg(target_os = "macos")]
pub mod layer;
#[cfg(all(target_os = "macos", feature = "panel"))]
pub mod panel;
#[cfg(feature = "theme")]
pub mod theme;
pub mod tray;
#[cfg(all(target_os = "macos", feature = "widgets"))]
pub mod widgets;
