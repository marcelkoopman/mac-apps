//! Indeterminate spinning progress indicator (`NSProgressIndicator`), the usual macOS "busy"
//! wheel.
//!
//! A [`Spinner`] is hidden while stopped. Add it to a view, then [`Spinner::start`] it when work
//! begins and [`Spinner::stop`] (or [`Spinner::remove`]) it when the work ends. All calls need
//! the main thread, like every AppKit view.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSControlSize, NSProgressIndicator, NSProgressIndicatorStyle, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// How big a [`Spinner`] is drawn: AppKit's small or regular control size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpinnerSize {
    /// Small control size (about 16 pt), for toolbars and next to text.
    Small,
    /// Regular control size (about 32 pt), for a spinner over a content area.
    #[default]
    Regular,
}

impl SpinnerSize {
    fn control_size(self) -> NSControlSize {
        match self {
            Self::Small => NSControlSize::Small,
            Self::Regular => NSControlSize::Regular,
        }
    }
}

/// An indeterminate, spinning `NSProgressIndicator` that is hidden while stopped.
#[derive(Debug)]
pub struct Spinner {
    view: Retained<NSProgressIndicator>,
}

impl Spinner {
    /// Create a stopped spinner of `size`, sized to fit and not yet in any view.
    pub fn new(mtm: MainThreadMarker, size: SpinnerSize) -> Self {
        let view =
            NSProgressIndicator::initWithFrame(NSProgressIndicator::alloc(mtm), NSRect::ZERO);
        view.setStyle(NSProgressIndicatorStyle::Spinning);
        view.setIndeterminate(true);
        view.setControlSize(size.control_size());
        view.setDisplayedWhenStopped(false);
        view.sizeToFit();
        Self { view }
    }

    /// The underlying indicator, for tooltips, accessibility or layout.
    pub fn view(&self) -> &NSProgressIndicator {
        &self.view
    }

    /// The fitted size of the spinner.
    pub fn size(&self) -> NSSize {
        self.view.frame().size
    }

    /// Add the spinner on top of `parent`'s subviews, centered over `area` (in `parent`'s
    /// coordinates, for example a sibling's frame). Moves it when it already has a superview.
    pub fn add_centered(&self, parent: &NSView, area: NSRect) {
        self.center_in(area);
        parent.addSubview(&self.view);
    }

    /// Add the spinner on top of `parent`'s subviews at `frame` (in `parent`'s coordinates).
    pub fn add_at(&self, parent: &NSView, frame: NSRect) {
        self.view.setFrame(frame);
        parent.addSubview(&self.view);
    }

    /// Center the spinner over `area`, keeping its size, for example after a relayout.
    pub fn center_in(&self, area: NSRect) {
        self.view.setFrame(centered_frame(self.size(), area));
    }

    /// Whether the spinner is currently in a view.
    pub fn is_added(&self) -> bool {
        // SAFETY: only checked for presence; the reference is dropped right away.
        unsafe { self.view.superview() }.is_some()
    }

    /// Show the spinner and start spinning.
    pub fn start(&self) {
        // SAFETY: `sender` is unused by NSProgressIndicator; `None` is allowed.
        unsafe { self.view.startAnimation(None) };
    }

    /// Stop spinning; the spinner hides itself (it is not displayed when stopped).
    pub fn stop(&self) {
        // SAFETY: `sender` is unused by NSProgressIndicator; `None` is allowed.
        unsafe { self.view.stopAnimation(None) };
    }

    /// Stop spinning and take the spinner out of its superview. It can be added again later.
    pub fn remove(&self) {
        self.stop();
        self.view.removeFromSuperview();
    }
}

/// Frame of `size` centered over `area`, on whole points so the spinner is drawn sharp.
fn centered_frame(size: NSSize, area: NSRect) -> NSRect {
    let x = (area.origin.x + (area.size.width - size.width) / 2.0).round();
    let y = (area.origin.y + (area.size.height - size.height) / 2.0).round();
    NSRect::new(NSPoint::new(x, y), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    #[test]
    fn centers_over_the_area() {
        let frame = centered_frame(NSSize::new(32.0, 32.0), rect(10.0, 20.0, 100.0, 60.0));
        assert_eq!(frame, rect(44.0, 34.0, 32.0, 32.0));
    }

    #[test]
    fn rounds_to_whole_points() {
        let frame = centered_frame(NSSize::new(16.0, 16.0), rect(0.0, 0.0, 33.0, 21.0));
        assert_eq!(frame.origin, NSPoint::new(9.0, 3.0));
        assert_eq!(frame.size, NSSize::new(16.0, 16.0));
    }

    #[test]
    fn larger_than_the_area_overhangs_evenly() {
        let frame = centered_frame(NSSize::new(32.0, 32.0), rect(0.0, 0.0, 16.0, 16.0));
        assert_eq!(frame.origin, NSPoint::new(-8.0, -8.0));
    }

    #[test]
    fn regular_is_the_default_size() {
        assert_eq!(SpinnerSize::default(), SpinnerSize::Regular);
        assert_eq!(SpinnerSize::Small.control_size(), NSControlSize::Small);
        assert_eq!(SpinnerSize::Regular.control_size(), NSControlSize::Regular);
    }
}
