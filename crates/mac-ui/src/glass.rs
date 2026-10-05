//! Window and panel backgrounds: Liquid Glass (`NSGlassEffectView`, macOS 26+) with a frosted
//! `NSVisualEffectView` fallback on older macOS, and [`group`]s that let nearby glass controls
//! (`NSGlassEffectContainerView`) render, merge and morph together.
//!
//! No separate macOS 27 API is bound in the current objc2 crates; when Apple ships one, extend
//! this module rather than adding a new crate.

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectContainerView, NSGlassEffectView,
    NSGlassEffectViewStyle, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_foundation::NSRect;

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

/// Which view [`group`] built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    /// `NSGlassEffectContainerView` (macOS 26+).
    Container,
    /// A plain `NSView`; older macOS has no glass to merge.
    Plain,
}

/// Result of [`group`]: a container for glass controls such as a row of
/// [`GlassButton`](crate::button::GlassButton)s.
#[derive(Debug)]
pub struct Group {
    root: Retained<NSView>,
    content: Retained<NSView>,
    kind: GroupKind,
}

impl Group {
    /// The view to add to the parent and give a frame.
    pub fn view(&self) -> &NSView {
        &self.root
    }

    /// Where the grouped controls go, in the coordinates of [`view`](Self::view)'s bounds. It
    /// tracks the group's size. Without the container it is [`view`](Self::view) itself.
    pub fn content(&self) -> &NSView {
        &self.content
    }

    /// Which view was built.
    pub fn kind(&self) -> GroupKind {
        self.kind
    }
}

/// Whether this macOS has `NSGlassEffectContainerView`. Checked at runtime.
pub fn group_is_available() -> bool {
    AnyClass::get(c"NSGlassEffectContainerView").is_some()
}

/// A group for glass controls placed close together: on macOS 26+ an
/// `NSGlassEffectContainerView` that renders them in one pass and merges controls that come
/// within `spacing` points of each other (0 only batches, without merging), else a plain view.
///
/// Add the controls to [`Group::content`], then position [`Group::view`].
pub fn group(mtm: MainThreadMarker, spacing: f64) -> Group {
    if !group_is_available() {
        let root = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
        return Group {
            content: root.clone(),
            root,
            kind: GroupKind::Plain,
        };
    }
    // Only reached when the class exists, so the binding's class lookup cannot fail.
    let container = NSGlassEffectContainerView::initWithFrame(
        NSGlassEffectContainerView::alloc(mtm),
        NSRect::ZERO,
    );
    container.setSpacing(merge_spacing(spacing));
    let content = NSView::initWithFrame(NSView::alloc(mtm), container.bounds());
    track_size(&content);
    container.setContentView(Some(&content));
    Group {
        root: container.into_super(),
        content,
        kind: GroupKind::Container,
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

/// `spacing` as the container takes it: negative or NaN becomes 0 (batch only, no merging).
fn merge_spacing(spacing: f64) -> f64 {
    if spacing > 0.0 { spacing } else { 0.0 }
}

/// Major version of the running macOS (`NSProcessInfo`), for tests that check the runtime
/// fallbacks against the OS they run on (CI runs them on macOS 26 and on an older macOS). `None`
/// when it cannot be read.
#[cfg(test)]
pub(crate) fn running_macos_major() -> Option<u32> {
    let version = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
    u32::try_from(version.majorVersion)
        .ok()
        .filter(|major| *major > 0)
}

#[cfg(test)]
mod tests {
    use super::{group_is_available, is_available, merge_spacing, running_macos_major};
    use objc2::ClassType;
    use objc2_app_kit::NSView;

    #[test]
    fn glass_classes_match_the_running_macos() {
        // Load AppKit before looking classes up by name.
        let _ = NSView::class();
        let Some(major) = running_macos_major() else {
            return;
        };
        let glass = major >= 26;
        assert_eq!(is_available(), glass, "NSGlassEffectView on macOS {major}");
        assert_eq!(
            group_is_available(),
            glass,
            "NSGlassEffectContainerView on macOS {major}"
        );
    }

    #[test]
    fn spacing_is_never_negative() {
        assert_eq!(merge_spacing(6.0), 6.0);
        assert_eq!(merge_spacing(0.0), 0.0);
        assert_eq!(merge_spacing(-3.0), 0.0);
        assert_eq!(merge_spacing(f64::NAN), 0.0);
    }
}
