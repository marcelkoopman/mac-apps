pub use mac_ui::theme::Appearance as Theme;
pub use mac_ui::theme::apply;

#[cfg_attr(test, allow(dead_code))]
const DEFAULTS_KEY: &str = "CopycraftAppearance";

/// Stored appearance, or [`Theme::System`] when none is stored.
/// Tests never read the real user defaults.
pub fn load() -> Theme {
    #[cfg(not(test))]
    {
        mac_ui::theme::load(DEFAULTS_KEY).unwrap_or(Theme::System)
    }
    #[cfg(test)]
    {
        Theme::System
    }
}

/// Store the appearance and apply it.
/// Tests never write the real user defaults.
pub fn save(theme: Theme) {
    #[cfg(not(test))]
    mac_ui::theme::store(DEFAULTS_KEY, theme);
    apply(theme);
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn theme_id(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "theme_system",
        Theme::Light => "theme_light",
        Theme::Dark => "theme_dark",
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn theme_from_id(id: &str) -> Option<Theme> {
    match id {
        "theme_system" => Some(Theme::System),
        "theme_light" => Some(Theme::Light),
        "theme_dark" => Some(Theme::Dark),
        _ => None,
    }
}

/// What Display asks the app to soften.
#[cfg(target_os = "macos")]
pub struct Display {
    /// "Verminder transparantie" / Reduce transparency.
    pub reduce_transparency: bool,
    /// "Verhoog contrast" / Increase contrast.
    pub increase_contrast: bool,
    /// "Verminder beweging" / Reduce motion.
    pub reduce_motion: bool,
}

/// The three display settings, read from the workspace. Off the Mac they stay off.
#[cfg(target_os = "macos")]
pub fn display() -> Display {
    use mac_ui::objc2_app_kit::NSWorkspace;
    let workspace = NSWorkspace::sharedWorkspace();
    Display {
        reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
        reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
    }
}

/// How windows are drawn for the current display settings ([`mac_ui::style::Chrome`]).
#[cfg(target_os = "macos")]
pub fn chrome() -> mac_ui::style::Chrome {
    let display = display();
    mac_ui::style::Chrome::for_display(
        display.reduce_transparency,
        display.increase_contrast,
        mac_ui::glass::is_available(),
    )
}

/// Fill and border for a content well.
///
/// The fill is the system text background, slightly see-through so the glass behind it
/// shows. Reduce Transparency and Increase Contrast make it opaque. Increase Contrast
/// draws a stronger border. `force_opaque` is for a cover that has to hide what is under it.
#[cfg(target_os = "macos")]
pub fn apply_well(well: &mac_ui::objc2_app_kit::NSBox, force_opaque: bool, border: bool) {
    use mac_ui::objc2_app_kit::NSColor;
    let chrome = chrome();
    let fill = NSColor::textBackgroundColor();
    let fill = if force_opaque || chrome.opaque_wells {
        fill
    } else {
        fill.colorWithAlphaComponent(0.72)
    };
    well.setFillColor(&fill);
    if !border {
        well.setBorderWidth(0.0);
        return;
    }
    if chrome.strong_borders {
        well.setBorderColor(&NSColor::labelColor());
        well.setBorderWidth(2.0);
    } else {
        well.setBorderColor(&NSColor::separatorColor());
        well.setBorderWidth(1.0);
    }
}

/// An opaque sRGB colour from a card theme palette.
#[cfg(target_os = "macos")]
pub fn srgb(
    rgb: crate::card_theme::Rgb,
) -> mac_ui::objc2::rc::Retained<mac_ui::objc2_app_kit::NSColor> {
    let (r, g, b) = rgb.unit();
    mac_ui::objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0)
}

/// The palette of the stored card theme; `None` for System.
#[cfg(target_os = "macos")]
pub fn card_palette() -> Option<crate::card_theme::Palette> {
    crate::settings::card_theme().palette()
}

/// [`apply_well`] for the card's own well. A fixed card theme fills it opaque in the theme's
/// background, whatever Reduce Transparency says, so the tested contrast is what is on screen;
/// Increase Contrast still gets the strong border (in the theme's text colour). System keeps
/// the see-through glass well.
#[cfg(target_os = "macos")]
pub fn apply_card_well(well: &mac_ui::objc2_app_kit::NSBox) {
    apply_well(well, false, true);
    let Some(palette) = card_palette() else {
        return;
    };
    well.setFillColor(&srgb(palette.background));
    if chrome().strong_borders {
        well.setBorderColor(&srgb(palette.text));
    }
}

/// The opaque cover over a masked well without blur (the default): the system text background,
/// or the card theme's background, so a masked card keeps its theme.
#[cfg(target_os = "macos")]
pub fn apply_card_shade(shade: &mac_ui::objc2_app_kit::NSBox) {
    apply_well(shade, true, false);
    if let Some(palette) = card_palette() {
        shade.setFillColor(&srgb(palette.background));
    }
}

/// Caption colour inside the card's well: the theme's secondary text (its text under Increase
/// Contrast), or [`caption_color`] without a fixed theme.
#[cfg(target_os = "macos")]
pub fn card_caption_color(
    palette: Option<crate::card_theme::Palette>,
) -> mac_ui::objc2::rc::Retained<mac_ui::objc2_app_kit::NSColor> {
    match palette {
        Some(p) if chrome().primary_captions => srgb(p.text),
        Some(p) => srgb(p.secondary),
        None => caption_color(),
    }
}

/// The AppKit appearance a fixed card theme puts on the card window (light or dark controls,
/// glass and system selection around its well). `None` for System: the window inherits the
/// app's appearance, so it follows macOS live.
///
/// The names are the documented string values of `NSAppearanceNameAqua` and
/// `NSAppearanceNameDarkAqua`; building them from a string keeps this free of `unsafe` (the
/// extern constants are `unsafe` to read). An unknown name gives `None`: the window then simply
/// inherits.
#[cfg(target_os = "macos")]
pub fn card_window_appearance()
-> Option<mac_ui::objc2::rc::Retained<mac_ui::objc2_app_kit::NSAppearance>> {
    let palette = card_palette()?;
    let name = if palette.dark {
        "NSAppearanceNameDarkAqua"
    } else {
        "NSAppearanceNameAqua"
    };
    mac_ui::objc2_app_kit::NSAppearance::appearanceNamed(
        &mac_ui::objc2_foundation::NSString::from_str(name),
    )
}

/// Caption colour. Increase Contrast promotes it to the primary label.
#[cfg(target_os = "macos")]
pub fn caption_color() -> mac_ui::objc2::rc::Retained<mac_ui::objc2_app_kit::NSColor> {
    use mac_ui::objc2_app_kit::NSColor;
    if chrome().primary_captions {
        NSColor::labelColor()
    } else {
        NSColor::secondaryLabelColor()
    }
}

/// How long a short fade should take. Zero when Reduce Motion is on.
#[cfg(target_os = "macos")]
pub fn motion_secs() -> f64 {
    if display().reduce_motion {
        0.0
    } else {
        mac_ui::metrics::MOTION_SECS
    }
}

/// Reduce Transparency or Increase Contrast changed: restyle the backdrops, wells and captions
/// of every open window. The welcome window is always opaque navy and needs nothing.
#[cfg(target_os = "macos")]
pub fn restyle_open_windows() {
    crate::macos_launcher::apply_window_chrome();
    crate::macos_settings::restyle();
    crate::macos_about::restyle();
    crate::macos_table_window::restyle();
    if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
        mac_ui::toast::restyle(mtm);
    }
}

#[cfg(test)]
mod tests {
    use super::{Theme, theme_from_id, theme_id};

    #[test]
    fn ids_roundtrip() {
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            assert_eq!(theme_from_id(theme_id(theme)), Some(theme));
        }
        assert_eq!(theme_from_id("nope"), None);
    }
}
