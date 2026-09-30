//! Borderless floating panels: creation, floating behaviour, activation and placement.
//!
//! The helpers are stateless. The app keeps the window (usually its own `NSWindow` subclass for
//! key handling), its delegate and any open/closed state.

use objc2::rc::{Allocated, Retained};
use objc2::{MainThreadMarker, Message, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSFloatingWindowLevel, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

pub use crate::activation::activate_app;
use crate::glass;
use crate::layer::round_view;

/// Used by [`near_cursor`] when no screen is found.
const FALLBACK_SCREEN: NSSize = NSSize::new(1440.0, 900.0);

/// Initialise `allocated` (an `NSWindow` or subclass) as a borderless, buffered window with a
/// `size` content rect at the origin.
pub fn borderless<W>(allocated: Allocated<W>, size: NSSize) -> Retained<W>
where
    W: Message + AsRef<NSWindow>,
{
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), size);
    // SAFETY: `W` is an NSWindow (sub)class (`AsRef<NSWindow>`), and this is NSWindow's
    // designated initialiser with argument types matching its signature.
    unsafe {
        msg_send![
            allocated,
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    }
}

/// Make `window` a transparent, shadowed panel that floats above normal windows, shows on every
/// Space and over full-screen apps, stays out of the window cycle and can be dragged by its
/// background. It is not released on close, so the caller's `Retained` stays valid.
pub fn configure_floating(window: &NSWindow) {
    // SAFETY: the caller holds a `Retained` to the window, so AppKit must not also release it
    // on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setHasShadow(true);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setLevel(NSFloatingWindowLevel);
    window.setMovableByWindowBackground(true);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
}

/// Set `delegate` as the window delegate. It receives the `NSWindowDelegate` messages it
/// implements (for example `windowDidResignKey:`).
///
/// # Safety
///
/// The window does not retain its delegate. Keep `delegate` alive for as long as it is set.
pub unsafe fn set_delegate<D: Message>(window: &NSWindow, delegate: &D) {
    // SAFETY: `setDelegate:` takes any object; the caller keeps it alive (see above).
    unsafe {
        let _: () = msg_send![window, setDelegate: delegate];
    }
}

/// Round the corners of `view` (usually the window's content view) and put the glass background
/// behind it; see [`glass::background`].
pub fn rounded_glass(mtm: MainThreadMarker, view: &NSView, radius: f64) -> glass::Background {
    round_view(view, radius);
    glass::background(mtm, view, radius)
}

/// Make `window` key and put it in front, also when the app is not active.
pub fn bring_to_front(window: &NSWindow) {
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
}

/// The screen containing `point` (screen coordinates), else the main screen.
pub fn screen_at(mtm: MainThreadMarker, point: NSPoint) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    for screen in screens.iter() {
        let frame = screen.frame();
        let inside = point.x >= frame.origin.x
            && point.x < frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y < frame.origin.y + frame.size.height;
        if inside {
            return Some(screen);
        }
    }
    NSScreen::mainScreen(mtm)
}

/// How [`near_cursor`] places a panel relative to the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NearCursor {
    /// Minimum distance to the edges of the visible screen area.
    pub margin: f64,
    /// The panel's left edge sits this far left of the pointer.
    pub lead_x: f64,
    /// Vertical gap between the pointer and the panel.
    pub gap_y: f64,
}

/// Frame of `size` near the mouse pointer on the screen under it (see [`frame_near`]).
pub fn near_cursor(mtm: MainThreadMarker, size: NSSize, place: &NearCursor) -> NSRect {
    let cursor = NSEvent::mouseLocation();
    let visible = screen_at(mtm, cursor)
        .map(|screen| screen.visibleFrame())
        .unwrap_or_else(|| NSRect::new(NSPoint::new(0.0, 0.0), FALLBACK_SCREEN));
    frame_near(cursor, visible, size, place)
}

/// Frame of `size` just above `point`, or below it when there is no room above, kept inside
/// `visible` with `place.margin` to spare where it fits.
pub fn frame_near(point: NSPoint, visible: NSRect, size: NSSize, place: &NearCursor) -> NSRect {
    let min_x = visible.origin.x + place.margin;
    let max_x = visible.origin.x + visible.size.width - size.width - place.margin;
    let x = clamp_axis(point.x - place.lead_x, min_x, max_x);
    let top = visible.origin.y + visible.size.height - place.margin;
    let mut y = point.y + place.gap_y;
    if y + size.height > top {
        y = point.y - place.gap_y - size.height;
    }
    y = clamp_axis(
        y,
        visible.origin.y + place.margin,
        (top - size.height).max(visible.origin.y),
    );
    NSRect::new(NSPoint::new(x, y), size)
}

/// `current` resized to `size` with its left and top edges kept in place.
pub fn keep_top_left(current: NSRect, size: NSSize) -> NSRect {
    let top = current.origin.y + current.size.height;
    NSRect::new(NSPoint::new(current.origin.x, top - size.height), size)
}

fn clamp_axis(value: f64, min: f64, max: f64) -> f64 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACE: NearCursor = NearCursor {
        margin: 8.0,
        lead_x: 36.0,
        gap_y: 12.0,
    };
    const SIZE: NSSize = NSSize::new(440.0, 300.0);

    fn screen() -> NSRect {
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0))
    }

    #[test]
    fn opens_above_the_pointer_when_there_is_room() {
        let frame = frame_near(NSPoint::new(500.0, 100.0), screen(), SIZE, &PLACE);
        assert_eq!(frame.origin, NSPoint::new(464.0, 112.0));
        assert_eq!(frame.size, SIZE);
    }

    #[test]
    fn flips_below_the_pointer_near_the_top() {
        let frame = frame_near(NSPoint::new(500.0, 800.0), screen(), SIZE, &PLACE);
        assert_eq!(frame.origin.y, 800.0 - 12.0 - 300.0);
    }

    #[test]
    fn stays_inside_the_screen_edges() {
        let left = frame_near(NSPoint::new(0.0, 100.0), screen(), SIZE, &PLACE);
        assert_eq!(left.origin.x, 8.0);
        let right = frame_near(NSPoint::new(1440.0, 100.0), screen(), SIZE, &PLACE);
        assert_eq!(right.origin.x, 1440.0 - 440.0 - 8.0);
    }

    #[test]
    fn resize_keeps_the_top_left_corner() {
        let current = NSRect::new(NSPoint::new(10.0, 100.0), NSSize::new(440.0, 200.0));
        let frame = keep_top_left(current, NSSize::new(440.0, 150.0));
        assert_eq!(frame.origin, NSPoint::new(10.0, 150.0));
    }
}
