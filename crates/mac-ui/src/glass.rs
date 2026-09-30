//! Window and panel backgrounds: Liquid Glass (`NSGlassEffectView`, macOS 26+) with a frosted
//! `NSVisualEffectView` fallback on older macOS.

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSGlassEffectViewStyle, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowOrderingMode,
};

use crate::layer::round_view;

/// Which effect [`background`] installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `NSGlassEffectView` (macOS 26+).
    Glass,
    /// `NSVisualEffectView`, material Popover, behind-window blending.
    Frosted,
}

/// Result of [`background`].
pub struct Background {
    /// The effect view, installed as the backmost subview of the parent. It tracks the parent's
    /// size.
    pub effect: Retained<NSView>,
    /// Where callers add their content, in the parent's coordinates (same origin and size).
    ///
    /// - [`Kind::Glass`]: a plain view set as the glass view's `contentView`, so content gets the
    ///   glass treatment.
    /// - [`Kind::Frosted`]: the parent itself. Content stays a sibling above the frosted view.
    pub content: Retained<NSView>,
    pub kind: Kind,
}

/// Whether this macOS has `NSGlassEffectView`. Checked at runtime, so it works on any macOS.
pub fn is_available() -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some()
}

/// Install a rounded glass (or frosted) background behind everything in `parent`.
///
/// Add the parent's content to [`Background::content`] afterwards.
pub fn background(mtm: MainThreadMarker, parent: &NSView, corner_radius: f64) -> Background {
    if is_available() {
        glass(mtm, parent, corner_radius)
    } else {
        frosted(mtm, parent, corner_radius)
    }
}

fn glass(mtm: MainThreadMarker, parent: &NSView, corner_radius: f64) -> Background {
    // Only reached when the class exists, so the binding's class lookup cannot fail.
    let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), parent.bounds());
    glass.setStyle(NSGlassEffectViewStyle::Regular);
    glass.setCornerRadius(corner_radius);
    track_size(&glass);
    let content = NSView::initWithFrame(NSView::alloc(mtm), glass.bounds());
    track_size(&content);
    glass.setContentView(Some(&content));
    install_backmost(parent, &glass);
    Background {
        effect: glass.into_super(),
        content,
        kind: Kind::Glass,
    }
}

fn frosted(mtm: MainThreadMarker, parent: &NSView, corner_radius: f64) -> Background {
    let frosted =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), parent.bounds());
    frosted.setMaterial(NSVisualEffectMaterial::Popover);
    frosted.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    frosted.setEmphasized(true);
    frosted.setState(NSVisualEffectState::Active);
    track_size(&frosted);
    round_view(&frosted, corner_radius);
    install_backmost(parent, &frosted);
    Background {
        effect: frosted.into_super(),
        content: parent.retain(),
        kind: Kind::Frosted,
    }
}

fn track_size(view: &NSView) {
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
}

fn install_backmost(parent: &NSView, view: &NSView) {
    parent.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, None);
}
