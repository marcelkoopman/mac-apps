//! Modal alerts: message, confirmation, button choice, text prompt and list pick.
//!
//! Every call needs the main thread, activates the app first (so the alert also comes to the
//! front in a menu bar / `LSUIElement` app) and blocks in `runModal` until it is answered.
//! `title` is the bold message text, `message` the informative text under it (may be empty).
//! Prompts and picks have OK (default, Return) and Cancel (Escape) buttons. The titles follow
//! [`crate::lang`]. A button whose title is the cancel label in either language ("Cancel" or
//! "Annuleer") answers Escape, marked on the button rather than left to AppKit's English title.
//! [`confirm_destructive`] is the exception: there Return and Escape both answer cancel, and
//! the destructive button has no key.
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

/// Answer of [`confirm_destructive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestructiveAnswer {
    /// The destructive button was clicked.
    pub confirmed: bool,
    /// "Don't ask again" was ticked and the action confirmed; a cancel never stores it.
    pub dont_ask_again: bool,
}

impl DestructiveAnswer {
    /// From the clicked button (0 cancel, 1 the action) and the checkbox.
    pub fn from_response(index: Option<usize>, suppressed: bool) -> Self {
        let confirmed = index == Some(1);
        Self {
            confirmed,
            dont_ask_again: confirmed && suppressed,
        }
    }
}

/// Ask before a destructive action. `cancel` is the default button: Return answers it, and
/// so does Escape (an invisible key catcher in the alert window, with the `widgets` feature;
/// without it Escape alone answers cancel and Return does nothing). `action` is marked
/// destructive (red on macOS 11+) and has no key: only a click (or Space with keyboard focus
/// on it) runs it. With `dont_ask`, a checkbox with that title is shown under the message.
pub fn confirm_destructive(
    mtm: MainThreadMarker,
    title: &str,
    message: &str,
    action: &str,
    cancel: &str,
    dont_ask: Option<&str>,
) -> DestructiveAnswer {
    use objc2::{ClassType, sel};
    use objc2_app_kit::{NSButton, NSControlStateValueOn};

    let alert = new_alert(mtm, title, message, &[cancel, action]);
    let buttons = alert.buttons();
    if let Some(cancel_button) = buttons.iter().next() {
        // Return belongs to cancel, the default button (`mark_cancel` gave it Escape; with
        // `widgets` the key catcher below answers Escape instead).
        if cfg!(feature = "widgets") {
            cancel_button.setKeyEquivalent(&NSString::from_str("\r"));
        }
        cancel_button.setAccessibilityLabel(Some(&NSString::from_str(cancel)));
    }
    if let Some(action_button) = buttons.iter().nth(1) {
        // The default button is the first one; Return must not run the action.
        action_button.setKeyEquivalent(&NSString::from_str(""));
        action_button.setAccessibilityLabel(Some(&NSString::from_str(action)));
        if NSButton::class().responds_to(sel!(setHasDestructiveAction:)) {
            action_button.setHasDestructiveAction(true);
        }
    }
    if let Some(label) = dont_ask {
        alert.setShowsSuppressionButton(true);
        if let Some(check) = alert.suppressionButton() {
            check.setTitle(&NSString::from_str(label));
            check.setAccessibilityLabel(Some(&NSString::from_str(label)));
        }
    }
    #[cfg(feature = "widgets")]
    let escape = escape_answers_cancel(mtm, &alert);
    let index = button_index(run(mtm, &alert));
    #[cfg(feature = "widgets")]
    drop(escape);
    let suppressed = alert
        .suppressionButton()
        .is_some_and(|check| check.state() == NSControlStateValueOn);
    DestructiveAnswer::from_response(index, dont_ask.is_some() && suppressed)
}

/// Escape in `alert` ends it as its first button (cancel), as `cancelOperation:` would: a
/// transparent 0 × 0 button in the alert window, keyed to Escape, that stops the modal session
/// with the first button's response. It takes no focus and VoiceOver skips it. The returned
/// target must outlive `runModal` (the button holds it weakly).
#[cfg(feature = "widgets")]
fn escape_answers_cancel(mtm: MainThreadMarker, alert: &NSAlert) -> Retained<preview::Watcher> {
    use objc2_app_kit::{NSButton, NSView};
    let watcher = preview::Watcher::new(
        mtm,
        Box::new(move || {
            NSApplication::sharedApplication(mtm).stopModalWithCode(NSAlertFirstButtonReturn)
        }),
    );
    alert.layout();
    let catcher = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
    catcher.setTransparent(true);
    catcher.setRefusesFirstResponder(true);
    catcher.setAccessibilityElement(false);
    catcher.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    crate::actions::connect(&catcher, &watcher, objc2::sel!(refresh:));
    if let Some(content) = alert.window().contentView() {
        let content: &NSView = &content;
        content.addSubview(&catcher);
    }
    watcher
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
    let typed = field.stringValue().to_string();
    // The text may be private (a prompt about copied text): do not leave it in the field.
    // `wipe_*` lives in `widgets`; without that feature (ticker builds -p ticker alone) clear the
    // string only so dialog still compiles.
    #[cfg(feature = "widgets")]
    {
        crate::widgets::wipe_field_editor(&field);
        crate::widgets::wipe_text_field(&field);
    }
    #[cfg(not(feature = "widgets"))]
    field.setStringValue(&NSString::from_str(""));
    Some(typed)
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

/// What [`form`] asks: an optional pop-up of `options` (none when empty, else starting at
/// `selected`), a text field pre-filled with `field_default` (none when `field_label` is
/// empty), checkboxes, each with a label, and a live preview under them. The default button is
/// titled `ok` (OK when empty), the second one is Cancel.
#[cfg(feature = "widgets")]
pub struct Form<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub ok: &'a str,
    pub options: &'a [&'a str],
    pub selected: usize,
    pub option_label: &'a str,
    pub field_label: &'a str,
    pub field_default: &'a str,
    /// Width of the text field: narrow for a number, wide for a pattern.
    pub field_width: f64,
    pub checks: &'a [&'a str],
    pub preview_label: &'a str,
}

/// What the user chose in a [`form`].
#[cfg(feature = "widgets")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormAnswer {
    /// The picked option (0 without a pop-up).
    pub option: usize,
    /// The field's text, untrimmed (empty without a field).
    pub text: String,
    pub checks: Vec<bool>,
}

/// The preview under a [`form`]: text, byte ranges to highlight, and whether it is an error
/// (drawn in red).
#[cfg(feature = "widgets")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preview {
    pub text: String,
    pub marks: Vec<std::ops::Range<usize>>,
    pub error: bool,
}

/// Works out a [`form`]'s preview from the picked option, the typed text and the checkboxes.
/// It runs on a background thread, so it may take a while on a long text.
#[cfg(feature = "widgets")]
pub type PreviewFn = std::sync::Arc<dyn Fn(usize, &str, &[bool]) -> Preview + Send + Sync>;

/// Ask with a [`Form`]. On every change the preview is worked out by `preview` on a background
/// thread and drawn when it is back, unless a newer change came first (a stale result is
/// dropped). Returns the answer on OK, `None` on Cancel.
#[cfg(feature = "widgets")]
pub fn form(mtm: MainThreadMarker, ask: &Form<'_>, preview: PreviewFn) -> Option<FormAnswer> {
    use objc2_app_kit::NSView;

    const ROW: f64 = 26.0;
    const LABEL_W: f64 = 120.0;
    const PREVIEW_H: f64 = 130.0;
    let ok_title = if ask.ok.is_empty() { ok() } else { ask.ok };
    let alert = new_alert(mtm, ask.title, ask.message, &[ok_title, cancel()]);
    let has_field = !ask.field_label.is_empty();
    let rows = usize::from(!ask.options.is_empty()) + usize::from(has_field) + ask.checks.len();
    let height = rows as f64 * (ROW + 4.0) + 8.0 + PREVIEW_H;
    let view = NSView::initWithFrame(NSView::alloc(mtm), accessory_frame(height));
    let label = |text: &str, y: f64| {
        let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        label.setFrame(NSRect::new(
            NSPoint::new(0.0, y + 4.0),
            NSSize::new(LABEL_W - 6.0, 18.0),
        ));
        view.addSubview(&label);
    };
    let mut y = height - ROW;

    let popup = if ask.options.is_empty() {
        None
    } else {
        label(ask.option_label, y);
        let popup = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            NSRect::new(
                NSPoint::new(LABEL_W, y),
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
        y -= ROW + 4.0;
        Some(popup)
    };

    let field = if has_field {
        label(ask.field_label, y);
        let field = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(
                NSPoint::new(LABEL_W, y + 1.0),
                NSSize::new(ask.field_width.min(ACCESSORY_WIDTH - LABEL_W), 24.0),
            ),
        );
        field.setStringValue(&NSString::from_str(ask.field_default));
        field.setAccessibilityLabel(Some(&NSString::from_str(ask.field_label)));
        // Every keystroke sends the action, so the preview follows the typing.
        field.setContinuous(true);
        view.addSubview(&field);
        y -= ROW + 4.0;
        Some(field)
    } else {
        None
    };

    let mut boxes = Vec::new();
    for title in ask.checks {
        let check = crate::actions::checkbox(mtm, title);
        check.setFrame(NSRect::new(
            NSPoint::new(LABEL_W, y),
            NSSize::new(ACCESSORY_WIDTH - LABEL_W, ROW),
        ));
        check.setAccessibilityLabel(Some(&NSString::from_str(title)));
        view.addSubview(&check);
        boxes.push(check);
        y -= ROW + 4.0;
    }

    let shown = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    shown.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(ACCESSORY_WIDTH, PREVIEW_H),
    ));
    shown.setFont(Some(&crate::fonts::monospace(
        crate::style::FONT_CAPTION,
        &[],
    )));
    shown.setAccessibilityLabel(Some(&NSString::from_str(ask.preview_label)));
    view.addSubview(&shown);
    preview::show_in(shown);

    let read = {
        let popup = popup.clone();
        let field = field.clone();
        let boxes = boxes.clone();
        move || {
            let option = popup
                .as_ref()
                .and_then(|popup| usize::try_from(popup.indexOfSelectedItem()).ok())
                .unwrap_or(0);
            let checks: Vec<bool> = boxes
                .iter()
                .map(|check| check.state() == objc2_app_kit::NSControlStateValueOn)
                .collect();
            let text = field
                .as_ref()
                .map(|field| field.stringValue().to_string())
                .unwrap_or_default();
            (option, text, checks)
        }
    };
    let watcher = preview::Watcher::new(mtm, {
        let read = read.clone();
        Box::new(move || {
            let (option, text, checks) = read();
            preview::start(preview.clone(), option, text, checks);
        })
    });
    watcher.refresh();
    if let Some(popup) = &popup {
        crate::actions::connect(popup, &watcher, objc2::sel!(refresh:));
    }
    if let Some(field) = &field {
        crate::actions::connect(field, &watcher, objc2::sel!(refresh:));
    }
    for check in &boxes {
        crate::actions::connect(check, &watcher, objc2::sel!(refresh:));
    }

    alert.setAccessoryView(Some(&view));
    alert.layout();
    if let Some(field) = &field {
        alert.window().setInitialFirstResponder(Some(field));
    }
    let answer = button_index(run(mtm, &alert));
    // The controls hold the watcher weakly; it lives until here. A preview still on its way
    // finds no label and is dropped.
    drop(watcher);
    preview::stop();
    if answer != Some(0) {
        return None;
    }
    let (option, text, checks) = read();
    if popup.is_some() && option >= ask.options.len() {
        return None;
    }
    Some(FormAnswer {
        option,
        text,
        checks,
    })
}

/// What [`fields`] asks: a bold `title`, a `message`, one labelled text field per entry of
/// `fields` (each `lines` high; Return ends the dialog like the default button) and a preview
/// under them. The default button is titled `ok`, the second one is Cancel.
#[cfg(feature = "widgets")]
pub struct Fields<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub ok: &'a str,
    pub fields: &'a [FieldAsk<'a>],
    pub preview_label: &'a str,
}

/// One text field of [`fields`]: its label above it and its height in lines of text.
#[cfg(feature = "widgets")]
pub struct FieldAsk<'a> {
    pub label: &'a str,
    pub lines: usize,
}

/// Works out [`fields`]'s preview from the typed texts, in the order of the fields.
#[cfg(feature = "widgets")]
pub type FieldsPreview = Box<dyn Fn(&[String]) -> String>;

/// Ask for several texts at once, with a live preview that is worked out from all of them on
/// every keystroke (on the main thread, so keep it cheap). Returns the texts as typed, in the
/// order of `ask.fields`, on the default button; `None` on Cancel. The typed texts may be
/// private: the fields, their editors and the preview are overwritten and emptied before this
/// returns.
#[cfg(feature = "widgets")]
pub fn fields(
    mtm: MainThreadMarker,
    ask: &Fields<'_>,
    preview: FieldsPreview,
) -> Option<Vec<String>> {
    use std::rc::Rc;

    use objc2_app_kit::{NSTextFieldBezelStyle, NSView};

    const WIDTH: f64 = 440.0;
    const LABEL_H: f64 = 18.0;
    const LINE_H: f64 = 17.0;
    const FIELD_PAD: f64 = 8.0;
    const GAP: f64 = 10.0;
    const PREVIEW_H: f64 = 120.0;
    let alert = new_alert(mtm, ask.title, ask.message, &[ask.ok, cancel()]);
    let field_h = |lines: usize| lines.max(1) as f64 * LINE_H + FIELD_PAD;
    let height: f64 = ask
        .fields
        .iter()
        .map(|f| LABEL_H + 2.0 + field_h(f.lines) + GAP)
        .sum::<f64>()
        + LABEL_H
        + 2.0
        + PREVIEW_H;
    let view = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, height)),
    );
    let label = |text: &str, y: f64| {
        let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        label.setFrame(NSRect::new(
            NSPoint::new(0.0, y),
            NSSize::new(WIDTH, LABEL_H),
        ));
        view.addSubview(&label);
    };
    let mut y = height;
    let mut inputs: Vec<Retained<NSTextField>> = Vec::new();
    for ask_field in ask.fields {
        y -= LABEL_H;
        label(ask_field.label, y);
        y -= 2.0 + field_h(ask_field.lines);
        // A wrapping label made editable: a bezeled field that wraps to several lines.
        let input = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
        input.setFrame(NSRect::new(
            NSPoint::new(0.0, y),
            NSSize::new(WIDTH, field_h(ask_field.lines)),
        ));
        input.setEditable(true);
        input.setSelectable(true);
        input.setBezeled(true);
        input.setBezelStyle(NSTextFieldBezelStyle::SquareBezel);
        input.setDrawsBackground(true);
        // Every keystroke sends the action, so the preview follows the typing.
        input.setContinuous(true);
        input.setAccessibilityLabel(Some(&NSString::from_str(ask_field.label)));
        view.addSubview(&input);
        inputs.push(input);
        y -= GAP;
    }
    y -= LABEL_H;
    label(ask.preview_label, y);
    let shown = NSTextField::wrappingLabelWithString(&NSString::from_str(""), mtm);
    shown.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(WIDTH, PREVIEW_H),
    ));
    shown.setFont(Some(&crate::fonts::monospace(
        crate::style::FONT_CAPTION,
        &[],
    )));
    shown.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
    shown.setAccessibilityLabel(Some(&NSString::from_str(ask.preview_label)));
    view.addSubview(&shown);

    let read = {
        let inputs = inputs.clone();
        move || -> Vec<String> {
            inputs
                .iter()
                .map(|input| input.stringValue().to_string())
                .collect()
        }
    };
    #[allow(clippy::type_complexity)]
    let preview: Rc<dyn Fn(&[String]) -> String> = Rc::from(preview);
    let watcher = preview::Watcher::new(mtm, {
        let read = read.clone();
        let shown = shown.clone();
        Box::new(move || shown.setStringValue(&NSString::from_str(&preview(&read()))))
    });
    watcher.refresh();
    for input in &inputs {
        crate::actions::connect(input, &watcher, objc2::sel!(refresh:));
    }

    alert.setAccessoryView(Some(&view));
    alert.layout();
    // Tab goes from field to field, top to bottom.
    alert.window().recalculateKeyViewLoop();
    if let Some(first) = inputs.first() {
        alert.window().setInitialFirstResponder(Some(first));
    }
    let answer = button_index(run(mtm, &alert));
    let typed = read();
    // The controls hold the watcher weakly; it lives until here.
    drop(watcher);
    for input in &inputs {
        crate::widgets::wipe_field_editor(input);
        crate::widgets::wipe_text_field(input);
    }
    shown.setStringValue(&NSString::from_str(""));
    (answer == Some(0)).then_some(typed)
}

/// The target of [`form`]'s controls, and the preview's way back from its thread.
#[cfg(feature = "widgets")]
mod preview {
    use std::cell::{Cell, RefCell};

    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSColor, NSTextField};

    use super::{Preview, PreviewFn};

    thread_local! {
        /// The preview label of the open form (main thread).
        static LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
        /// The newest preview asked for; an older one is dropped when it arrives.
        static NEWEST: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn show_in(label: Retained<NSTextField>) {
        LABEL.with(|slot| *slot.borrow_mut() = Some(label));
    }

    pub(super) fn stop() {
        LABEL.with(|slot| *slot.borrow_mut() = None);
        NEWEST.set(NEWEST.get().wrapping_add(1));
    }

    /// Work the preview out on a thread; draw it on the main thread if nothing newer was asked.
    pub(super) fn start(preview: PreviewFn, option: usize, text: String, checks: Vec<bool>) {
        let generation = NEWEST.get().wrapping_add(1);
        NEWEST.set(generation);
        let spawned = std::thread::Builder::new()
            .name("mac-ui-form-preview".into())
            .spawn(move || {
                let shown = preview(option, &text, &checks);
                crate::actions::run_on_main(move || draw(generation, &shown));
            });
        if let Err(e) = spawned {
            eprintln!("mac-ui: preview skipped: {e}");
        }
    }

    fn draw(generation: u64, shown: &Preview) {
        if NEWEST.get() != generation {
            return;
        }
        LABEL.with(|slot| {
            let borrowed = slot.borrow();
            let Some(label) = borrowed.as_ref() else {
                return;
            };
            let color = if shown.error {
                NSColor::systemRedColor()
            } else {
                NSColor::labelColor()
            };
            let font = crate::fonts::monospace(crate::style::FONT_CAPTION, &[]);
            let attr = crate::text::AttrText::new(&shown.text, &font, &color);
            let mark = NSColor::findHighlightColor();
            for range in &shown.marks {
                if shown.text.get(range.clone()).is_some() {
                    attr.background(range, &mark);
                }
            }
            label.setAttributedStringValue(&attr.into_attributed());
            #[cfg(feature = "widgets")]
            crate::motion::crossfade(label);
        });
    }

    pub(super) struct Ivars {
        refresh: Box<dyn Fn()>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements. `refresh:` takes the sender and
        // returns void, as an action method; the ivars need no Objective-C cleanup.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "MacUiFormWatcher"]
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
        pub(super) fn new(mtm: MainThreadMarker, refresh: Box<dyn Fn()>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars { refresh });
            // SAFETY: NSObject's `init` on a freshly allocated instance with its ivars set.
            unsafe { msg_send![super(this), init] }
        }

        pub(super) fn refresh(&self) {
            (self.ivars().refresh)();
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
    fn destructive_answer_needs_the_action_button() {
        use super::DestructiveAnswer;
        let cancel = DestructiveAnswer::from_response(Some(0), true);
        assert!(!cancel.confirmed && !cancel.dont_ask_again);
        let none = DestructiveAnswer::from_response(None, true);
        assert!(!none.confirmed && !none.dont_ask_again);
        let ok = DestructiveAnswer::from_response(Some(1), false);
        assert!(ok.confirmed && !ok.dont_ask_again);
        let ok_quiet = DestructiveAnswer::from_response(Some(1), true);
        assert!(ok_quiet.confirmed && ok_quiet.dont_ask_again);
    }

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
