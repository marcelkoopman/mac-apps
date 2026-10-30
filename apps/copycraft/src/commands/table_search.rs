//! Search in the table window: which rows of the grid match the query, and the size on the meta
//! line while a filter is on ("120 of 64,659 rows × 8 columns"). A view only: the table, its
//! versions, Copy, Copy as JSON/CSV and Save stay the whole version. Nothing here is logged.

use super::table::{META_SEPARATOR, group_thousands};

/// The items of the table window's toolbar, in order (`macos_table_window::toolbar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableToolbarItem {
    /// *Table ▾*.
    Table,
    /// *Chart*: the version shown as a chart (hidden without Swift Charts in the build).
    Chart,
    /// *Profile*: missing values, duplicates, types and outliers of the version shown.
    Profile,
    Copy,
    CopyJson,
    CopyCsv,
    Save,
    /// A flexible space.
    Space,
    /// The "Click the table to reveal it" hint while masked.
    Hint,
    /// The search field (⌘F), last as in other Mac apps.
    Search,
}

/// Table ▾, Chart, Profile, Copy, Copy as JSON, Copy as CSV, Save, the hint between flexible spaces and the
/// search field. The versions are the strip along the bottom ([`super::table_version_tiles`]).
pub fn table_window_toolbar_items() -> [TableToolbarItem; 11] {
    use TableToolbarItem::*;
    [
        Table, Chart, Profile, Copy, CopyJson, CopyCsv, Save, Space, Hint, Space, Search,
    ]
}

/// Whether *Chart* is on for `view`: its grid is there and its columns allow a chart.
pub fn table_window_chart_on(view: &super::TableWindowView) -> bool {
    !view.grid.is_empty() && crate::chart_data::ChartRequest::first(&view.chart_columns).is_some()
}

/// Whether *Profile* is on for `view`: its grid is there and read as a table (any frame: CSV,
/// TSV, a JSON table, a pasted grid).
pub fn table_window_profile_on(view: &super::TableWindowView) -> bool {
    !view.grid.is_empty() && !view.chart_columns.is_empty()
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
#[cfg(test)]
pub fn matching_rows<S: AsRef<str>>(rows: &[Vec<S>], query: &str) -> Option<Vec<usize>> {
    matching_rows_while(rows, query, || true)
}

/// Rows between two looks at whether a search on a thread is still wanted.
pub const SEARCH_CHECK_ROWS: usize = 1_000;

/// [`matching_rows`] that asks `wanted` every [`SEARCH_CHECK_ROWS`] rows and stops (`None`)
/// once it says no: a search on a thread that a newer one replaced, or a closed window.
pub fn matching_rows_while<S: AsRef<str>>(
    rows: &[Vec<S>],
    query: &str,
    wanted: impl Fn() -> bool,
) -> Option<Vec<usize>> {
    let mut needle = table_search_needle(query)?;
    let mut found = Vec::new();
    let mut stopped = false;
    for (index, row) in rows.iter().enumerate() {
        if index % SEARCH_CHECK_ROWS == 0 && !wanted() {
            stopped = true;
            break;
        }
        if row
            .iter()
            .any(|cell| contains_ignore_case(cell.as_ref(), &needle))
        {
            found.push(index);
        }
    }
    zeroize::Zeroize::zeroize(&mut needle);
    (!stopped).then_some(found)
}

/// Above this many cells (rows × columns) the search runs on a thread of its own, so typing
/// in the search field never waits for a large table.
pub const SEARCH_ON_THREAD_CELLS: usize = 20_000;

/// Whether a search of `rows` × `columns` runs on a thread ([`SEARCH_ON_THREAD_CELLS`]).
pub fn search_on_thread(rows: usize, columns: usize) -> bool {
    rows.saturating_mul(columns.max(1)) > SEARCH_ON_THREAD_CELLS
}

/// The rows matching `query` and the body with only their lines: what the table window
/// draws while a search filters it. `None` for an empty query, or when `wanted` stopped it
/// ([`matching_rows_while`]).
pub fn filter_sheet<S: AsRef<str>>(
    rows: &[Vec<S>],
    body: &str,
    query: &str,
    wanted: impl Fn() -> bool,
) -> Option<(Vec<usize>, zeroize::Zeroizing<String>)> {
    let matches = matching_rows_while(rows, query, &wanted)?;
    if !wanted() {
        return None;
    }
    let kept = zeroize::Zeroizing::new(keep_lines(body, &matches));
    Some((matches, kept))
}

/// Numbers the table window's searches on a thread, so an older result never replaces a
/// newer one: only the result of the newest [`start`](Self::start) is
/// [`current`](Self::current). A search done on the main thread, closing the window or
/// Clear also [`start`](Self::start) (and so make any on a thread stale). Shared with the
/// search threads (a clone is the same counter), which stop once theirs is no longer current.
#[derive(Debug, Default, Clone)]
pub struct SearchGeneration(std::sync::Arc<std::sync::atomic::AtomicU64>);

impl SearchGeneration {
    pub fn start(&self) -> u64 {
        use std::sync::atomic::Ordering;
        self.0.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }

    pub fn current(&self, generation: u64) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire) == generation
    }
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
    let found = crate::locale::tf(
        "table_rows_found",
        &[&group_thousands(shown), &group_thousands(rows), &rows_word],
    );
    format!("{found} × {columns} {columns_word}")
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
