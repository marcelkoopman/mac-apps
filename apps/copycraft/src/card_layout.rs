//! The card's vertical layout: which rows it has and where each one goes. Plain arithmetic, so
//! it is tested on Linux; `macos_launcher` places the views.

use crate::commands::{self, ChipFrame};

pub const PAD: f64 = 14.0;
pub const HEADER_H: f64 = 32.0;
pub const PREVIEW_H: f64 = 264.0;
pub const META_H: f64 = 22.0;
pub const SEARCH_H: f64 = 36.0;
/// Fixed in-item field under the header. Hidden until the well is revealed.
pub const ITEM_FIND_H: f64 = 28.0;
pub const GAP: f64 = 8.0;

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
) -> Sections {
    let meta_h = if meta.is_empty() { 0.0 } else { META_H };
    let search_h = if searching { SEARCH_H } else { 0.0 };
    let find_h = if item_find { ITEM_FIND_H } else { 0.0 };
    let gap_after_find = if find_h > 0.0 { 6.0 } else { 0.0 };
    let mut chips_h = if show_empty {
        28.0
    } else {
        commands::chips_height(frames)
    };
    if show_capsule {
        chips_h = chips_h.max(commands::CHIP_PITCH);
    }
    let below_preview = meta_h > 0.0 || search_h > 0.0 || chips_h > 0.0;
    let gap_after_preview = if below_preview { GAP } else { 0.0 };
    let gap_after_meta = if meta_h > 0.0 && (search_h > 0.0 || chips_h > 0.0) {
        GAP
    } else {
        0.0
    };
    let gap_after_search = if search_h > 0.0 && chips_h > 0.0 {
        4.0
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
    Sections {
        height,
        header_y,
        find_y,
        preview_y,
        meta_y,
        search_y,
        chips_y,
        chips_h,
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP, META_H, PAD, place_sections};
    use crate::commands::{
        CHIP_GAP, CHIP_PITCH, ChipFrame, NAV_SPAN, VERSION_SPAN, layout_chips, trailing_reserve,
        version_capsule_x,
    };

    fn one_row() -> Vec<ChipFrame> {
        layout_chips(&[80.0], 400.0, 0.0)
    }

    #[test]
    fn the_version_capsule_shares_the_chip_row_so_the_card_does_not_grow() {
        let plain = place_sections("2 lines", false, &one_row(), false, false, false);
        let capsule = place_sections("2 lines", false, &one_row(), false, true, false);
        assert_eq!(capsule, plain);
        assert_eq!(plain.chips_y, PAD);
        assert_eq!(plain.meta_y, plain.chips_y + plain.chips_h + GAP);
        assert_eq!(plain.preview_y, plain.meta_y + META_H + GAP);
    }

    #[test]
    fn an_empty_card_has_no_gaps_below_the_well() {
        let placed = place_sections("", false, &[], false, false, false);
        assert_eq!(placed.preview_y, PAD);
        let capsule = place_sections("", false, &[], false, true, false);
        assert_eq!(capsule.chips_h, CHIP_PITCH);
    }

    #[test]
    fn the_version_capsule_sits_left_of_the_history_capsule_and_chips_keep_clear() {
        let inner = 412.0;
        assert_eq!(
            version_capsule_x(inner, true) + VERSION_SPAN + CHIP_GAP + NAV_SPAN,
            inner
        );
        assert_eq!(version_capsule_x(inner, false) + VERSION_SPAN, inner);
        assert_eq!(trailing_reserve(false, false), 0.0);
        assert_eq!(trailing_reserve(false, true), CHIP_GAP + VERSION_SPAN);
        // First-row chips end before the capsules; the rest wrap.
        for nav in [false, true] {
            let frames = layout_chips(&[90.0; 4], inner, trailing_reserve(nav, true));
            let end = frames
                .iter()
                .filter(|frame| frame.row == 0)
                .map(|frame| frame.x + frame.width)
                .fold(0.0, f64::max);
            assert!(end + CHIP_GAP <= version_capsule_x(inner, nav), "{nav}");
            assert!(frames.iter().any(|frame| frame.row == 1));
        }
    }
}
