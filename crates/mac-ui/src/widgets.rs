//! AppKit control constructors with the shared look: borderless, no focus ring, system fonts.
//!
//! The helpers only configure the view. Callers set frames, visibility, targets and delegates.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSBox, NSBoxType, NSButton, NSColor, NSFocusRingType, NSFont, NSImageAlignment, NSImageScaling,
    NSImageView, NSLineBreakMode, NSSearchField, NSTextField, NSTitlePosition, NSView,
};
use objc2_foundation::{NSRect, NSSize, NSString};

/// Read-only, single-line label in the system font, truncating at the tail.
pub fn label(mtm: MainThreadMarker, size: f64, color: &NSColor) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::ZERO);
    field.setEditable(false);
    field.setSelectable(false);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(color));
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    field.setUsesSingleLineMode(true);
    field
}

/// Editable text field without border or background, in the system font and `labelColor`.
pub fn plain_field(mtm: MainThreadMarker, size: f64, placeholder: &str) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::ZERO);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(true);
    field.setSelectable(true);
    field.setFocusRingType(NSFocusRingType::None);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

/// Search field that reports every keystroke, keeps no recents and draws no focus ring.
pub fn search_field(
    mtm: MainThreadMarker,
    size: f64,
    placeholder: &str,
) -> Retained<NSSearchField> {
    let field = NSSearchField::initWithFrame(NSSearchField::alloc(mtm), NSRect::ZERO);
    field.setSendsSearchStringImmediately(true);
    field.setSendsWholeSearchString(false);
    field.setMaximumRecents(0);
    field.setRecentsAutosaveName(None);
    field.setFocusRingType(NSFocusRingType::None);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

/// Borderless text button in the system font, without focus ring.
pub fn text_button(mtm: MainThreadMarker, title: &str, size: f64) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
    button.setBordered(false);
    button.setFocusRingType(NSFocusRingType::None);
    button.setTitle(&NSString::from_str(title));
    button.setFont(Some(&NSFont::systemFontOfSize(size)));
    button
}

/// Read-only image view that scales proportionally (up or down) and centers the image.
pub fn image_view(mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = NSImageView::initWithFrame(NSImageView::alloc(mtm), NSRect::ZERO);
    view.setEditable(false);
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    view.setImageAlignment(NSImageAlignment::AlignCenter);
    view
}

/// Borderless box filled with `color`, with rounded corners and no content margins.
pub fn filled_box(mtm: MainThreadMarker, radius: f64, color: &NSColor) -> Retained<NSBox> {
    let fill = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::ZERO);
    fill.setBoxType(NSBoxType::Custom);
    fill.setBorderWidth(0.0);
    fill.setCornerRadius(radius);
    fill.setTitlePosition(NSTitlePosition::NoTitle);
    fill.setContentViewMargins(NSSize::new(0.0, 0.0));
    fill.setFillColor(color);
    fill
}

/// Round the view's corners through its layer and clip its content to them.
pub fn round_view(view: &NSView, radius: f64) {
    view.setWantsLayer(true);
    // SAFETY: `layer` returns the view's CALayer or nil; both setters exist on CALayer.
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
}

/// Move the view to the front of its siblings. Does nothing without a superview.
pub fn raise_view(view: &NSView) {
    // SAFETY: the superview is used right away, while the view hierarchy keeps it alive.
    if let Some(parent) = unsafe { view.superview() } {
        parent.addSubview(view);
    }
}
