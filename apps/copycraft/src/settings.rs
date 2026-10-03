//! Copycraft's own settings in the standard user defaults. None of them holds clipboard data.

/// Settings the `⋯` menu changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Empty the pasteboard a minute after copycraft wrote a copy labelled sensitive, unless
    /// something else was copied meanwhile. On by default.
    pub clear_sensitive: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clear_sensitive: true,
        }
    }
}

const CLEAR_SENSITIVE_KEY: &str = "CopycraftClearSensitiveCopies";

/// Stored settings; a missing key keeps its default.
pub fn load() -> Settings {
    let mut settings = Settings::default();
    if let Some(value) = load_bool(CLEAR_SENSITIVE_KEY) {
        settings.clear_sensitive = value;
    }
    settings
}

pub fn set_clear_sensitive(on: bool) {
    store_bool(CLEAR_SENSITIVE_KEY, on);
}

#[cfg(target_os = "macos")]
fn load_bool(key: &str) -> Option<bool> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    defaults.objectForKey(&key)?;
    Some(defaults.boolForKey(&key))
}

#[cfg(target_os = "macos")]
fn store_bool(key: &str, value: bool) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults().setBool_forKey(value, &NSString::from_str(key));
}

#[cfg(not(target_os = "macos"))]
fn load_bool(_key: &str) -> Option<bool> {
    None
}

#[cfg(not(target_os = "macos"))]
fn store_bool(_key: &str, _value: bool) {}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn sensitive_copies_are_cleared_by_default() {
        assert!(Settings::default().clear_sensitive);
    }
}
