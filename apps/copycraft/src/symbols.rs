//! Special characters the card copies as plain text. The list and the decision to ignore
//! copycraft's own pasteboard change are plain data, so they are tested without AppKit.

use std::sync::atomic::{AtomicIsize, Ordering};

/// The list when the user has not stored one. Order is the order shown.
pub const DEFAULT: &[&str] = &[
    "✓", "✗", "€", "→", "←", "•", "…", "–", "—", "°", "±", "×", "≠", "≤", "≥", "©",
];

/// Recently used characters kept in front of the list.
pub const MAX_RECENT: usize = 8;

/// How many characters the settings field may hold. More than this is not a character list.
const MAX_CATALOG: usize = 48;

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

/// One line for the user defaults and the settings field.
pub(crate) fn format_list(symbols: &[String]) -> String {
    symbols.join(" ")
}

/// The settings field. `None` when it is empty or not a list of characters: the caller uses
/// [`default_list`]. Whitespace, commas and semicolons separate characters. A repeat is kept
/// once, in the first position. A character is one to four Unicode scalars and not a word of
/// letters or digits, so a sentence does not become the list.
pub(crate) fn parse_catalog(raw: &str) -> Option<Vec<String>> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut catalog = Vec::new();
    for token in raw.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '|')) {
        if token.is_empty() {
            continue;
        }
        let symbol = valid_symbol(token)?;
        if catalog.len() == MAX_CATALOG {
            return None;
        }
        if !catalog.iter().any(|item| item == &symbol) {
            catalog.push(symbol);
        }
    }
    if catalog.is_empty() {
        None
    } else {
        Some(catalog)
    }
}

/// One special character, not a word and not a control character.
fn valid_symbol(token: &str) -> Option<String> {
    let count = token.chars().count();
    if !(1..=4).contains(&count) {
        return None;
    }
    if token.chars().any(char::is_control) || token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(token.to_string())
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

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT, MAX_RECENT, default_list, display_order, format_list, gate, note_used,
        parse_catalog, parse_recent,
    };

    fn owned(symbols: &[&str]) -> Vec<String> {
        symbols.iter().copied().map(str::to_string).collect()
    }

    #[test]
    fn default_list_is_the_standard_order() {
        assert_eq!(
            default_list(),
            owned(&[
                "✓", "✗", "€", "→", "←", "•", "…", "–", "—", "°", "±", "×", "≠", "≤", "≥", "©",
            ])
        );
        assert_eq!(default_list(), owned(DEFAULT));
    }

    #[test]
    fn recent_moves_to_the_front_without_duplicates_and_stops_at_eight() {
        let symbols = ["✓", "✗", "€", "→", "←", "•", "…", "–", "—"];
        let mut recent = Vec::new();
        for symbol in symbols {
            recent = note_used(&recent, symbol);
        }
        assert_eq!(recent.len(), MAX_RECENT);
        assert_eq!(recent[0], "—");
        assert!(!recent.iter().any(|item| item == "✓"));
        let unique = {
            let mut copy = recent.clone();
            copy.sort();
            copy.dedup();
            copy
        };
        assert_eq!(unique.len(), recent.len());

        recent = note_used(&recent, "€");
        assert_eq!(recent[0], "€");
        assert_eq!(recent.iter().filter(|item| item.as_str() == "€").count(), 1);
        assert_eq!(recent.len(), MAX_RECENT);

        assert_eq!(
            parse_recent("€ → € ← • … – — ° ± ×"),
            owned(&["€", "→", "←", "•", "…", "–", "—", "°"])
        );
    }

    #[test]
    fn recent_characters_lead_the_catalog_once_each() {
        let catalog = default_list();
        let recent = note_used(&note_used(&Vec::new(), "©"), "€");
        let shown = display_order(&catalog, &recent);
        assert_eq!(shown[0], "€");
        assert_eq!(shown[1], "©");
        assert_eq!(shown.len(), catalog.len());
        assert_eq!(shown.iter().filter(|item| item.as_str() == "€").count(), 1);
        assert_eq!(display_order(&catalog, &owned(&["☆"])), catalog);
    }

    #[test]
    fn empty_or_invalid_input_falls_back_and_restore_is_the_default() {
        assert!(parse_catalog("").is_none());
        assert!(parse_catalog("  , ; ").is_none());
        assert!(parse_catalog("hello").is_none());
        assert!(parse_catalog("€ hello").is_none());
        for raw in ["", "   ", "words", "€ and more"] {
            assert_eq!(
                parse_catalog(raw).unwrap_or_else(default_list),
                default_list()
            );
        }
        assert_eq!(
            parse_catalog("€, →; ←|•  •").unwrap(),
            owned(&["€", "→", "←", "•"])
        );
        let mut tokens = Vec::new();
        for offset in 0..49 {
            tokens.push(char::from_u32(0x2200 + offset).unwrap().to_string());
        }
        assert!(parse_catalog(&tokens.join(" ")).is_none());
        tokens.pop();
        assert_eq!(parse_catalog(&tokens.join(" ")).unwrap().len(), 48);
        assert_eq!(
            parse_catalog(&format_list(&default_list())).unwrap(),
            default_list()
        );
    }

    #[test]
    fn an_own_symbol_copy_stays_out_of_history() {
        use crate::clipboard::ClipboardHistory;

        let own = 41;
        let decision = gate(Some(own), own);
        assert!(decision.ignore());
        assert!(!decision.record);
        assert!(!decision.scan);

        // The poller returns before it reads or records when the gate ignores the change.
        // This does not write the pasteboard.
        let mut history = ClipboardHistory::default();
        if decision.record {
            history.record("€".to_string());
        }
        assert!(history.is_empty());

        let later = gate(Some(own), own + 1);
        assert!(later.record);
        assert!(!later.ignore());
        if later.record {
            history.record("hello".to_string());
        }
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0), Some("hello"));
    }
}
