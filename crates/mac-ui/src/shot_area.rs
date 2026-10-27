//! The arithmetic behind `screenshot`: the area dragged out, the window under the pointer, the
//! two coordinate systems and the thumbnail's place. No AppKit, so it builds and is tested on
//! every platform.
//!
//! AppKit puts the origin at the bottom left of the main screen, y up; Core Graphics and
//! ScreenCaptureKit at its top left, y down. A capture's source rect is in points, relative to
//! the top left of its display.

/// A rectangle in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// The part of `self` inside `other`, or `None` when they do not overlap.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let bottom = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let top = (self.y + self.height).min(other.y + other.height);
        (right > left && top > bottom).then(|| Rect::new(left, bottom, right - left, top - bottom))
    }
}

/// What the window picker and a capture leave out: every window of process `pid` (an app's own,
/// unless the user lets them in), and single windows by `CGWindowID` (the overlay, the
/// thumbnail and an app's windows that always stay out). ScreenCaptureKit ignores a window's
/// `sharingType` since macOS 15, so these are left out by id instead.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaveOut {
    pub pid: Option<i32>,
    pub windows: Vec<u32>,
}

impl LeaveOut {
    /// Whether the window `window` of process `owner` (when known) stays out.
    pub fn leaves_out(&self, owner: Option<i32>, window: u32) -> bool {
        (self.pid.is_some() && owner == self.pid) || self.windows.contains(&window)
    }

    /// `self` with the windows `more` left out too (zero, an unset window number, is skipped).
    pub fn with(mut self, more: impl IntoIterator<Item = u32>) -> Self {
        for window in more {
            if window != 0 && !self.windows.contains(&window) {
                self.windows.push(window);
            }
        }
        self
    }
}

/// How long a capture waits after the overlay is ordered out, so the window server has drawn a
/// frame without it: about three frames at 60 Hz.
pub const AFTER_OVERLAY_SECS: f64 = 0.05;
const _: () = assert!(AFTER_OVERLAY_SECS > 0.016 && AFTER_OVERLAY_SECS < 0.2);

/// A drag shorter than this (in points, either way) is a click: the whole screen.
pub const CLICK_SLOP: f64 = 4.0;

/// The rectangle between where a drag started and where the pointer is now.
pub fn dragged(start: (f64, f64), now: (f64, f64)) -> Rect {
    Rect::new(
        start.0.min(now.0),
        start.1.min(now.1),
        (start.0 - now.0).abs(),
        (start.1 - now.1).abs(),
    )
}

/// Whether a drag from `start` to `end` is only a click.
pub fn is_click(start: (f64, f64), end: (f64, f64)) -> bool {
    (start.0 - end.0).abs() < CLICK_SLOP && (start.1 - end.1).abs() < CLICK_SLOP
}

/// `area` (AppKit screen coordinates) as a capture source rect on the display whose AppKit
/// frame is `screen`: relative to the display's top left, y down, cut to the display. `None`
/// when nothing of it is on that display.
pub fn source_rect(area: &Rect, screen: &Rect) -> Option<Rect> {
    let inside = area.intersection(screen)?;
    Some(Rect::new(
        inside.x - screen.x,
        (screen.y + screen.height) - (inside.y + inside.height),
        inside.width,
        inside.height,
    ))
}

/// A Core Graphics rectangle (global, origin at the main screen's top left, y down) in AppKit
/// coordinates, `main_height` being the main screen's height.
pub fn from_core_graphics(rect: &Rect, main_height: f64) -> Rect {
    Rect::new(
        rect.x,
        main_height - (rect.y + rect.height),
        rect.width,
        rect.height,
    )
}

/// Pixels for `points` at `scale` pixels per point: at least one.
pub fn pixels(points: f64, scale: f64) -> usize {
    let value = (points * scale).round();
    if value.is_finite() && value >= 1.0 {
        value as usize
    } else {
        1
    }
}

/// The front-most of `windows` (front to back, AppKit frames) under the point, by its id.
pub fn window_at(windows: &[(u32, Rect)], x: f64, y: f64) -> Option<(u32, Rect)> {
    windows
        .iter()
        .find(|(_, frame)| frame.contains(x, y))
        .copied()
}

/// The thumbnail's size for a picture of `width` × `height` pixels: its shape, within `max`
/// points either way, never larger than the picture.
pub fn fit(width: f64, height: f64, max: f64) -> (f64, f64) {
    if width <= 0.0 || height <= 0.0 {
        return (max, max);
    }
    let scale = (max / width).min(max / height).min(1.0);
    ((width * scale).max(1.0), (height * scale).max(1.0))
}

/// Where the thumbnail of `size` goes: the bottom-right corner of the visible screen area
/// `visible`, `margin` from its edges.
pub fn bottom_right(visible: &Rect, size: (f64, f64), margin: f64) -> Rect {
    Rect::new(
        visible.x + visible.width - margin - size.0,
        visible.y + margin,
        size.0,
        size.1,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn leave_out_by_process_and_by_window() {
        let none = LeaveOut::default();
        assert!(!none.leaves_out(Some(7), 1));
        let own = LeaveOut {
            pid: Some(7),
            windows: Vec::new(),
        };
        assert!(own.leaves_out(Some(7), 1));
        assert!(!own.leaves_out(Some(8), 1));
        assert!(!own.leaves_out(None, 1));
        // Own windows let in, but the overlay (40), Settings (41) and the thumbnail stay out.
        let some = LeaveOut::default().with([40, 41, 0, 40]);
        assert_eq!(some.windows, vec![40, 41]);
        assert!(some.leaves_out(Some(7), 40));
        assert!(some.leaves_out(None, 41));
        assert!(!some.leaves_out(Some(7), 42));
    }

    use super::*;

    #[test]
    fn a_drag_spans_both_corners_either_way_and_a_tiny_one_is_a_click() {
        let rect = Rect::new(10.0, 20.0, 90.0, 40.0);
        assert_eq!(dragged((10.0, 20.0), (100.0, 60.0)), rect);
        assert_eq!(dragged((100.0, 60.0), (10.0, 20.0)), rect);
        assert_eq!(dragged((100.0, 20.0), (10.0, 60.0)), rect);
        assert!(is_click((10.0, 10.0), (12.0, 13.0)));
        assert!(!is_click((10.0, 10.0), (10.0, 20.0)));
        assert!(!is_click((10.0, 10.0), (20.0, 10.0)));
    }

    #[test]
    fn an_area_becomes_a_top_left_source_rect_on_its_display() {
        // The main screen: 1440 × 900 at the origin.
        let main = Rect::new(0.0, 0.0, 1440.0, 900.0);
        // 100 × 50 with its top edge 100 pt below the top of the screen.
        let area = Rect::new(20.0, 750.0, 100.0, 50.0);
        assert_eq!(
            source_rect(&area, &main),
            Some(Rect::new(20.0, 100.0, 100.0, 50.0))
        );
        // A second screen left of the main one, its bottom 100 pt higher.
        let left = Rect::new(-1920.0, 100.0, 1920.0, 1080.0);
        let area = Rect::new(-1900.0, 1080.0, 200.0, 100.0);
        assert_eq!(
            source_rect(&area, &left),
            Some(Rect::new(20.0, 0.0, 200.0, 100.0))
        );
        // Off the display: nothing; half on it: the half that is.
        assert_eq!(
            source_rect(&Rect::new(2000.0, 0.0, 10.0, 10.0), &main),
            None
        );
        assert_eq!(
            source_rect(&Rect::new(1400.0, 0.0, 100.0, 10.0), &main),
            Some(Rect::new(1400.0, 890.0, 40.0, 10.0))
        );
    }

    #[test]
    fn core_graphics_frames_flip_around_the_main_screen() {
        // A window 100 pt from the top of a 900 pt main screen, 200 pt high.
        let flipped = from_core_graphics(&Rect::new(50.0, 100.0, 300.0, 200.0), 900.0);
        assert_eq!(flipped, Rect::new(50.0, 600.0, 300.0, 200.0));
        // And back: flipping twice is the same rectangle.
        assert_eq!(
            from_core_graphics(&flipped, 900.0),
            Rect::new(50.0, 100.0, 300.0, 200.0)
        );
    }

    #[test]
    fn pixels_round_and_never_reach_zero() {
        assert_eq!(pixels(100.0, 2.0), 200);
        assert_eq!(pixels(100.4, 1.0), 100);
        assert_eq!(pixels(0.1, 1.0), 1);
        assert_eq!(pixels(f64::NAN, 2.0), 1);
    }

    #[test]
    fn the_front_most_window_under_the_pointer_wins() {
        let windows = [
            (7, Rect::new(100.0, 100.0, 200.0, 200.0)),
            (3, Rect::new(0.0, 0.0, 1000.0, 1000.0)),
        ];
        assert_eq!(window_at(&windows, 150.0, 150.0).map(|(id, _)| id), Some(7));
        assert_eq!(window_at(&windows, 50.0, 50.0).map(|(id, _)| id), Some(3));
        assert_eq!(window_at(&windows, 5000.0, 50.0), None);
    }

    #[test]
    fn the_thumbnail_keeps_the_shape_and_sits_bottom_right() {
        assert_eq!(fit(2000.0, 1000.0, 200.0), (200.0, 100.0));
        assert_eq!(fit(1000.0, 2000.0, 200.0), (100.0, 200.0));
        // A small picture is not blown up.
        assert_eq!(fit(80.0, 40.0, 200.0), (80.0, 40.0));
        assert_eq!(fit(0.0, 40.0, 200.0), (200.0, 200.0));
        let visible = Rect::new(0.0, 80.0, 1440.0, 795.0);
        assert_eq!(
            bottom_right(&visible, (200.0, 100.0), 16.0),
            Rect::new(1224.0, 96.0, 200.0, 100.0)
        );
    }
}
