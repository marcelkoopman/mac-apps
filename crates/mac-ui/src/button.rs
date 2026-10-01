//! Push buttons in the system style: Liquid Glass (`NSBezelStyle::Glass`) on macOS 26+, a
//! standard bezel before.
//!
//! A [`GlassButton`] is one `NSButton`: AppKit draws the bezel, the pressed, disabled and focused
//! states, and the focus ring. It replaces the hand-built [`OverlayButton`] (a filled `NSBox`
//! under a transparent button).
//!
//! - [`GlassButton::symbol`]: SF Symbol, circle shape. The accessibility label doubles as the
//!   button title, so VoiceOver never reads an empty button.
//! - [`GlassButton::pill`]: text title, capsule shape, sized with `sizeToFit`.
//!
//! Callers set the frame, visibility and tooltip, and wire target and action
//! ([`GlassButton::set_action`] or `NSControl::setTarget`/`setAction` on [`GlassButton::button`]).
//!
//! [`OverlayButton`]: crate::widgets::OverlayButton

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAccessibility, NSBezelStyle, NSButton, NSCellImagePosition, NSColor, NSControlBorderShape,
    NSControlSize, NSFont, NSImageScaling, NSLineBreakMode, NSTintProminence, NSView,
};
use objc2_foundation::{NSRect, NSSize, NSString};

use crate::glass;
use crate::icon::system_symbol;

/// Outline of a [`GlassButton`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Round, for icon-only buttons. Give the button a square frame.
    Circle,
    /// Fully rounded ends, for text buttons.
    Capsule,
}

/// Which bezel family [`GlassButton`]s get on this macOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// `NSBezelStyle::Glass` with a [`Shape`] border (macOS 26+).
    Glass,
    /// `NSBezelStyle::FlexiblePush`, the push bezel that follows the frame height (before
    /// macOS 26). Callers give the buttons fixed heights, which the regular push bezel ignores.
    Standard,
}

impl Look {
    /// The look of this macOS, checked at runtime: [`Look::Glass`] when AppKit has the glass
    /// classes and `NSButton` responds to `setBorderShape:`, else [`Look::Standard`].
    pub fn current() -> Self {
        if glass::is_available() && NSButton::class().responds_to(sel!(setBorderShape:)) {
            Self::Glass
        } else {
            Self::Standard
        }
    }

    /// Bezel style for this look.
    pub fn bezel_style(self) -> NSBezelStyle {
        match self {
            Self::Glass => NSBezelStyle::Glass,
            Self::Standard => NSBezelStyle::FlexiblePush,
        }
    }

    /// Border shape to set for `shape`, or `None` where `setBorderShape:` does not exist
    /// (before macOS 26).
    pub fn border_shape(self, shape: Shape) -> Option<NSControlBorderShape> {
        match (self, shape) {
            (Self::Glass, Shape::Circle) => Some(NSControlBorderShape::Circle),
            (Self::Glass, Shape::Capsule) => Some(NSControlBorderShape::Capsule),
            (Self::Standard, _) => None,
        }
    }
}

/// AppKit control size of a [`GlassButton`]. It also sets the system font size of a pill title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// Small controls, for secondary buttons inside content.
    Small,
    /// Regular controls.
    #[default]
    Regular,
    /// Large controls (macOS 11+).
    Large,
}

impl ButtonSize {
    fn control_size(self) -> NSControlSize {
        match self {
            Self::Small => NSControlSize::Small,
            Self::Regular => NSControlSize::Regular,
            Self::Large => NSControlSize::Large,
        }
    }
}

/// One `NSButton` with the system glass (or standard) bezel. See the [module docs](self).
#[derive(Debug)]
pub struct GlassButton {
    button: Retained<NSButton>,
    shape: Shape,
    look: Look,
    /// Frame size right after the last `sizeToFit`; callers may set other frames afterwards.
    fitted: Cell<NSSize>,
}

impl GlassButton {
    /// Circle button showing SF Symbol `symbol` at `point_size` points (template image).
    ///
    /// `label` names the action for VoiceOver ("Copy", "Save"). It is the button title, kept out
    /// of sight by the image-only position, and the accessibility label. Without SF Symbols
    /// (before macOS 11, or an unknown name) the button shows `fallback` text in the system
    /// font at `point_size` instead. An empty `label` falls back to `fallback`, then `symbol`.
    pub fn symbol(
        mtm: MainThreadMarker,
        symbol: &str,
        label: &str,
        fallback: &str,
        point_size: f64,
    ) -> Self {
        let this = Self::new(mtm, Shape::Circle);
        let label = accessible_label(label, fallback, symbol);
        let title = NSString::from_str(label);
        if let Some(image) = system_symbol(symbol, label) {
            image.setTemplate(true);
            image.setSize(NSSize::new(point_size, point_size));
            this.button.setImage(Some(&image));
            this.button.setImagePosition(NSCellImagePosition::ImageOnly);
            this.button
                .setImageScaling(NSImageScaling::ScaleProportionallyDown);
            this.button.setTitle(&title);
        } else {
            this.button.setTitle(&NSString::from_str(fallback));
            this.button
                .setFont(Some(&NSFont::systemFontOfSize(point_size)));
        }
        this.button.setAccessibilityLabel(Some(&title));
        this.fit();
        this
    }

    /// Capsule button with `title` in the system font for `size`, sized to fit the title. A
    /// narrower frame truncates the title at the tail.
    pub fn pill(mtm: MainThreadMarker, title: &str, size: ButtonSize) -> Self {
        let this = Self::new(mtm, Shape::Capsule);
        this.button.setControlSize(size.control_size());
        this.button.setFont(Some(&NSFont::systemFontOfSize(
            NSFont::systemFontSizeForControlSize(size.control_size()),
        )));
        // A frame narrower than the title cuts it with an ellipsis instead of clipping.
        this.button
            .setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        this.set_title(title);
        this
    }

    fn new(mtm: MainThreadMarker, shape: Shape) -> Self {
        let look = Look::current();
        // `initWithFrame:` makes a momentary push button; only the bezel and shape change.
        let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
        button.setBezelStyle(look.bezel_style());
        if let Some(border) = look.border_shape(shape) {
            button.setBorderShape(border);
        }
        Self {
            button,
            shape,
            look,
            fitted: Cell::new(NSSize::new(0.0, 0.0)),
        }
    }

    fn fit(&self) {
        self.button.sizeToFit();
        self.fitted.set(self.button.frame().size);
    }

    /// The button, for frames, visibility, tooltips, `setEnabled` and target/action.
    pub fn button(&self) -> &NSButton {
        &self.button
    }

    /// The button as a plain view, for `addSubview`, `raise_view` and the like.
    pub fn view(&self) -> &NSView {
        &self.button
    }

    /// Outline the button was built with.
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Bezel family the button was built with.
    pub fn look(&self) -> Look {
        self.look
    }

    /// Replace the title (and the accessibility label, which follows it) and resize the button
    /// to fit. Use on pills; a symbol button keeps its image either way.
    pub fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        self.button.setTitle(&title);
        self.button.setAccessibilityLabel(Some(&title));
        self.fit();
    }

    /// Accessibility label read by VoiceOver, for titles that do not say what the button does
    /// (e.g. "<" for "Older").
    pub fn set_accessibility_label(&self, label: &str) {
        self.button
            .setAccessibilityLabel(Some(&NSString::from_str(label)));
    }

    /// Fitted size from the last `sizeToFit` (construction or [`set_title`](Self::set_title)).
    pub fn fitted_size(&self) -> NSSize {
        self.fitted.get()
    }

    /// Fitted width rounded up to whole points and capped at `max`, for a pill laid out by
    /// frame.
    pub fn width_within(&self, max: f64) -> f64 {
        capped_width(self.fitted_size().width, max)
    }

    /// Mark the button as the preferred or selected one: primary tint prominence on macOS 26+,
    /// the accent color as bezel color before. `false` returns to the automatic tint.
    pub fn set_prominent(&self, prominent: bool) {
        if NSButton::class().responds_to(sel!(setTintProminence:)) {
            self.button.setTintProminence(prominence(prominent));
        } else {
            let accent = prominent.then(NSColor::controlAccentColor);
            self.button.setBezelColor(accent.as_deref());
        }
    }

    /// Send `action` (a `sender`-taking method such as `copyClicked:`) to `target` on click.
    ///
    /// # Safety
    ///
    /// The button does not retain `target`. Keep it alive for as long as the button can be
    /// clicked, and make sure it implements `action` taking one object argument.
    pub unsafe fn set_action(&self, target: &AnyObject, action: Sel) {
        // SAFETY: the caller keeps `target` alive and responding to `action` (see above).
        unsafe {
            self.button.setTarget(Some(target));
            self.button.setAction(Some(action));
        }
    }
}

/// `label`, else `fallback`, else `symbol`: the first that is not blank.
fn accessible_label<'a>(label: &'a str, fallback: &'a str, symbol: &'a str) -> &'a str {
    [label, fallback]
        .into_iter()
        .find(|text| !text.trim().is_empty())
        .unwrap_or(symbol)
}

/// `fitted` rounded up to whole points, at most `max` and never negative.
fn capped_width(fitted: f64, max: f64) -> f64 {
    fitted.ceil().min(max).max(0.0)
}

fn prominence(prominent: bool) -> NSTintProminence {
    if prominent {
        NSTintProminence::Primary
    } else {
        NSTintProminence::Automatic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_look_uses_the_glass_bezel_and_the_shape() {
        assert_eq!(Look::Glass.bezel_style(), NSBezelStyle::Glass);
        assert_eq!(
            Look::Glass.border_shape(Shape::Circle),
            Some(NSControlBorderShape::Circle)
        );
        assert_eq!(
            Look::Glass.border_shape(Shape::Capsule),
            Some(NSControlBorderShape::Capsule)
        );
    }

    #[test]
    fn current_look_falls_back_before_macos_26() {
        let Some(major) = crate::glass::running_macos_major() else {
            return;
        };
        let look = Look::current();
        eprintln!("button look on macOS {major}: {look:?}");
        if major < 26 {
            assert_eq!(look, Look::Standard, "button look on macOS {major}");
        }
        // On 26+ Glass also needs `setBorderShape:`, which AppKit does not offer every binary
        // (the CI test binary on macOS 26 gets Standard), so either look is valid there.
        if look == Look::Glass {
            assert!(crate::glass::is_available());
        }
    }

    #[test]
    fn standard_look_uses_a_flexible_push_bezel_without_border_shape() {
        assert_eq!(Look::Standard.bezel_style(), NSBezelStyle::FlexiblePush);
        assert_eq!(Look::Standard.border_shape(Shape::Circle), None);
        assert_eq!(Look::Standard.border_shape(Shape::Capsule), None);
    }

    #[test]
    fn label_falls_back_to_the_text_then_the_symbol() {
        assert_eq!(accessible_label("Copy", "⎘", "doc.on.doc"), "Copy");
        assert_eq!(accessible_label("  ", "⎘", "doc.on.doc"), "⎘");
        assert_eq!(accessible_label("", "", "doc.on.doc"), "doc.on.doc");
    }

    #[test]
    fn width_rounds_up_and_stays_within_the_cap() {
        assert_eq!(capped_width(80.2, 200.0), 81.0);
        assert_eq!(capped_width(250.0, 200.0), 200.0);
        assert_eq!(capped_width(10.0, -5.0), 0.0);
    }

    #[test]
    fn prominent_means_primary_tint() {
        assert_eq!(prominence(true), NSTintProminence::Primary);
        assert_eq!(prominence(false), NSTintProminence::Automatic);
    }

    #[test]
    fn regular_is_the_default_size() {
        assert_eq!(ButtonSize::default(), ButtonSize::Regular);
        assert_eq!(ButtonSize::Small.control_size(), NSControlSize::Small);
        assert_eq!(ButtonSize::Large.control_size(), NSControlSize::Large);
    }
}
