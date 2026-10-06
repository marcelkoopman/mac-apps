//! Special characters the card copies as plain text. The list and the decision to ignore
//! copycraft's own pasteboard change are plain data, so they are tested without AppKit.

use std::sync::atomic::{AtomicIsize, Ordering};

/// The list when the user has not stored one. Order is the order shown.
pub const DEFAULT: &[&str] = &[
    "✓", "✗", "€", "→", "←", "•", "…", "–", "—", "°", "±", "×", "≠", "≤", "≥", "©",
];

/// Recently used characters kept in front of the list.
pub const MAX_RECENT: usize = 8;

/// No ignored change yet. A real `NSPasteboard.changeCount` is never this.
const NO_CHANGE: isize = isize::MIN;

/// Change count of the symbol copy last written. The poller skips that change.
static IGNORED_CHANGE: AtomicIsize = AtomicIsize::new(NO_CHANGE);

/// The standard list, owned, for the popover and for a restored setting.
pub(crate) fn default_list() -> Vec<String> {
    DEFAULT.iter().copied().map(str::to_string).collect()
}

/// Characters in the popover: `recent` first (newest first, each once, at most [`MAX_RECENT`]),
/// then the catalog. A recent character that is not in the catalog stays stored but is not shown,
/// so removing it from the list stays removed.
pub(crate) fn display_order(catalog: &[String], recent: &[String]) -> Vec<String> {
    let mut shown = Vec::with_capacity(catalog.len().saturating_add(recent.len()));
    for symbol in recent.iter().take(MAX_RECENT) {
        if catalog.iter().any(|item| item == symbol) && !shown.iter().any(|item| item == symbol) {
            shown.push(symbol.clone());
        }
    }
    for symbol in catalog {
        if !shown.iter().any(|item| item == symbol) {
            shown.push(symbol.clone());
        }
    }
    shown
}

/// Put `symbol` first. A repeat moves up, and the list stays at [`MAX_RECENT`] with no duplicates.
pub(crate) fn note_used(recent: &[String], symbol: &str) -> Vec<String> {
    let mut next = Vec::with_capacity(MAX_RECENT.min(recent.len().saturating_add(1)));
    next.push(symbol.to_string());
    for item in recent {
        if item != symbol && next.len() < MAX_RECENT {
            next.push(item.clone());
        }
    }
    next
}

/// Recent characters as stored: whitespace-separated, newest first, at most [`MAX_RECENT`].
pub(crate) fn parse_recent(raw: &str) -> Vec<String> {
    let mut recent = Vec::new();
    for token in raw.split_whitespace() {
        if recent.len() == MAX_RECENT {
            break;
        }
        if !recent.iter().any(|item| item == token) {
            recent.push(token.to_string());
        }
    }
    recent
}

/// One line for the user defaults.
pub(crate) fn format_list(symbols: &[String]) -> String {
    symbols.join(" ")
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
