//! The table window's version strip: one small card per version of the table along the bottom
//! of the window, as the history strip shows the copies along the bottom of the card. The
//! version shown is marked; a click shows that version ([`CommandId::TableVersion`]), ⌘Z and
//! ⇧⌘Z step as before. Like the history strip it shows with two or more versions only
//! ([`super::strip_shown`]). Pure logic; `macos_table_window/strip.rs` draws it. Nothing here
//! is logged.

use zeroize::Zeroize;

use super::table::group_thousands;
use super::{CommandId, TableWindowView, strip_range, strip_shown, thumb_lines};

/// One small card of the version strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionTile {
    /// The version: 0 is the original. What a click shows.
    pub index: usize,
    /// "v2", drawn small above the lines like a copy's type mark.
    pub mark: String,
    /// The step ("Original", "Duplicates removed") and, once known, the size
    /// ("4 rows × 6 columns"), cut like a history card's lines ([`thumb_lines`]).
    pub lines: String,
    /// "Version 2 of 3, Duplicates removed": tooltip and VoiceOver label.
    pub spoken: String,
    /// The version shown.
    pub current: bool,
}

impl VersionTile {
    pub fn wipe(&mut self) {
        self.lines.zeroize();
        self.spoken.zeroize();
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
/// version's size from `sizes` (rows, columns) when it was seen.
///
/// `framed`: the version a switch is on its way to; its card is framed instead of the shown
/// version's, so a click frames its card at once.
pub fn table_version_tiles(
    view: &TableWindowView,
    sizes: &[Option<(usize, usize)>],
    framed: Option<usize>,
) -> Vec<VersionTile> {
    let labels = &view.versions.labels;
    if labels.is_empty() {
        return Vec::new();
    }
    let current = framed
        .unwrap_or(view.versions.current)
        .min(labels.len() - 1);
    strip_range(labels.len(), current)
        .map(|index| {
            let label = &labels[index];
            let mut text = match sizes.get(index).copied().flatten() {
                Some((rows, columns)) => format!("{label}\n{}", table_size_text(rows, columns)),
                None => label.clone(),
            };
            let lines = thumb_lines(&text);
            text.zeroize();
            VersionTile {
                index,
                mark: format!("v{}", index + 1),
                lines,
                spoken: crate::locale::tf("version_spoken", &[&(index + 1), &labels.len(), label]),
                current: index == current,
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
        for label in &mut self.labels {
            label.zeroize();
        }
        self.labels = labels.clone();
        if !view.grid.is_empty()
            && let Some(slot) = self.sizes.get_mut(view.versions.current)
        {
            *slot = Some((view.rows, view.columns.len()));
        }
    }

    pub fn sizes(&self) -> &[Option<(usize, usize)>] {
        &self.sizes
    }

    pub fn wipe(&mut self) {
        for label in &mut self.labels {
            label.zeroize();
        }
        self.labels.clear();
        self.sizes.clear();
        self.key = 0;
    }
}
