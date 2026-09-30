//! AppKit control constructors with the shared look: borderless, no focus ring, system fonts.
//!
//! The helpers only configure the view. Callers set frames, visibility, targets and delegates.

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSBorderType, NSBox, NSBoxType, NSButton, NSCellImagePosition, NSColor, NSFocusRingType,
    NSFont, NSImageAlignment, NSImageScaling, NSImageView, NSLineBreakMode, NSScrollView,
    NSSearchField, NSTextAlignment, NSTextField, NSTextView, NSTitlePosition, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::icon::system_symbol;

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

pub use crate::layer::round_view;

/// Move the view to the front of its siblings. Does nothing without a superview.
pub fn raise_view(view: &NSView) {
    // SAFETY: the superview is used right away, while the view hierarchy keeps it alive.
    if let Some(parent) = unsafe { view.superview() } {
        parent.addSubview(view);
    }
}

/// A drawn button: `root` holds the background (and label), `hit` is the clickable button on top.
///
/// Position and show `root`; wire target and action on `hit`.
pub struct OverlayButton {
    pub root: Retained<NSView>,
    pub hit: Retained<NSButton>,
}

/// Round `diameter`-sized button showing SF Symbol `symbol` (`symbol_size` points, template,
/// `labelColor` tint) on a `controlBackgroundColor` disc at 92% opacity. Without SF Symbols
/// (before macOS 11) it shows `fallback` text at `fallback_font_size` instead.
pub fn symbol_button(
    mtm: MainThreadMarker,
    symbol: &str,
    fallback: &str,
    diameter: f64,
    symbol_size: f64,
    fallback_font_size: f64,
) -> OverlayButton {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(diameter, diameter));
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let fill = filled_box(mtm, diameter / 2.0, &NSColor::controlBackgroundColor());
    fill.setFrame(frame);
    fill.setAlphaValue(0.92);
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setFocusRingType(NSFocusRingType::None);
    hit.setImagePosition(NSCellImagePosition::ImageOnly);
    hit.setImageScaling(NSImageScaling::ScaleProportionallyDown);
    if let Some(image) = system_symbol(symbol, fallback) {
        image.setTemplate(true);
        image.setSize(NSSize::new(symbol_size, symbol_size));
        hit.setImage(Some(&image));
        hit.setTitle(&NSString::from_str(""));
        if hit.respondsToSelector(sel!(setContentTintColor:)) {
            hit.setContentTintColor(Some(&NSColor::labelColor()));
        }
    } else {
        hit.setTitle(&NSString::from_str(fallback));
        hit.setFont(Some(&NSFont::systemFontOfSize(fallback_font_size)));
    }
    root.addSubview(&fill);
    root.addSubview(&hit);
    OverlayButton { root, hit }
}

/// `width`×`height` pill (`unemphasizedSelectedContentBackgroundColor`) with a centered
/// `title` label (`font_size`, `labelColor`, `label_height` tall, vertically centered) and a
/// transparent button over it.
pub fn pill_button(
    mtm: MainThreadMarker,
    title: &str,
    width: f64,
    height: f64,
    font_size: f64,
    label_height: f64,
) -> OverlayButton {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let fill = filled_box(
        mtm,
        height / 2.0,
        &NSColor::unemphasizedSelectedContentBackgroundColor(),
    );
    fill.setFrame(frame);
    let text = label(mtm, font_size, &NSColor::labelColor());
    text.setAlignment(NSTextAlignment::Center);
    text.setLineBreakMode(NSLineBreakMode::ByClipping);
    text.setFrame(NSRect::new(
        NSPoint::new(0.0, (height - label_height) / 2.0),
        NSSize::new(width, label_height),
    ));
    text.setStringValue(&NSString::from_str(title));
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setTransparent(true);
    hit.setTitle(&NSString::from_str(""));
    hit.setFocusRingType(NSFocusRingType::None);
    root.addSubview(&fill);
    root.addSubview(&text);
    root.addSubview(&hit);
    OverlayButton { root, hit }
}

/// Read-only, non-selectable rich text view without background, with `inset` around the text.
pub fn read_only_text_view(mtm: MainThreadMarker, inset: NSSize) -> Retained<NSTextView> {
    let text = NSTextView::initWithFrame(NSTextView::alloc(mtm), NSRect::ZERO);
    text.setEditable(false);
    text.setSelectable(false);
    text.setUsesFindPanel(false);
    text.setDrawsBackground(false);
    text.setRichText(true);
    text.setTextContainerInset(inset);
    text
}

/// Set up `scroll` (any NSScrollView subclass) as a transparent, borderless scroller for `text`:
/// both scrollers, auto-hidden, no automatic content insets.
pub fn configure_text_scroll(scroll: &NSScrollView, text: &NSTextView) {
    scroll.setDrawsBackground(false);
    scroll.setBorderType(NSBorderType::NoBorder);
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setAutomaticallyAdjustsContentInsets(false);
    scroll.contentView().setDrawsBackground(false);
    scroll.setDocumentView(Some(text));
}
