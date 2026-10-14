//! Chip, navigation and capsule geometry, plus keyboard stepping between chips.

use super::*;

pub const MAX_VISIBLE: usize = 8;
pub const CHIP_PILL_H: f64 = mac_ui::metrics::BUTTON_H;
pub const CHIP_GAP: f64 = mac_ui::metrics::BUTTON_GAP;
/// Vertical step from one chip row to the next: the pill, then the same gap as between buttons.
pub const CHIP_PITCH: f64 = CHIP_PILL_H + CHIP_GAP;
/// History chevrons: square hit areas, as tall as a chip.
pub const NAV_BUTTON: f64 = mac_ui::metrics::BUTTON_H;
/// The "3 / 20" position between the chevrons. Fixed and wide enough for the longest label
/// (history holds 20), so the row never changes width while stepping.
pub const NAV_COUNT_W: f64 = 44.0;
/// The history controls: chevron, gap, position, gap, chevron.
pub const NAV_SPAN: f64 = NAV_BUTTON + CHIP_GAP + NAV_COUNT_W + CHIP_GAP + NAV_BUTTON;

/// Left edge of the position label inside a history or version row.
pub fn nav_mid_x() -> f64 {
    NAV_BUTTON + CHIP_GAP
}

/// Left edge of the trailing chevron inside a history or version row.
pub fn nav_end_x() -> f64 {
    NAV_BUTTON + CHIP_GAP + NAV_COUNT_W + CHIP_GAP
}

/// A history chevron: its SF Symbol, the glyph drawn without SF Symbols, its name (the
/// VoiceOver label and tooltip, never drawn) and which way it steps through history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chevron {
    pub symbol: &'static str,
    pub glyph: &'static str,
    /// Locale key of the name (`Previous`, `Next`).
    pub name_key: &'static str,
    /// Steps to an older copy (the position number goes up).
    pub older: bool,
    /// Its arrow key ([`arrow_action`]), drawn (`←`) and spoken (`Left Arrow`).
    pub arrow: &'static str,
    /// Locale key of the spoken arrow (`Left Arrow`).
    pub spoken_arrow_key: &'static str,
}

/// `‹` on the left: the previous number, toward 1 (the newest copy).
pub const PREVIOUS_CHEVRON: Chevron = Chevron {
    symbol: "chevron.left",
    glyph: "‹",
    name_key: "previous",
    older: false,
    arrow: "←",
    spoken_arrow_key: "left_arrow",
};

/// `›` on the right: the next number, toward N (the oldest copy).
pub const NEXT_CHEVRON: Chevron = Chevron {
    symbol: "chevron.right",
    glyph: "›",
    name_key: "next",
    older: true,
    arrow: "→",
    spoken_arrow_key: "right_arrow",
};

impl Chevron {
    /// The spoken name of the button: `Previous`.
    pub fn name(&self) -> &'static str {
        crate::locale::t(self.name_key)
    }

    /// The command a click runs.
    pub fn command(&self) -> CommandId {
        if self.older {
            CommandId::HistoryOlder
        } else {
            CommandId::HistoryNewer
        }
    }

    /// Whether there is somewhere to go: `‹` is off at 1, `›` at N.
    pub fn enabled(&self, nav: &HistoryNav) -> bool {
        if self.older {
            nav.can_older
        } else {
            nav.can_newer
        }
    }

    /// Tooltip: the name and its keys, `Previous (← or ⌘←)` (`Vorige (← of ⌘←)`).
    pub fn tooltip(&self) -> String {
        crate::locale::tf("chevron_tooltip", &[&self.name(), &self.arrow, &self.arrow])
    }

    /// VoiceOver hint: `Left Arrow, or Command-Left Arrow`.
    pub fn spoken_shortcut(&self) -> String {
        let arrow = crate::locale::t(self.spoken_arrow_key);
        crate::locale::tf("chevron_spoken", &[&arrow])
    }
}

/// Where the keyboard focus is when an arrow key reaches the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowFocus {
    /// The card itself: the search field while it is empty (also before a search), the empty
    /// find field, a button or the reveal cover.
    Card,
    /// The search or find field with text in it: plain ← → move its caret.
    TextWithContent,
}

/// What an arrow key does on the card ([`arrow_action`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowAction {
    /// Step through history like this chevron. At an end (or without history) nothing moves
    /// and the card beeps.
    History(Chevron),
    /// Move the chip selection ([`step_chip`]).
    Chip { dx: isize, dy: isize },
    /// Leave the key to the text field (its caret moves).
    Text,
}

/// The arrow-key rule:
/// - ⌘← / ⌘→ always step through history (Previous / Next), also from a field with text.
/// - ← / → move the caret in a search or find field that has text. Otherwise they step through
///   history while the history capsule is shown, and move between the chips when it is not.
/// - ↑ / ↓ move between the chips: to the row above or below, or to the previous or next chip
///   when there is no row that way (so every chip stays reachable when ← → step history).
///
/// The read-only text in the well handles its own arrows when it has the focus (a click in it
/// puts a caret or selection there), so it never gets here for plain arrows.
pub fn arrow_action(
    key: Key,
    command: bool,
    focus: ArrowFocus,
    has_history: bool,
) -> Option<ArrowAction> {
    let (chevron, dx) = match key {
        Key::Left => (PREVIOUS_CHEVRON, -1),
        Key::Right => (NEXT_CHEVRON, 1),
        Key::Up => return Some(ArrowAction::Chip { dx: 0, dy: -1 }),
        Key::Down => return Some(ArrowAction::Chip { dx: 0, dy: 1 }),
        _ => return None,
    };
    Some(if command {
        ArrowAction::History(chevron)
    } else if focus == ArrowFocus::TextWithContent {
        ArrowAction::Text
    } else if has_history {
        ArrowAction::History(chevron)
    } else {
        ArrowAction::Chip { dx, dy: 0 }
    })
}

/// Empty space kept on the right of the first chip row so the capsule fits.
pub const NAV_RESERVE: f64 = CHIP_GAP + NAV_SPAN;
/// The table's version capsule (`↶ v2/3 ↷`), the history capsule's size.
pub const VERSION_SPAN: f64 = NAV_SPAN;

/// Room the first chip row keeps on its right for the capsules: the history capsule (`nav`)
/// and, left of it, the version capsule (`versions`).
pub fn trailing_reserve(nav: bool, versions: bool) -> f64 {
    let mut reserve = 0.0;
    if nav {
        reserve += NAV_RESERVE;
    }
    if versions {
        reserve += CHIP_GAP + VERSION_SPAN;
    }
    reserve
}

/// Left edge of the version capsule in a chip row ending at `right`: just left of the history
/// capsule when there is one, else at the right end.
pub fn version_capsule_x(right: f64, nav: bool) -> f64 {
    let nav_room = if nav { NAV_SPAN + CHIP_GAP } else { 0.0 };
    right - nav_room - VERSION_SPAN
}

/// Narrowest chip, so short titles ("Copy", "File") keep a comfortable click target.
pub const CHIP_MIN_W: f64 = 56.0;
/// Widest chip. A longer title is cut at the tail by the pill itself.
pub const CHIP_MAX_W: f64 = 220.0;

/// Laid-out width of a chip whose pill measures `measured` points after `sizeToFit`: rounded up
/// to whole points and kept within [`CHIP_MIN_W`]..=[`CHIP_MAX_W`]. NaN gives the minimum.
pub fn chip_width(measured: f64) -> f64 {
    if measured.is_nan() {
        return CHIP_MIN_W;
    }
    measured.ceil().clamp(CHIP_MIN_W, CHIP_MAX_W)
}

/// Flow chips of the given `widths` (from [`chip_width`]) into rows `width` wide.
/// `trailing` is kept clear on the right of the first row only.
pub fn layout_chips(widths: &[f64], width: f64, trailing: f64) -> Vec<ChipFrame> {
    let mut frames = Vec::with_capacity(widths.len());
    let mut x = 0.0;
    let mut row = 0usize;
    for &chip in widths {
        let limit = if row == 0 {
            (width - trailing).max(0.0)
        } else {
            width
        };
        // A first-row chip too wide for the room left of the capsules starts the next row.
        let first_and_crowded = row == 0 && trailing > 0.0;
        if (x > 0.0 || first_and_crowded) && x + chip > limit {
            row += 1;
            x = 0.0;
        }
        frames.push(ChipFrame {
            x,
            row,
            width: chip,
        });
        x += chip + CHIP_GAP;
    }
    frames
}

/// The history capsule for `len` copies with `cursor` on screen (0 is the newest entry).
/// `None` (no capsule) with fewer than two copies: there is nowhere to go. A direction with
/// nowhere to go stays visible and dimmed.
pub fn history_nav(len: usize, cursor: usize) -> Option<HistoryNav> {
    if len < 2 {
        return None;
    }
    let cursor = cursor.min(len - 1);
    Some(HistoryNav {
        can_older: cursor + 1 < len,
        can_newer: cursor > 0,
        position: cursor + 1,
        total: len,
    })
}

pub fn step_history(len: usize, cursor: usize, older: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let nav = history_nav(len, cursor)?;
    let cursor = cursor.min(len - 1);
    if older && nav.can_older {
        Some(cursor + 1)
    } else if !older && nav.can_newer {
        Some(cursor - 1)
    } else {
        None
    }
}

pub fn chips_height(frames: &[ChipFrame]) -> f64 {
    if frames.is_empty() {
        0.0
    } else {
        (frames.iter().map(|frame| frame.row).max().unwrap_or(0) + 1) as f64 * CHIP_PITCH
    }
}

pub fn step_chip(frames: &[ChipFrame], index: usize, dx: isize, dy: isize) -> usize {
    if frames.is_empty() {
        return 0;
    }
    let index = index.min(frames.len() - 1);
    if dy == 0 {
        return (index as isize + dx).clamp(0, frames.len() as isize - 1) as usize;
    }
    let current = frames[index];
    let target = current.row as isize + dy;
    let center = current.x + current.width / 2.0;
    let in_row = (target >= 0)
        .then(|| {
            frames
                .iter()
                .enumerate()
                .filter(|(_, frame)| frame.row == target as usize)
                .min_by(|(_, a), (_, b)| {
                    let da = (a.x + a.width / 2.0 - center).abs();
                    let db = (b.x + b.width / 2.0 - center).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(next, _)| next)
        })
        .flatten();
    // No row that way: the previous or next chip in order instead.
    in_row.unwrap_or_else(|| (index as isize + dy).clamp(0, frames.len() as isize - 1) as usize)
}

/// Commands that close the card themselves once they have what they need from it. Open table
/// reads the card's entry (history cursor, chosen file) first: a card closed before that would
/// forget the file and the entry, and the window would never open.
pub fn closes_card_itself(id: &CommandId) -> bool {
    matches!(id, CommandId::TableOpenWindow)
}

pub fn keeps_card_open(id: &CommandId) -> bool {
    matches!(
        id,
        CommandId::History(_)
            | CommandId::HistoryOlder
            | CommandId::HistoryNewer
            | CommandId::Appearance(_)
            | CommandId::ClearSensitive
            | CommandId::KeepHistory(_)
            | CommandId::ClearClipboard
            | CommandId::ClearHistory
            | CommandId::Pause
            | CommandId::IgnoreNext
            | CommandId::IgnoreFrontApp
            | CommandId::Pin
            | CommandId::Unpin
            | CommandId::DeleteHistory
            | CommandId::ClearPins
            | CommandId::Settings
            | CommandId::About
            | CommandId::Welcome
            | CommandId::Clear
            | CommandId::ChooseFile
            | CommandId::UseClipboard
            | CommandId::ShowAll
            | CommandId::Original
            | CommandId::Format
            | CommandId::Convert
            | CommandId::Decode
            | CommandId::Dataframe
            | CommandId::Schema
            | CommandId::Sample
            | CommandId::Info
            | CommandId::Ocr
            | CommandId::Qr
            | CommandId::Diff
            | CommandId::DiffToggle(_)
            | CommandId::DiffShown
            | CommandId::DiffSwap
            | CommandId::Copy
            | CommandId::Save
            | CommandId::TableStep(_)
            | CommandId::TableUndo
            | CommandId::TableRedo
            | CommandId::TableVersion(_)
            | CommandId::TableMenu
            | CommandId::ImageMenu
            | CommandId::ImageStep(_)
            | CommandId::ImageResizeCustom
            | CommandId::TextMenu
            | CommandId::ToggleLineNumbers
            | CommandId::TextHead
            | CommandId::TextTail
            | CommandId::TextReverse
            | CommandId::TextColumn
            | CommandId::TextFilter
            | CommandId::TextComplexity
            | CommandId::AgentCommands
            | CommandId::AgentOutline
            | CommandId::AgentCheck
            | CommandId::GotoLine(_)
            | CommandId::TableDateOrder(_)
            | CommandId::TableChooseColumns
    )
}

/// Split the card's chips into the ones shown (at most [`mac_ui::style::MAX_CARD_CHIPS`]) and the ones
/// that move under `…`. Original and the menus (Lines…, Image ▾, Table ▾) always stay visible,
/// in their order; the rest fill the room left, in order.
pub fn split_chips(chips: Vec<Command>) -> (Vec<Command>, Vec<Command>) {
    let keeps = |cmd: &Command| {
        matches!(
            cmd.id,
            CommandId::Original | CommandId::TextMenu | CommandId::ImageMenu | CommandId::TableMenu
        )
    };
    let kept = chips.iter().filter(|cmd| keeps(cmd)).count();
    let mut room = mac_ui::style::MAX_CARD_CHIPS.saturating_sub(kept);
    let (mut shown, mut rest) = (Vec::new(), Vec::new());
    for cmd in chips {
        if keeps(&cmd) {
            shown.push(cmd);
        } else if room > 0 {
            room -= 1;
            shown.push(cmd);
        } else {
            rest.push(cmd);
        }
    }
    (shown, rest)
}

/// SF Symbol for `id` on its chip or menu item, if it has one. The title stays the label, so
/// VoiceOver reads the same name as before.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn symbol_for(id: &CommandId) -> Option<&'static str> {
    Some(match id {
        CommandId::Original => "photo",
        CommandId::Visit => "safari",
        CommandId::Format => "wand.and.stars",
        CommandId::Convert => "arrow.triangle.2.circlepath",
        CommandId::Decode => "lock.open",
        CommandId::Schema => "list.bullet.indent",
        CommandId::Sample => "doc.badge.plus",
        CommandId::Info => "info.circle",
        CommandId::Ocr => "text.viewfinder",
        CommandId::Qr => "qrcode",
        CommandId::Diff => "arrow.left.arrow.right",
        CommandId::Copy => "doc.on.doc",
        CommandId::Save => "square.and.arrow.down",
        CommandId::OpenPreview => "eye",
        CommandId::ClearClipboard | CommandId::ClearHistory | CommandId::Clear => "trash",
        CommandId::ChooseFile => "folder",
        CommandId::UseClipboard => "doc.on.clipboard",
        CommandId::ShowAll => "text.justify",
        CommandId::Settings => "gearshape",
        CommandId::About => "info.circle",
        CommandId::ExportDiagnostics => "stethoscope",
        CommandId::Welcome => "hand.wave",
        CommandId::Quit => "power",
        CommandId::Pause => "pause.circle",
        CommandId::IgnoreNext => "forward.end",
        CommandId::IgnoreFrontApp => "app.badge.checkmark",
        CommandId::ClearSensitive => "timer",
        CommandId::Pin => "pin",
        CommandId::Unpin => "pin.slash",
        CommandId::DeleteHistory => "minus.circle",
        CommandId::DiffShown => "arrow.left.arrow.right",
        CommandId::DiffSwap => "arrow.left.arrow.right.circle",
        CommandId::TextMenu => "list.number",
        CommandId::TextComplexity => "gauge.with.dots.needle.50percent",
        CommandId::AgentCommands => "terminal",
        CommandId::AgentOutline => "list.bullet.indent",
        CommandId::AgentCheck => "checklist",
        CommandId::ImageMenu => "photo",
        CommandId::TableMenu | CommandId::TextColumn => "tablecells",
        CommandId::TableOpenWindow => "macwindow",
        CommandId::TableChooseColumns => "checklist",
        CommandId::TableUndo => "arrow.uturn.backward",
        CommandId::TableRedo => "arrow.uturn.forward",
        CommandId::TextHead => "arrow.up.to.line",
        CommandId::TextTail => "arrow.down.to.line",
        CommandId::TextReverse => "arrow.up.arrow.down",
        CommandId::TextFilter => "line.3.horizontal.decrease",
        CommandId::HistoryOlder => "chevron.left",
        CommandId::HistoryNewer => "chevron.right",
        _ => return None,
    })
}
