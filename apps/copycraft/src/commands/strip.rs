//! The history strip: small cards of the copies in history along the bottom of the card, the
//! one on screen marked. Pure logic (what shows, which one is current, what text a card has);
//! `macos_launcher/strip.rs` draws it. Clicking a card only shows that copy, it never copies.

use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroize;

/// Height of the strip row: a card and the room around it.
pub const STRIP_H: f64 = 56.0;
/// One small card.
pub const THUMB_W: f64 = 92.0;
pub const THUMB_GAP: f64 = 6.0;
/// Text on a card: at most this many lines, each at most [`THUMB_LINE_CHARS`] characters.
pub const THUMB_LINES: usize = 2;
pub const THUMB_LINE_CHARS: usize = 18;
/// A picture is drawn small only when its bytes are at most this many; a larger one shows its
/// type mark instead (no heavy decode for a thumbnail).
pub const THUMB_PICTURE_MAX_BYTES: usize = 3_000_000;
/// Cards drawn at most (history keeps 20; this keeps a longer one light).
pub const MAX_THUMBS: usize = 30;

/// One small card of the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    /// Position in history (0 is the newest), what a click shows.
    pub index: usize,
    /// The type mark (`{}`, `md`, `img`), with the pin in front of a pinned copy.
    pub mark: String,
    /// One or two short lines of the text. Empty for a picture.
    pub lines: String,
    /// A small picture's bytes (shared, not copied), when the copy is a picture that is light
    /// enough ([`THUMB_PICTURE_MAX_BYTES`]).
    pub picture: Option<crate::clipboard::SecretBytes>,
}

impl Thumb {
    pub fn wipe(&mut self) {
        self.lines.zeroize();
        self.mark.zeroize();
    }
}

/// The first lines of `text` for a card: trimmed, empty lines skipped, each cut at
/// [`THUMB_LINE_CHARS`] characters (with `…`). At most [`THUMB_LINES`] lines.
pub fn thumb_lines(text: &str) -> String {
    let mut out = String::new();
    for (count, line) in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        if count == THUMB_LINES {
            break;
        }
        if count > 0 {
            out.push('\n');
        }
        let mut chars = line.chars();
        out.extend(chars.by_ref().take(THUMB_LINE_CHARS));
        if chars.next().is_some() {
            out.push('…');
        }
    }
    out
}

/// The strip shows with two or more copies, unless it was turned off with ⌘Y.
pub fn strip_shown(len: usize, turned_off: bool) -> bool {
    len > 1 && !turned_off
}

/// Whether a card is masked: with the privacy filter on every copy is, like the card's well,
/// except the one on screen once it was revealed. Masked cards show their type mark only.
pub fn thumb_masked(privacy_filter: bool, current: bool, revealed: bool) -> bool {
    privacy_filter && !(current && revealed)
}

/// The cards to draw for `len` copies: all, at most [`MAX_THUMBS`], around `cursor` when more.
pub fn strip_range(len: usize, cursor: usize) -> std::ops::Range<usize> {
    if len <= MAX_THUMBS {
        return 0..len;
    }
    let cursor = cursor.min(len - 1);
    let start = cursor.saturating_sub(MAX_THUMBS / 2).min(len - MAX_THUMBS);
    start..start + MAX_THUMBS
}

/// Width of the strip's content for `count` cards.
pub fn strip_content_width(count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    count as f64 * THUMB_W + (count - 1) as f64 * THUMB_GAP
}

/// Left edge of card number `slot` (0-based, in the order drawn).
pub fn thumb_x(slot: usize) -> f64 {
    slot as f64 * (THUMB_W + THUMB_GAP)
}

/// The scroll offset that brings card `slot` into a strip `visible` wide holding `count`
/// cards, with `current` the offset now: unchanged when it already shows fully, else the least
/// move, kept within the content.
pub fn scroll_to_thumb(slot: usize, count: usize, visible: f64, current: f64) -> f64 {
    let left = thumb_x(slot);
    let right = left + THUMB_W;
    let max = (strip_content_width(count) - visible).max(0.0);
    let target = if left < current {
        left
    } else if right > current + visible {
        right - visible
    } else {
        current
    };
    target.clamp(0.0, max)
}

static STRIP_OFF: AtomicBool = AtomicBool::new(false);

/// ⌘Y (or the `⋯` menu item) hid the strip. One choice for all cards, in memory.
pub fn strip_turned_off() -> bool {
    STRIP_OFF.load(Ordering::Relaxed)
}

/// Flip it; returns whether the strip is now turned off.
pub fn toggle_strip() -> bool {
    !STRIP_OFF.fetch_xor(true, Ordering::Relaxed)
}

/// The title of the `⋯` item that turns the strip on or off.
pub fn strip_toggle_title(turned_off: bool) -> &'static str {
    if turned_off {
        crate::locale::t("strip_show")
    } else {
        crate::locale::t("strip_hide")
    }
}

/// What VoiceOver reads for a card: "Item 2 of 5", with the current one named.
pub fn thumb_spoken(position: usize, total: usize, current: bool) -> String {
    let item = crate::locale::tf("item_of", &[&position, &total]);
    if current {
        format!("{item}, {}", crate::locale::t("strip_current"))
    } else {
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_holds_two_short_lines() {
        assert_eq!(thumb_lines("een\n\n  twee  \ndrie"), "een\ntwee");
        assert_eq!(thumb_lines(""), "");
        assert_eq!(thumb_lines("  \n \n"), "");
        let long = thumb_lines("abcdefghijklmnopqrstuvwxyz");
        assert_eq!(long.chars().count(), THUMB_LINE_CHARS + 1);
        assert!(long.ends_with('…'));
        // Characters, not bytes.
        assert_eq!(
            thumb_lines("é".repeat(THUMB_LINE_CHARS).as_str())
                .chars()
                .count(),
            THUMB_LINE_CHARS
        );
    }

    #[test]
    fn the_strip_needs_two_copies_and_can_be_turned_off() {
        assert!(!strip_shown(0, false));
        assert!(!strip_shown(1, false));
        assert!(strip_shown(2, false));
        assert!(!strip_shown(5, true));
    }

    #[test]
    fn cards_are_masked_like_the_well() {
        // Filter on: masked, except the revealed current one.
        assert!(thumb_masked(true, false, false));
        assert!(thumb_masked(true, true, false));
        assert!(!thumb_masked(true, true, true));
        assert!(
            thumb_masked(true, false, true),
            "revealing one card reveals only that one"
        );
        // Filter off: nothing is masked.
        assert!(!thumb_masked(false, false, false));
    }

    #[test]
    fn a_long_history_draws_a_window_around_the_current_copy() {
        assert_eq!(strip_range(0, 0), 0..0);
        assert_eq!(strip_range(20, 7), 0..20);
        assert_eq!(strip_range(100, 0), 0..MAX_THUMBS);
        assert_eq!(strip_range(100, 99), 70..100);
        let middle = strip_range(100, 50);
        assert_eq!(middle.len(), MAX_THUMBS);
        assert!(middle.contains(&50));
        assert!(strip_range(100, 500).contains(&99));
    }

    #[test]
    fn the_strip_scrolls_just_enough_to_show_the_current_card() {
        let visible = 400.0;
        let count = 20;
        // Already in view: no move.
        assert_eq!(scroll_to_thumb(1, count, visible, 0.0), 0.0);
        // Past the right edge: its right edge meets the strip's.
        let at = scroll_to_thumb(10, count, visible, 0.0);
        assert_eq!(at, thumb_x(10) + THUMB_W - visible);
        // Before the left edge: its left edge.
        assert_eq!(scroll_to_thumb(2, count, visible, 500.0), thumb_x(2));
        // Never past the content, never negative.
        let last = scroll_to_thumb(19, count, visible, 0.0);
        assert_eq!(last, strip_content_width(count) - visible);
        assert_eq!(scroll_to_thumb(0, 2, 800.0, 30.0), 0.0);
        assert_eq!(strip_content_width(0), 0.0);
    }
}
