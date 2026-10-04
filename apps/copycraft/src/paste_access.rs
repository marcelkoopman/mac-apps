//! Pasteboard privacy (macOS 15.4+, see `docs/pasteboard-privacy/FINDINGS.md`): which reads
//! Copycraft makes of the general pasteboard, and when a reading is made again.
//!
//! The change count, the list of types and `availableTypeFromArray:` read no content and are
//! not expected to show the privacy alert; `stringForType:`, `dataForType:`,
//! `propertyListForType:`, `readObjectsForClasses:` and `NSImage initWithPasteboard:` do, and
//! that alert is modal and blocks the thread that reads. So: with Always Deny, no content reads
//! at all; and a reading (also one that came back empty, failed or was denied) is made again
//! only for another change count, other types or another access setting, never every tick.
//! Pure logic; `macos_pasteboard` asks AppKit.

use std::hash::{Hash, Hasher};

/// `NSPasteboard.accessBehavior` of the general pasteboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessBehavior {
    /// Never asked yet: the first content read asks.
    Default,
    /// Asks on every content read that is not a user-originated paste.
    Ask,
    AlwaysAllow,
    /// Content reads come back empty (without asking).
    AlwaysDeny,
    /// A value this build does not know.
    Other(isize),
}

impl AccessBehavior {
    /// From `NSPasteboardAccessBehavior`'s raw value.
    pub fn from_raw(raw: isize) -> Self {
        match raw {
            0 => Self::Default,
            1 => Self::Ask,
            2 => Self::AlwaysAllow,
            3 => Self::AlwaysDeny,
            other => Self::Other(other),
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::Default => "default".to_string(),
            Self::Ask => "ask".to_string(),
            Self::AlwaysAllow => "alwaysAllow".to_string(),
            Self::AlwaysDeny => "alwaysDeny".to_string(),
            Self::Other(raw) => format!("unknown ({raw})"),
        }
    }
}

/// The card's placeholder while Always Deny is set.
pub const DENIED_NOTE: &str = "Clipboard access denied in Privacy & Security";

/// Whether Copycraft reads the pasteboard's contents. `None`: macOS before 15.4, which has no
/// pasteboard privacy. Only Always Deny stops the reads; Default, Ask and Always Allow keep
/// today's behaviour.
pub fn may_read_content(behavior: Option<AccessBehavior>) -> bool {
    behavior != Some(AccessBehavior::AlwaysDeny)
}

/// The line logged once at startup.
pub fn startup_log_line(behavior: Option<AccessBehavior>) -> String {
    match behavior {
        Some(behavior) => format!("copycraft: pasteboard accessBehavior {}", behavior.name()),
        None => {
            "copycraft: pasteboard accessBehavior not available (before macOS 15.4)".to_string()
        }
    }
}

/// What a reading of the pasteboard was made for: the change count, its types (as a hash, no
/// content) and whether content reads were allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadKey {
    pub change_count: isize,
    pub types: u64,
    pub allowed: bool,
}

impl ReadKey {
    pub fn new<'a>(
        change_count: isize,
        types: impl IntoIterator<Item = &'a str>,
        allowed: bool,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for kind in types {
            kind.hash(&mut hasher);
        }
        Self {
            change_count,
            types: hasher.finish(),
            allowed,
        }
    }
}

/// Read the contents again? Only when the key changed: another copy, a promised picture adding
/// its types, or the access setting changed. A reading that came back empty, failed or was
/// denied stays until then, so a denied read is not retried (and an alert not shown again) on
/// every tick.
pub fn read_again(last: Option<&ReadKey>, now: &ReadKey) -> bool {
    last != Some(now)
}

/// The text type was declared but reading it gave nothing: the read failed or was denied.
pub fn text_read_failed(text_declared: bool, text_read: bool) -> bool {
    text_declared && !text_read
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_values_are_apples() {
        assert_eq!(AccessBehavior::from_raw(0), AccessBehavior::Default);
        assert_eq!(AccessBehavior::from_raw(1), AccessBehavior::Ask);
        assert_eq!(AccessBehavior::from_raw(2), AccessBehavior::AlwaysAllow);
        assert_eq!(AccessBehavior::from_raw(3), AccessBehavior::AlwaysDeny);
        assert_eq!(AccessBehavior::from_raw(9), AccessBehavior::Other(9));
        assert_eq!(AccessBehavior::Other(9).name(), "unknown (9)");
    }

    #[test]
    fn only_always_deny_stops_content_reads() {
        assert!(may_read_content(None));
        assert!(may_read_content(Some(AccessBehavior::Default)));
        assert!(may_read_content(Some(AccessBehavior::Ask)));
        assert!(may_read_content(Some(AccessBehavior::AlwaysAllow)));
        assert!(may_read_content(Some(AccessBehavior::Other(7))));
        assert!(!may_read_content(Some(AccessBehavior::AlwaysDeny)));
    }

    #[test]
    fn the_startup_line_names_the_setting() {
        assert_eq!(
            startup_log_line(Some(AccessBehavior::AlwaysDeny)),
            "copycraft: pasteboard accessBehavior alwaysDeny"
        );
        assert!(startup_log_line(None).contains("before macOS 15.4"));
    }

    #[test]
    fn a_failed_or_denied_read_is_not_retried_until_the_key_changes() {
        let text = ["public.utf8-plain-text"];
        let first = ReadKey::new(7, text, true);
        assert!(read_again(None, &first));
        // The read came back empty (denied once): the next ticks see the same key.
        assert!(text_read_failed(true, false));
        for _ in 0..10 {
            assert!(!read_again(Some(&first), &ReadKey::new(7, text, true)));
        }
        // A new copy, or a promised picture adding its type, reads again once.
        assert!(read_again(Some(&first), &ReadKey::new(8, text, true)));
        assert!(read_again(
            Some(&first),
            &ReadKey::new(7, ["public.utf8-plain-text", "public.png"], true)
        ));
    }

    #[test]
    fn changing_the_access_setting_reads_again() {
        let denied = ReadKey::new(3, ["public.utf8-plain-text"], false);
        let allowed = ReadKey::new(3, ["public.utf8-plain-text"], true);
        assert!(read_again(Some(&denied), &allowed));
        assert!(read_again(Some(&allowed), &denied));
    }

    #[test]
    fn a_read_text_or_no_text_type_is_no_failure() {
        assert!(!text_read_failed(true, true));
        assert!(!text_read_failed(false, false));
    }
}
