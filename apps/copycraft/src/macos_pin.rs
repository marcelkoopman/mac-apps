//! The pin of a pinned copy, drawn once for every place that shows it: the SF Symbol
//! [`commands::PIN_SYMBOL`] described as [`commands::pin_label`] ("Vastgezet" / "Pinned").
//! The history strip's cards wear it as [`pin_badge`]; the history menu's items are to take
//! [`pin_image`] in place of `clipboard::PIN_MARK` (icons HIG plan, point 5).

use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_app_kit::{NSAccessibility, NSBox, NSColor, NSImage, NSImageScaling};
use mac_ui::objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::commands;

/// The pin symbol, with its label as the accessibility description; `None` where SF Symbols
/// are missing.
pub(crate) fn pin_image() -> Option<Retained<NSImage>> {
    mac_ui::icon::system_symbol(commands::PIN_SYMBOL, commands::pin_label())
}

/// The pin on a round backing `side` points wide, which keeps it readable on text and on a
/// picture alike. Drawing only: the card's label says "pinned" ([`commands::thumb_mark`]).
pub(crate) fn pin_badge(mtm: MainThreadMarker, side: f64) -> Retained<NSBox> {
    let backing = mac_ui::widgets::filled_box(
        mtm,
        side / 2.0,
        &NSColor::windowBackgroundColor().colorWithAlphaComponent(0.85),
    );
    backing.setAccessibilityElement(false);
    if let Some(image) = pin_image() {
        let view = mac_ui::widgets::image_view(mtm);
        view.setImage(Some(&image));
        view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        view.setContentTintColor(Some(&NSColor::labelColor()));
        let inset = 3.0;
        view.setFrame(NSRect::new(
            NSPoint::new(inset, inset),
            NSSize::new(side - 2.0 * inset, side - 2.0 * inset),
        ));
        view.setAccessibilityElement(false);
        backing.addSubview(&view);
    }
    backing
}
