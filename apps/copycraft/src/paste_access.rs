//! Pasteboard privacy (macOS 15.4+, see `docs/pasteboard-privacy/FINDINGS.md`): which reads
//! Copycraft makes of the general pasteboard, and when.
//!
//! The change count, the list of types and `availableTypeFromArray:` read no content and are
//! not expected to show the privacy alert; `stringForType:`, `dataForType:`,
//! `propertyListForType:`, `readObjectsForClasses:` and `NSImage initWithPasteboard:` do, and
//! that alert is modal and blocks the thread that reads. The access setting picks the strategy
//! ([`strategy`], option C in FINDINGS): Always Allow (and macOS before 15.4) reads every copy
//! as it comes; Default and Ask read a copy only once the card is shown, in one pass for that
//! copy (one alert); Always Deny reads nothing. A reading (also one that came back empty,
//! failed or was denied) is made again only for another change count, other types or another
//! setting, never every tick ([`plan`]). Pure logic; `macos_pasteboard` asks AppKit.

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

/// The card's placeholder (and the icon's tooltip) for a copy not read yet: with Default or Ask
/// the copy is read once the card is shown.
pub const PENDING_NOTE: &str = "New copy — open the card to view";

/// The card's one-time explanation the first time Copycraft reads with Default or Ask.
pub const ASK_NOTE: &str = "macOS asks before Copycraft may read a copy from another app.\n\
Choose Allow to see the copy here.\n\
Privacy & Security › Paste from Other Apps can set Copycraft to Always Allow.";

/// How Copycraft reads the pasteboard's contents for an access setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadStrategy {
    /// Read each copy as it comes, while polling (Always Allow; macOS before 15.4, which has no
    /// pasteboard privacy).
    Now,
    /// Watch the change count and types only; read a copy once the card is shown, in one pass
    /// (Default, Ask, and a setting this build does not know: no alert from the background).
    WhenShown,
    /// Read nothing (Always Deny).
    Never,
}

/// The strategy for `behavior`. `None`: macOS before 15.4.
pub fn strategy(behavior: Option<AccessBehavior>) -> ReadStrategy {
    match behavior {
        None | Some(AccessBehavior::AlwaysAllow) => ReadStrategy::Now,
        Some(AccessBehavior::AlwaysDeny) => ReadStrategy::Never,
        Some(AccessBehavior::Default | AccessBehavior::Ask | AccessBehavior::Other(_)) => {
            ReadStrategy::WhenShown
        }
    }
}

/// The card shows [`ASK_NOTE`] (once, before the first read) when copies are read on showing
/// and it was not shown before.
pub fn explain_first(strategy: ReadStrategy, explained: bool) -> bool {
    strategy == ReadStrategy::WhenShown && !explained
}

/// The line logged when the setting changes while Copycraft runs.
pub fn change_log_line(behavior: Option<AccessBehavior>) -> String {
    startup_log_line(behavior).replace("accessBehavior", "accessBehavior now")
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
/// content) and the read strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadKey {
    pub change_count: isize,
    pub types: u64,
    pub strategy: ReadStrategy,
}

impl ReadKey {
    pub fn new<'a>(
        change_count: isize,
        types: impl IntoIterator<Item = &'a str>,
        strategy: ReadStrategy,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for kind in types {
            kind.hash(&mut hasher);
        }
        Self {
            change_count,
            types: hasher.finish(),
            strategy,
        }
    }
}

/// The reading kept for the pasteboard: its key, and whether its contents were read (not a
/// copy waiting for the card, nor Always Deny).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    pub key: ReadKey,
    pub content_read: bool,
}

/// What to do for the pasteboard as it is now ([`plan`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPlan {
    /// Keep the reading made before: same key (also one that came back empty, failed or was
    /// denied), or a copy read on showing and the card has closed since.
    Reuse,
    /// Read the contents, once for this key.
    Read,
    /// A copy not read yet: wait until the card is shown ([`PENDING_NOTE`]).
    Wait,
    /// Always Deny: no content reads.
    Deny,
}

/// What to do for the pasteboard at `now`, given the reading kept and whether the card is
/// shown. A copy is read at most once: a denied or failed read is not retried (and an alert not
/// shown again) until another copy or other types; a copy read already is not read again for
/// another setting either. Always Deny never reads.
pub fn plan(kept: Option<&Kept>, now: &ReadKey, shown: bool) -> ReadPlan {
    let exact = kept.is_some_and(|kept| kept.key == *now);
    let read_before = kept.is_some_and(|kept| {
        kept.content_read
            && kept.key.change_count == now.change_count
            && kept.key.types == now.types
    });
    match now.strategy {
        ReadStrategy::Never if exact => ReadPlan::Reuse,
        ReadStrategy::Never => ReadPlan::Deny,
        ReadStrategy::Now if exact || read_before => ReadPlan::Reuse,
        ReadStrategy::Now => ReadPlan::Read,
        ReadStrategy::WhenShown if read_before => ReadPlan::Reuse,
        ReadStrategy::WhenShown if shown => ReadPlan::Read,
        ReadStrategy::WhenShown if exact => ReadPlan::Reuse,
        ReadStrategy::WhenShown => ReadPlan::Wait,
    }
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
    fn the_setting_picks_the_strategy() {
        assert_eq!(strategy(None), ReadStrategy::Now);
        assert_eq!(
            strategy(Some(AccessBehavior::AlwaysAllow)),
            ReadStrategy::Now
        );
        assert_eq!(
            strategy(Some(AccessBehavior::Default)),
            ReadStrategy::WhenShown
        );
        assert_eq!(strategy(Some(AccessBehavior::Ask)), ReadStrategy::WhenShown);
        assert_eq!(
            strategy(Some(AccessBehavior::Other(7))),
            ReadStrategy::WhenShown
        );
        assert_eq!(
            strategy(Some(AccessBehavior::AlwaysDeny)),
            ReadStrategy::Never
        );
    }

    #[test]
    fn the_explanation_comes_once_and_only_when_reading_on_showing() {
        assert!(explain_first(ReadStrategy::WhenShown, false));
        assert!(!explain_first(ReadStrategy::WhenShown, true));
        assert!(!explain_first(ReadStrategy::Now, false));
        assert!(!explain_first(ReadStrategy::Never, false));
    }

    #[test]
    fn the_startup_line_names_the_setting() {
        assert_eq!(
            startup_log_line(Some(AccessBehavior::AlwaysDeny)),
            "copycraft: pasteboard accessBehavior alwaysDeny"
        );
        assert!(startup_log_line(None).contains("before macOS 15.4"));
    }

    const TEXT: [&str; 1] = ["public.utf8-plain-text"];

    fn kept(key: ReadKey, plan: ReadPlan) -> Kept {
        Kept {
            key,
            content_read: plan == ReadPlan::Read,
        }
    }

    #[test]
    fn always_allow_reads_each_copy_once_while_polling() {
        let first = ReadKey::new(7, TEXT, ReadStrategy::Now);
        assert_eq!(plan(None, &first, false), ReadPlan::Read);
        let read = kept(first, ReadPlan::Read);
        // The read came back empty (denied once): the next ticks see the same key.
        assert!(text_read_failed(true, false));
        for _ in 0..10 {
            assert_eq!(plan(Some(&read), &first, false), ReadPlan::Reuse);
        }
        // A new copy, or a promised picture adding its type, reads again once.
        let next = ReadKey::new(8, TEXT, ReadStrategy::Now);
        assert_eq!(plan(Some(&read), &next, false), ReadPlan::Read);
        let more = ReadKey::new(
            7,
            ["public.utf8-plain-text", "public.png"],
            ReadStrategy::Now,
        );
        assert_eq!(plan(Some(&read), &more, false), ReadPlan::Read);
    }

    #[test]
    fn under_ask_a_copy_waits_for_the_card_and_is_read_once() {
        let copy = ReadKey::new(7, TEXT, ReadStrategy::WhenShown);
        // Polling with the card closed: no read, however many ticks.
        let mut last = None;
        for _ in 0..10 {
            let step = plan(last.as_ref(), &copy, false);
            assert_eq!(
                step,
                if last.is_none() {
                    ReadPlan::Wait
                } else {
                    ReadPlan::Reuse
                }
            );
            last = Some(kept(copy, ReadPlan::Wait));
        }
        // The card opens: one read for this change count.
        assert_eq!(plan(last.as_ref(), &copy, true), ReadPlan::Read);
        let read = kept(copy, ReadPlan::Read);
        // Every later look, with the card open or closed, keeps that reading (also when it came
        // back empty: Don't Allow is not asked again).
        for shown in [true, false, true, true, false] {
            assert_eq!(plan(Some(&read), &copy, shown), ReadPlan::Reuse);
        }
        // The next copy waits again, or is read at once while the card is open.
        let next = ReadKey::new(8, TEXT, ReadStrategy::WhenShown);
        assert_eq!(plan(Some(&read), &next, false), ReadPlan::Wait);
        assert_eq!(plan(Some(&read), &next, true), ReadPlan::Read);
    }

    #[test]
    fn always_deny_reads_nothing() {
        let copy = ReadKey::new(3, TEXT, ReadStrategy::Never);
        assert_eq!(plan(None, &copy, true), ReadPlan::Deny);
        let denied = Kept {
            key: copy,
            content_read: false,
        };
        assert_eq!(plan(Some(&denied), &copy, true), ReadPlan::Reuse);
    }

    #[test]
    fn changing_the_access_setting_is_picked_up() {
        let denied = Kept {
            key: ReadKey::new(3, TEXT, ReadStrategy::Never),
            content_read: false,
        };
        let allow = ReadKey::new(3, TEXT, ReadStrategy::Now);
        assert_eq!(plan(Some(&denied), &allow, false), ReadPlan::Read);
        let ask = ReadKey::new(3, TEXT, ReadStrategy::WhenShown);
        assert_eq!(plan(Some(&denied), &ask, false), ReadPlan::Wait);
        // A copy read already is not read (asked) again for another setting, but Always Deny
        // hides it.
        let read = kept(allow, ReadPlan::Read);
        assert_eq!(plan(Some(&read), &ask, true), ReadPlan::Reuse);
        assert_eq!(plan(Some(&read), &ask, false), ReadPlan::Reuse);
        assert_eq!(
            plan(
                Some(&read),
                &ReadKey::new(3, TEXT, ReadStrategy::Never),
                true
            ),
            ReadPlan::Deny
        );
        assert_eq!(
            change_log_line(Some(AccessBehavior::AlwaysAllow)),
            "copycraft: pasteboard accessBehavior now alwaysAllow"
        );
    }

    #[test]
    fn a_read_text_or_no_text_type_is_no_failure() {
        assert!(!text_read_failed(true, true));
        assert!(!text_read_failed(false, false));
    }
}
