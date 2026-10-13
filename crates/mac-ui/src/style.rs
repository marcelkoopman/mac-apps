//! One look for every window: spacing, corners, type sizes, animation timings, the accent
//! colour, and the rules for Reduce Motion and Reduce Transparency. Platform-independent, so
//! the rules are tested everywhere; the AppKit side lives in `glass` and `motion`.

/// The spacing scale, in points.
pub const SPACE_XS: f64 = 4.0;
pub const SPACE_S: f64 = 8.0;
pub const SPACE_M: f64 = 12.0;
pub const SPACE_L: f64 = 16.0;
pub const SPACE_XL: f64 = 24.0;
/// Every step of the scale, smallest first.
pub const SPACING: [f64; 5] = [SPACE_XS, SPACE_S, SPACE_M, SPACE_L, SPACE_XL];

/// Corner radius of a panel, a well, and a pill (half its height).
pub const RADIUS_PANEL: f64 = SPACE_L;
pub const RADIUS_WELL: f64 = SPACE_S;
pub const RADIUS_TOAST: f64 = SPACE_M;

/// Type sizes: title and body share one size, the caption is smaller; code uses SF Mono.
pub const FONT_TITLE: f64 = 13.0;
pub const FONT_BODY: f64 = 13.0;
pub const FONT_CAPTION: f64 = 12.0;
pub const FONT_CODE: f64 = 12.0;

/// Animation timings, in seconds.
pub const CARD_IN_SECS: f64 = 0.15;
pub const CARD_OUT_SECS: f64 = 0.10;
pub const CROSSFADE_SECS: f64 = 0.12;
pub const TOAST_FADE_SECS: f64 = 0.15;
/// How long the toast stays before it fades out.
pub const TOAST_HOLD_SECS: f64 = 1.2;

/// The one accent colour: the navy of the app icon (sRGB, 0 to 1). Everything else uses
/// system colours.
pub const ACCENT_RGB: (f64, f64, f64) = (20.0 / 255.0, 30.0 / 255.0, 60.0 / 255.0);

/// How an animation runs under the user's Reduce Motion setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// Fade duration; 0 is instant.
    pub fade_secs: f64,
    /// The scale the movement starts from; 1 is no movement.
    pub scale_from: f64,
}

impl Motion {
    /// The card appearing: a fade only. Growing the window frame made the card jump, so
    /// nothing moves, with or without Reduce Motion.
    pub fn card_in(_reduce_motion: bool) -> Self {
        Self {
            fade_secs: CARD_IN_SECS,
            scale_from: 1.0,
        }
    }

    /// The card going away: a short fade, never movement.
    pub fn card_out(_reduce_motion: bool) -> Self {
        Self {
            fade_secs: CARD_OUT_SECS,
            scale_from: 1.0,
        }
    }

    /// New page or new filter results: a crossfade, which Reduce Motion allows.
    pub fn crossfade(_reduce_motion: bool) -> Self {
        Self {
            fade_secs: CROSSFADE_SECS,
            scale_from: 1.0,
        }
    }

    pub fn moves(self) -> bool {
        (self.scale_from - 1.0).abs() > f64::EPSILON
    }
}

/// What sits behind a window's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// Liquid Glass (`NSGlassEffectView`, macOS 26+).
    Glass,
    /// Vibrancy (`NSVisualEffectView`) on older macOS.
    Vibrancy,
    /// An opaque system background: Reduce Transparency is on.
    Solid,
}

impl Backdrop {
    pub fn choose(reduce_transparency: bool, glass_available: bool) -> Self {
        match (reduce_transparency, glass_available) {
            (true, _) => Backdrop::Solid,
            (false, true) => Backdrop::Glass,
            (false, false) => Backdrop::Vibrancy,
        }
    }
}

/// At most this many chips show on a card; the rest go under the `…` menu.
pub const MAX_CARD_CHIPS: usize = 4;

/// How a window is drawn for the display settings: its backdrop, whether content wells are
/// opaque and strongly bordered, and whether captions use the primary label colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    pub backdrop: Backdrop,
    /// Wells drop their see-through fill (Reduce Transparency or Increase Contrast).
    pub opaque_wells: bool,
    /// Wells get a 2 pt label-coloured border (Increase Contrast).
    pub strong_borders: bool,
    /// Captions use the primary label colour instead of the secondary one (Increase Contrast).
    pub primary_captions: bool,
}

impl Chrome {
    pub fn for_display(reduce_transparency: bool, increase_contrast: bool, glass: bool) -> Self {
        Self {
            backdrop: Backdrop::choose(reduce_transparency, glass),
            opaque_wells: reduce_transparency || increase_contrast,
            strong_borders: increase_contrast,
            primary_captions: increase_contrast,
        }
    }
}

/// A frame scaled by `scale` about its centre: (x, y, width, height).
pub fn scaled_about_centre(frame: (f64, f64, f64, f64), scale: f64) -> (f64, f64, f64, f64) {
    let (x, y, w, h) = frame;
    let (sw, sh) = (w * scale, h * scale);
    (x + (w - sw) / 2.0, y + (h - sh) / 2.0, sw, sh)
}

/// WCAG relative luminance of an sRGB colour (0 to 1 per channel).
pub fn luminance((r, g, b): (f64, f64, f64)) -> f64 {
    let lin = |c: f64| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two colours (1 to 21).
pub fn contrast(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG AA for body text.
pub const AA_BODY: f64 = 4.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_is_one_ascending_scale_on_a_four_point_grid() {
        assert_eq!(SPACING, [4.0, 8.0, 12.0, 16.0, 24.0]);
        assert!(SPACING.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(SPACING.iter().all(|step| step % 4.0 == 0.0));
        const { assert!(RADIUS_WELL < RADIUS_PANEL) };
    }

    #[test]
    fn timings_are_short_and_ordered() {
        assert_eq!(CARD_IN_SECS, 0.15);
        assert_eq!(CARD_OUT_SECS, 0.10);
        assert_eq!(CROSSFADE_SECS, 0.12);
        assert_eq!(TOAST_HOLD_SECS, 1.2);
        const { assert!(CARD_OUT_SECS < CARD_IN_SECS) };
        assert!(
            [CARD_IN_SECS, CARD_OUT_SECS, CROSSFADE_SECS, TOAST_FADE_SECS]
                .iter()
                .all(|secs| *secs > 0.0 && *secs <= 0.2)
        );
    }

    #[test]
    fn reduce_motion_keeps_the_fade_and_drops_the_movement() {
        let normal = Motion::card_in(false);
        assert!(!normal.moves(), "the card never moves or grows");
        assert!(normal.fade_secs > 0.0);
        let reduced = Motion::card_in(true);
        assert!(!reduced.moves());
        assert!(reduced.fade_secs > 0.0);
        for reduce in [false, true] {
            assert!(!Motion::card_out(reduce).moves());
            assert!(!Motion::crossfade(reduce).moves());
        }
    }

    #[test]
    fn reduce_transparency_is_always_solid() {
        assert_eq!(Backdrop::choose(true, true), Backdrop::Solid);
        assert_eq!(Backdrop::choose(true, false), Backdrop::Solid);
        assert_eq!(Backdrop::choose(false, true), Backdrop::Glass);
        assert_eq!(Backdrop::choose(false, false), Backdrop::Vibrancy);
    }

    #[test]
    fn chrome_for_every_display_combination() {
        for reduce in [false, true] {
            for contrast in [false, true] {
                for glass in [false, true] {
                    let chrome = Chrome::for_display(reduce, contrast, glass);
                    let backdrop = match (reduce, glass) {
                        (true, _) => Backdrop::Solid,
                        (false, true) => Backdrop::Glass,
                        (false, false) => Backdrop::Vibrancy,
                    };
                    assert_eq!(chrome.backdrop, backdrop, "{reduce} {contrast} {glass}");
                    assert_eq!(chrome.opaque_wells, reduce || contrast);
                    assert_eq!(chrome.strong_borders, contrast);
                    assert_eq!(chrome.primary_captions, contrast);
                }
            }
        }
        assert_eq!(MAX_CARD_CHIPS, 4);
    }

    #[test]
    fn scaling_keeps_the_centre() {
        let (x, y, w, h) = scaled_about_centre((100.0, 50.0, 400.0, 200.0), 0.96);
        assert!((x + w / 2.0 - 300.0).abs() < 1e-9);
        assert!((y + h / 2.0 - 150.0).abs() < 1e-9);
        assert!((w - 384.0).abs() < 1e-9);
    }

    #[test]
    fn text_on_solid_and_accent_backgrounds_meets_wcag_aa() {
        let white = (1.0, 1.0, 1.0);
        let black = (0.0, 0.0, 0.0);
        // Label colour on the solid window backgrounds (light, dark) and on the navy tint.
        let light_bg = (0.925, 0.925, 0.925);
        let dark_bg = (0.196, 0.196, 0.196);
        assert!(contrast(black, light_bg) >= AA_BODY);
        assert!(contrast(white, dark_bg) >= AA_BODY);
        assert!(contrast(white, ACCENT_RGB) >= AA_BODY);
        assert!((contrast(white, black) - 21.0).abs() < 1e-9);
    }
}
