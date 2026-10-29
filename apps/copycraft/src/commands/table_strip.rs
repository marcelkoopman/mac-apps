//! The table window's version strip: one small card per version of the table along the bottom
//! of the window, as the history strip shows the copies along the bottom of the card. The
//! version shown is marked; a click shows that version ([`CommandId::TableVersion`]), ⌘Z and
//! ⇧⌘Z step as before. Like the history strip it shows with two or more versions only
//! ([`super::strip_shown`]). Every card but the original's has a × in its top-right corner, as
//! a history card, shown on the version shown and the card under the pointer; it, or ⌫, takes
//! that version's step out ([`CommandId::RemoveTableVersion`]), ⌘Z right after puts it back
//! ([`CommandId::RestoreTableVersion`]). Pure logic; `macos_table_window/strip.rs` draws it. Nothing here
//! is logged.

use zeroize::Zeroize;

use super::table::group_thousands;
use super::{CommandId, TableWindowView, strip_range, strip_shown, thumb_lines};

/// One small card of the version strip, read as a history card: the copy's type mark small
/// above the version's first lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionTile {
    /// The version: 0 is the original. What a click shows.
    pub index: usize,
    /// The copy's type mark (`csv`), as on its history card.
    pub mark: String,
    /// The version's first lines, cut like a history card's ([`thumb_lines`]): the copied
    /// text for the original, the version written as CSV once it was shown; until then its
    /// step ("Duplicates removed").
    pub lines: String,
    /// "Version 2 of 3, Duplicates removed": the VoiceOver label.
    pub spoken: String,
    /// "Version 2 of 3": the VoiceOver label while the cards are masked.
    pub spoken_masked: String,
    /// "CSV · Duplicates removed · 4 rows × 6 columns": the tooltip, a history card's type
    /// with the step and (once known) the size in place of its age.
    pub tooltip: String,
    /// The version shown.
    pub current: bool,
    /// It has a × ([`version_remove_click`]): every version but the original.
    pub removable: bool,
}

impl VersionTile {
    pub fn wipe(&mut self) {
        self.lines.zeroize();
        self.spoken.zeroize();
        self.tooltip.zeroize();
    }
}

impl Drop for VersionTile {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// What a click on version card `index` does: show that version.
pub fn version_tile_click(index: usize) -> CommandId {
    CommandId::TableVersion(index)
}

/// What the × on version card `index` does: take that version's step out. The original (0)
/// has no ×.
pub fn version_remove_click(index: usize) -> Option<CommandId> {
    (index > 0).then_some(CommandId::RemoveTableVersion(index))
}

/// Whether version card `index` shows its ×: as a history card's ([`super::remove_shown`]) on
/// the version shown (`current`, the card framed) and the one under the pointer; never on the
/// original's.
pub fn version_remove_shown(index: usize, current: usize, hovered: Option<usize>) -> bool {
    index > 0 && super::remove_shown(index, current, hovered)
}

/// ⌫ in the table window takes the version shown out, as ⌫ on the card takes the copy the
/// history strip marks. Not while a text field has the keyboard (the search), not on the
/// original, and only while the strip shows.
pub fn table_window_delete_key(editing_text: bool, view: &TableWindowView) -> Option<CommandId> {
    if editing_text || !table_strip_shown(view) {
        return None;
    }
    version_remove_click(view.versions.current)
}

/// ⌘Z / ⇧⌘Z in the table window: right after a version was taken out, ⌘Z puts it back (as
/// the history strip's ⌘Z, [`super::restore_key`]); else they step through the versions
/// ([`super::undo_key`]).
pub fn table_window_undo_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    view: Option<&TableWindowView>,
) -> Option<CommandId> {
    let view = view?;
    super::restore_key(chars, command, shift, other_modifier, view.can_restore)
        .map(|_| CommandId::RestoreTableVersion)
        .or_else(|| super::undo_key(chars, command, shift, other_modifier, Some(&view.versions)))
}

/// The strip shows with two or more versions, as the history strip does with two or more
/// copies. One version: no strip.
pub fn table_strip_shown(view: &TableWindowView) -> bool {
    strip_shown(view.versions.labels.len(), false)
}

/// "64,659 rows × 8 columns" (NL "64.659 rijen × 8 kolommen"), as on the meta line.
pub fn table_size_text(rows: usize, columns: usize) -> String {
    let t = crate::locale::t;
    format!(
        "{} {} × {columns} {}",
        group_thousands(rows),
        if rows == 1 { t("row") } else { t("rows") },
        if columns == 1 {
            t("column")
        } else {
            t("columns")
        }
    )
}

/// The cards for `view` (at most [`super::MAX_THUMBS`], around the version shown), each
/// version's lines and size from `seen` once it was shown.
///
/// `framed`: the version a switch is on its way to; its card is framed instead of the shown
/// version's, so a click frames its card at once.
pub fn table_version_tiles(
    view: &TableWindowView,
    seen: &VersionSizes,
    framed: Option<usize>,
) -> Vec<VersionTile> {
    let labels = &view.versions.labels;
    if labels.is_empty() {
        return Vec::new();
    }
    let current = framed
        .unwrap_or(view.versions.current)
        .min(labels.len() - 1);
    let count = labels.len();
    strip_range(count, current)
        .map(|index| {
            let label = &labels[index];
            let known = match index {
                0 => Some(&view.original_lines),
                _ => seen.lines.get(index).and_then(Option::as_ref),
            };
            // A version not shown yet: its step, as a card without a preview has its title.
            let lines = known
                .filter(|lines| !lines.is_empty())
                .cloned()
                .unwrap_or_else(|| thumb_lines(label));
            let mut parts: Vec<String> = Vec::new();
            if !view.kind_name.is_empty() {
                parts.push(view.kind_name.to_string());
            }
            if index > 0 {
                parts.push(label.clone());
            }
            if let Some((rows, columns)) = seen.sizes.get(index).copied().flatten() {
                parts.push(table_size_text(rows, columns));
            }
            let tooltip = parts.join(" · ");
            for part in &mut parts {
                part.zeroize();
            }
            VersionTile {
                index,
                mark: view.mark.clone(),
                lines,
                spoken: crate::locale::tf("version_spoken", &[&(index + 1), &count, label]),
                spoken_masked: crate::locale::tf("version_spoken_masked", &[&(index + 1), &count]),
                tooltip,
                current: index == current,
                removable: index > 0,
            }
        })
        .collect()
}

/// The sizes of the versions the window has shown, in memory only: the versions are worked
/// out one at a time, so a card says the size of a version once it was on screen. Forgotten
/// for another table, and from the first step that differs when the steps change.
#[derive(Debug, Default)]
pub struct VersionSizes {
    /// Which table ([`TableWindowView::table_key`]).
    key: u64,
    labels: Vec<String>,
    sizes: Vec<Option<(usize, usize)>>,
    /// Each version's first lines once it was shown (the original's are always known).
    lines: Vec<Option<String>>,
}

impl VersionSizes {
    /// Take in `view`: its versions, and the size of the one shown once its grid is there.
    pub fn note(&mut self, view: &TableWindowView) {
        let labels = &view.versions.labels;
        if self.key != view.table_key {
            self.wipe();
            self.key = view.table_key;
        }
        // Keep a size only while every step up to it is still the same.
        let same = self
            .labels
            .iter()
            .zip(labels)
            .take_while(|(old, new)| old == new)
            .count();
        self.sizes.truncate(same);
        self.sizes.resize(labels.len(), None);
        for gone in self.lines.iter_mut().skip(same).flatten() {
            gone.zeroize();
        }
        self.lines.truncate(same);
        self.lines.resize(labels.len(), None);
        for label in &mut self.labels {
            label.zeroize();
        }
        self.labels = labels.clone();
        // While the app works on the table the grid on screen can be another version's (a
        // step before it was just taken out): only a finished view says the version's size.
        if view.working {
            return;
        }
        if !view.grid.is_empty()
            && let Some(slot) = self.sizes.get_mut(view.versions.current)
        {
            *slot = Some((view.rows, view.columns.len()));
        }
        if !view.lines.is_empty()
            && let Some(slot) = self.lines.get_mut(view.versions.current)
        {
            if let Some(old) = slot.as_mut() {
                old.zeroize();
            }
            *slot = Some(view.lines.clone());
        }
    }

    #[cfg(test)]
    pub fn sizes(&self) -> &[Option<(usize, usize)>] {
        &self.sizes
    }

    pub fn wipe(&mut self) {
        for label in &mut self.labels {
            label.zeroize();
        }
        self.labels.clear();
        self.sizes.clear();
        for lines in self.lines.iter_mut().flatten() {
            lines.zeroize();
        }
        self.lines.clear();
        self.key = 0;
    }
}
