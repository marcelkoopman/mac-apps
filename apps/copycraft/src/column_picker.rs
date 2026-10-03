//! The column picker ("Choose columns…" in Table ▾): which columns of the version shown to
//! keep, as one step. Column names and types only, never cell values. Plain logic, tested on
//! Linux; `macos_launcher` draws it.

use crate::table::TableOp;

/// One column in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerColumn {
    /// The real name (the tooltip; the step takes it).
    pub name: String,
    /// The name as the card shows it, a long shared prefix shortened.
    pub shown: String,
    /// "number", "text", … ([`crate::dataframe::friendly_type`]).
    pub kind: String,
}

/// The picker's state: every column, which are kept, and the filter typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnPicker {
    columns: Vec<PickerColumn>,
    kept: Vec<bool>,
    query: String,
}

impl ColumnPicker {
    /// Every column kept, as it opens.
    pub fn new(columns: Vec<PickerColumn>) -> Self {
        let kept = vec![true; columns.len()];
        Self {
            columns,
            kept,
            query: String::new(),
        }
    }

    pub fn columns(&self) -> &[PickerColumn] {
        &self.columns
    }

    pub fn is_kept(&self, index: usize) -> bool {
        self.kept.get(index).copied().unwrap_or(false)
    }

    pub fn set_kept(&mut self, index: usize, kept: bool) {
        if let Some(slot) = self.kept.get_mut(index) {
            *slot = kept;
        }
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
    }

    /// The columns the filter lets through, in the table's order: the shown or the real name
    /// contains every word typed (case-insensitive).
    pub fn visible(&self) -> Vec<usize> {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        (0..self.columns.len())
            .filter(|&index| {
                let column = &self.columns[index];
                let haystack = format!("{} {}", column.shown, column.name).to_lowercase();
                words.iter().all(|word| haystack.contains(word.as_str()))
            })
            .collect()
    }

    /// All: keep every column the filter shows (all of them without a filter).
    pub fn keep_all(&mut self) {
        for index in self.visible() {
            self.kept[index] = true;
        }
    }

    /// None: keep none of the columns the filter shows.
    pub fn keep_none(&mut self) {
        for index in self.visible() {
            self.kept[index] = false;
        }
    }

    pub fn kept_count(&self) -> usize {
        self.kept.iter().filter(|kept| **kept).count()
    }

    /// "Keeping 8 of 20".
    pub fn count_line(&self) -> String {
        format!("Keeping {} of {}", self.kept_count(), self.columns.len())
    }

    /// Apply is there when at least one column is kept and at least one is left out.
    pub fn can_apply(&self) -> bool {
        let kept = self.kept_count();
        kept > 0 && kept < self.columns.len()
    }

    /// The one step Apply takes: the kept columns in the table's order, labelled
    /// "Kept 8 of 20 columns". `None` when Apply is off ([`can_apply`](Self::can_apply)).
    pub fn step(&self) -> Option<TableOp> {
        self.can_apply().then(|| TableOp::SelectColumns {
            columns: self
                .columns
                .iter()
                .zip(&self.kept)
                .filter(|(_, kept)| **kept)
                .map(|(column, _)| column.name.clone())
                .collect(),
            kept_of: Some(self.columns.len()),
        })
    }

    /// What VoiceOver reads for a column's checkbox: its real name (not the "…" the card
    /// shows) and type, "Sunbox 7 - PV1, number".
    pub fn spoken(&self, index: usize) -> String {
        let column = &self.columns[index];
        format!("{}, {}", column.name, column.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::{ColumnPicker, PickerColumn};
    use crate::table::TableOp;

    fn picker(names: &[&str]) -> ColumnPicker {
        ColumnPicker::new(
            names
                .iter()
                .map(|name| PickerColumn {
                    name: name.to_string(),
                    shown: name.replace("Sunbox 7 - ", "… "),
                    kind: "number".to_string(),
                })
                .collect(),
        )
    }

    #[test]
    fn the_kept_columns_become_one_step_in_the_table_order() {
        let mut picker = picker(&["Date", "Sunbox 7 - PV1", "Sunbox 7 - PV2", "Home", "Grid"]);
        // Opens with every column kept: nothing to apply.
        assert_eq!(picker.count_line(), "Keeping 5 of 5");
        assert!(!picker.can_apply());
        assert_eq!(picker.step(), None);
        // Unticked in any order; the step keeps the table's order.
        picker.set_kept(3, false);
        picker.set_kept(1, false);
        assert_eq!(picker.count_line(), "Keeping 3 of 5");
        let step = picker.step().expect("step");
        assert_eq!(
            step,
            TableOp::SelectColumns {
                columns: vec!["Date".into(), "Sunbox 7 - PV2".into(), "Grid".into()],
                kept_of: Some(5),
            }
        );
        assert_eq!(step.label(), "Kept 3 of 5 columns");
        // Not in the Move column to front submenu.
        assert_eq!(step.group(), None);
        // Ticked again: back to nothing changed.
        picker.set_kept(1, true);
        picker.set_kept(3, true);
        assert!(!picker.can_apply());
    }

    #[test]
    fn none_keeps_nothing_and_cannot_apply() {
        let mut picker = picker(&["a", "b", "c"]);
        picker.keep_none();
        assert_eq!(picker.count_line(), "Keeping 0 of 3");
        assert!(!picker.can_apply());
        assert_eq!(picker.step(), None);
        picker.set_kept(2, true);
        assert_eq!(
            picker.step(),
            Some(TableOp::SelectColumns {
                columns: vec!["c".into()],
                kept_of: Some(3),
            })
        );
        picker.keep_all();
        assert_eq!(picker.kept_count(), 3);
    }

    #[test]
    fn the_filter_narrows_the_list_and_all_or_none_act_on_what_it_shows() {
        let mut picker = picker(&["Date", "Sunbox 7 - PV1", "Sunbox 7 - PV2", "Home"]);
        // The real name matches too (the shown one has the prefix shortened).
        picker.set_query("sunbox");
        assert_eq!(picker.visible(), [1, 2]);
        picker.set_query("pv 2");
        assert_eq!(picker.visible(), [2]);
        picker.set_query("PV");
        picker.keep_none();
        assert_eq!(picker.count_line(), "Keeping 2 of 4");
        assert!(picker.is_kept(0) && !picker.is_kept(1) && !picker.is_kept(2));
        picker.set_query("");
        assert_eq!(picker.visible(), [0, 1, 2, 3]);
        assert_eq!(
            picker.step().map(|step| step.label()).as_deref(),
            Some("Kept 2 of 4 columns")
        );
        assert_eq!(picker.spoken(1), "Sunbox 7 - PV1, number");
    }
}
