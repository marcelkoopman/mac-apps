//! The Settings window's rows: names, notes, checkboxes, popups and fields, placed from the
//! top down.

use super::*;

pub(super) fn wire(control: &NSButton, action: mac_ui::objc2::runtime::Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(control, delegate, action);
        }
    });
}

pub(super) fn take(top: &mut f64, height: f64) -> f64 {
    *top -= height;
    *top
}

pub(super) fn gap(top: &mut f64, height: f64) {
    *top -= height;
}

pub(super) fn place_name(mtm: MainThreadMarker, parent: &NSView, title: &str, y: f64) {
    place_label(mtm, parent, title, y, LABEL_W);
}

pub(super) fn place_label(mtm: MainThreadMarker, parent: &NSView, title: &str, y: f64, width: f64) {
    let field = widgets::label(mtm, mac_ui::metrics::FONT_BODY, &NSColor::labelColor());
    field.setStringValue(&NSString::from_str(title));
    field.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(width, BUTTON_H),
    ));
    parent.addSubview(&field);
}

pub(super) fn place_note(
    mtm: MainThreadMarker,
    parent: &NSView,
    text: &str,
    y: f64,
) -> Retained<NSTextField> {
    let note = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    note.setStringValue(&NSString::from_str(text));
    note.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, NOTE_H),
    ));
    parent.addSubview(&note);
    note
}

pub(super) fn place_explanation(mtm: MainThreadMarker, parent: &NSView, text: &str, top: &mut f64) {
    gap(top, NOTE_GAP);
    place_note(mtm, parent, text, take(top, NOTE_H));
}

pub(super) fn checkbox(mtm: MainThreadMarker, title: &str, y: f64) -> Retained<NSButton> {
    let button = mac_ui::actions::checkbox(mtm, title);
    button.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, BUTTON_H),
    ));
    button
}

/// A checkbox for a setting that depends on the one above it, [`INDENT`] in.
pub(super) fn indented_checkbox(mtm: MainThreadMarker, title: &str, y: f64) -> Retained<NSButton> {
    let button = checkbox(mtm, title, y);
    button.setFrame(NSRect::new(
        NSPoint::new(MARGIN + INDENT, y),
        NSSize::new(WIDTH - MARGIN * 2.0 - INDENT, BUTTON_H),
    ));
    button
}

pub(super) fn popup(mtm: MainThreadMarker, y: f64) -> Retained<NSPopUpButton> {
    let x = MARGIN + LABEL_W + mac_ui::metrics::BUTTON_GAP;
    NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(x, y),
            NSSize::new(WIDTH - MARGIN - x, BUTTON_H),
        ),
        false,
    )
}

pub(super) fn text_field(mtm: MainThreadMarker, y: f64) -> Retained<NSTextField> {
    let field = widgets::plain_field(mtm, mac_ui::metrics::FONT_BODY, "");
    field.setBezeled(true);
    field.setDrawsBackground(true);
    field.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, BUTTON_H),
    ));
    field
}

pub(super) fn place_pill(button: &GlassButton, y: f64, max_w: f64) {
    let width = button.width_within(max_w).max(BUTTON_H);
    button.view().setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(width, BUTTON_H),
    ));
}

pub(super) fn state_of(on: bool) -> isize {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}
