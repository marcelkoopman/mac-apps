//! AppKit control constructors with the shared look: borderless, native focus rings, system fonts.
//!
//! The helpers only configure the view. Callers set frames, visibility, targets and delegates.
//! Text views also get sizing ([`fit_text_view`]), find marks ([`mark_ranges`]) and wiping
//! ([`wipe_text_view`], [`wipe_text_field`], [`wipe_field_editor`]). [`set_target_action`] and
//! [`set_text_delegate`] wire controls to an app's delegate object.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSBackgroundColorAttributeName, NSBorderType, NSBox, NSBoxType, NSButton,
    NSColor, NSControl, NSFont, NSForegroundColorAttributeName, NSImageAlignment, NSImageScaling,
    NSImageView, NSLineBreakMode, NSScrollView, NSSearchField, NSTextField, NSTextView,
    NSTitlePosition, NSView,
};
use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize, NSString};

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

define_class!(
    /// Scroll view that never takes keyboard focus, so Tab skips it.
    #[unsafe(super(NSScrollView))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacUiPassiveTextScroll"]
    struct PassiveTextScroll;

    impl PassiveTextScroll {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            false
        }
    }
);

/// A [`configure_text_scroll`] scroller for `text` that never becomes first responder, for
/// previews the keyboard should pass over.
pub fn passive_text_scroll(mtm: MainThreadMarker, text: &NSTextView) -> Retained<NSScrollView> {
    let allocated = PassiveTextScroll::alloc(mtm);
    let scroll: Retained<PassiveTextScroll> =
        unsafe { msg_send![allocated, initWithFrame: NSRect::ZERO] };
    let scroll = scroll.into_super();
    configure_text_scroll(&scroll, text);
    scroll
}

/// Send `action` to `target` when `control` is clicked.
///
/// # Safety
///
/// `target` must implement `action` as a method taking the sender (`-action:(id)sender`). The
/// control holds `target` weakly, so keep it alive for as long as clicks should reach it.
pub unsafe fn set_target_action(control: &NSControl, target: &AnyObject, action: Sel) {
    unsafe {
        control.setTarget(Some(target));
        control.setAction(Some(action));
    }
}

/// Make `delegate` the delegate of `field` (text changes, editing commands).
///
/// # Safety
///
/// Every `NSTextFieldDelegate` / `NSControlTextEditingDelegate` method `delegate` implements must
/// have the protocol's signature. The field holds `delegate` weakly, so keep it alive.
pub unsafe fn set_text_delegate(field: &NSTextField, delegate: &AnyObject) {
    unsafe {
        let _: () = msg_send![field, setDelegate: delegate];
    }
}

/// Let `text` grow in both directions without wrapping, so long lines scroll sideways in its
/// scroll view.
pub fn scroll_text_both_ways(text: &NSTextView) {
    const LARGE: f64 = 10_000_000.0;
    text.setHorizontallyResizable(true);
    text.setVerticallyResizable(true);
    text.setMaxSize(NSSize::new(LARGE, LARGE));
    // SAFETY: the container is used right away, while the text view keeps it alive.
    if let Some(container) = unsafe { text.textContainer() } {
        container.setWidthTracksTextView(false);
        container.setHeightTracksTextView(false);
        container.setContainerSize(NSSize::new(LARGE, LARGE));
    }
}

/// Size `text` to everything it holds, so its scroll view can reach the last line. With
/// `wrap_width` the lines wrap at that width (prose); without, long lines are kept and scroll
/// sideways as well as down.
pub fn fit_text_view(text: &NSTextView, wrap_width: Option<f64>) {
    // SAFETY (both): used right away, while the text view keeps them alive.
    let Some(container) = (unsafe { text.textContainer() }) else {
        return;
    };
    let Some(manager) = (unsafe { text.layoutManager() }) else {
        return;
    };
    const LARGE: f64 = 10_000_000.0;
    let inset = text.textContainerInset();
    if let Some(width) = wrap_width {
        text.setHorizontallyResizable(false);
        text.setVerticallyResizable(true);
        text.setMaxSize(NSSize::new(width.max(1.0), LARGE));
        container.setWidthTracksTextView(true);
        container.setHeightTracksTextView(false);
        let inner = (width - inset.width * 2.0).max(1.0);
        container.setContainerSize(NSSize::new(inner, LARGE));
        text.setFrameSize(NSSize::new(
            width.max(1.0),
            text.frame().size.height.max(1.0),
        ));
    } else {
        scroll_text_both_ways(text);
    }
    manager.ensureLayoutForTextContainer(&container);
    let used = manager.usedRectForTextContainer(&container);
    let width = match wrap_width {
        Some(width) => width,
        None => used.size.width + inset.width * 2.0,
    };
    let height = used.size.height + inset.height * 2.0;
    text.setFrameSize(NSSize::new(width.max(1.0), height.max(1.0)));
}

/// Paint find matches in `view`: `ranges` are UTF-16 `(location, length)` pairs in its text
/// (see `find`), `current` the one drawn like the find indicator and scrolled to with `scroll`.
/// Earlier marks are removed first; no ranges just clears them. The view's text is not copied.
pub fn mark_ranges(
    view: &NSTextView,
    ranges: &[(usize, usize)],
    current: Option<usize>,
    scroll: bool,
) {
    // SAFETY: the layout manager is used right away, while the text view keeps it alive.
    let Some(manager) = (unsafe { view.layoutManager() }) else {
        return;
    };
    let length = view.string().length();
    if length > 0 {
        let all = NSRange {
            location: 0,
            length,
        };
        // SAFETY: removing attributes takes no value.
        unsafe {
            manager.removeTemporaryAttribute_forCharacterRange(NSBackgroundColorAttributeName, all);
            manager.removeTemporaryAttribute_forCharacterRange(NSForegroundColorAttributeName, all);
        }
    }
    if ranges.is_empty() {
        return;
    }
    // The system find colours. The current match is drawn like the find indicator, with dark
    // text on the opaque highlight, so it stays readable in dark mode. Other matches get a light
    // wash of the same colour under the normal text colour.
    let hot = NSColor::findHighlightColor();
    let hot_text = NSColor::blackColor();
    let wash = hot.colorWithAlphaComponent(0.35);
    for (index, &(location, len)) in ranges.iter().enumerate() {
        if location + len > length {
            break;
        }
        let range = NSRange {
            location,
            length: len,
        };
        let is_current = current == Some(index);
        let color = if is_current { &hot } else { &wash };
        // SAFETY: both background and foreground colour attributes take an NSColor value.
        unsafe {
            manager.addTemporaryAttribute_value_forCharacterRange(
                NSBackgroundColorAttributeName,
                color,
                range,
            );
            if is_current {
                manager.addTemporaryAttribute_value_forCharacterRange(
                    NSForegroundColorAttributeName,
                    &hot_text,
                    range,
                );
            }
        }
    }
    if scroll
        && let Some(&(location, len)) = current.and_then(|index| ranges.get(index))
        && location + len <= length
    {
        view.scrollRangeToVisible(NSRange {
            location,
            length: len,
        });
    }
}

/// Overwrite the characters of `view` with NULs, then empty it, so the text does not linger
/// in its storage. Best effort: copies AppKit made earlier are out of reach.
pub fn wipe_text_view(view: &NSTextView) {
    // SAFETY: the storage is used right away, while the text view keeps it alive.
    if let Some(storage) = unsafe { view.textStorage() } {
        let length = storage.length();
        if length > 0 {
            let zeros = "\0".repeat(length);
            storage.replaceCharactersInRange_withString(
                NSRange {
                    location: 0,
                    length,
                },
                &NSString::from_str(&zeros),
            );
        }
    }
    view.setString(&NSString::from_str(""));
}

/// [`wipe_text_view`] for a text field's value.
pub fn wipe_text_field(field: &NSTextField) {
    let current = field.stringValue();
    let length = current.length();
    if length > 0 {
        let zeros = "\0".repeat(length);
        field.setStringValue(&NSString::from_str(&zeros));
    }
    field.setStringValue(&NSString::from_str(""));
}

/// The same for the field editor of `field` while it is being edited (it holds its own copy of
/// the text).
pub fn wipe_field_editor(field: &NSTextField) {
    if let Some(editor) = field.currentEditor() {
        let current = editor.string();
        let length = current.length();
        if length > 0 {
            let zeros = "\0".repeat(length);
            editor.setString(&NSString::from_str(&zeros));
        }
        editor.setString(&NSString::from_str(""));
    }
}
