//! Copycraft's own settings in the standard user defaults. None of them holds clipboard data.

use crate::dataframe::ReadOptions;
use crate::hotkey;

/// Which language the interface uses. [`Language::System`] follows the Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// The macOS preferred language (English when that is not Dutch).
    #[default]
    System,
    Nl,
    En,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Nl => "nl",
            Self::En => "en",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "system" => Some(Self::System),
            "nl" => Some(Self::Nl),
            "en" => Some(Self::En),
            _ => None,
        }
    }
}

/// Settings the Settings window and the `⋯` menu change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Empty the pasteboard a minute after copycraft wrote a copy labelled sensitive, unless
    /// something else was copied meanwhile. On by default.
    pub clear_sensitive: bool,
    /// Forget history this many minutes after the last copy (one of [`HISTORY_MINUTES`]); 0
    /// keeps it until it is cleared, the screen locks, the Mac sleeps or the user switches.
    pub history_minutes: u32,
    /// The privacy filter: copied content stays masked (blurred or blank) until it is clicked.
    /// Off shows content directly, unmasked. On by default.
    pub privacy_filter: bool,
    /// Gaussian blur over a masked well. Off falls back to the opaque shade. On by default.
    pub blur: bool,
    /// Prefer `mm/dd/yyyy` when a date column fits both orders. Off (dd/mm) by default.
    pub date_month_first: bool,
    /// Global hotkey as a [`global_hotkey::hotkey::HotKey`] string (`control+meta+KeyC`).
    /// `None` means the default ([`hotkey::open`]).
    pub hotkey: Option<String>,
    /// Interface language. [`Language::System`] until a choice is stored.
    pub language: Language,
    /// "Show all" on a long text shows it one page at a time, like `more`. On by default.
    pub page_long_text: bool,
    /// VoiceOver says confirmations such as the "Copied" toast. Off keeps the toast itself.
    /// On by default.
    pub announce_confirmations: bool,
    /// The card's Wis/Clear button asks first. "Don't ask again" turns it off; Settings turns
    /// it back on. On by default.
    pub confirm_clear: bool,
    /// Width of the card under the menu bar icon. Only a size is stored, never content.
    pub window_size: WindowSize,
    /// Notification when an AI action finished while the card is hidden or the app is in the
    /// background. On by default. See `notifications.rs`.
    pub notify_ai: bool,
    /// Notification when a PDF's text recognition took a while and the card is hidden. On.
    pub notify_ocr: bool,
    /// Notification for an error the user must know (pin list full, AI unavailable), once per
    /// kind per session. On by default.
    pub notify_errors: bool,
    /// Notification "Possibly sensitive text copied" (no content). On by default.
    pub notify_secret: bool,
}

/// How wide the card is: a choice in the `⋯` menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowSize {
    Compact,
    Normal,
    Wide,
}

impl WindowSize {
    pub const ALL: [WindowSize; 3] = [Self::Compact, Self::Normal, Self::Wide];

    /// The card's width in points, before it is cut to the screen ([`Self::width_on`]).
    pub fn width(self) -> f64 {
        match self {
            Self::Compact => 440.0,
            Self::Normal => 640.0,
            Self::Wide => 800.0,
        }
    }

    /// The width on a screen whose usable width is `visible`: never more than 60% of it, and
    /// never less than the compact width (a small screen keeps the old card).
    pub fn width_on(self, visible: f64) -> f64 {
        let compact = Self::Compact.width();
        self.width().min((visible * 0.6).max(compact)).max(compact)
    }

    fn id(self) -> i64 {
        match self {
            Self::Compact => 0,
            Self::Normal => 1,
            Self::Wide => 2,
        }
    }

    pub fn from_id(id: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.id() == id)
    }
}

/// The choices in the `⋯` menu, in menu order. 0: no time limit.
pub const HISTORY_MINUTES: [u32; 4] = [5, 15, 60, 0];

impl Default for Settings {
    fn default() -> Self {
        Self {
            clear_sensitive: true,
            history_minutes: 15,
            privacy_filter: true,
            blur: true,
            date_month_first: false,
            hotkey: None,
            language: Language::System,
            page_long_text: true,
            announce_confirmations: true,
            confirm_clear: true,
            window_size: WindowSize::Normal,
            notify_ai: true,
            notify_ocr: true,
            notify_errors: true,
            notify_secret: true,
        }
    }
}

const CLEAR_SENSITIVE_KEY: &str = "CopycraftClearSensitiveCopies";
const HISTORY_MINUTES_KEY: &str = "CopycraftHistoryMinutes";
const PRIVACY_FILTER_KEY: &str = "CopycraftPrivacyFilter";
const BLUR_KEY: &str = "CopycraftBlurMaskedWell";
const DATE_MONTH_FIRST_KEY: &str = "CopycraftDateMonthFirst";
const HOTKEY_KEY: &str = "CopycraftHotkey";
const LANGUAGE_KEY: &str = "CopycraftLanguage";
const PAGE_LONG_TEXT_KEY: &str = "CopycraftPageLongText";
const ANNOUNCE_KEY: &str = "CopycraftAnnounceConfirmations";
const WINDOW_SIZE_KEY: &str = "CopycraftWindowSize";
const CONFIRM_CLEAR_KEY: &str = "CopycraftConfirmClear";
const NOTIFY_AI_KEY: &str = "CopycraftNotifyAi";
const NOTIFY_OCR_KEY: &str = "CopycraftNotifyOcr";
const NOTIFY_ERRORS_KEY: &str = "CopycraftNotifyErrors";
const NOTIFY_SECRET_KEY: &str = "CopycraftNotifySecret";
/// Whether notifications may be shown: [`crate::notifications::Permission`], as a number. Not
/// content.
const NOTIFY_PERMISSION_KEY: &str = "CopycraftNotifyPermission";
/// The card has explained the pasteboard privacy alert once (Default or Ask).
const PASTE_ALERT_EXPLAINED_KEY: &str = "CopycraftPasteAlertExplained";
/// Stored (true) once the welcome window closed; missing on first launch.
const WELCOME_KEY: &str = "hasSeenWelcome";
/// Newest symbol first, at most [`crate::symbols::MAX_RECENT`], separated by spaces.
const SYMBOL_RECENT_KEY: &str = "CopycraftSymbolRecent";
/// The symbol list, space-separated. Missing, empty or invalid means the standard list.
const SYMBOLS_KEY: &str = "CopycraftSymbols";
/// Bundle ids whose copies are never read. One id per line. Not clipboard data.
const IGNORED_APPS_KEY: &str = "CopycraftIgnoredApps";

/// How many bundle ids the ignore list keeps.
pub const IGNORED_APP_MAX: usize = 50;

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
    if let Some(value) = load_bool(PRIVACY_FILTER_KEY) {
        settings.privacy_filter = value;
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
    if let Some(language) = load_string(LANGUAGE_KEY).and_then(|text| Language::parse(&text)) {
        settings.language = language;
    }
    if let Some(value) = load_bool(PAGE_LONG_TEXT_KEY) {
        settings.page_long_text = value;
    }
    settings.announce_confirmations = announce_from(load_bool(ANNOUNCE_KEY));
    settings.confirm_clear = confirm_clear_from(load_bool(CONFIRM_CLEAR_KEY));
    if let Some(size) = load_int(WINDOW_SIZE_KEY).and_then(WindowSize::from_id) {
        settings.window_size = size;
    }
    if let Some(value) = load_bool(NOTIFY_AI_KEY) {
        settings.notify_ai = value;
    }
    if let Some(value) = load_bool(NOTIFY_OCR_KEY) {
        settings.notify_ocr = value;
    }
    if let Some(value) = load_bool(NOTIFY_ERRORS_KEY) {
        settings.notify_errors = value;
    }
    if let Some(value) = load_bool(NOTIFY_SECRET_KEY) {
        settings.notify_secret = value;
    }
    settings
}

/// The notification switches, from the stored values (a missing one is on).
pub fn notify_switches() -> crate::notifications::Switches {
    let settings = load();
    crate::notifications::Switches {
        ai: settings.notify_ai,
        ocr: settings.notify_ocr,
        errors: settings.notify_errors,
        secret: settings.notify_secret,
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_notify(switch: crate::notifications::Switch, on: bool) {
    use crate::notifications::Switch;
    let key = match switch {
        Switch::Ai => NOTIFY_AI_KEY,
        Switch::Ocr => NOTIFY_OCR_KEY,
        Switch::Errors => NOTIFY_ERRORS_KEY,
        Switch::Secret => NOTIFY_SECRET_KEY,
    };
    store_bool(key, on);
}

/// Whether notifications may be shown. [`crate::notifications::Permission::Unknown`] until the
/// first notification asked.
pub fn notify_permission() -> crate::notifications::Permission {
    crate::notifications::Permission::from_id(load_int(NOTIFY_PERMISSION_KEY).unwrap_or(0))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_notify_permission(permission: crate::notifications::Permission) {
    store_int(NOTIFY_PERMISSION_KEY, permission.id());
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

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_window_size(size: WindowSize) {
    store_int(WINDOW_SIZE_KEY, size.id());
}

pub fn set_history_minutes(minutes: u32) {
    store_int(HISTORY_MINUTES_KEY, i64::from(minutes));
}

/// Whether the privacy filter is on: the one place that decides if copied content may be masked
/// (see [`crate::commands::effective_mask`]). A missing key means on.
pub fn privacy_filter() -> bool {
    load_bool(PRIVACY_FILTER_KEY).unwrap_or(Settings::default().privacy_filter)
}

pub fn set_privacy_filter(on: bool) {
    store_bool(PRIVACY_FILTER_KEY, on);
}

pub fn set_blur(on: bool) {
    store_bool(BLUR_KEY, on);
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_page_long_text(on: bool) {
    store_bool(PAGE_LONG_TEXT_KEY, on);
}

/// Whether VoiceOver announces a confirmation, from the stored value: missing means on.
pub fn announce_from(stored: Option<bool>) -> bool {
    stored.unwrap_or(Settings::default().announce_confirmations)
}

/// Whether the "Copied" toast is also announced to VoiceOver.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn announce_confirmations() -> bool {
    announce_from(load_bool(ANNOUNCE_KEY))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_announce_confirmations(on: bool) {
    store_bool(ANNOUNCE_KEY, on);
}

/// Whether Wis/Clear asks first, from the stored value: missing means it asks.
pub fn confirm_clear_from(stored: Option<bool>) -> bool {
    stored.unwrap_or(Settings::default().confirm_clear)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn confirm_clear() -> bool {
    confirm_clear_from(load_bool(CONFIRM_CLEAR_KEY))
}

/// `false` after "Don't ask again"; `true` from Settings.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_confirm_clear(on: bool) {
    store_bool(CONFIRM_CLEAR_KEY, on);
}

pub fn set_date_month_first(on: bool) {
    store_bool(DATE_MONTH_FIRST_KEY, on);
}

// Choice set by `set_language` on this test thread. Unset means `Language::System`, so a
// round-trip that writes the user defaults cannot change the strings another test asserts.
#[cfg(test)]
thread_local! {
    static LANGUAGE_CHOICE: std::cell::Cell<Option<Language>> = const { std::cell::Cell::new(None) };
}

/// The stored language. A missing or unknown value is [`Language::System`].
pub fn language() -> Language {
    #[cfg(test)]
    {
        LANGUAGE_CHOICE
            .with(std::cell::Cell::get)
            .unwrap_or(Language::System)
    }
    #[cfg(not(test))]
    {
        load_string(LANGUAGE_KEY)
            .and_then(|text| Language::parse(&text))
            .unwrap_or(Language::System)
    }
}

pub fn set_language(language: Language) {
    store_string(LANGUAGE_KEY, language.as_str());
    #[cfg(test)]
    LANGUAGE_CHOICE.with(|choice| choice.set(Some(language)));
}

/// Drop this thread's test choice so [`language`] is [`Language::System`] again.
#[cfg(test)]
pub(crate) fn clear_language_choice() {
    LANGUAGE_CHOICE.with(|choice| choice.set(None));
}

/// One language round-trip at a time: the tests write the same user-defaults key.
#[cfg(test)]
pub(crate) fn language_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
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

/// Compact label of the stored chord (`⌃⌘C`).
pub fn hotkey_label() -> String {
    hotkey::label(&hotkey())
}

/// Whether the welcome window should open: only while `hasSeenWelcome` was never stored.
pub fn should_show_welcome(seen: Option<bool>) -> bool {
    seen.is_none()
}

/// First launch: `hasSeenWelcome` is missing from the user defaults.
pub fn welcome_due() -> bool {
    should_show_welcome(load_bool(WELCOME_KEY))
}

/// The welcome window closed: do not open it on its own again.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn mark_welcome_seen() {
    store_bool(WELCOME_KEY, true);
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

/// A bundle id Copycraft will keep: reverse-DNS, so it contains a dot. A letter or digit,
/// then letters, digits, dots and hyphens, with no empty segment and no trailing dot.
/// A word without a dot (`not an id`) is not an id.
pub fn valid_bundle_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return false;
    }
    id.contains('.') && !id.ends_with('.') && !id.split('.').any(|segment| segment.is_empty())
}

/// Bundle ids from `raw` (one per line, or separated by spaces or commas). Invalid lines,
/// duplicates and Copycraft's own id are dropped. At most [`IGNORED_APP_MAX`].
pub fn parse_ignored(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let id = piece.trim();
        if !valid_bundle_id(id) || id == crate::capture::OWN_BUNDLE {
            continue;
        }
        if out.iter().any(|existing| existing == id) {
            continue;
        }
        if out.len() == IGNORED_APP_MAX {
            break;
        }
        out.push(id.to_string());
    }
    out
}

/// Bundle ids whose copies are not read. Missing means none.
pub fn ignored_bundles() -> Vec<String> {
    load_string(IGNORED_APPS_KEY)
        .map(|raw| parse_ignored(&raw))
        .unwrap_or_default()
}

pub fn bundle_ignored(id: &str) -> bool {
    id != crate::capture::OWN_BUNDLE && ignored_bundles().iter().any(|item| item == id)
}

/// The ignore list on one line, for the settings field.
pub fn ignored_apps_text() -> String {
    ignored_bundles().join(" ")
}

/// Store `raw`. Invalid ids are dropped.
pub fn set_ignored_apps(raw: &str) {
    store_string(IGNORED_APPS_KEY, &parse_ignored(raw).join("\n"));
}

/// Append `bundle` when it is a new, valid, foreign id and the list is not full.
pub fn ignore_app(bundle: &str) -> bool {
    let mut list = ignored_bundles();
    if list.len() >= IGNORED_APP_MAX || list.iter().any(|item| item == bundle) {
        return false;
    }
    list.push(bundle.to_string());
    let parsed = parse_ignored(&list.join("\n"));
    if parsed.len() == list.len() - 1 {
        return false;
    }
    store_string(IGNORED_APPS_KEY, &parsed.join("\n"));
    true
}

/// Put the standard list back.
pub fn restore_symbol_catalog() {
    store_string(
        SYMBOLS_KEY,
        &crate::symbols::format_list(&crate::symbols::default_list()),
    );
}

#[cfg(all(target_os = "macos", not(test)))]
fn load_bool(key: &str) -> Option<bool> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    defaults.objectForKey(&key)?;
    Some(defaults.boolForKey(&key))
}

#[cfg(all(target_os = "macos", not(test)))]
fn store_bool(key: &str, value: bool) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults().setBool_forKey(value, &NSString::from_str(key));
}

#[cfg(all(target_os = "macos", not(test)))]
fn load_int(key: &str) -> Option<i64> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    defaults.objectForKey(&key)?;
    Some(defaults.integerForKey(&key) as i64)
}

#[cfg(all(target_os = "macos", not(test)))]
fn store_int(key: &str, value: i64) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults()
        .setInteger_forKey(value as isize, &NSString::from_str(key));
}

#[cfg(all(target_os = "macos", not(test)))]
fn load_string(key: &str) -> Option<String> {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(key);
    let value = defaults.stringForKey(&key)?;
    Some(value.to_string())
}

#[cfg(all(target_os = "macos", not(test)))]
fn store_string(key: &str, value: &str) {
    use mac_ui::objc2_foundation::{NSString, NSUserDefaults};
    // SAFETY: NSString is a valid NSObject for the defaults dictionary.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&*NSString::from_str(value)), &NSString::from_str(key));
    }
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn load_bool(_key: &str) -> Option<bool> {
    None
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn store_bool(_key: &str, _value: bool) {}

// Tests keep settings in memory (per test thread) on every platform, so a setting can
// round-trip and no test ever reads or writes the real user defaults.
#[cfg(test)]
thread_local! {
    static MEMORY: std::cell::RefCell<std::collections::HashMap<String, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(test)]
fn load_bool(key: &str) -> Option<bool> {
    MEMORY.with(|map| map.borrow().get(key).copied())
}

#[cfg(test)]
fn store_bool(key: &str, value: bool) {
    MEMORY.with(|map| map.borrow_mut().insert(key.to_owned(), value));
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn load_int(_key: &str) -> Option<i64> {
    None
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn store_int(_key: &str, _value: i64) {}

#[cfg(test)]
thread_local! {
    static MEMORY_INT: std::cell::RefCell<std::collections::HashMap<String, i64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(test)]
fn load_int(key: &str) -> Option<i64> {
    MEMORY_INT.with(|map| map.borrow().get(key).copied())
}

#[cfg(test)]
fn store_int(key: &str, value: i64) {
    MEMORY_INT.with(|map| map.borrow_mut().insert(key.to_owned(), value));
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn load_string(_key: &str) -> Option<String> {
    None
}

#[cfg(all(not(target_os = "macos"), not(test)))]
fn store_string(_key: &str, _value: &str) {}

#[cfg(test)]
thread_local! {
    static MEMORY_STRING: std::cell::RefCell<std::collections::HashMap<String, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(test)]
fn load_string(key: &str) -> Option<String> {
    MEMORY_STRING.with(|map| map.borrow().get(key).cloned())
}

#[cfg(test)]
fn store_string(key: &str, value: &str) {
    MEMORY_STRING.with(|map| map.borrow_mut().insert(key.to_owned(), value.to_owned()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_width_follows_the_choice_and_stays_on_the_screen() {
        use super::WindowSize::{Compact, Normal, Wide};
        assert_eq!(super::Settings::default().window_size, Normal);
        assert_eq!(Compact.width(), 440.0);
        // A roomy screen gives the choice as is.
        assert_eq!(Normal.width_on(1920.0), 640.0);
        assert_eq!(Wide.width_on(2560.0), 800.0);
        // Never more than 60% of the usable width.
        assert_eq!(Wide.width_on(1280.0), 768.0);
        assert_eq!(Normal.width_on(1000.0), 600.0);
        // A small screen keeps the compact card, never less.
        assert_eq!(Normal.width_on(600.0), 440.0);
        assert_eq!(Compact.width_on(300.0), 440.0);
        // Each size is wider than the one before on a roomy screen.
        assert!(Compact.width_on(2000.0) < Normal.width_on(2000.0));
        assert!(Normal.width_on(2000.0) < Wide.width_on(2000.0));
    }

    #[test]
    fn notifications_are_on_by_default_and_the_permission_starts_unknown() {
        use crate::notifications::Permission;
        let settings = super::Settings::default();
        assert!(settings.notify_ai && settings.notify_ocr);
        assert!(settings.notify_errors && settings.notify_secret);
        assert_eq!(super::notify_permission(), Permission::Unknown);
        let switches = super::notify_switches();
        assert!(switches.ocr && switches.ai && switches.errors && switches.secret);
        // A switch round-trips on its own; the others stay on.
        super::set_notify(crate::notifications::Switch::Ocr, false);
        let switches = super::notify_switches();
        assert!(!switches.ocr && switches.ai && switches.errors && switches.secret);
        super::set_notify_permission(Permission::Refused);
        assert_eq!(super::notify_permission(), Permission::Refused);
    }

    #[test]
    fn a_stored_window_size_round_trips_and_a_bad_one_is_ignored() {
        for size in super::WindowSize::ALL {
            assert_eq!(super::WindowSize::from_id(size.id()), Some(size));
        }
        assert_eq!(super::WindowSize::from_id(7), None);
        assert_eq!(super::WindowSize::from_id(-1), None);
    }

    use super::Settings;

    #[test]
    fn welcome_shows_only_while_the_key_is_missing() {
        assert!(super::should_show_welcome(None));
        assert!(!super::should_show_welcome(Some(true)));
        assert!(!super::should_show_welcome(Some(false)));
    }

    #[test]
    fn tests_start_from_an_empty_settings_store() {
        // Each test thread has its own in-memory store: nothing here touches the real defaults.
        std::thread::spawn(|| {
            assert!(super::welcome_due());
            assert_eq!(super::load_bool(super::WELCOME_KEY), None);
        })
        .join()
        .expect("test thread");
    }

    #[test]
    fn closing_the_welcome_marks_it_seen() {
        super::store_bool(super::WELCOME_KEY, false);
        assert!(!super::welcome_due());
        super::mark_welcome_seen();
        assert_eq!(super::load_bool(super::WELCOME_KEY), Some(true));
        assert!(!super::welcome_due());
    }

    #[test]
    fn defaults_match_the_previous_behaviour() {
        let settings = Settings::default();
        assert!(settings.clear_sensitive);
        assert_eq!(settings.history_minutes, 15);
        assert!(settings.privacy_filter);
        assert!(settings.blur);
        assert!(!settings.date_month_first);
        assert!(settings.hotkey.is_none());
        assert_eq!(settings.language, super::Language::System);
        assert!(super::HISTORY_MINUTES.contains(&settings.history_minutes));
    }

    #[test]
    fn language_round_trips_system_nl_and_en() {
        let _lock = super::language_test_lock();
        let previous = super::load().language;
        struct Restore(super::Language);
        impl Drop for Restore {
            fn drop(&mut self) {
                super::set_language(self.0);
                super::clear_language_choice();
            }
        }
        let _restore = Restore(previous);
        super::clear_language_choice();
        assert_eq!(super::language(), super::Language::System);
        super::set_language(super::Language::Nl);
        assert_eq!(super::language(), super::Language::Nl);
        assert_eq!(super::load().language, super::Language::Nl);
        super::set_language(super::Language::En);
        assert_eq!(super::language(), super::Language::En);
        assert_eq!(super::load().language, super::Language::En);
        super::set_language(super::Language::System);
        assert_eq!(super::language(), super::Language::System);
        assert_eq!(super::load().language, super::Language::System);
        // An unknown stored value falls back to system.
        super::store_string(super::LANGUAGE_KEY, "fr");
        assert_eq!(super::load().language, super::Language::System);
        assert!(super::Language::parse("fr").is_none());
    }

    #[test]
    fn privacy_filter_is_on_until_stored_off_and_round_trips() {
        assert!(super::privacy_filter());
        assert!(super::load().privacy_filter);
        super::set_privacy_filter(false);
        assert!(!super::privacy_filter());
        assert!(!super::load().privacy_filter);
        // The other settings are untouched.
        assert!(super::load().blur);
        super::set_privacy_filter(true);
        assert!(super::privacy_filter());
        assert!(super::load().privacy_filter);
    }

    #[test]
    fn preferred_read_options_follow_the_date_setting() {
        assert!(!super::preferred_read_options().month_first);
    }

    #[test]
    fn ignored_apps_keep_only_valid_foreign_bundle_ids() {
        let raw = "com.apple.Safari\nnot an id\ncom..broken\ncom.apple.\n.hidden\n\
                   nl.marcelkoopman.copycraft\ncom.apple.Safari\ncom.apple.Notes, com.example.app-1";
        let ids = super::parse_ignored(raw);
        assert_eq!(
            ids,
            vec![
                "com.apple.Safari".to_string(),
                "com.apple.Notes".to_string(),
                "com.example.app-1".to_string(),
            ]
        );
        let mut many = String::new();
        for i in 0..super::IGNORED_APP_MAX + 5 {
            many.push_str(&format!("app.item{i}\n"));
        }
        assert_eq!(super::parse_ignored(&many).len(), super::IGNORED_APP_MAX);
        assert!(!super::valid_bundle_id(""));
        assert!(!super::valid_bundle_id("nl.marcelkoopman.copycraft."));
    }

    #[test]
    fn confirmations_are_announced_unless_turned_off() {
        assert!(super::Settings::default().announce_confirmations);
        assert!(super::announce_from(None));
        assert!(super::announce_from(Some(true)));
        assert!(!super::announce_from(Some(false)));
    }

    #[test]
    fn clear_asks_unless_told_not_to() {
        assert!(super::Settings::default().confirm_clear);
        assert!(super::confirm_clear_from(None));
        assert!(super::confirm_clear_from(Some(true)));
        assert!(!super::confirm_clear_from(Some(false)));
    }
}
