//! The pin of a pinned copy, drawn once for every place that shows it: the SF Symbol
//! [`commands::PIN_SYMBOL`] described as [`commands::pin_label`] ("Vastgezet" / "Pinned").
//! The history strip's cards wear it as [`pin_badge`]; the rows of the history menus (the card's
//! `⋯` › History and the status menu's *History ▸*) take [`pin_image`] through
//! [`pin_menu_item`], and a pinned search chip shows the same symbol. No title carries a mark.
//! The status menu's NSMenu is reached once, in `status_ns_menu`, which also serves
//! [`show_status_shortcut`] (the § shortcut beside *Take Screenshot…*).

use global_hotkey::hotkey::Modifiers;
use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSBox, NSColor, NSEventModifierFlags, NSImage, NSImageScaling, NSMenu,
    NSMenuItem,
};
use mac_ui::objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use mac_ui::tray_icon::menu::{ContextMenu, Menu};

use crate::commands;

/// The pin symbol, with its label as the accessibility description; `None` where SF Symbols
/// are missing.
pub(crate) fn pin_image() -> Option<Retained<NSImage>> {
    mac_ui::icon::system_symbol(commands::PIN_SYMBOL, commands::pin_label())
}

/// A pinned history row: the pin as the item's image, and VoiceOver reads its title with
/// "Vastgezet" / "Pinned" after it.
pub(crate) fn pin_menu_item(item: &NSMenuItem) {
    if let Some(image) = pin_image() {
        item.setImage(Some(&image));
    }
    let label = commands::pinned_label(&item.title().to_string());
    item.setAccessibilityLabel(Some(&NSString::from_str(&label)));
}

/// The pin on the pinned rows of the status menu's *History ▸* (its only submenu): `pinned`
/// holds one flag per row, in order. muda's items take only an RGBA icon, not an SF Symbol,
/// so the image goes on the NSMenuItems muda made.
pub(crate) fn pin_status_history(menu: &Menu, pinned: &[bool]) {
    if !pinned.contains(&true) {
        return;
    }
    let Some(ns_menu) = status_ns_menu(menu) else {
        return;
    };
    let Some(history) = ns_menu.itemArray().iter().find_map(|item| item.submenu()) else {
        return;
    };
    for (item, pinned) in history.itemArray().iter().zip(pinned) {
        if *pinned {
            pin_menu_item(&item);
        }
    }
}

/// The shortcut beside the status menu's item titled `title`: `key` with `mods`, set on the
/// NSMenuItem muda made. For the § key, which muda's accelerators cannot show; AppKit draws it
/// as ⌥⇧§. Only shown there: the chord itself is the global hotkey.
pub(crate) fn show_status_shortcut(menu: &Menu, title: &str, key: &str, mods: Modifiers) {
    let Some(ns_menu) = status_ns_menu(menu) else {
        return;
    };
    let Some(item) = ns_menu
        .itemArray()
        .iter()
        .find(|item| item.title().to_string() == title)
    else {
        return;
    };
    let mut mask = NSEventModifierFlags::empty();
    for (modifier, flag) in [
        (Modifiers::CONTROL, NSEventModifierFlags::Control),
        (Modifiers::ALT, NSEventModifierFlags::Option),
        (Modifiers::SHIFT, NSEventModifierFlags::Shift),
        (Modifiers::META, NSEventModifierFlags::Command),
    ] {
        if mods.contains(modifier) {
            mask |= flag;
        }
    }
    item.setKeyEquivalent(&NSString::from_str(key));
    item.setKeyEquivalentModifierMask(mask);
}

/// muda's NSMenu for the status `menu`, retained.
fn status_ns_menu(menu: &Menu) -> Option<Retained<NSMenu>> {
    // SAFETY: `ns_menu` is muda's own NSMenu for `menu` (tray-icon retains the same pointer in
    // `set_menu`); it lives as long as `menu`, which the caller holds, and `retain` takes a
    // reference of our own. `Retained::retain` returns `None` for a null pointer. The status
    // menu is built on the main thread.
    unsafe { Retained::retain(menu.ns_menu().cast::<NSMenu>()) }
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
