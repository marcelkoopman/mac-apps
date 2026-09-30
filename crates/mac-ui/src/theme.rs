//! App appearance (System / Light / Dark): apply it and persist it in the user defaults.

/// Appearance the app is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Appearance {
    /// Follow the system setting.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    /// Value stored in the user defaults: `"system"`, `"light"` or `"dark"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Inverse of [`Appearance::as_str`].
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// Appearance stored under `defaults_key` in the standard user defaults, if any.
pub fn load(defaults_key: &str) -> Option<Appearance> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(defaults_key);
        let value = defaults.stringForKey(&key)?;
        Appearance::parse(value.to_string().as_str())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = defaults_key;
        None
    }
}

/// Store `appearance` under `defaults_key` in the standard user defaults.
pub fn store(defaults_key: &str, appearance: Appearance) {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSUserDefaults};
        let defaults = NSUserDefaults::standardUserDefaults();
        let key = NSString::from_str(defaults_key);
        // SAFETY: an NSString is a valid property-list value for the user defaults.
        unsafe {
            defaults.setObject_forKey(Some(&NSString::from_str(appearance.as_str())), &key);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (defaults_key, appearance);
    }
}

/// Set the app-wide appearance. Does nothing off the main thread.
pub fn apply(appearance: Appearance) {
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
        match appearance {
            Appearance::System => app.setAppearance(None),
            Appearance::Light => {
                // SAFETY: NSAppearanceNameAqua is a framework constant.
                let named = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameAqua) };
                app.setAppearance(named.as_deref());
            }
            Appearance::Dark => {
                // SAFETY: NSAppearanceNameDarkAqua is a framework constant.
                let named = unsafe { NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua) };
                app.setAppearance(named.as_deref());
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = appearance;
    }
}

#[cfg(test)]
mod tests {
    use super::Appearance;

    #[test]
    fn stored_values_roundtrip() {
        for appearance in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(Appearance::parse(appearance.as_str()), Some(appearance));
        }
        assert_eq!(Appearance::parse("nope"), None);
    }

    #[test]
    fn default_follows_the_system() {
        assert_eq!(Appearance::default(), Appearance::System);
    }
}
