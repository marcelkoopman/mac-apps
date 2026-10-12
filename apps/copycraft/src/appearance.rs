pub use mac_ui::theme::Appearance as Theme;
pub use mac_ui::theme::apply;

const DEFAULTS_KEY: &str = "CopycraftAppearance";

/// Stored appearance, or [`Theme::System`] when none is stored.
pub fn load() -> Theme {
    mac_ui::theme::load(DEFAULTS_KEY).unwrap_or(Theme::System)
}

/// Store the appearance and apply it.
pub fn save(theme: Theme) {
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

/// Fill and border for a content well.
///
/// The fill is the system text background, slightly see-through so the glass behind it
/// shows. Reduce Transparency and Increase Contrast make it opaque. Increase Contrast
/// draws a stronger border. `force_opaque` is for a cover that has to hide what is under it.
#[cfg(target_os = "macos")]
pub fn apply_well(well: &mac_ui::objc2_app_kit::NSBox, force_opaque: bool, border: bool) {
    use mac_ui::objc2_app_kit::NSColor;
    let display = display();
    let fill = NSColor::textBackgroundColor();
    let fill = if force_opaque || display.reduce_transparency || display.increase_contrast {
        fill
    } else {
        fill.colorWithAlphaComponent(0.72)
    };
    well.setFillColor(&fill);
    if !border {
        well.setBorderWidth(0.0);
        return;
    }
    if display.increase_contrast {
        well.setBorderColor(&NSColor::labelColor());
        well.setBorderWidth(2.0);
    } else {
        well.setBorderColor(&NSColor::separatorColor());
        well.setBorderWidth(1.0);
    }
}

/// Caption colour. Increase Contrast promotes it to the primary label.
#[cfg(target_os = "macos")]
pub fn caption_color() -> mac_ui::objc2::rc::Retained<mac_ui::objc2_app_kit::NSColor> {
    use mac_ui::objc2_app_kit::NSColor;
    if display().increase_contrast {
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
