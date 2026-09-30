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
