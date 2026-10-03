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
/// The table's version bar under the well, while a table has more than one version.
pub const VERSION_H: f64 = 28.0;
pub const GAP: f64 = 8.0;

/// The card's height and the bottom of each row (AppKit counts y from the bottom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sections {
    pub height: f64,
    pub header_y: f64,
    pub find_y: f64,
    pub preview_y: f64,
    pub version_y: f64,
    pub meta_y: f64,
    pub search_y: f64,
    pub chips_y: f64,
    pub chips_h: f64,
}

/// Rows from the top: header, in-item find (`item_find`), the well, the version bar
/// (`version_bar`), the meta line (when `meta` is not empty), the search field (`searching`)
/// and the chips.
pub fn place_sections(
    meta: &str,
    searching: bool,
    frames: &[ChipFrame],
    show_empty: bool,
    show_nav: bool,
    item_find: bool,
    version_bar: bool,
) -> Sections {
    let meta_h = if meta.is_empty() { 0.0 } else { META_H };
    let search_h = if searching { SEARCH_H } else { 0.0 };
    let find_h = if item_find { ITEM_FIND_H } else { 0.0 };
    let gap_after_find = if find_h > 0.0 { 6.0 } else { 0.0 };
    let version_h = if version_bar { VERSION_H } else { 0.0 };
    let mut chips_h = if show_empty {
        28.0
    } else {
        commands::chips_height(frames)
    };
    if show_nav {
        chips_h = chips_h.max(commands::CHIP_PITCH);
    }
    let gap_after_version = if version_bar && (meta_h > 0.0 || search_h > 0.0 || chips_h > 0.0) {
        6.0
    } else {
        0.0
    };
    let below_preview = version_h > 0.0 || meta_h > 0.0 || search_h > 0.0 || chips_h > 0.0;
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
        + version_h
        + gap_after_version
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
    cursor -= gap_after_preview + version_h;
    let version_y = cursor;
    cursor -= gap_after_version + meta_h;
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
        version_y,
        meta_y,
        search_y,
        chips_y,
        chips_h,
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP, PAD, VERSION_H, place_sections};
    use crate::commands::ChipFrame;

    fn one_row() -> Vec<ChipFrame> {
        crate::commands::layout_chips(&[80.0], 400.0, 0.0)
    }

    #[test]
    fn the_version_bar_takes_a_row_under_the_well() {
        let without = place_sections("2 lines", false, &one_row(), false, false, false, false);
        let with = place_sections("2 lines", false, &one_row(), false, false, false, true);
        assert_eq!(with.height, without.height + VERSION_H + 6.0);
        // The rows above stay where they are, measured from the top.
        assert_eq!(
            with.height - with.preview_y,
            without.height - without.preview_y
        );
        assert_eq!(with.version_y, with.preview_y - GAP - VERSION_H);
        assert_eq!(with.meta_y, with.version_y - 6.0 - super::META_H);
        // The rows below keep their place from the bottom.
        assert_eq!(with.chips_y, without.chips_y);
        assert_eq!(with.chips_y, PAD);
    }

    #[test]
    fn an_empty_card_has_no_gaps_below_the_well() {
        let placed = place_sections("", false, &[], false, false, false, false);
        assert_eq!(placed.preview_y, PAD);
        let with_bar = place_sections("", false, &[], false, false, false, true);
        assert_eq!(with_bar.version_y, PAD);
    }
}
