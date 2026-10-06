//! Special characters the card copies as plain text. The list and the decision to ignore
//! copycraft's own pasteboard change are plain data, so they are tested without AppKit.

use std::sync::atomic::{AtomicIsize, Ordering};

/// The list when the user has not stored one. Order is the order shown.
pub const DEFAULT: &[&str] = &[
    "✓", "✗", "€", "→", "←", "•", "…", "–", "—", "°", "±", "×", "≠", "≤", "≥", "©",
];

/// No ignored change yet. A real `NSPasteboard.changeCount` is never this.
const NO_CHANGE: isize = isize::MIN;

/// Change count of the symbol copy last written. The poller skips that change.
static IGNORED_CHANGE: AtomicIsize = AtomicIsize::new(NO_CHANGE);

/// The standard list, owned, for the popover and for a restored setting.
pub(crate) fn default_list() -> Vec<String> {
    DEFAULT.iter().copied().map(str::to_string).collect()
}

/// Remember `change` as copycraft's own symbol write. The next poll must not record or scan it.
pub(crate) fn ignore_change(change: isize) {
    IGNORED_CHANGE.store(change, Ordering::SeqCst);
}

/// The change count [`ignore_change`] stored, if any.
pub(crate) fn ignored_change() -> Option<isize> {
    match IGNORED_CHANGE.load(Ordering::SeqCst) {
        NO_CHANGE => None,
        change => Some(change),
    }
}

/// What the poller may do with one pasteboard change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChangeGate {
    /// Put the text in history.
    pub record: bool,
    /// Read the pasteboard. A read classifies and labels the text, so skipping it skips the scan.
    pub scan: bool,
}

impl ChangeGate {
    /// Leave the change alone: no history entry and no read.
    pub(crate) fn ignore(self) -> bool {
        !self.record && !self.scan
    }
}

/// `change` is a symbol copy when it is the one [`ignore_change`] stored.
pub(crate) fn gate(ignored: Option<isize>, change: isize) -> ChangeGate {
    let own = ignored == Some(change);
    ChangeGate {
        record: !own,
        scan: !own,
    }
}
