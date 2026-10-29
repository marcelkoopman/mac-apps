//! Search in the table window: which rows of the grid match the query, and the size on the meta
//! line while a filter is on ("120 of 64,659 rows × 8 columns"). A view only: the table, its
//! versions, Copy, Copy as JSON/CSV and Save stay the whole version. Nothing here is logged.

use super::table::{META_SEPARATOR, group_thousands};

/// The items of the table window's toolbar, in order (`macos_table_window::toolbar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableToolbarItem {
    /// *Table ▾*.
    Table,
    Copy,
    CopyJson,
    CopyCsv,
    Save,
    /// A flexible space.
    Space,
    /// The "Click the table to reveal it" hint while masked.
    Hint,
    /// The version group (↶, the version list, ↷).
    Versions,
    /// The search field (⌘F), last as in other Mac apps.
    Search,
}

/// Table ▾, Copy, Copy as JSON, Copy as CSV, Save, the hint between flexible spaces, the
/// version group and the search field.
pub fn table_window_toolbar_items() -> [TableToolbarItem; 10] {
    use TableToolbarItem::*;
    [
        Table, Copy, CopyJson, CopyCsv, Save, Space, Hint, Space, Versions, Search,
    ]
}

/// The query as it is matched: trimmed and lower-cased; `None` when nothing is left (all rows).
pub fn table_search_needle(query: &str) -> Option<String> {
    let query = query.trim();
    (!query.is_empty()).then(|| query.to_lowercase())
}

/// `hay` contains `needle` (already lower-cased, [`table_search_needle`]) ignoring case. No
/// allocation: ASCII text compares bytes, other text compares lower-cased characters.
pub fn contains_ignore_case(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if hay.is_ascii() && needle.is_ascii() {
        let (hay, needle) = (hay.as_bytes(), needle.as_bytes());
        return hay.len() >= needle.len()
            && hay
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle));
    }
    hay.char_indices().any(|(at, _)| {
        let mut rest = hay[at..].chars().flat_map(char::to_lowercase);
        needle.chars().all(|want| rest.next() == Some(want))
    })
}

/// The indices of `rows` with a cell (its shown text) that contains `query`, ignoring case.
/// `None` for an empty query: every row shows.
pub fn matching_rows<S: AsRef<str>>(rows: &[Vec<S>], query: &str) -> Option<Vec<usize>> {
    let mut needle = table_search_needle(query)?;
    let found = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.iter()
                .any(|cell| contains_ignore_case(cell.as_ref(), &needle))
        })
        .map(|(index, _)| index)
        .collect();
    zeroize::Zeroize::zeroize(&mut needle);
    Some(found)
}

/// The lines of `body` (one per row) at `keep` (increasing), joined again.
pub fn keep_lines(body: &str, keep: &[usize]) -> String {
    let mut out = String::with_capacity(body.len().min(keep.len().saturating_mul(64)));
    let mut wanted = keep.iter().copied().peekable();
    for (index, line) in body.split('\n').enumerate() {
        let Some(&next) = wanted.peek() else {
            break;
        };
        if index == next {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
            wanted.next();
        }
    }
    out
}

/// "120 of 64,659 rows × 8 columns" (NL "120 van 64.659 rijen × 8 kolommen").
pub fn filtered_size(shown: usize, rows: usize, columns: usize) -> String {
    let t = crate::locale::t;
    let rows_word = if rows == 1 { t("row") } else { t("rows") };
    let columns_word = if columns == 1 {
        t("column")
    } else {
        t("columns")
    };
    format!(
        "{} × {columns} {columns_word}",
        crate::locale::tf(
            "table_rows_found",
            &[&group_thousands(shown), &group_thousands(rows), &rows_word]
        )
    )
}

/// `meta` with its size (before the first separator) said as [`filtered_size`].
pub fn filtered_meta(meta: &str, shown: usize, rows: usize, columns: usize) -> String {
    let size_end = meta.find(META_SEPARATOR).unwrap_or(meta.len());
    format!(
        "{}{}",
        filtered_size(shown, rows, columns),
        &meta[size_end..]
    )
}

/// The table row of drawn row `row`: itself without a filter, else the `row`th match.
pub fn table_row_shown(matches: Option<&[usize]>, row: usize) -> Option<usize> {
    match matches {
        None => Some(row),
        Some(matches) => matches.get(row).copied(),
    }
}
