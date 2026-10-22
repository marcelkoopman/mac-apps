//! The history strip: small cards of the copies in history along the bottom of the card, the
//! one on screen marked. Pure logic (what shows, which one is current, what text a card has);
//! `macos_launcher/strip.rs` draws it. Clicking a card only shows that copy, it never copies.

use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroize;

/// One card of the strip: [`THUMB_W`] × [`THUMB_H`] points.
pub const THUMB_W: f64 = 138.0;
pub const THUMB_H: f64 = 78.0;
/// Room above and below a card inside the strip row.
pub const THUMB_MARGIN: f64 = 2.0;
/// Height of the strip row: a card and the room around it.
pub const STRIP_H: f64 = THUMB_H + 2.0 * THUMB_MARGIN;
pub const THUMB_GAP: f64 = 7.0;
/// Space between a card's edge and its text or picture.
pub const THUMB_PAD: f64 = 6.0;
/// The ✕ in a card's top-right corner: its size and its distance to the card's edges.
pub const THUMB_REMOVE_SIZE: f64 = 18.0;
pub const THUMB_REMOVE_INSET: f64 = 5.0;
/// Text on a card: at most this many lines, each at most [`THUMB_LINE_CHARS`] characters.
pub const THUMB_LINES: usize = 3;
pub const THUMB_LINE_CHARS: usize = 20;
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
    /// The first few short lines of the text ([`THUMB_LINES`]). Empty for a picture.
    pub lines: String,
    /// A small picture's bytes (shared, not copied), when the copy is a picture that is light
    /// enough ([`THUMB_PICTURE_MAX_BYTES`]).
    pub picture: Option<crate::clipboard::SecretBytes>,
    /// `A` (before) or `B` (after) while this copy is one side of the diff, else `None`.
    pub side: Option<&'static str>,
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

/// The card (slot, in the order drawn) at `x` in a strip of `count` cards; `None` in a gap or
/// past the end.
pub fn thumb_at(x: f64, count: usize) -> Option<usize> {
    if x < 0.0 {
        return None;
    }
    let slot = (x / (THUMB_W + THUMB_GAP)) as usize;
    (slot < count && x - thumb_x(slot) <= THUMB_W).then_some(slot)
}

/// The ✕'s frame inside a card, `(x, y, side)` from the card's bottom-left corner (AppKit
/// counts y from the bottom): the top-right corner.
pub fn remove_frame() -> (f64, f64, f64) {
    (
        THUMB_W - THUMB_REMOVE_SIZE - THUMB_REMOVE_INSET,
        THUMB_H - THUMB_REMOVE_SIZE - THUMB_REMOVE_INSET,
        THUMB_REMOVE_SIZE,
    )
}

/// What a click on strip card `index` does: show that copy, or with ⌘ or ⇧ held compare it
/// with the copy on screen ([`super::CommandId::StripCompare`]).
pub fn thumb_click(index: usize, command: bool, shift: bool) -> super::CommandId {
    if command || shift {
        super::CommandId::StripCompare(index)
    } else {
        super::CommandId::HistoryView(index)
    }
}

/// The type mark of a card, with the diff side in front while it is compared (`A · {}`).
pub fn thumb_mark(thumb: &Thumb) -> String {
    match thumb.side {
        Some(side) => format!("{side} · {}", thumb.mark),
        None => thumb.mark.clone(),
    }
}

/// Height of the one-line hint under the strip (⌘-click a card to compare).
pub const STRIP_HINT_H: f64 = 16.0;

/// The hint row under the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripHint {
    /// No row: no strip, or compare was used before this card opened.
    #[default]
    None,
    /// The row with the hint.
    Shown,
    /// Compare was just used: the row stays empty until the card closes, so the card keeps
    /// its height.
    Blank,
}

impl StripHint {
    /// The card keeps a row for it.
    pub fn has_row(self) -> bool {
        self != Self::None
    }
}

/// The hint under the strip: while the strip shows, until ⌘- or ⇧-click compare was used once.
/// `row_this_card`: compare had not been used when the card opened; `used`: it has been now.
pub fn strip_hint(strip_shown: bool, row_this_card: bool, used: bool) -> StripHint {
    match (strip_shown && row_this_card, used) {
        (false, _) => StripHint::None,
        (true, false) => StripHint::Shown,
        (true, true) => StripHint::Blank,
    }
}

/// Some card of the strip is one side of a diff.
pub fn strip_comparing(items: &[Thumb]) -> bool {
    items.iter().any(|thumb| thumb.side.is_some())
}

/// What Esc on the card does, in order: empty the in-item field, close the search, end a
/// comparison of two strip cards, close the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeStep {
    ClearQuery,
    CloseSearch,
    EndCompare,
    Close,
}

pub fn escape_step(query_empty: bool, searching: bool, comparing: bool) -> EscapeStep {
    if !query_empty {
        EscapeStep::ClearQuery
    } else if searching {
        EscapeStep::CloseSearch
    } else if comparing {
        EscapeStep::EndCompare
    } else {
        EscapeStep::Close
    }
}

/// A card's ✕ shows on the marked card (history position `cursor`) and on the one under the
/// pointer.
pub fn remove_shown(index: usize, cursor: usize, hovered: Option<usize>) -> bool {
    index == cursor || hovered == Some(index)
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
    #[test]
    fn the_compare_hint_shows_with_the_strip_until_compare_was_used() {
        use super::{StripHint, strip_hint};
        assert_eq!(strip_hint(true, true, false), StripHint::Shown);
        // Used in this card: the row stays, empty, so the card does not shrink under the pointer.
        assert_eq!(strip_hint(true, true, true), StripHint::Blank);
        assert!(StripHint::Blank.has_row());
        // Used before this card opened: no row at all.
        assert_eq!(strip_hint(true, false, true), StripHint::None);
        // No strip, no hint.
        assert_eq!(strip_hint(false, true, false), StripHint::None);
        assert!(!StripHint::None.has_row());
    }

    use super::*;

    #[test]
    fn a_card_holds_three_short_lines() {
        assert_eq!(thumb_lines("een\n\n  twee  \ndrie"), "een\ntwee\ndrie");
        assert_eq!(thumb_lines("1\n2\n3\n4\n5\n6"), "1\n2\n3");
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

    #[test]
    fn the_pointer_finds_the_card_under_it() {
        assert_eq!(thumb_at(0.0, 3), Some(0));
        assert_eq!(thumb_at(THUMB_W, 3), Some(0));
        // In the gap between two cards: none.
        assert_eq!(thumb_at(THUMB_W + THUMB_GAP / 2.0, 3), None);
        assert_eq!(thumb_at(thumb_x(1) + 1.0, 3), Some(1));
        assert_eq!(thumb_at(thumb_x(2) + 1.0, 3), Some(2));
        // Past the last card, or left of the first.
        assert_eq!(thumb_at(thumb_x(3) + 1.0, 3), None);
        assert_eq!(thumb_at(-1.0, 3), None);
        assert_eq!(thumb_at(1.0, 0), None);
    }

    #[test]
    fn cards_are_one_and_a_half_times_the_old_size_and_the_strip_holds_them() {
        assert_eq!((THUMB_W, THUMB_H), (1.5 * 92.0, 1.5 * 52.0));
        assert_eq!(STRIP_H, 82.0);
        assert_eq!(STRIP_H, THUMB_H + 2.0 * THUMB_MARGIN);
        // Cards that show whole in the strip of a compact, normal and wide card (inner width:
        // card width minus twice the 16 pt margin).
        let fitting = |inner: f64| {
            (1..=MAX_THUMBS)
                .take_while(|n| strip_content_width(*n) <= inner)
                .count()
        };
        assert_eq!(fitting(440.0 - 32.0), 2);
        assert_eq!(fitting(640.0 - 32.0), 4);
        assert_eq!(fitting(800.0 - 32.0), 5);
    }

    #[test]
    fn the_remove_cross_sits_in_the_top_right_corner_inside_the_card() {
        let (x, y, side) = remove_frame();
        assert_eq!(x + side + THUMB_REMOVE_INSET, THUMB_W);
        assert_eq!(y + side + THUMB_REMOVE_INSET, THUMB_H);
        assert!(x > THUMB_W / 2.0 && y > THUMB_H / 2.0);
        // The pointer over the ✕ finds its card, in every slot.
        for slot in 0..3 {
            assert_eq!(thumb_at(thumb_x(slot) + x + side / 2.0, 3), Some(slot));
        }
    }

    fn thumb(index: usize, side: Option<&'static str>) -> Thumb {
        Thumb {
            index,
            mark: "{}".to_string(),
            lines: String::new(),
            picture: None,
            side,
        }
    }

    #[test]
    fn command_or_shift_click_compares_a_plain_click_shows() {
        use crate::commands::CommandId;
        assert_eq!(thumb_click(2, false, false), CommandId::HistoryView(2));
        assert_eq!(thumb_click(2, true, false), CommandId::StripCompare(2));
        assert_eq!(thumb_click(2, false, true), CommandId::StripCompare(2));
        assert_eq!(thumb_click(0, true, true), CommandId::StripCompare(0));
    }

    #[test]
    fn compared_cards_carry_their_side_and_esc_ends_the_comparison_first() {
        assert_eq!(thumb_mark(&thumb(0, None)), "{}");
        assert_eq!(thumb_mark(&thumb(0, Some("A"))), "A · {}");
        assert!(!strip_comparing(&[thumb(0, None), thumb(1, None)]));
        assert!(strip_comparing(&[thumb(0, Some("B")), thumb(1, Some("A"))]));
        assert_eq!(escape_step(false, true, true), EscapeStep::ClearQuery);
        assert_eq!(escape_step(true, true, true), EscapeStep::CloseSearch);
        assert_eq!(escape_step(true, false, true), EscapeStep::EndCompare);
        assert_eq!(escape_step(true, false, false), EscapeStep::Close);
    }

    #[test]
    fn the_remove_cross_shows_on_the_marked_and_the_hovered_card() {
        assert!(remove_shown(1, 1, None));
        assert!(remove_shown(2, 1, Some(2)));
        assert!(!remove_shown(0, 1, Some(2)));
        assert!(!remove_shown(0, 1, None));
    }
}
