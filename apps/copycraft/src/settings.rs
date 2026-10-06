//! Copycraft's own settings in the standard user defaults. None of them holds clipboard data.

use crate::dataframe::ReadOptions;
use crate::hotkey;

/// Settings the Settings window and the `⋯` menu change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Empty the pasteboard a minute after copycraft wrote a copy labelled sensitive, unless
    /// something else was copied meanwhile. On by default.
    pub clear_sensitive: bool,
    /// Forget history this many minutes after the last copy (one of [`HISTORY_MINUTES`]); 0
    /// keeps it until it is cleared, the screen locks, the Mac sleeps or the user switches.
    pub history_minutes: u32,
    /// Gaussian blur over a masked well. Off falls back to the opaque shade. On by default.
    pub blur: bool,
    /// Prefer `mm/dd/yyyy` when a date column fits both orders. Off (dd/mm) by default.
    pub date_month_first: bool,
    /// Global hotkey as a [`global_hotkey::hotkey::HotKey`] string (`control+alt+super+KeyC`).
    /// `None` means the default ([`hotkey::open`]).
    pub hotkey: Option<String>,
}

/// The choices in the `⋯` menu, in menu order. 0: no time limit.
pub const HISTORY_MINUTES: [u32; 4] = [5, 15, 60, 0];

impl Default for Settings {
    fn default() -> Self {
        Self {
            clear_sensitive: true,
            history_minutes: 15,
            blur: true,
            date_month_first: false,
            hotkey: None,
        }
    }
}

const CLEAR_SENSITIVE_KEY: &str = "CopycraftClearSensitiveCopies";
const HISTORY_MINUTES_KEY: &str = "CopycraftHistoryMinutes";
const BLUR_KEY: &str = "CopycraftBlurMaskedWell";
const DATE_MONTH_FIRST_KEY: &str = "CopycraftDateMonthFirst";
const HOTKEY_KEY: &str = "CopycraftHotkey";
/// The card has explained the pasteboard privacy alert once (Default or Ask).
const PASTE_ALERT_EXPLAINED_KEY: &str = "CopycraftPasteAlertExplained";
/// Newest symbol first, at most [`crate::symbols::MAX_RECENT`], separated by spaces.
const SYMBOL_RECENT_KEY: &str = "CopycraftSymbolRecent";
/// The symbol list, space-separated. Missing, empty or invalid means the standard list.
const SYMBOLS_KEY: &str = "CopycraftSymbols";

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
    if let Some(value) = load_bool(BLUR_KEY) {
        settings.blur = value;
    }
    if let Some(value) = load_bool(DATE_MONTH_FIRST_KEY) {
        settings.date_month_first = value;
    }
    if let Some(text) = load_string(HOTKEY_KEY).filter(|text| !text.is_empty()) {
        settings.hotkey = Some(text);
    }
    settings
}

/// How a new table is read: the stored date-order preference.
pub fn preferred_read_options() -> ReadOptions {
    ReadOptions {
        month_first: load().date_month_first,
    }
}

/// A fresh [`crate::table::TableVersions`] that starts with the preferred date order.
pub fn preferred_table() -> crate::table::TableVersions {
    let mut table = crate::table::TableVersions::default();
    table.set_options(preferred_read_options());
    table
}

pub fn set_clear_sensitive(on: bool) {
    store_bool(CLEAR_SENSITIVE_KEY, on);
}

pub fn set_history_minutes(minutes: u32) {
    store_int(HISTORY_MINUTES_KEY, i64::from(minutes));
}

pub fn set_blur(on: bool) {
    store_bool(BLUR_KEY, on);
}

pub fn set_date_month_first(on: bool) {
    store_bool(DATE_MONTH_FIRST_KEY, on);
}

/// Store `chord` when it is safe ([`hotkey::is_safe`]); otherwise keep the previous value.
pub fn set_hotkey(chord: &global_hotkey::hotkey::HotKey) -> bool {
    if !hotkey::is_safe(chord) {
        return false;
    }
    store_string(HOTKEY_KEY, &hotkey::to_storage(chord));
    true
}

/// The stored chord, or the default.
pub fn hotkey() -> global_hotkey::hotkey::HotKey {
    hotkey::from_storage(load().hotkey.as_deref())
}

/// Compact label of the stored chord (`⌃⌥⌘C`).
pub fn hotkey_label() -> String {
    hotkey::label(&hotkey())
}

/// Whether the card has shown the one-time explanation of the pasteboard privacy alert.
pub fn paste_alert_explained() -> bool {
    load_bool(PASTE_ALERT_EXPLAINED_KEY).unwrap_or(false)
}

pub fn set_paste_alert_explained() {
    store_bool(PASTE_ALERT_EXPLAINED_KEY, true);
}

/// Recently copied symbols, newest first. Missing or blank means none yet.
pub fn symbol_recent() -> Vec<String> {
    load_string(SYMBOL_RECENT_KEY)
        .map(|raw| crate::symbols::parse_recent(&raw))
        .unwrap_or_default()
}

/// Remember `symbol` as the newest one (at most [`crate::symbols::MAX_RECENT`], no duplicate).
pub fn remember_symbol(symbol: &str) {
    let next = crate::symbols::note_used(&symbol_recent(), symbol);
    store_string(SYMBOL_RECENT_KEY, &crate::symbols::format_list(&next));
}

/// The symbol list. A missing, empty or invalid stored value is the standard list.
pub fn symbol_catalog() -> Vec<String> {
    load_string(SYMBOLS_KEY)
        .and_then(|raw| crate::symbols::parse_catalog(&raw))
        .unwrap_or_else(crate::symbols::default_list)
}

/// The list as one line, for the settings field.
pub fn symbol_catalog_text() -> String {
    crate::symbols::format_list(&symbol_catalog())
}

/// Store `raw`. Empty or invalid input is stored as the standard list.
pub fn set_symbol_catalog(raw: &str) {
    let list = crate::symbols::parse_catalog(raw).unwrap_or_else(crate::symbols::default_list);
    store_string(SYMBOLS_KEY, &crate::symbols::format_list(&list));
}

/// Put the standard list back.
pub fn restore_symbol_catalog() {
    store_string(
        SYMBOLS_KEY,
        &crate::symbols::format_list(&crate::symbols::default_list()),
    );
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

#[cfg(target_os = "macos")]
fn load_string(key: &str) -> Option<String> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    let value = defaults.stringForKey(&key)?;
    Some(value.to_string())
}

#[cfg(target_os = "macos")]
fn store_string(key: &str, value: &str) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    // SAFETY: NSString is a valid NSObject for the defaults dictionary.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&*NSString::from_str(value)), &NSString::from_str(key));
    }
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

#[cfg(not(target_os = "macos"))]
fn load_string(_key: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
fn store_string(_key: &str, _value: &str) {}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn defaults_match_the_previous_behaviour() {
        let settings = Settings::default();
        assert!(settings.clear_sensitive);
        assert_eq!(settings.history_minutes, 15);
        assert!(settings.blur);
        assert!(!settings.date_month_first);
        assert!(settings.hotkey.is_none());
        assert!(super::HISTORY_MINUTES.contains(&settings.history_minutes));
    }

    #[test]
    fn preferred_read_options_follow_the_date_setting() {
        assert!(!super::preferred_read_options().month_first);
    }
}
