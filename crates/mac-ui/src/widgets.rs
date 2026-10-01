//! AppKit control constructors with the shared look: borderless, native focus rings, system fonts.
//!
//! The helpers only configure the view. Callers set frames, visibility, targets and delegates.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAccessibility, NSBorderType, NSBox, NSBoxType, NSButton, NSColor, NSFont, NSImageAlignment,
    NSImageScaling, NSImageView, NSLineBreakMode, NSScrollView, NSSearchField, NSTextField,
    NSTextView, NSTitlePosition, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::button::{ButtonSize, GlassButton};

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
/// It keeps the native focus ring, so keyboard focus stays visible.
pub fn plain_field(mtm: MainThreadMarker, size: f64, placeholder: &str) -> Retained<NSTextField> {
    let field = NSTextField::initWithFrame(NSTextField::alloc(mtm), NSRect::ZERO);
    field.setBordered(false);
    field.setDrawsBackground(false);
    field.setEditable(true);
    field.setSelectable(true);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

/// Search field in the system's rounded style that reports every keystroke and keeps no recents.
/// It keeps the native border and focus ring.
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
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(&NSColor::labelColor()));
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field
}

/// Borderless text button in the system font. It keeps the native focus ring for keyboard
/// navigation (Full Keyboard Access).
pub fn text_button(mtm: MainThreadMarker, title: &str, size: f64) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
    button.setBordered(false);
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

/// Borderless box filled with `color`, with rounded corners and no content margins. A purely
/// decorative backdrop, so it is hidden from accessibility (VoiceOver skips it).
pub fn filled_box(mtm: MainThreadMarker, radius: f64, color: &NSColor) -> Retained<NSBox> {
    let fill = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::ZERO);
    fill.setBoxType(NSBoxType::Custom);
    fill.setBorderWidth(0.0);
    fill.setCornerRadius(radius);
    fill.setTitlePosition(NSTitlePosition::NoTitle);
    fill.setContentViewMargins(NSSize::new(0.0, 0.0));
    fill.setFillColor(color);
    fill.setAccessibilityElement(false);
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

/// A button as two views: `root` to position and show, `hit` to wire target and action.
///
/// Legacy shape of [`symbol_button`] and [`pill_button`]. Both now return one
/// [`GlassButton`](crate::button::GlassButton) `NSButton` as `root` and `hit` alike, so existing
/// callers keep working.
#[deprecated(note = "use `mac_ui::button::GlassButton`, a single NSButton with the system bezel")]
#[derive(Debug, Clone)]
pub struct OverlayButton {
    pub root: Retained<NSView>,
    pub hit: Retained<NSButton>,
}

#[allow(deprecated)]
impl OverlayButton {
    fn from_glass(button: &GlassButton, frame: NSRect) -> Self {
        let hit = button.button().retain();
        hit.setFrame(frame);
        let root = button.view().retain();
        Self { root, hit }
    }
}

/// `diameter`-sized circle [`GlassButton`] showing SF Symbol `symbol` at `symbol_size` points.
/// Without SF Symbols (before macOS 11) it shows `fallback` text at `fallback_font_size`
/// instead. VoiceOver reads `fallback` as the label.
#[deprecated(note = "use `GlassButton::symbol`, which takes an accessibility label")]
#[allow(deprecated)]
pub fn symbol_button(
    mtm: MainThreadMarker,
    symbol: &str,
    fallback: &str,
    diameter: f64,
    symbol_size: f64,
    fallback_font_size: f64,
) -> OverlayButton {
    let button = GlassButton::symbol(mtm, symbol, fallback, fallback, symbol_size);
    if button.button().image().is_none() {
        button
            .button()
            .setFont(Some(&NSFont::systemFontOfSize(fallback_font_size)));
    }
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(diameter, diameter));
    OverlayButton::from_glass(&button, frame)
}

/// `width`×`height` capsule [`GlassButton`] titled `title` in the system font at `font_size`.
/// `label_height` is ignored: the button centers its own title.
#[deprecated(note = "use `GlassButton::pill`, which sizes itself with sizeToFit")]
#[allow(deprecated)]
pub fn pill_button(
    mtm: MainThreadMarker,
    title: &str,
    width: f64,
    height: f64,
    font_size: f64,
    label_height: f64,
) -> OverlayButton {
    let _ = label_height;
    let button = GlassButton::pill(mtm, title, ButtonSize::Regular);
    button
        .button()
        .setFont(Some(&NSFont::systemFontOfSize(font_size)));
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
    OverlayButton::from_glass(&button, frame)
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
