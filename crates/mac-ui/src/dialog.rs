//! Modal alerts: message, confirmation, button choice, text prompt and list pick.
//!
//! Every call needs the main thread, activates the app first (so the alert also comes to the
//! front in a menu bar / `LSUIElement` app) and blocks in `runModal` until it is answered.
//! `title` is the bold message text, `message` the informative text under it (may be empty).
//! Prompts and picks have OK (default, Return) and Cancel (Escape) buttons. The titles follow
//! [`crate::lang`]. A button whose title is the cancel label in either language ("Cancel" or
//! "Annuleer") answers Escape, marked on the button rather than left to AppKit's English title.
//!
//! Do not start one while a menu is still tracking (see [`can_run_modal`]): run it from the
//! event loop once the run loop is back in its default mode.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAlert, NSAlertFirstButtonReturn, NSApplication, NSMenuItem,
    NSModalPanelWindowLevel, NSModalResponse, NSPopUpButton, NSTextField,
};
use objc2_foundation::{NSDefaultRunLoopMode, NSPoint, NSRect, NSRunLoop, NSSize, NSString};

use crate::activation::activate_app;
use crate::lang;

fn ok() -> &'static str {
    lang::t("ok")
}

fn cancel() -> &'static str {
    lang::t("cancel")
}

/// Width of the text field and pop-up button under the message.
const ACCESSORY_WIDTH: f64 = 300.0;

/// Show `title` and `message` with a single OK button.
pub fn alert(mtm: MainThreadMarker, title: &str, message: &str) {
    let alert = new_alert(mtm, title, message, &[ok()]);
    run(mtm, &alert);
}

/// Ask a yes/no question. `ok` is the default button (Return); returns true when it is clicked.
pub fn confirm(mtm: MainThreadMarker, title: &str, message: &str, ok: &str, cancel: &str) -> bool {
    buttons(mtm, title, message, &[ok, cancel]) == Some(0)
}

/// Show `labels` as buttons and return the index of the clicked one. The first label is the
/// default button (Return, rightmost). A button titled with the cancel label in either language
/// also answers Escape ([`lang::is_cancel`]). Without labels AppKit shows a single OK button,
/// reported as index 0.
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
/// be empty) on OK, `None` on Cancel.
pub fn prompt_text(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    default: &str,
) -> Option<String> {
    let alert = new_alert(mtm, title, message, &[ok(), cancel()]);
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), accessory_frame(24.0));
    field.setStringValue(&NSString::from_str(default));
    // VoiceOver names the field after the question.
    field.setAccessibilityLabel(Some(&NSString::from_str(accessory_label(title, message))));
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
/// picked index on OK, `None` on Cancel or without options.
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
    let alert = new_alert(mtm, title, message, &[ok(), cancel()]);
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
    popup.setAccessibilityLabel(Some(&NSString::from_str(accessory_label(title, message))));
    alert.setAccessoryView(Some(&popup));
    if button_index(run(mtm, &alert)) != Some(0) {
        return None;
    }
    usize::try_from(popup.indexOfSelectedItem())
        .ok()
        .filter(|&index| index < options.len())
}

/// What [`choose_with_number`] asks: a pop-up of `options` (starting at `selected`) and a
/// number field (pre-filled with `number_default`), each with a label, above a live preview.
#[cfg(feature = "widgets")]
pub struct NumberChoice<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub options: &'a [&'a str],
    pub selected: usize,
    pub option_label: &'a str,
    pub number_label: &'a str,
    pub number_default: &'a str,
    pub preview_label: &'a str,
}

/// Pick one of `ask.options` and type a number, with a preview under them that `preview`
/// fills from the picked index and the typed text, again on every change. Returns the picked
/// index and the typed text (untrimmed) on OK, `None` on Cancel or without options.
#[cfg(feature = "widgets")]
pub fn choose_with_number(
    mtm: MainThreadMarker,
    ask: &NumberChoice<'_>,
    preview: impl Fn(usize, &str) -> String + 'static,
) -> Option<(usize, String)> {
    use objc2_app_kit::NSView;

    const ROW: f64 = 26.0;
    const LABEL_W: f64 = 120.0;
    const PREVIEW_H: f64 = 110.0;
    if ask.options.is_empty() {
        return None;
    }
    let alert = new_alert(mtm, ask.title, ask.message, &[ok(), cancel()]);
    let height = ROW * 2.0 + 12.0 + PREVIEW_H;
    let view = NSView::initWithFrame(NSView::alloc(mtm), accessory_frame(height));
    let label = |text: &str, y: f64| {
        let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        label.setFrame(NSRect::new(
            NSPoint::new(0.0, y + 4.0),
            NSSize::new(LABEL_W - 6.0, 18.0),
        ));
        view.addSubview(&label);
    };

    let popup_y = height - ROW;
    label(ask.option_label, popup_y);
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(LABEL_W, popup_y),
            NSSize::new(ACCESSORY_WIDTH - LABEL_W, ROW),
        ),
        false,
    );
    let menu = popup.menu()?;
    for option in ask.options {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(option));
        menu.addItem(&item);
    }
    if ask.selected < ask.options.len() {
        popup.selectItemAtIndex(ask.selected as isize);
    }
    popup.setAccessibilityLabel(Some(&NSString::from_str(ask.option_label)));
    view.addSubview(&popup);

    let field_y = popup_y - ROW - 6.0;
    label(ask.number_label, field_y);
    let field = NSTextField::initWithFrame(
        NSTextField::alloc(mtm),
        NSRect::new(
            NSPoint::new(LABEL_W, field_y + 1.0),
            NSSize::new(80.0, 24.0),
        ),
    );
    field.setStringValue(&NSString::from_str(ask.number_default));
    field.setAccessibilityLabel(Some(&NSString::from_str(ask.number_label)));
    // Every keystroke sends the action, so the preview follows the typing.
    field.setContinuous(true);
    view.addSubview(&field);

    let shown = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    shown.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(ACCESSORY_WIDTH, PREVIEW_H),
    ));
    shown.setFont(Some(&crate::fonts::monospace(11.0, &[])));
    shown.setAccessibilityLabel(Some(&NSString::from_str(ask.preview_label)));
    view.addSubview(&shown);

    let watcher = preview::Watcher::new(
        mtm,
        preview::Ivars {
            popup: popup.clone(),
            field: field.clone(),
            shown,
            preview: Box::new(preview),
        },
    );
    watcher.refresh();
    crate::actions::connect(&popup, &watcher, objc2::sel!(refresh:));
    crate::actions::connect(&field, &watcher, objc2::sel!(refresh:));

    alert.setAccessoryView(Some(&view));
    alert.layout();
    alert.window().setInitialFirstResponder(Some(&field));
    let answer = button_index(run(mtm, &alert));
    // The controls hold the watcher weakly; it lives until here.
    drop(watcher);
    if answer != Some(0) {
        return None;
    }
    let index = usize::try_from(popup.indexOfSelectedItem())
        .ok()
        .filter(|&index| index < ask.options.len())?;
    Some((index, field.stringValue().to_string()))
}

/// The target of [`choose_with_number`]'s pop-up and field: fills the preview.
#[cfg(feature = "widgets")]
mod preview {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSPopUpButton, NSTextField};
    use objc2_foundation::NSString;

    pub(super) struct Ivars {
        pub(super) popup: Retained<NSPopUpButton>,
        pub(super) field: Retained<NSTextField>,
        pub(super) shown: Retained<NSTextField>,
        pub(super) preview: Box<dyn Fn(usize, &str) -> String>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements. `refresh:` takes the sender and
        // returns void, as an action method; the ivars need no Objective-C cleanup.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "MacUiPreviewWatcher"]
        #[ivars = Ivars]
        pub(super) struct Watcher;

        impl Watcher {
            #[unsafe(method(refresh:))]
            fn refresh_sent(&self, _sender: Option<&AnyObject>) {
                self.refresh();
            }
        }
    );

    impl Watcher {
        pub(super) fn new(mtm: MainThreadMarker, ivars: Ivars) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(ivars);
            // SAFETY: NSObject's `init` on a freshly allocated instance with its ivars set.
            unsafe { msg_send![super(this), init] }
        }

        pub(super) fn refresh(&self) {
            let ivars = self.ivars();
            let index = usize::try_from(ivars.popup.indexOfSelectedItem()).unwrap_or(0);
            let typed = ivars.field.stringValue().to_string();
            let text = (ivars.preview)(index, &typed);
            ivars.shown.setStringValue(&NSString::from_str(&text));
        }
    }
}

/// Accessibility label for a prompt's text field or pop-up: the bold title, else the message.
fn accessory_label<'a>(title: &'a str, message: &'a str) -> &'a str {
    if title.trim().is_empty() {
        message
    } else {
        title
    }
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
    // AppKit only binds Escape to a button titled "Cancel". Mark every cancel label ourselves
    // ("Annuleer" included) so the key follows the language, not that English title.
    mark_cancel(&alert);
    alert
}

/// Escape on each button whose title is [`lang::is_cancel`].
///
/// AppKit binds Escape only when the title is the English word "Cancel", and that binding
/// replaces Return even when Cancel is the first button. Setting the same key here keeps that
/// for "Cancel" and gives "Annuleer" the same key.
fn mark_cancel(alert: &NSAlert) {
    for button in alert.buttons().iter() {
        if lang::is_cancel(&button.title().to_string()) {
            button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
        }
    }
}

/// Mode the main run loop is currently in (e.g. `kCFRunLoopDefaultMode`,
/// `NSEventTrackingRunLoopMode` while a menu is open, `NSModalPanelRunLoopMode` inside a modal).
/// `None` when the run loop is not running.
pub fn run_loop_mode(_mtm: MainThreadMarker) -> Option<String> {
    NSRunLoop::currentRunLoop()
        .currentMode()
        .map(|mode| mode.to_string())
}

/// True when a modal alert can start now: the run loop is idle in its default mode (or not
/// running). False while a menu is tracking or another modal session runs; starting `runModal`
/// then can leave the alert hidden behind the closing menu.
pub fn can_run_modal(_mtm: MainThreadMarker) -> bool {
    let Some(mode) = NSRunLoop::currentRunLoop().currentMode() else {
        return true;
    };
    // SAFETY: `NSDefaultRunLoopMode` is an immutable NSString constant exported by Foundation.
    let default = unsafe { NSDefaultRunLoopMode };
    mode.isEqualToString(default)
}

/// Whether the app is the active (frontmost) app, e.g. to log if activation worked.
pub fn app_is_active(mtm: MainThreadMarker) -> bool {
    NSApplication::sharedApplication(mtm).isActive()
}

fn run(mtm: MainThreadMarker, alert: &NSAlert) -> NSModalResponse {
    activate_app(mtm);
    // An accessory (LSUIElement) app is often not made active on macOS 14+ (cooperative
    // activation), and a window of an inactive app can stay behind the frontmost app. Build the
    // alert window now and put it in front at modal-panel level regardless of activation.
    alert.layout();
    let window = alert.window();
    window.setHidesOnDeactivate(false);
    window.setLevel(NSModalPanelWindowLevel);
    window.center();
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
    alert.runModal()
}

/// 0 for the first button added, 1 for the second, ...
fn button_index(response: NSModalResponse) -> Option<usize> {
    usize::try_from(response - NSAlertFirstButtonReturn).ok()
}

fn accessory_frame(height: f64) -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ACCESSORY_WIDTH, height))
}

#[cfg(test)]
mod tests {
    use super::accessory_label;

    #[test]
    fn accessory_label_prefers_the_title() {
        assert_eq!(
            accessory_label("Asset name", "Shown in the menu"),
            "Asset name"
        );
        assert_eq!(
            accessory_label("  ", "Shown in the menu"),
            "Shown in the menu"
        );
        assert_eq!(accessory_label("", ""), "");
    }
}
