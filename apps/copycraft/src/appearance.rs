#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

const DEFAULTS_KEY: &str = "CopycraftAppearance";

impl Theme {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn as_id(self) -> &'static str {
        match self {
            Self::System => "theme_system",
            Self::Light => "theme_light",
            Self::Dark => "theme_dark",
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "theme_system" => Some(Self::System),
            "theme_light" => Some(Self::Light),
            "theme_dark" => Some(Self::Dark),
            _ => None,
        }
    }

    pub fn load() -> Self {
        load_stored().unwrap_or(Self::System)
    }

    pub fn save(self) {
        store(self);
        apply(self);
    }
}

fn load_stored() -> Option<Theme> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(DEFAULTS_KEY);
        let value = defaults.stringForKey(&key)?;
        match value.to_string().as_str() {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            "system" => Some(Theme::System),
            _ => None,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn store(theme: Theme) {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(DEFAULTS_KEY);
        let value = match theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        unsafe {
            defaults.setObject_forKey(Some(&NSString::from_str(value)), &key);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = theme;
    }
}

pub fn apply(theme: Theme) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{
            NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
        };
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        match theme {
            Theme::System => app.setAppearance(None),
            Theme::Light => {
                let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) };
                app.setAppearance(appearance.as_deref());
            }
            Theme::Dark => {
                let appearance = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) };
                app.setAppearance(appearance.as_deref());
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = theme;
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn ids_roundtrip() {
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            assert_eq!(Theme::from_id(theme.as_id()), Some(theme));
        }
        assert_eq!(Theme::from_id("nope"), None);
    }
}
