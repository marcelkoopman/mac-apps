//! Parts the history strip on the card (`macos_launcher/strip.rs`) and the version strip in
//! the table window (`macos_table_window/strip.rs`) share, so their cards' × looks and
//! follows the pointer the same way. Nothing is logged.

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, Sel};
use mac_ui::objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSButton, NSScrollView, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSViewBoundsDidChangeNotification,
};
use mac_ui::objc2_foundation::{NSNotificationCenter, NSPoint, NSRect, NSSize, NSString};

use crate::commands;

/// The × in a small card's top-right corner ([`commands::remove_frame`]), saying `label`,
/// hidden until the strip shows it; the caller wires its action and sets its tag.
pub(crate) fn remove_button(mtm: MainThreadMarker, label: &str) -> Retained<NSButton> {
    let (x, y, side) = commands::remove_frame();
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(side, side)),
    );
    button.setBordered(false);
    if let Some(image) = mac_ui::icon::system_symbol("xmark.circle.fill", label) {
        button.setImage(Some(&image));
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::ImageOnly);
        button.setImageScaling(mac_ui::objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
        button.setContentTintColor(Some(&crate::appearance::caption_color()));
    } else {
        button.setTitle(&NSString::from_str("✕"));
    }
    button.setToolTip(Some(&NSString::from_str(label)));
    button.setAccessibilityLabel(Some(&NSString::from_str(label)));
    button.setHidden(true);
    button
}

/// One tracking area on a strip's document view, always active and following its visible
/// part, sends the pointer's entering, moving and leaving to `owner` (`mouseEntered:` /
/// `mouseMoved:` / `mouseExited:`); the clip view's bounds changes (`scrolled`) cover
/// scrolling under a still pointer. So the × follows the pointer and goes when it leaves the
/// strip or the window. `owner` must live as long as the process (a tracking area and the
/// notification centre do not retain it) and answer those selectors.
pub(crate) fn watch_pointer(
    owner: &AnyObject,
    scroll: &NSScrollView,
    document: &NSView,
    scrolled: Sel,
) {
    let options = NSTrackingAreaOptions::MouseEnteredAndExited
        | NSTrackingAreaOptions::MouseMoved
        | NSTrackingAreaOptions::ActiveAlways
        | NSTrackingAreaOptions::InVisibleRect;
    // SAFETY: the owner is a process-lived delegate (the launcher's or the table window's,
    // kept by their DELEGATE slots; a tracking area does not retain its owner) that answers
    // mouseEntered:, mouseMoved: and mouseExited:. No user info. The rect is ignored with
    // InVisibleRect.
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            options,
            Some(owner),
            None,
        )
    };
    document.addTrackingArea(&area);
    let clip = scroll.contentView();
    clip.setPostsBoundsChangedNotifications(true);
    let object: &AnyObject = &clip;
    // SAFETY: `scrolled` is a method of the owner taking the notification; the owner lives
    // as long as the process and the clip view as long as its strip.
    // NSViewBoundsDidChangeNotification is an immutable AppKit constant.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            owner,
            scrolled,
            Some(NSViewBoundsDidChangeNotification),
            Some(object),
        );
    }
}
