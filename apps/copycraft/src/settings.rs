//! Copycraft's own settings in the standard user defaults. None of them holds clipboard data.

/// Settings the `⋯` menu changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Empty the pasteboard a minute after copycraft wrote a copy labelled sensitive, unless
    /// something else was copied meanwhile. On by default.
    pub clear_sensitive: bool,
    /// Forget history this many minutes after the last copy (one of [`HISTORY_MINUTES`]); 0
    /// keeps it until it is cleared, the screen locks, the Mac sleeps or the user switches.
    pub history_minutes: u32,
}

/// The choices in the `⋯` menu, in menu order. 0: no time limit.
pub const HISTORY_MINUTES: [u32; 4] = [5, 15, 60, 0];

impl Default for Settings {
    fn default() -> Self {
        Self {
            clear_sensitive: true,
            history_minutes: 15,
        }
    }
}

const CLEAR_SENSITIVE_KEY: &str = "CopycraftClearSensitiveCopies";
const HISTORY_MINUTES_KEY: &str = "CopycraftHistoryMinutes";

/// Stored settings; a missing key keeps its default.
pub fn load() -> Settings {
    let mut settings = Settings::default();
    if let Some(value) = load_bool(CLEAR_SENSITIVE_KEY) {
        settings.clear_sensitive = value;
    }
    if let Some(minutes) = load_int(HISTORY_MINUTES_KEY)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|minutes| HISTORY_MINUTES.contains(minutes))
    {
        settings.history_minutes = minutes;
    }
    settings
}

pub fn set_clear_sensitive(on: bool) {
    store_bool(CLEAR_SENSITIVE_KEY, on);
}

pub fn set_history_minutes(minutes: u32) {
    store_int(HISTORY_MINUTES_KEY, i64::from(minutes));
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

#[cfg(target_os = "macos")]
fn load_int(key: &str) -> Option<i64> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    defaults.objectForKey(&key)?;
    Some(defaults.integerForKey(&key) as i64)
}

#[cfg(target_os = "macos")]
fn store_int(key: &str, value: i64) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults()
        .setInteger_forKey(value as isize, &NSString::from_str(key));
}

#[cfg(not(target_os = "macos"))]
fn load_bool(_key: &str) -> Option<bool> {
    None
}

#[cfg(not(target_os = "macos"))]
fn store_bool(_key: &str, _value: bool) {}

#[cfg(not(target_os = "macos"))]
fn load_int(_key: &str) -> Option<i64> {
    None
}

#[cfg(not(target_os = "macos"))]
fn store_int(_key: &str, _value: i64) {}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn sensitive_copies_are_cleared_by_default() {
        assert!(Settings::default().clear_sensitive);
    }

    #[test]
    fn history_is_kept_fifteen_minutes_by_default() {
        assert_eq!(Settings::default().history_minutes, 15);
        assert!(super::HISTORY_MINUTES.contains(&Settings::default().history_minutes));
    }
}
