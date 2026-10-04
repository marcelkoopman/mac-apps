//! Table ▾ › Join with / Left join with / Append rows of: the table shown combined with the
//! version another history entry shows. The other table is held by the step itself (shared,
//! not copied), so the step replays without that entry, and with it its sensitivity labels:
//! the result keeps the labels of both sources ([`crate::table::TableVersions::inherited`]).
//! Written out here (no `lazy`, `polars-ops` joins): keys match on the text of their cells,
//! so a number key matches the same number read as text.

use std::collections::HashMap;
use std::sync::Arc;

use polars::prelude::*;

use crate::dataframe::cell_text;
use crate::sensitivity::Label;

/// Most rows a join makes (a key repeated on both sides multiplies them).
pub const JOIN_MAX_ROWS: usize = 1_000_000;

/// The other table of a combining step: its frame as it was when the step was taken and the
/// labels of both sources then. Equal only to itself (the same frame), so steps stay `Eq`.
#[derive(Clone)]
pub struct OtherTable {
    frame: Arc<DataFrame>,
    /// The sensitivity labels of the two tables combined, sorted, once each.
    pub labels: Vec<Label>,
}

impl OtherTable {
    pub fn new(frame: DataFrame, mut labels: Vec<Label>) -> Self {
        labels.sort();
        labels.dedup();
        Self {
            frame: Arc::new(frame),
            labels,
        }
    }

    pub fn frame(&self) -> &DataFrame {
        &self.frame
    }

    /// "a 4 × 3 table", for a version's label (no names or values).
    pub fn described(&self) -> String {
        let (rows, columns) = self.frame.shape();
        format!("a {rows} × {columns} table")
    }
}

impl PartialEq for OtherTable {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frame, &other.frame) && self.labels == other.labels
    }
}

impl Eq for OtherTable {}

impl std::fmt::Debug for OtherTable {
    /// The shape only: the frame holds copied data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtherTable")
            .field("shape", &self.frame.shape())
            .field("labels", &self.labels)
            .finish()
    }
}

/// The columns both tables have, in the order of `left`: the keys a join is offered on.
pub fn shared_columns(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|name| right.contains(name))
        .cloned()
        .collect()
}

fn cell_texts(column: &Column) -> Vec<Option<String>> {
    (0..column.len())
        .map(|row| column.get(row).ok().and_then(cell_text))
        .collect()
}

/// A name no column of `taken` has: `want`, else `want_right`, `want_right 2`, ….
fn free_name(taken: &[String], want: &str) -> String {
    if !taken.iter().any(|name| name == want) {
        return want.to_string();
    }
    let right = format!("{want}_right");
    std::iter::once(right.clone())
        .chain((2..).map(|n| format!("{right} {n}")))
        .find(|name| !taken.iter().any(|taken| taken == name))
        .expect("a free name")
}

/// Join `left` with `right` on the column `key` both have: for every row of `left` (in its
/// order) each row of `right` with the same key, in `right`'s order; with `keep_unmatched` (a
/// left join) a row without one too, its `right` cells empty. Empty keys match nothing. The
/// key column once, then `left`'s columns, then `right`'s others (a name `left` has already
/// gets `_right`).
pub fn join(
    left: &DataFrame,
    right: &DataFrame,
    key: &str,
    keep_unmatched: bool,
) -> Result<DataFrame, String> {
    let fail = |e: PolarsError| e.to_string();
    let left_key = left
        .column(key)
        .map_err(|_| format!("This table has no column {key}"))?;
    let right_key = right
        .column(key)
        .map_err(|_| format!("The other table has no column {key}"))?;
    let mut index: HashMap<String, Vec<IdxSize>> = HashMap::new();
    for (row, text) in cell_texts(right_key).into_iter().enumerate() {
        if let Some(text) = text {
            index.entry(text).or_default().push(row as IdxSize);
        }
    }
    let mut left_rows: Vec<IdxSize> = Vec::new();
    let mut right_rows: Vec<Option<IdxSize>> = Vec::new();
    for (row, text) in cell_texts(left_key).into_iter().enumerate() {
        match text.as_ref().and_then(|text| index.get(text)) {
            Some(matches) => {
                for &other in matches {
                    left_rows.push(row as IdxSize);
                    right_rows.push(Some(other));
                }
            }
            None if keep_unmatched => {
                left_rows.push(row as IdxSize);
                right_rows.push(None);
            }
            None => {}
        }
        if left_rows.len() > JOIN_MAX_ROWS {
            return Err(format!(
                "The join would make more than {} rows",
                crate::commands::group_thousands(JOIN_MAX_ROWS)
            ));
        }
    }
    if left_rows.is_empty() {
        return Err(format!("No row matches on {key}"));
    }
    let height = left_rows.len();
    let left_rows = IdxCa::from_vec(PlSmallStr::EMPTY, left_rows);
    let right_rows: IdxCa = right_rows.into_iter().collect();
    let mut columns: Vec<Column> = Vec::with_capacity(left.width() + right.width());
    let mut names: Vec<String> = Vec::new();
    // The key first, then the rest of the left table.
    let ordered = std::iter::once(left_key).chain(
        left.columns()
            .iter()
            .filter(|column| column.name().as_str() != key),
    );
    for column in ordered {
        names.push(column.name().to_string());
        columns.push(column.take(&left_rows).map_err(fail)?);
    }
    for column in right.columns() {
        if column.name().as_str() == key {
            continue;
        }
        let name = free_name(&names, column.name().as_str());
        names.push(name.clone());
        let taken = column.take(&right_rows).map_err(fail)?;
        columns.push(taken.with_name(name.into()));
    }
    DataFrame::new(height, columns).map_err(fail)
}

/// The rows of `right` after those of `left`. The columns of `left`, then the ones only
/// `right` has; a column one table lacks is empty in its rows; a column with another type
/// in each becomes text.
pub fn concat(left: &DataFrame, right: &DataFrame) -> Result<DataFrame, String> {
    let fail = |e: PolarsError| e.to_string();
    let mut names: Vec<PlSmallStr> = left.get_column_names_owned();
    for name in right.get_column_names_owned() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.len() >= left.width() + right.width() && left.width() > 0 && right.width() > 0 {
        return Err("The tables have no column in common".to_string());
    }
    let height = left.height() + right.height();
    let mut columns = Vec::with_capacity(names.len());
    for name in names {
        let (top, bottom) = (left.column(&name).ok(), right.column(&name).ok());
        let dtype = match (top, bottom) {
            (Some(a), Some(b)) if a.dtype() == b.dtype() => a.dtype().clone(),
            (Some(_), Some(_)) => DataType::String,
            (Some(a), None) => a.dtype().clone(),
            (None, Some(b)) => b.dtype().clone(),
            (None, None) => unreachable!("a column of one of them"),
        };
        let part = |column: Option<&Column>, rows: usize| -> Result<Column, String> {
            match column {
                Some(column) if column.dtype() == &dtype => Ok(column.clone()),
                Some(column) => {
                    let texts: StringChunked = cell_texts(column).into_iter().collect();
                    Ok(texts.with_name(name.clone()).into_column())
                }
                None => Ok(Column::full_null(name.clone(), rows, &dtype)),
            }
        };
        let mut column = part(top, left.height())?;
        column
            .append(&part(bottom, right.height())?)
            .map_err(fail)?;
        columns.push(column);
    }
    DataFrame::new(height, columns).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::{OtherTable, concat, join, shared_columns};
    use crate::dataframe::parse_table;
    use crate::sensitivity::Label;
    use polars::prelude::*;

    fn names(df: &DataFrame) -> Vec<String> {
        df.get_column_names()
            .into_iter()
            .map(|name| name.to_string())
            .collect()
    }

    fn texts(df: &DataFrame, column: &str) -> Vec<Option<String>> {
        let column = df.column(column).unwrap();
        (0..column.len())
            .map(|row| column.get(row).ok().and_then(crate::dataframe::cell_text))
            .collect()
    }

    #[test]
    fn an_inner_join_keeps_the_rows_with_a_match_and_a_left_join_all_of_the_left() {
        let orders =
            parse_table("order,customer,total\n1,c1,10\n2,c2,20\n3,c9,30\n4,c1,40").unwrap();
        let customers = parse_table("customer,name,total\nc1,Ann,x\nc2,Bob,y\nc3,Cor,z").unwrap();
        let inner = join(&orders, &customers, "customer", false).expect("join");
        assert_eq!(
            names(&inner),
            ["customer", "order", "total", "name", "total_right"]
        );
        assert_eq!(inner.height(), 3);
        assert_eq!(
            texts(&inner, "name"),
            [Some("Ann".into()), Some("Bob".into()), Some("Ann".into())]
        );
        // The left table's types stay.
        assert_eq!(inner.column("total").unwrap().dtype(), &DataType::Int64);
        let left = join(&orders, &customers, "customer", true).expect("left join");
        assert_eq!(left.height(), 4);
        assert_eq!(texts(&left, "name")[2], None);
        assert_eq!(texts(&left, "customer")[2], Some("c9".into()));
        // A key repeated on the right gives a row per match.
        let twice = parse_table("customer,tag\nc1,a\nc1,b").unwrap();
        assert_eq!(
            join(&orders, &twice, "customer", false).unwrap().height(),
            4
        );
        // Keys match on their text: a number key meets the same number as text.
        let numbers = parse_table("id,v\n1,a\n2,b").unwrap();
        let mixed = parse_table("id,w\n2,x\nq,y").unwrap();
        assert_eq!(join(&numbers, &mixed, "id", false).unwrap().height(), 1);
        assert_eq!(
            join(&orders, &customers, "order", false).err().as_deref(),
            Some("The other table has no column order")
        );
        let none = parse_table("customer,v\nzz,1").unwrap();
        assert_eq!(
            join(&orders, &none, "customer", false).err().as_deref(),
            Some("No row matches on customer")
        );
        assert_eq!(
            shared_columns(
                &["a".into(), "customer".into(), "b".into()],
                &["customer".into(), "a".into()]
            ),
            ["a", "customer"]
        );
    }

    #[test]
    fn concat_appends_rows_lining_up_columns_by_name() {
        let top = parse_table("name,n\nann,1\nbob,2").unwrap();
        let bottom = parse_table("n,city,name\n3,Delft,cor\nx,Ede,dirk").unwrap();
        let out = concat(&top, &bottom).expect("concat");
        assert_eq!(names(&out), ["name", "n", "city"]);
        assert_eq!(out.height(), 4);
        assert_eq!(
            texts(&out, "name"),
            ["ann", "bob", "cor", "dirk"].map(|s| Some(s.to_string()))
        );
        // Numbers on top, text below: the column becomes text.
        assert_eq!(out.column("n").unwrap().dtype(), &DataType::String);
        assert_eq!(texts(&out, "city")[0], None);
        let same = concat(&top, &top).unwrap();
        assert_eq!(same.column("n").unwrap().dtype(), &DataType::Int64);
        let apart = parse_table("x,y\n1,2").unwrap();
        assert_eq!(
            concat(&top, &apart).err().as_deref(),
            Some("The tables have no column in common")
        );
    }

    #[test]
    fn the_other_table_is_equal_only_to_itself_and_shows_its_shape_only() {
        let frame = parse_table("iban,n\nNL91ABNA0417164300,1").unwrap();
        let other = OtherTable::new(
            frame.clone(),
            vec![Label::Financial, Label::Pii, Label::Financial],
        );
        assert_eq!(other.labels, [Label::Pii, Label::Financial]);
        assert_eq!(other.clone(), other);
        assert_ne!(OtherTable::new(frame, vec![]), other);
        assert_eq!(other.described(), "a 1 × 2 table");
        let debug = format!("{other:?}");
        assert!(!debug.contains("NL91"), "{debug}");
    }
}
