//! Strings in the standard user defaults (the app's preferences), read and written safely.

use objc2_foundation::{NSString, NSUserDefaults};

/// The string stored under `key`, if any.
pub fn load_string(key: &str) -> Option<String> {
    let value = NSUserDefaults::standardUserDefaults().stringForKey(&NSString::from_str(key))?;
    Some(value.to_string())
}

/// Store `value` under `key`.
pub fn store_string(key: &str, value: &str) {
    let value = NSString::from_str(value);
    let key = NSString::from_str(key);
    // SAFETY: an NSString is a valid property-list value for the user defaults.
    unsafe { NSUserDefaults::standardUserDefaults().setObject_forKey(Some(&*value), &key) };
}
