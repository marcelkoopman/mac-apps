//! Shared spacing, corners and type sizes.
//!
//! Space is 8, 12 and 16 points. Type is a title, body text and a caption, all in the
//! system font. Callers colour captions with the secondary label, and with the primary
//! label when Increase Contrast is on.

use crate::corners;

/// Tight space: gaps between buttons, and the inset that yields [`WELL_RADIUS`].
pub const SPACE_S: f64 = 8.0;
/// Medium space: padding inside a well, and the gap above a caption.
pub const SPACE_M: f64 = 12.0;
/// Loose space: the window margin and the panel corner.
pub const SPACE_L: f64 = 16.0;

/// Inset of content from a window or panel edge.
pub const MARGIN: f64 = SPACE_L;
/// Gap between neighbouring buttons.
pub const BUTTON_GAP: f64 = SPACE_S;
/// Gap between groups of settings.
pub const SECTION_GAP: f64 = SPACE_L;

/// Corner radius of a panel.
pub const PANEL_RADIUS: f64 = SPACE_L;
/// Corner radius of a content well.
///
/// A well that sits [`MARGIN`] inside a [`PANEL_RADIUS`] panel would clamp to
/// [`corners::MIN_RADIUS`]. The well keeps this 8 pt radius instead, which is the
/// concentric radius of the panel at an 8 pt inset.
pub const WELL_RADIUS: f64 = corners::concentric_radius(PANEL_RADIUS, SPACE_S);

/// Title and body use one size. The caption is smaller.
pub const FONT_TITLE: f64 = 13.0;
pub const FONT_BODY: f64 = 13.0;
pub const FONT_CAPTION: f64 = 12.0;

/// Shared height of a pill or a symbol button.
pub const BUTTON_H: f64 = 28.0;

/// Short move, in seconds. Skip it when Reduce Motion is on.
pub const MOTION_SECS: f64 = 0.15;

const _: () = assert!(WELL_RADIUS == SPACE_S);
const _: () = assert!(MARGIN > WELL_RADIUS);
