//! Modal alerts: message, confirmation, button choice, text prompt and list pick.
//!
//! Every call needs the main thread, activates the app first (so the alert also comes to the
//! front in a menu bar / `LSUIElement` app) and blocks in `runModal` until it is answered.
//! `title` is the bold message text, `message` the informative text under it (may be empty).
//! Prompts and picks have "OK" (default, Return) and "Cancel" (Escape) buttons.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSMenuItem, NSModalResponse, NSPopUpButton, NSTextField,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::activation::activate_app;

const OK: &str = "OK";
const CANCEL: &str = "Cancel";
/// Width of the text field and pop-up button under the message.
const ACCESSORY_WIDTH: f64 = 300.0;

/// Show `title` and `message` with a single "OK" button.
pub fn alert(mtm: MainThreadMarker, title: &str, message: &str) {
    let alert = new_alert(mtm, title, message, &[OK]);
    run(mtm, &alert);
}

/// Ask a yes/no question. `ok` is the default button (Return); returns true when it is clicked.
pub fn confirm(mtm: MainThreadMarker, title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    buttons(mtm, title, message, &[ok, cancel]) == Some(0)
}

/// Show `labels` as buttons and return the index of the clicked one. The first label is the
/// default button (Return, rightmost); a button titled "Cancel" also answers Escape. Without
/// labels AppKit shows a single "OK" button, reported as index 0.
pub fn buttons(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    labels: &[&str],
) -> Option<usize> {
    let alert = new_alert(mtm, title, message, labels);
    button_index(run(mtm, &alert))
}

/// Ask for a line of text, pre-filled with `default`. Returns the text as typed (untrimmed, may
/// be empty) on "OK", `None` on "Cancel".
pub fn prompt_text(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    default: &str,
) -> Option<String> {
    let alert = new_alert(mtm, title, message, &[OK, CANCEL]);
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), accessory_frame(24.0));
    field.setStringValue(&NSString::from_str(default));
    alert.setAccessoryView(Some(&field));
    // Build the window now so the field can take the focus (with its text selected).
    alert.layout();
    alert.window().setInitialFirstResponder(Some(&field));
    if button_index(run(mtm, &alert)) != Some(0) {
        return None;
    }
    Some(field.stringValue().to_string())
}

/// Let the user pick one of `options` from a pop-up button, starting at `selected`. Returns the
/// picked index on "OK", `None` on "Cancel" or without options.
pub fn choose(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    options: &[&str],
    selected: usize,
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let alert = new_alert(mtm, title, message, &[OK, CANCEL]);
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        accessory_frame(26.0),
        false,
    );
    // Items go through the menu: `addItemWithTitle:` would drop duplicate titles and shift
    // the indices.
    let menu = popup.menu()?;
    for option in options {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(option));
        menu.addItem(&item);
    }
    if selected < options.len() {
        popup.selectItemAtIndex(selected as isize);
    }
    alert.setAccessoryView(Some(&popup));
    if button_index(run(mtm, &alert)) != Some(0) {
        return None;
    }
    usize::try_from(popup.indexOfSelectedItem())
        .ok()
        .filter(|&index| index < options.len())
}

fn new_alert(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    labels: &[&str],
) -> Retained<NSAlert> {
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(message));
    for label in labels {
        alert.addButtonWithTitle(&NSString::from_str(label));
    }
    alert
}

fn run(mtm: MainThreadMarker, alert: &NSAlert) -> NSModalResponse {
    activate_app(mtm);
    alert.runModal()
}

/// 0 for the first button added, 1 for the second, ...
fn button_index(response: NSModalResponse) -> Option<usize> {
    usize::try_from(response - NSAlertFirstButtonReturn).ok()
}

fn accessory_frame(height: f64) -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ACCESSORY_WIDTH, height))
}
