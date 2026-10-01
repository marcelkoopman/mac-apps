//! Concentric corner radii for nested rounded shapes.
//!
//! A shape inset by `inset` points inside a rounded container looks concentric when its corner
//! radius is the container's radius minus the inset, so both curves share one centre. Pure
//! arithmetic, available on every platform.

/// Smallest radius [`concentric_radius`] returns. Below this a rounded fill reads as a sharp
/// corner with a stray anti-aliased pixel, so deep insets keep a small visible rounding instead.
pub const MIN_RADIUS: f64 = 4.0;

/// Corner radius for a shape inset `inset` points inside a container with corner radius
/// `outer`: `outer - inset`, clamped at [`MIN_RADIUS`]. A NaN or negative result also gives
/// [`MIN_RADIUS`].
pub const fn concentric_radius(outer: f64, inset: f64) -> f64 {
    // `max` returns the other operand when one is NaN.
    (outer - inset).max(MIN_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtracts_the_inset() {
        assert_eq!(concentric_radius(24.0, 14.0), 10.0);
        assert_eq!(concentric_radius(16.0, 8.0), 8.0);
    }

    #[test]
    fn zero_inset_keeps_the_outer_radius() {
        assert_eq!(concentric_radius(16.0, 0.0), 16.0);
    }

    #[test]
    fn clamps_at_the_minimum() {
        assert_eq!(concentric_radius(16.0, 14.0), MIN_RADIUS);
        assert_eq!(concentric_radius(16.0, 12.0), MIN_RADIUS);
        assert_eq!(concentric_radius(8.0, 20.0), MIN_RADIUS);
        assert_eq!(concentric_radius(0.0, 0.0), MIN_RADIUS);
    }

    #[test]
    fn nan_gives_the_minimum() {
        assert_eq!(concentric_radius(f64::NAN, 4.0), MIN_RADIUS);
        assert_eq!(concentric_radius(16.0, f64::NAN), MIN_RADIUS);
    }

    #[test]
    fn usable_in_const_context() {
        const R: f64 = concentric_radius(20.0, 6.0);
        assert_eq!(R, 14.0);
    }
}
