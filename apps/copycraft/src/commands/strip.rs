//! The history strip: small cards of the copies in history along the bottom of the card, the
//! one on screen marked. Pure logic (what shows, which one is current, what text a card has);
//! `macos_launcher/strip.rs` draws it. Clicking a card only shows that copy, it never copies.

use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroize;

use super::{Command, CommandId, command, diff_action};

/// One card of the strip: [`THUMB_W`] × [`THUMB_H`] points.
pub const THUMB_W: f64 = 138.0;
pub const THUMB_H: f64 = 78.0;
/// Room above and below a card inside the strip row.
pub const THUMB_MARGIN: f64 = 2.0;
/// Height of the strip row: a card and the room around it.
pub const STRIP_H: f64 = THUMB_H + 2.0 * THUMB_MARGIN;
pub const THUMB_GAP: f64 = 7.0;
/// The fade on the strip's right edge while more cards follow (collections HIG: overflow
/// shows), in points.
pub const STRIP_FADE: f64 = 12.0;
/// Space between a card's edge and its text or picture: inside the card's one button, which
/// covers the whole card, so a click and the pointer's hover use the same area.
pub const THUMB_PAD: f64 = 6.0;
/// The ✕ in a card's top-right corner: its size and its distance to the card's edges.
/// At least 20 pt, so it is easy to hit.
pub const THUMB_REMOVE_SIZE: f64 = 20.0;
pub const THUMB_REMOVE_INSET: f64 = 4.0;
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
    /// The history entry's id: the strip keeps a card's views by it, so a removal moves the
    /// other cards instead of building them again.
    pub id: Option<u64>,
    /// The type mark (`{}`, `md`, `img`); for a link the word VoiceOver reads (*Link*), as
    /// its card draws [`Thumb::symbol`] instead.
    pub mark: String,
    /// A link's SF Symbol (`globe`, as on the Link card), drawn in place of the mark, with the
    /// domain next to it ([`link_thumb`]). `None` for other copies.
    pub symbol: Option<&'static str>,
    /// The copy is pinned: the card wears the pin ([`pin_frame`], [`PIN_SYMBOL`]).
    pub pinned: bool,
    /// The first few short lines of the text ([`THUMB_LINES`]). Empty for a picture.
    pub lines: String,
    /// A small picture's bytes (shared, not copied), when the copy is a picture that is light
    /// enough ([`THUMB_PICTURE_MAX_BYTES`]).
    pub picture: Option<crate::clipboard::SecretBytes>,
    /// `A` (before) or `B` (after) while this copy is one side of the diff, else `None`.
    pub side: Option<&'static str>,
    /// The type's name for the tooltip ([`thumb_kind_name`]): no content.
    pub kind: &'static str,
    /// When it was copied, for the tooltip's age.
    pub copied: Option<std::time::Instant>,
    /// Its right-click (⌃-click) menu, [`thumb_menu`]. No content in the titles.
    pub menu: Vec<Command>,
}

impl Thumb {
    pub fn wipe(&mut self) {
        self.lines.zeroize();
        self.mark.zeroize();
    }
}

/// The right-click (⌃-click) menu of strip card `index`: Pin or Unpin, the diff action of
/// that copy (`id`, dimmed for a picture, as in `⋯` › History), a separator, Remove from
/// History ([`separator_before`]). `⋯` and the ✕ keep the same actions.
pub fn thumb_menu(
    index: usize,
    id: Option<u64>,
    pinned: bool,
    is_text: bool,
    picks: &[u64],
) -> Vec<Command> {
    let pin = if pinned {
        command(
            CommandId::StripPin(index),
            crate::locale::t("unpin"),
            crate::locale::t("detail_unpin"),
            "",
        )
    } else {
        command(
            CommandId::StripPin(index),
            crate::locale::t("pin"),
            crate::locale::t("detail_pin"),
            "",
        )
    };
    let mut menu = vec![pin];
    if let Some(id) = id {
        menu.push(diff_action(CommandId::DiffToggle(id), picks, id, is_text));
    }
    menu.push(command(
        CommandId::RemoveHistory(index),
        crate::locale::t("history_remove_item"),
        crate::locale::t("detail_remove_history"),
        "",
    ));
    menu
}

/// The tag of row `slot` of strip card `index`'s menu, and back ([`thumb_menu_pick`]).
pub fn thumb_menu_tag(index: usize, slot: usize) -> isize {
    (index * THUMB_MENU_ROWS + slot.min(THUMB_MENU_ROWS - 1)) as isize
}

/// Strip card and menu row of a tag made by [`thumb_menu_tag`]; `None` for a negative tag.
pub fn thumb_menu_pick(tag: isize) -> Option<(usize, usize)> {
    let tag = usize::try_from(tag).ok()?;
    Some((tag / THUMB_MENU_ROWS, tag % THUMB_MENU_ROWS))
}

const THUMB_MENU_ROWS: usize = 4;

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
        out.push_str(&cut_line(line));
    }
    out
}

/// A link's small card: the Link card's symbol for its place (`globe`, `network`, `doc`) and
/// its lines, the domain first, then the known site or the path (cut in the middle). `None`
/// when `text` is not one URL ([`crate::link_card::parse`]). Never the full URL, which the
/// card cuts after a few characters (`https://www.youtube…`).
pub fn link_thumb(text: &str) -> Option<(&'static str, String)> {
    let link = crate::link_card::parse(text)?;
    let mut lines = cut_line(&link.domain());
    // `localhost · poort 3000` says again what the domain line says.
    let site = link
        .site
        .as_ref()
        .filter(|_| link.place == crate::link_card::Place::Web);
    let second = match site {
        Some(site) => Some(cut_line(site)),
        None if !link.path.is_empty() && link.path != "/" => {
            Some(crate::link_card::middle_cut(&link.path, THUMB_LINE_CHARS))
        }
        None => None,
    };
    if let Some(second) = second {
        lines.push('\n');
        lines.push_str(&second);
    }
    // A YouTube video ID on its own line: it tells two video tiles apart.
    if let Some(id) = link.video_id.as_ref().filter(|_| site.is_some()) {
        lines.push('\n');
        lines.push_str(&cut_line(id));
    }
    Some((link.place.symbol(), lines))
}

/// `line` cut at [`THUMB_LINE_CHARS`] characters, with `…`.
fn cut_line(line: &str) -> String {
    let mut chars = line.chars();
    let mut out: String = chars.by_ref().take(THUMB_LINE_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// The strip shows with two or more copies, unless it was turned off with ⌘Y.
pub fn strip_shown(len: usize, turned_off: bool) -> bool {
    len > 1 && !turned_off
}

/// Whether the strip keeps its drawn picture for the picture with allocation `id`: only while
/// one of its cards (`items`) shows that picture. A picture taken out of history (a token
/// found in it late, Delete, Wipe) loses its drawn tile with the next strip. Pure.
pub fn thumb_kept(id: usize, items: &[Thumb]) -> bool {
    items.iter().any(|thumb| {
        thumb
            .picture
            .as_ref()
            .is_some_and(|picture| picture.allocation_id() == id)
    })
}

/// The type's name on a card's tooltip: *JSON*, *Afbeelding*, *Tekst*.
pub fn thumb_kind_name(kind: crate::format::FormatKind) -> &'static str {
    use crate::format::FormatKind;
    match kind {
        FormatKind::Text | FormatKind::Plain => crate::locale::t("strip_kind_text"),
        other => other.source_heading(),
    }
}

/// A card's tooltip: its type and how long ago it was copied, never its content.
pub fn thumb_tooltip(kind: &str, age: Option<std::time::Duration>) -> String {
    thumb_tooltip_in(crate::locale::lang(), kind, age)
}

fn thumb_tooltip_in(
    lang: crate::locale::Lang,
    kind: &str,
    age: Option<std::time::Duration>,
) -> String {
    use crate::locale::{t_in, tf_in};
    let Some(age) = age else {
        return kind.to_string();
    };
    let minutes = age.as_secs() / 60;
    let ago = if minutes == 0 {
        t_in(lang, "strip_age_now").to_string()
    } else if minutes < 60 {
        tf_in(lang, "strip_age_minutes", &[&minutes])
    } else {
        tf_in(lang, "strip_age_hours", &[&(minutes / 60)])
    };
    format!("{kind} · {ago}")
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

/// The card (slot, in the order drawn) at `(x, y)` in a strip of `count` cards, `y` from the
/// strip's bottom; `None` in a gap, above or below the cards, or past the end. The same area
/// as the card's button, the whole [`THUMB_W`] × [`THUMB_H`] card.
pub fn thumb_at(x: f64, y: f64, count: usize) -> Option<usize> {
    if x < 0.0 || y < THUMB_MARGIN || y > THUMB_MARGIN + THUMB_H {
        return None;
    }
    let slot = (x / (THUMB_W + THUMB_GAP)) as usize;
    (slot < count && x - thumb_x(slot) <= THUMB_W).then_some(slot)
}

/// The card (slot) that gets the focus after the card in `slot` was removed, with `count`
/// cards left: the one that took its place (its right neighbour), else the last one (its left
/// neighbour); `None` when none are left.
pub fn focus_after_remove(slot: usize, count: usize) -> Option<usize> {
    count.checked_sub(1).map(|last| slot.min(last))
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

/// What a click on strip card `index` does: show that copy, or with ⌘ held compare it with
/// the copy on screen ([`super::CommandId::StripCompare`]; on a card already compared it takes
/// that card out of the diff). ⇧ is a plain click.
pub fn thumb_click(index: usize, command: bool) -> super::CommandId {
    if command {
        super::CommandId::StripCompare(index)
    } else {
        super::CommandId::HistoryView(index)
    }
}

/// The type mark VoiceOver reads for a card, with "pinned" after it for a pinned copy and the
/// diff side while it is compared (`{}, Pinned, compared, side A`). On screen both are marks
/// on the card ([`pin_frame`], [`side_badge_frame`]).
pub fn thumb_mark(thumb: &Thumb) -> String {
    let mut spoken = thumb.mark.clone();
    if thumb.pinned {
        spoken = format!("{spoken}, {}", pin_label());
    }
    if let Some(side) = thumb.side {
        spoken = format!(
            "{spoken}, {}",
            crate::locale::tf("strip_compare_side", &[&side])
        );
    }
    spoken
}

/// The pin of a pinned copy: one SF Symbol and one label, shared by the strip's cards, the
/// history menus' items and the search chips; see `crate::macos_pin`.
pub const PIN_SYMBOL: &str = "pin.fill";

/// What the pin says: "Vastgezet" / "Pinned" (VoiceOver, the symbol's description).
pub fn pin_label() -> &'static str {
    crate::locale::t("pinned")
}

/// VoiceOver's name for a pinned history row titled `title`: "…, Vastgezet" / "…, Pinned".
pub fn pinned_label(title: &str) -> String {
    format!("{title}, {}", pin_label())
}

/// The pin on a pinned card: its round backing's side and its distance to the card's edges.
pub const THUMB_PIN_SIZE: f64 = 16.0;
pub const THUMB_PIN_INSET: f64 = 4.0;

/// The pin's frame inside a card, `(x, y, side)` from the card's bottom-left corner: the
/// bottom-right corner, clear of the ✕ (top right) and the side badge (top left).
pub fn pin_frame() -> (f64, f64, f64) {
    (
        THUMB_W - THUMB_PIN_SIZE - THUMB_PIN_INSET,
        THUMB_PIN_INSET,
        THUMB_PIN_SIZE,
    )
}

/// How far a card's text stops before the right edge: the padding, and clear of the pin on
/// a pinned card.
pub fn text_tail(pinned: bool) -> f64 {
    if pinned {
        THUMB_PIN_SIZE + THUMB_PIN_INSET + 2.0
    } else {
        THUMB_PAD
    }
}

/// The `A` / `B` badge of a compared card: its size and its distance to the card's edges.
pub const THUMB_BADGE_W: f64 = 16.0;
pub const THUMB_BADGE_H: f64 = 14.0;
pub const THUMB_BADGE_INSET: f64 = 4.0;

/// The badge's frame inside a card, `(x, y, width, height)` from the card's bottom-left
/// corner: the top-left corner, clear of the ✕ in the top-right one.
pub fn side_badge_frame() -> (f64, f64, f64, f64) {
    (
        THUMB_BADGE_INSET,
        THUMB_H - THUMB_BADGE_H - THUMB_BADGE_INSET,
        THUMB_BADGE_W,
        THUMB_BADGE_H,
    )
}

/// How far a card's first line (the type mark) is indented: the padding, and past the badge
/// while the card is compared.
pub fn mark_indent(compared: bool) -> f64 {
    if compared {
        THUMB_BADGE_INSET + THUMB_BADGE_W + 3.0
    } else {
        THUMB_PAD
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

/// The hint under the strip: while the strip shows, until ⌘-click compare was used once.
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

/// Whether the strip's right edge fades ([`STRIP_FADE`]): more cards follow past it. Scrolled
/// `offset` into content `content` wide, `visible` wide; off at the end of the row.
pub fn strip_fades(offset: f64, visible: f64, content: f64) -> bool {
    offset + visible < content - 0.5
}

/// The scroll offset that brings card `slot` into a strip `visible` wide holding `count`
/// cards, with `current` the offset now: unchanged when it already shows fully (clear of the
/// right edge's fade while more follow), else the least move, kept within the content.
pub fn scroll_to_thumb(slot: usize, count: usize, visible: f64, current: f64) -> f64 {
    let left = thumb_x(slot);
    let right = left + THUMB_W + if slot + 1 < count { STRIP_FADE } else { 0.0 };
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
    fn the_pin_sits_bottom_right_clear_of_the_cross_the_badge_and_the_text() {
        let (x, y, side) = pin_frame();
        assert_eq!((x + side + THUMB_PIN_INSET, y), (THUMB_W, THUMB_PIN_INSET));
        let (rx, ry, _) = remove_frame();
        assert!(y + side < ry && x + side <= THUMB_W);
        let (_, by, bw, _) = side_badge_frame();
        assert!(y + side < by && x > bw);
        assert!(rx <= THUMB_W);
        // The text of a pinned card stops before the pin.
        assert!(THUMB_W - text_tail(true) < x);
        assert_eq!(text_tail(false), THUMB_PAD);
        assert_eq!(PIN_SYMBOL, "pin.fill");
    }

    #[test]
    fn the_right_edge_fades_until_the_end_of_the_row() {
        let content = strip_content_width(8);
        assert!(strip_fades(0.0, 408.0, content));
        assert!(strip_fades(content - 409.0, 408.0, content));
        assert!(!strip_fades(content - 408.0, 408.0, content));
        // All cards fit: no fade.
        assert!(!strip_fades(0.0, 408.0, strip_content_width(2)));
    }

    #[test]
    fn the_strip_scrolls_just_enough_to_show_the_current_card() {
        let visible = 400.0;
        let count = 20;
        // Already in view: no move.
        assert_eq!(scroll_to_thumb(1, count, visible, 0.0), 0.0);
        // Past the right edge: its right edge meets the strip's.
        let at = scroll_to_thumb(10, count, visible, 0.0);
        assert_eq!(at, thumb_x(10) + THUMB_W + STRIP_FADE - visible);
        // A card under the fade is not in view yet.
        let under = thumb_x(2) + THUMB_W + STRIP_FADE / 2.0 - visible;
        assert_eq!(
            scroll_to_thumb(2, count, visible, under),
            thumb_x(2) + THUMB_W + STRIP_FADE - visible
        );
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
        const MID: f64 = STRIP_H / 2.0;
        assert_eq!(thumb_at(0.0, MID, 3), Some(0));
        assert_eq!(thumb_at(THUMB_W, MID, 3), Some(0));
        // In the gap between two cards: none.
        assert_eq!(thumb_at(THUMB_W + THUMB_GAP / 2.0, MID, 3), None);
        assert_eq!(thumb_at(thumb_x(1) + 1.0, MID, 3), Some(1));
        assert_eq!(thumb_at(thumb_x(2) + 1.0, MID, 3), Some(2));
        // Past the last card, or left of the first.
        assert_eq!(thumb_at(thumb_x(3) + 1.0, MID, 3), None);
        assert_eq!(thumb_at(-1.0, MID, 3), None);
        assert_eq!(thumb_at(1.0, MID, 0), None);
        // The whole card counts, corners included (the button covers it); the margin does not.
        assert_eq!(thumb_at(0.5, THUMB_MARGIN + 0.5, 3), Some(0));
        assert_eq!(thumb_at(0.5, THUMB_MARGIN + THUMB_H - 0.5, 3), Some(0));
        assert_eq!(thumb_at(1.0, THUMB_MARGIN / 2.0, 3), None);
        assert_eq!(thumb_at(1.0, STRIP_H, 3), None);
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
            assert_eq!(
                thumb_at(
                    thumb_x(slot) + x + side / 2.0,
                    THUMB_MARGIN + y + side / 2.0,
                    3
                ),
                Some(slot)
            );
        }
    }

    #[test]
    fn a_link_card_shows_the_symbol_and_the_domain_not_the_url() {
        use super::link_thumb;
        let (symbol, lines) =
            link_thumb("https://www.youtube.com/watch?v=dQw4w9WgXcQ").expect("a link");
        assert_eq!(symbol, "globe");
        assert!(lines.starts_with("youtube.com\n"), "{lines}");
        assert!(!lines.contains("://") && !lines.contains("www."), "{lines}");
        // The video ID on its own line, after the site.
        assert_eq!(lines.lines().nth(2), Some("dQw4w9WgXcQ"), "{lines}");
        assert!(lines.lines().count() <= super::THUMB_LINES);
        // Facebook keeps its ID hidden.
        let (_, lines) =
            link_thumb("https://www.facebook.com/reel/1092344577040057").expect("a link");
        assert_eq!(lines.lines().next(), Some("facebook.com"));
        assert!(!lines.contains("1092344577040057"), "{lines}");
        assert!(lines.lines().count() <= super::THUMB_LINES);
        assert!(
            lines
                .lines()
                .all(|line| line.chars().count() <= super::THUMB_LINE_CHARS + 1)
        );
        let (symbol, lines) = link_thumb("http://localhost:3000/").expect("a link");
        assert_eq!(symbol, "network");
        assert_eq!(lines, "localhost:3000");
        assert_eq!(link_thumb("two words"), None);
    }

    fn thumb(index: usize, side: Option<&'static str>) -> Thumb {
        Thumb {
            index,
            id: Some(index as u64),
            mark: "{}".to_string(),
            symbol: None,
            pinned: false,
            lines: String::new(),
            picture: None,
            side,
            kind: "JSON",
            copied: None,
            menu: Vec::new(),
        }
    }

    #[test]
    fn the_tooltip_names_the_type_and_age_and_no_content() {
        use crate::locale::Lang::{En, Nl};
        use std::time::Duration;
        assert_eq!(thumb_tooltip_in(En, "JSON", None), "JSON");
        assert_eq!(
            thumb_tooltip_in(En, "JSON", Some(Duration::from_secs(20))),
            "JSON · copied just now"
        );
        assert_eq!(
            thumb_tooltip_in(En, "Text", Some(Duration::from_secs(150))),
            "Text · copied 2 min ago"
        );
        assert_eq!(
            thumb_tooltip_in(En, "Image", Some(Duration::from_secs(7300))),
            "Image · copied 2 h ago"
        );
        assert_eq!(
            thumb_tooltip_in(Nl, "Tekst", Some(Duration::from_secs(150))),
            "Tekst · 2 min geleden gekopieerd"
        );
        assert_eq!(
            thumb_kind_name(crate::format::FormatKind::Plain),
            crate::locale::t("strip_kind_text")
        );
    }

    #[test]
    fn after_a_removal_the_right_neighbour_gets_the_focus_else_the_left() {
        // Card 1 of 4 removed: 3 left, the old card 2 now sits in slot 1.
        assert_eq!(focus_after_remove(1, 3), Some(1));
        // The last card removed: its left neighbour.
        assert_eq!(focus_after_remove(3, 3), Some(2));
        assert_eq!(focus_after_remove(0, 1), Some(0));
        assert_eq!(focus_after_remove(0, 0), None);
    }

    #[test]
    fn the_side_badge_sits_top_left_clear_of_the_cross_and_the_mark() {
        let (x, y, w, h) = side_badge_frame();
        let (cross_x, _, _) = remove_frame();
        assert!(x >= 0.0 && y >= 0.0 && x + w < cross_x && y + h <= THUMB_H);
        assert!(y > THUMB_H / 2.0, "top half");
        // The mark line starts past the badge only while compared.
        assert!(mark_indent(true) > x + w);
        assert_eq!(mark_indent(false), THUMB_PAD);
    }

    #[test]
    fn the_remove_cross_is_at_least_20_points() {
        let (_, _, side) = remove_frame();
        assert!(side >= 20.0);
    }

    #[test]
    fn only_command_click_compares_a_plain_or_shift_click_shows() {
        use crate::commands::CommandId;
        assert_eq!(thumb_click(2, false), CommandId::HistoryView(2));
        assert_eq!(thumb_click(2, true), CommandId::StripCompare(2));
    }

    #[test]
    fn compared_cards_carry_their_side_and_esc_ends_the_comparison_first() {
        assert_eq!(thumb_mark(&thumb(0, None)), "{}");
        assert_eq!(
            thumb_mark(&thumb(0, Some("A"))),
            format!("{{}}, {}", crate::locale::tf("strip_compare_side", &[&"A"]))
        );
        let pinned = Thumb {
            pinned: true,
            ..thumb(0, Some("B"))
        };
        assert_eq!(
            thumb_mark(&pinned),
            format!(
                "{{}}, {}, {}",
                pin_label(),
                crate::locale::tf("strip_compare_side", &[&"B"])
            )
        );
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
