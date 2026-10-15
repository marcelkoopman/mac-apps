//! The card's vertical layout: which rows it has and where each one goes. Plain arithmetic, so
//! it is tested on Linux; `macos_launcher` places the views.

use crate::commands::{self, ChipFrame};

pub const PAD: f64 = mac_ui::metrics::MARGIN;
pub const HEADER_H: f64 = 32.0;
pub const PREVIEW_H: f64 = 264.0;
pub const META_H: f64 = 22.0;
pub const SEARCH_H: f64 = 36.0;
/// Fixed in-item field under the header. The field stays hidden until the well is revealed.
/// The card still keeps this row, so blurring and revealing do not resize the window.
pub const ITEM_FIND_H: f64 = mac_ui::metrics::BUTTON_H;
pub const GAP: f64 = mac_ui::metrics::BUTTON_GAP;

/// The card's height and the bottom of each row (AppKit counts y from the bottom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sections {
    pub height: f64,
    pub header_y: f64,
    pub find_y: f64,
    pub preview_y: f64,
    pub meta_y: f64,
    pub search_y: f64,
    pub chips_y: f64,
    pub chips_h: f64,
    /// Bottom of the history strip row (the card's last row); its height is 0 without a strip.
    pub strip_y: f64,
    pub strip_h: f64,
}

/// Height of the meta row. One line is [`META_H`]. A newline in the text does not add a row:
/// the footer stays one line and truncates in the middle. Empty meta takes no row.
pub fn meta_block_height(meta: &str) -> f64 {
    if meta.is_empty() { 0.0 } else { META_H }
}

/// Rows from the top: header, in-item find (`item_find`), the well, the meta line (when `meta`
/// is not empty), the search field (`searching`) and the chips. `show_capsule` (the history or
/// the table's version capsule) keeps a chip row even without chips.
pub fn place_sections(
    meta: &str,
    searching: bool,
    frames: &[ChipFrame],
    show_empty: bool,
    show_capsule: bool,
    item_find: bool,
    strip: bool,
) -> Sections {
    let meta_h = meta_block_height(meta);
    let search_h = if searching { SEARCH_H } else { 0.0 };
    let find_h = if item_find { ITEM_FIND_H } else { 0.0 };
    let gap_after_find = if find_h > 0.0 { GAP } else { 0.0 };
    let mut chips_h = if show_empty {
        28.0
    } else {
        commands::chips_height(frames)
    };
    if show_capsule {
        chips_h = chips_h.max(commands::CHIP_PITCH);
    }
    let strip_h = if strip { commands::STRIP_H } else { 0.0 };
    let below_preview = meta_h > 0.0 || search_h > 0.0 || chips_h > 0.0 || strip_h > 0.0;
    let gap_after_preview = if below_preview { GAP } else { 0.0 };
    let gap_after_meta = if meta_h > 0.0 && (search_h > 0.0 || chips_h > 0.0 || strip_h > 0.0) {
        GAP
    } else {
        0.0
    };
    let gap_after_search = if search_h > 0.0 && (chips_h > 0.0 || strip_h > 0.0) {
        GAP
    } else {
        0.0
    };
    let gap_after_chips = if chips_h > 0.0 && strip_h > 0.0 {
        GAP
    } else {
        0.0
    };
    let height = PAD
        + HEADER_H
        + GAP
        + find_h
        + gap_after_find
        + PREVIEW_H
        + gap_after_preview
        + meta_h
        + gap_after_meta
        + search_h
        + gap_after_search
        + chips_h
        + gap_after_chips
        + strip_h
        + PAD;
    let mut cursor = height - PAD;
    cursor -= HEADER_H;
    let header_y = cursor;
    cursor -= GAP + find_h;
    let find_y = cursor;
    cursor -= gap_after_find + PREVIEW_H;
    let preview_y = cursor;
    cursor -= gap_after_preview + meta_h;
    let meta_y = cursor;
    cursor -= gap_after_meta + search_h;
    let search_y = cursor;
    cursor -= gap_after_search + chips_h;
    let chips_y = cursor;
    cursor -= gap_after_chips + strip_h;
    let strip_y = cursor;
    Sections {
        height,
        header_y,
        find_y,
        preview_y,
        meta_y,
        search_y,
        chips_y,
        chips_h,
        strip_y,
        strip_h,
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP, META_H, PAD, place_sections};
    use crate::commands::{
        CHIP_GAP, CHIP_PITCH, ChipFrame, VERSION_SPAN, layout_chips, trailing_reserve,
        version_capsule_x,
    };

    fn one_row() -> Vec<ChipFrame> {
        layout_chips(&[80.0], 400.0, 0.0)
    }

    #[test]
    fn the_version_capsule_shares_the_chip_row_so_the_card_does_not_grow() {
        let plain = place_sections("2 lines", false, &one_row(), false, false, false, false);
        let capsule = place_sections("2 lines", false, &one_row(), false, true, false, false);
        assert_eq!(capsule, plain);
        assert_eq!(plain.chips_y, PAD);
        assert_eq!(plain.meta_y, plain.chips_y + plain.chips_h + GAP);
        assert_eq!(plain.preview_y, plain.meta_y + META_H + GAP);
    }

    #[test]
    fn a_newline_in_the_meta_stays_on_one_line() {
        let one = place_sections(
            "photo.heic  ·  HEIC  4032×3024",
            false,
            &one_row(),
            false,
            false,
            false,
            false,
        );
        let two = place_sections(
            "photo…ic\nHEIC  4032×3024",
            false,
            &one_row(),
            false,
            false,
            false,
            false,
        );
        assert_eq!(one.preview_y, one.meta_y + META_H + GAP);
        assert_eq!(two.height, one.height);
        assert_eq!(two.meta_y, one.meta_y);
        assert_eq!(two.preview_y, one.preview_y);
    }

    #[test]
    fn an_empty_card_has_no_gaps_below_the_well() {
        let placed = place_sections("", false, &[], false, false, false, false);
        assert_eq!(placed.preview_y, PAD);
        let capsule = place_sections("", false, &[], false, true, false, false);
        assert_eq!(capsule.chips_h, CHIP_PITCH);
    }

    #[test]
    fn the_version_capsule_ends_the_chip_row_and_chips_keep_clear() {
        let inner = 412.0;
        assert_eq!(version_capsule_x(inner) + VERSION_SPAN, inner);
        assert_eq!(trailing_reserve(false), 0.0);
        assert_eq!(trailing_reserve(true), CHIP_GAP + VERSION_SPAN);
        // First-row chips end before the capsule; the rest wrap.
        let frames = layout_chips(&[90.0; 5], inner, trailing_reserve(true));
        let end = frames
            .iter()
            .filter(|frame| frame.row == 0)
            .map(|frame| frame.x + frame.width)
            .fold(0.0, f64::max);
        assert!(end + CHIP_GAP <= version_capsule_x(inner));
        assert!(frames.iter().any(|frame| frame.row == 1));
    }

    #[test]
    fn the_history_strip_is_the_last_row_under_the_chips() {
        let without = place_sections("2 lines", false, &one_row(), false, false, false, false);
        let with = place_sections("2 lines", false, &one_row(), false, false, false, true);
        assert_eq!(with.strip_h, crate::commands::STRIP_H);
        assert_eq!(without.strip_h, 0.0);
        assert_eq!(with.strip_y, PAD);
        // The card grows by the strip and one gap; everything above keeps its distance to the top.
        assert_eq!(with.height - without.height, crate::commands::STRIP_H + GAP);
        assert_eq!(with.chips_y, PAD + crate::commands::STRIP_H + GAP);
        assert_eq!(
            with.header_y - with.preview_y,
            without.header_y - without.preview_y
        );
        // Chips and strip do not overlap.
        assert!(with.strip_y + with.strip_h + GAP <= with.chips_y);
        // No chips: no extra gap.
        let bare = place_sections("", false, &[], false, false, false, true);
        assert_eq!(bare.strip_y, PAD);
        assert_eq!(bare.chips_h, 0.0);
    }
}
