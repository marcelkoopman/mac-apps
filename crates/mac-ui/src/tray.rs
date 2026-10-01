//! Generic rows for a menu bar (tray) menu.

use tray_icon::menu::{IconMenuItem, MenuItem, NativeIcon};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Menu id of [`quit_item`]. Match it in the app's `MenuEvent` loop.
pub const QUIT_ID: &str = "quit";

/// Disabled, informational row such as a hotkey hint or the version.
///
/// On macOS the title is drawn in `secondaryLabelColor`.
pub fn info_item(label: &str) -> MenuItem {
    let item = MenuItem::new(label, false, None);
    #[cfg(target_os = "macos")]
    {
        use objc2::AnyThread;
        use objc2_app_kit::{NSColor, NSForegroundColorAttributeName};
        use objc2_foundation::{NSAttributedString, NSMutableAttributedString, NSRange, NSString};

        let ns = NSString::from_str(label);
        let attr =
            NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
        let all = NSRange::new(0, ns.length());
        // SAFETY: NSForegroundColorAttributeName takes an NSColor value.
        unsafe {
            attr.addAttribute_value_range(
                NSForegroundColorAttributeName,
                &NSColor::secondaryLabelColor(),
                all,
            );
        }
        let title: &NSAttributedString = &attr;
        item.set_attributed_title(Some(title));
    }
    item
}

/// Version row text: `"{app} {version}"`.
///
/// Pass the app's own version, e.g. `env!("CARGO_PKG_VERSION")` expanded in the app crate.
pub fn version_label(app: &str, version: &str) -> String {
    format!("{app} {version}")
}

/// Version row: an [`info_item`] showing [`version_label`].
pub fn version_item(app: &str, version: &str) -> MenuItem {
    info_item(&version_label(app, version))
}

/// Builder with `icon`, drawn as a template image on macOS when `template` is set (see
/// [`set_icon`]). Elsewhere the icon is used as-is.
pub fn with_icon(builder: TrayIconBuilder, icon: Icon, template: bool) -> TrayIconBuilder {
    #[cfg(target_os = "macos")]
    if template {
        return builder.with_icon_templated(icon);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    builder.with_icon(icon)
}

/// Show `icon` in the menu bar. With `template` set, macOS draws it from its alpha channel only
/// (`NSImage.isTemplate`) in the menu bar's colour, so it follows light, dark and tinted menu
/// bars. Without it the colours are kept, for a state that must stand out (an alert). Elsewhere
/// the icon is used as-is.
///
/// # Errors
///
/// When `tray_icon` cannot set the icon.
pub fn set_icon(tray: &TrayIcon, icon: Icon, template: bool) -> tray_icon::Result<()> {
    #[cfg(target_os = "macos")]
    if template {
        return tray.set_icon_templated(Some(icon));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    tray.set_icon(Some(icon))
}

/// VoiceOver label of the menu bar button of `tray`, for an icon without a title or a title
/// that does not say what the app is. The tooltip stays the help text.
///
/// macOS only, on the main thread; elsewhere it does nothing.
pub fn set_accessibility_label(tray: &TrayIcon, label: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSAccessibility;
        use objc2_foundation::NSString;

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(button) = tray.ns_status_item().and_then(|item| item.button(mtm)) else {
            return;
        };
        button.setAccessibilityLabel(Some(&NSString::from_str(label)));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (tray, label);
}

/// Enabled quit row with id [`QUIT_ID`] and the native stop icon.
pub fn quit_item(label: &str) -> IconMenuItem {
    IconMenuItem::with_id_and_native_icon(
        QUIT_ID,
        label,
        true,
        Some(NativeIcon::StopProgress),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::{QUIT_ID, version_label};

    #[test]
    fn version_label_joins_app_and_version() {
        assert_eq!(version_label("Copycraft", "1.2.3"), "Copycraft 1.2.3");
    }

    #[test]
    fn quit_id_is_stable() {
        assert_eq!(QUIT_ID, "quit");
    }
}
