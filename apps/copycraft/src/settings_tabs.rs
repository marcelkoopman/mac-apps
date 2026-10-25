//! The Settings window's tabs and sizes, without AppKit, so they are tested on every platform
//! (`macos_settings` builds the window from them). Each tab has its own height: the window
//! takes the height of the tab shown and keeps its top edge in place when it changes.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use mac_ui::metrics::{BUTTON_H, MARGIN, SECTION_GAP, SPACE_S};
use mac_ui::style::SPACE_XS;

pub const WIDTH: f64 = 480.0;
pub const NOTE_GAP: f64 = 2.0;
pub const NOTE_H: f64 = 16.0;
/// One control plus its one-line explanation.
pub const SETTING_H: f64 = BUTTON_H + NOTE_GAP + NOTE_H;
/// A labelled setting with a field and a button under the explanation.
pub const FIELD_BLOCK_H: f64 = SETTING_H + SPACE_S + BUTTON_H + SPACE_S + BUTTON_H;
/// The tab bar: one segmented control across the top of the window.
pub const TAB_BAR_H: f64 = BUTTON_H;
/// The Meldingen block under Algemeen: its heading, the four switches in two rows of two, one
/// gap and a note, then a gap and Announce confirmations.
pub const NOTIFY_H: f64 =
    BUTTON_H + 2.0 * BUTTON_H + SPACE_S + NOTE_GAP + NOTE_H + SPACE_S + SETTING_H;
/// Algemeen: five settings (the card's hotkey, the screenshot hotkey, language, date order,
/// Open at Login), then the Meldingen block, five gaps.
pub const GENERAL_H: f64 = 5.0 * SETTING_H + 5.0 * SPACE_S + NOTIFY_H;
/// The settings under "Keep clipboard history": retention, sensitive copies and ask before
/// clearing, one row each without a note (the note is the tooltip), after a gap.
pub const HISTORY_SUB_H: f64 = SPACE_S + 3.0 * BUTTON_H + 2.0 * SPACE_XS;
/// Privacy: four settings ("Keep clipboard history", the privacy filter, blur, CopyCraft in
/// screenshots), the second line of the history note, the rows under history, the
/// ignored-apps block, four gaps.
pub const PRIVACY_H: f64 = 4.0 * SETTING_H + NOTE_H + HISTORY_SUB_H + 4.0 * SPACE_S + FIELD_BLOCK_H;
/// The symbols on Weergave: the labelled setting, then the field with Restore on the same row.
pub const FIELD_ROW_BLOCK_H: f64 = SETTING_H + SPACE_S + BUTTON_H;
/// Weergave: the card theme row (popup and preview, no note), the pages setting, two gaps and
/// the symbols block.
pub const DISPLAY_H: f64 = BUTTON_H + SETTING_H + 2.0 * SPACE_S + FIELD_ROW_BLOCK_H;

/// The window's content height around a pane of `pane_h`: margin, tab bar, gap, pane, margin.
pub const fn window_height(pane_h: f64) -> f64 {
    MARGIN + TAB_BAR_H + SECTION_GAP + pane_h + MARGIN
}

const _: () = assert!(SETTING_H == 46.0);
const _: () = assert!(FIELD_BLOCK_H == 118.0);
const _: () = assert!(FIELD_ROW_BLOCK_H == 82.0);
const _: () = assert!(GENERAL_H == 434.0);
const _: () = assert!(HISTORY_SUB_H == 100.0);
const _: () = assert!(PRIVACY_H == 450.0);
const _: () = assert!(DISPLAY_H == 172.0);
const _: () = assert!(NOTIFY_H == 164.0);
/// The tallest tabs (Privacy, then Algemeen) fit a 13" MacBook Air (about 560 pt for a window) with room to spare.
const _: () = assert!(window_height(PRIVACY_H) == 526.0);
const _: () = assert!(window_height(GENERAL_H) == 510.0);

/// The tabs of the window, in the order of the segments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    General,
    Privacy,
    Display,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::General, Tab::Privacy, Tab::Display];

    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    pub fn at(index: usize) -> Option<Tab> {
        Tab::ALL.get(index).copied()
    }

    /// The tab to open on from the stored index: General when nothing is stored, and for
    /// anything out of range (3 and 4 were the old Meldingen and Feedback tabs).
    pub fn from_stored(stored: Option<i64>) -> Tab {
        stored
            .and_then(|index| usize::try_from(index).ok())
            .and_then(Tab::at)
            .unwrap_or(Tab::General)
    }

    /// The tab one `step` away, wrapping around at both ends.
    pub fn step(self, step: Step) -> Tab {
        let count = Tab::ALL.len();
        let at = match step {
            Step::Next => (self.index() + 1) % count,
            Step::Previous => (self.index() + count - 1) % count,
        };
        Tab::ALL[at]
    }

    /// The segment title, also the window title (literal keys, for the locale scan test).
    pub fn title(self) -> &'static str {
        match self {
            Tab::General => crate::locale::t("group_general"),
            Tab::Privacy => crate::locale::t("group_privacy"),
            Tab::Display => crate::locale::t("group_display"),
        }
    }

    /// The height of this tab's pane: exactly what its settings take.
    pub fn pane_height(self) -> f64 {
        match self {
            Tab::General => GENERAL_H,
            Tab::Privacy => PRIVACY_H,
            Tab::Display => DISPLAY_H,
        }
    }

    /// The window's content height while this tab is shown.
    pub fn window_height(self) -> f64 {
        window_height(self.pane_height())
    }
}

/// Which way a tab-switching key moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Next,
    Previous,
}

/// The modifiers held with a key, as plain flags (no AppKit), for [`step_for_key`].
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods {
    pub command: bool,
    pub control: bool,
    pub shift: bool,
    pub option: bool,
}

/// The tab step a key is, as in Safari and System Settings: ⌃Tab and ⌘⇧] go to the next tab,
/// ⌃⇧Tab and ⌘⇧[ to the previous one. `key_code` is `NSEvent.keyCode`, `chars` its
/// `charactersIgnoringModifiers` (with Shift that is `}` or `{` on most layouts, so both are
/// accepted).
pub fn step_for_key(key_code: u16, chars: &str, mods: Mods) -> Option<Step> {
    if mods.option {
        return None;
    }
    if key_code == mac_ui::keys::code::TAB && mods.control && !mods.command {
        return Some(if mods.shift {
            Step::Previous
        } else {
            Step::Next
        });
    }
    if mods.command && mods.shift && !mods.control {
        return match chars {
            "]" | "}" => Some(Step::Next),
            "[" | "{" => Some(Step::Previous),
            _ => None,
        };
    }
    None
}

/// The new bottom edge (`y` grows upwards, as on macOS) of a window frame at `y` with height
/// `old_h` that becomes `new_h` tall with its top edge where it was.
pub fn keep_top(y: f64, old_h: f64, new_h: f64) -> f64 {
    y + old_h - new_h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_its_own_height_and_the_tallest_fits() {
        let heights: Vec<f64> = Tab::ALL.iter().map(|tab| tab.window_height()).collect();
        assert_eq!(heights, [510.0, 526.0, 248.0]);
        // Under about 560 pt, the room a 13" MacBook Air has for a window.
        assert!(heights.iter().all(|h| *h <= 526.0));
    }

    #[test]
    fn a_resize_keeps_the_top_edge() {
        // 100 pt from the bottom, 300 tall: the top is at 400 before and after.
        assert_eq!(keep_top(100.0, 300.0, 456.0) + 456.0, 400.0);
        assert_eq!(keep_top(100.0, 300.0, 158.0) + 158.0, 400.0);
        assert_eq!(keep_top(100.0, 300.0, 300.0), 100.0);
    }

    #[test]
    fn tabs_round_trip_their_index() {
        for (at, tab) in Tab::ALL.into_iter().enumerate() {
            assert_eq!(tab.index(), at);
            assert_eq!(Tab::at(at), Some(tab));
        }
        assert_eq!(Tab::at(Tab::ALL.len()), None);
    }

    #[test]
    fn stepping_wraps_around_both_ends() {
        assert_eq!(Tab::General.step(Step::Next), Tab::Privacy);
        assert_eq!(Tab::Privacy.step(Step::Next), Tab::Display);
        assert_eq!(Tab::Display.step(Step::Next), Tab::General);
        assert_eq!(Tab::General.step(Step::Previous), Tab::Display);
        assert_eq!(Tab::Display.step(Step::Previous), Tab::Privacy);
        assert_eq!(Tab::Privacy.step(Step::Previous), Tab::General);
    }

    #[test]
    fn control_tab_and_command_shift_brackets_step() {
        let tab = mac_ui::keys::code::TAB;
        let control = Mods {
            control: true,
            ..Mods::default()
        };
        let control_shift = Mods {
            shift: true,
            ..control
        };
        let command_shift = Mods {
            command: true,
            shift: true,
            ..Mods::default()
        };
        assert_eq!(step_for_key(tab, "\t", control), Some(Step::Next));
        assert_eq!(
            step_for_key(tab, "\u{19}", control_shift),
            Some(Step::Previous)
        );
        assert_eq!(step_for_key(0x1E, "}", command_shift), Some(Step::Next));
        assert_eq!(step_for_key(0x1E, "]", command_shift), Some(Step::Next));
        assert_eq!(step_for_key(0x21, "{", command_shift), Some(Step::Previous));
        assert_eq!(step_for_key(0x21, "[", command_shift), Some(Step::Previous));
        // Plain Tab moves the focus; ⌘] without Shift, ⌘Tab, ⌥ chords and letters do nothing.
        assert_eq!(step_for_key(tab, "\t", Mods::default()), None);
        let command = Mods {
            command: true,
            ..Mods::default()
        };
        assert_eq!(step_for_key(0x1E, "]", command), None);
        assert_eq!(step_for_key(tab, "\t", command), None);
        let option = Mods {
            option: true,
            ..control
        };
        assert_eq!(step_for_key(tab, "\t", option), None);
        assert_eq!(step_for_key(0x00, "a", command_shift), None);
    }

    #[test]
    fn the_stored_tab_is_clamped_to_the_three_tabs() {
        assert_eq!(Tab::from_stored(None), Tab::General);
        assert_eq!(Tab::from_stored(Some(0)), Tab::General);
        assert_eq!(Tab::from_stored(Some(1)), Tab::Privacy);
        assert_eq!(Tab::from_stored(Some(2)), Tab::Display);
        for old in [3, 4, 99, -1, i64::MAX, i64::MIN] {
            assert_eq!(Tab::from_stored(Some(old)), Tab::General);
        }
    }
}
