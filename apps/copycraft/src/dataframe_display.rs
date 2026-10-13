// How the card shows a table: column names with a long shared prefix shortened, and for wide
// tables a column overview. Display only: Copy, Save and the conversions keep the real names.

/// Whether the Dataframe view shows the column overview. Always off: Table shows the grid;
/// column picking is *Choose columns…* only. `choice` / `width` kept for call-site compatibility.
pub fn shows_overview(_choice: Option<bool>, _width: usize) -> bool {
    false
}
/// Longest "values" summary in the overview, so a row stays short.
pub const OVERVIEW_VALUES_CHARS: usize = 64;
/// Space between the overview's columns.
const OVERVIEW_GAP: &str = "   ";
/// The most common text value is cut to this many characters (with "…") in its summary.
const OVERVIEW_COMMON_CHARS: usize = 20;
/// A prefix is shortened when this many columns share it…
const PREFIX_MIN_COLUMNS: usize = 3;
/// …and it is at least this many characters long (separator included).
const PREFIX_MIN_CHARS: usize = 10;
/// Where a product or group name ends in a column name ("Sunbox 7 - PV1 Generation").
const PREFIX_SEPARATORS: [&str; 6] = [" - ", " – ", " — ", ": ", " | ", " / "];
/// Stands for the shortened prefix.
const ELLIPSIS: &str = "… ";

/// The prefixes of `name` that end in a separator, shortest first.
fn prefixes(name: &str) -> Vec<&str> {
    let mut ends: Vec<usize> = PREFIX_SEPARATORS
        .iter()
        .flat_map(|sep| name.match_indices(sep).map(move |(at, _)| at + sep.len()))
        .filter(|&end| end < name.len())
        .collect();
    ends.sort_unstable();
    ends.dedup();
    ends.into_iter().map(|end| &name[..end]).collect()
}

/// Column names for display: where at least [`PREFIX_MIN_COLUMNS`] names share a prefix of
/// [`PREFIX_MIN_CHARS`] or more up to a separator, it becomes "…" in each (the longest such
/// prefix per name, so each group is shortened on its own). Names stay as they are when
/// shortening would make two of them the same.
pub fn display_names(names: &[String]) -> Vec<String> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for name in names {
        for prefix in prefixes(name) {
            *counts.entry(prefix).or_default() += 1;
        }
    }
    let shown: Vec<String> = names
        .iter()
        .map(|name| {
            let shared = prefixes(name).into_iter().rev().find(|prefix| {
                prefix.chars().count() >= PREFIX_MIN_CHARS
                    && counts.get(prefix).copied().unwrap_or(0) >= PREFIX_MIN_COLUMNS
            });
            match shared {
                Some(prefix) => format!("{ELLIPSIS}{}", &name[prefix.len()..]),
                None => name.clone(),
            }
        })
        .collect();
    let unique: std::collections::HashSet<&String> = shown.iter().collect();
    if unique.len() == shown.len() {
        shown
    } else {
        names.to_vec()
    }
}

/// `df` with its columns renamed for display ([`display_names`]).
fn display_frame(df: DataFrame) -> DataFrame {
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let shown = display_names(&names);
    if shown == names {
        return df;
    }
    let mut df = df;
    if df.set_column_names(&shown).is_err() {
        // Not expected: `display_names` keeps the names unique.
        return df;
    }
    df
}

/// A cell as text; `None` for an empty one.
pub fn cell_text(value: AnyValue) -> Option<String> {
    match value {
        AnyValue::Null => None,
        AnyValue::String(text) => Some(text.to_string()),
        AnyValue::StringOwned(text) => Some(text.to_string()),
        other => Some(other.to_string()),
    }
}

/// The type of a column as the overview names it: number, whole number, date, datetime, text,
/// yes/no; polars' name for the others.
pub fn friendly_type(dtype: &DataType) -> String {
    match dtype {
        DataType::Float32 | DataType::Float64 => crate::locale::t("type_number").to_string(),
        dtype if dtype.is_integer() => crate::locale::t("type_whole_number").to_string(),
        dtype if dtype.is_decimal() => crate::locale::t("type_number").to_string(),
        DataType::Date => crate::locale::t("type_date").to_string(),
        DataType::Datetime(_, _) => crate::locale::t("type_datetime").to_string(),
        DataType::String => crate::locale::t("type_text").to_string(),
        DataType::Boolean => crate::locale::t("type_yes_no").to_string(),
        DataType::Null => crate::locale::t("type_empty").to_string(),
        other => other.to_string(),
    }
}

/// A number in a summary: whole floats keep one decimal (`0.0`, `12.0`), others their
/// shortest form (`13.68`), at most four decimals; very large or small ones in `e` notation.
fn number_text(value: &AnyValue) -> Option<String> {
    let float = match value {
        AnyValue::Float64(v) => *v,
        AnyValue::Float32(v) => f64::from(*v),
        AnyValue::Null => return None,
        other => return cell_text(other.clone()),
    };
    if !float.is_finite() {
        return Some(float.to_string());
    }
    let magnitude = float.abs();
    if magnitude != 0.0 && !(1e-4..1e15).contains(&magnitude) {
        return Some(format!("{float:.3e}"));
    }
    if float.fract() == 0.0 {
        return Some(format!("{float:.1}"));
    }
    let shortest = float.to_string();
    let decimals = shortest.split_once('.').map_or(0, |(_, d)| d.len());
    if decimals <= 4 {
        return Some(shortest);
    }
    let rounded = format!("{float:.4}");
    Some(rounded.trim_end_matches('0').trim_end_matches('.').to_string())
}

/// A date or datetime in a summary, without a zero fraction of a second.
fn moment_text(value: AnyValue) -> Option<String> {
    let text = cell_text(value)?;
    let trimmed = match text.split_once('.') {
        Some((head, fraction)) if fraction.chars().all(|c| c == '0') => head.to_string(),
        _ => text,
    };
    Some(trimmed)
}

/// `text` cut to `max` characters, the last one "…".
fn clipped(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A column's values in a few words, for the overview: numbers and dates as their range
/// ("0.0 – 13.68", "2026-03-19 – 2026-09-27"), yes/no as counts ("true 3 · false 2"), text as
/// "12 distinct · most common: Jan", one value as "always 0.0", then " · 3 empty" when cells
/// are empty. At most [`OVERVIEW_VALUES_CHARS`] characters, so none is cut mid-value.
pub fn values_summary(column: &Column) -> String {
    let empty = column.null_count();
    let filled = column.len() - empty;
    let empty_note = (empty > 0).then(|| crate::locale::tf("values_empty", &[&empty]));
    if filled == 0 {
        return empty_note.unwrap_or_default();
    }
    let dtype = column.dtype();
    let base = if dtype.is_primitive_numeric() || dtype.is_decimal() {
        range_summary(column, number_text)
    } else if dtype.is_temporal() && !matches!(dtype, DataType::Duration(_)) {
        range_summary(column, |value| moment_text(value.clone()))
    } else if dtype == &DataType::Boolean {
        bool_summary(column)
    } else {
        None
    }
    .unwrap_or_else(|| text_summary(column));
    let joined = match &empty_note {
        Some(note) => format!("{base} · {note}"),
        None => base.clone(),
    };
    if joined.chars().count() <= OVERVIEW_VALUES_CHARS {
        joined
    } else {
        clipped(&base, OVERVIEW_VALUES_CHARS)
    }
}

/// "min – max", or "always X" when they are the same.
fn range_summary(column: &Column, show: impl Fn(&AnyValue) -> Option<String>) -> Option<String> {
    let low = column.min_reduce().ok()?;
    let high = column.max_reduce().ok()?;
    let low = show(low.value())?;
    let high = show(high.value())?;
    Some(if low == high {
        crate::locale::tf("values_always", &[&low])
    } else {
        format!("{low} – {high}")
    })
}

fn bool_summary(column: &Column) -> Option<String> {
    let values = column.bool().ok()?;
    let yes = values.sum().unwrap_or(0) as usize;
    let no = values.len() - values.null_count() - yes;
    Some(match (yes, no) {
        (_, 0) => crate::locale::t("values_always_true").to_string(),
        (0, _) => crate::locale::t("values_always_false").to_string(),
        _ => crate::locale::tf("values_true_false", &[&yes, &no]),
    })
}

/// "N distinct · most common: X" (the first of the most common when several tie), "N distinct"
/// when every value is there once, "always X" for one value.
fn text_summary(column: &Column) -> String {
    let mut counts: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new();
    for row in 0..column.len() {
        if let Some(value) = column.get(row).ok().and_then(cell_text) {
            let next = counts.len();
            counts.entry(value).or_insert((0, next)).0 += 1;
        }
    }
    let distinct = counts.len();
    let common = counts
        .iter()
        .max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.cmp(&a.1.1)))
        .map(|(value, (count, _))| (value.as_str(), *count));
    match common {
        Some((value, _)) if distinct == 1 => {
            crate::locale::tf("values_always", &[&clipped(value, OVERVIEW_COMMON_CHARS)])
        }
        Some((value, count)) if count > 1 => crate::locale::tf(
            "values_distinct_common",
            &[&distinct, &clipped(value, OVERVIEW_COMMON_CHARS)],
        ),
        _ => crate::locale::tf("values_distinct", &[&distinct]),
    }
}

/// The column overview: a plain list, not a polars grid, so it does not read as a transposed
/// table. A line "20 columns · 192 rows", then the headings column, type and values, then one
/// aligned line per column with its (display) name, its type ([`friendly_type`]) and its values
/// in a few words ([`values_summary`]). Any table with a column has one; a wide one opens on
/// it ([`shows_overview`]). `None` without columns.
pub fn overview(df: &DataFrame) -> Option<String> {
    if df.width() == 0 {
        return None;
    }
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let types: Vec<String> = df.columns().iter().map(|c| friendly_type(c.dtype())).collect();
    let values: Vec<String> = df.columns().iter().map(values_summary).collect();
    let rows = df.height();
    let counts: [&dyn std::fmt::Display; 2] = [
        &crate::commands::group_thousands(names.len()),
        &crate::commands::group_thousands(rows),
    ];
    let head = if rows == 1 {
        crate::locale::tf("overview_head_one_row", &counts)
    } else {
        crate::locale::tf("overview_head", &counts)
    };
    Some(overview_list(&head, &display_names(&names), &types, &values))
}

/// The overview's headings: column, type, values.
pub fn overview_headings() -> (&'static str, &'static str, &'static str) {
    (
        crate::locale::t("overview_column"),
        crate::locale::t("overview_type"),
        crate::locale::t("overview_values"),
    )
}

/// `head`, a blank line, the headings and one line per column, padded to align.
fn overview_list(head: &str, names: &[String], types: &[String], values: &[String]) -> String {
    let width = |cells: &[String], heading: &str| {
        cells
            .iter()
            .map(|cell| cell.chars().count())
            .chain([heading.len()])
            .max()
            .unwrap_or(0)
    };
    let (column_head, type_head, values_head) = overview_headings();
    let name_w = width(names, column_head);
    let type_w = width(types, type_head);
    let line = |name: &str, kind: &str, value: &str| {
        let text = format!("{name:<name_w$}{OVERVIEW_GAP}{kind:<type_w$}{OVERVIEW_GAP}{value}");
        text.trim_end().to_string()
    };
    let mut out = vec![head.to_string(), String::new(), line(column_head, type_head, values_head)];
    for ((name, kind), value) in names.iter().zip(types).zip(values) {
        out.push(line(name, kind, value));
    }
    out.join("\n")
}

/// Where a column ends on a line of a polars grid: the first column separator after the left
/// border (`┬` in the top border, `┆` in the rows, `╪` under the header, `┴` at the bottom).
const COLUMN_SEPARATORS: [char; 4] = ['┬', '┆', '╪', '┴'];

/// One line of the frozen first column ([`frozen_column`]), in UTF-16 units of the grid text
/// (what AppKit counts in): the part shown up to and including the first column separator,
/// and the line's newline, when it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenLine {
    pub start: usize,
    pub end: usize,
    pub newline: Option<usize>,
}

/// The first column of a polars grid (the card's table view), line by line, so the card can
/// keep it in view while the grid scrolls sideways. Lines outside the box keep nothing.
/// `None` when `grid` has no box with at least two columns (one column has nothing to scroll past).
pub fn frozen_column(grid: &str) -> Option<Vec<FrozenLine>> {
    let top = grid.lines().find(|line| line.starts_with('┌'))?;
    if !top.contains(COLUMN_SEPARATORS) {
        return None;
    }
    let total = grid.encode_utf16().count();
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in grid.split('\n') {
        let text = line.strip_suffix('\r').unwrap_or(line);
        let boxed = text.starts_with(['┌', '│', '╞', '├', '└']);
        let cut = if boxed {
            text.char_indices()
                .find(|(_, c)| COLUMN_SEPARATORS.contains(c))
                .map_or(text.len(), |(at, c)| at + c.len_utf8())
        } else {
            0
        };
        let utf16 = |s: &str| s.encode_utf16().count();
        let end = offset + utf16(&text[..cut]);
        let line_len = utf16(line);
        let newline = (offset + line_len < total).then_some(offset + line_len);
        lines.push(FrozenLine {
            start: offset,
            end,
            newline,
        });
        offset += line_len + 1;
    }
    Some(lines)
}

/// One polars grid, split into columns for the table window. The window paints this itself
/// (aligned columns, no box drawing). `None` when `grid` is not that box, so the window can
/// keep the plain grid.
#[derive(Debug)]
pub(crate) struct GridSheet {
    pub headers: Vec<String>,
    pub numeric: Vec<bool>,
    pub rows: Vec<Vec<String>>,
}

/// Column widths and the tab-stopped header and body. `stops` are `(location, right_aligned)`
/// in text-container points, strictly increasing. A numeric column, including the first, is a
/// right stop; a text column after the first is a left stop. The first text column has no stop.
#[derive(Debug)]
pub(crate) struct TableLayout {
    pub widths: Vec<f64>,
    pub stops: Vec<(f64, bool)>,
    pub header: String,
    pub body: String,
}

impl TableLayout {
    /// Width of the cells, including the gaps between columns, in the same points as [`Self::stops`].
    pub(crate) fn content_width(&self) -> f64 {
        let gaps = COL_GAP * self.widths.len().saturating_sub(1) as f64;
        self.widths.iter().copied().sum::<f64>() + gaps
    }

    /// Where the thin column separators go, in the same points as [`Self::stops`]: in the
    /// middle of each gap, so every cell has the same padding on both sides. None after the
    /// last column.
    pub(crate) fn separators(&self) -> Vec<f64> {
        let mut x = 0.0;
        let mut lines = Vec::with_capacity(self.widths.len().saturating_sub(1));
        for (index, width) in self.widths.iter().enumerate() {
            x += width;
            if index + 1 < self.widths.len() {
                lines.push((x + COL_GAP / 2.0).round());
                x += COL_GAP;
            }
        }
        lines
    }
}

const COL_GAP: f64 = 16.0;
const TEXT_MIN: f64 = 36.0;
const TEXT_MAX: f64 = 240.0;
const TEXT_PAD: f64 = 4.0;
const NUM_PAD: f64 = 8.0;

/// Parse a polars UTF8 grid: names from the first `│` row above `╞`, dtypes from the last
/// non-dash row of that header, then one `│` row per record until the bottom border.
pub(crate) fn grid_sheet(grid: &str) -> Option<GridSheet> {
    let mut header_lines: Vec<Vec<String>> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut in_body = false;
    let mut saw_rule = false;
    for line in grid.lines() {
        let line = line.trim_end().trim_end_matches('\r');
        if line.is_empty() || line.starts_with("shape:") {
            continue;
        }
        if line.starts_with('╞') {
            saw_rule = true;
            in_body = true;
            continue;
        }
        if line.starts_with('└') {
            break;
        }
        if !line.starts_with('│') {
            continue;
        }
        let cells = split_grid_cells(line);
        if in_body {
            rows.push(cells);
        } else {
            header_lines.push(cells);
        }
    }
    if !saw_rule || header_lines.is_empty() {
        return None;
    }
    let headers = header_lines.first()?.clone();
    if headers.is_empty() || headers.iter().all(String::is_empty) {
        return None;
    }
    let width = headers.len();
    let dtype = header_lines.iter().enumerate().rev().find(|(index, line)| {
        *index > 0 && line.iter().any(|cell| !cell.is_empty() && !is_dash_cell(cell))
    });
    let mut numeric = vec![false; width];
    if let Some((_, dtype)) = dtype {
        for (index, cell) in dtype.iter().enumerate().take(width) {
            numeric[index] = dtype_is_numeric(cell);
        }
    } else {
        for (index, slot) in numeric.iter_mut().enumerate() {
            *slot = column_is_numeric(&rows, index);
        }
    }
    for row in &mut rows {
        if row.len() < width {
            row.resize(width, String::new());
        } else {
            row.truncate(width);
        }
    }
    Some(GridSheet {
        headers,
        numeric,
        rows,
    })
}

/// Fixed column widths from `em` (a wide letter) and `digit` (a figure). Text is clipped to
/// [`TEXT_MAX`]; a number keeps its full width. Cells that do not fit gain a tail ellipsis.
pub(crate) fn layout_table(sheet: &GridSheet, em: f64, digit: f64) -> TableLayout {
    let em = em.max(1.0);
    let digit = digit.max(1.0);
    let width_count = sheet.headers.len();
    let mut widths = Vec::with_capacity(width_count);
    for index in 0..width_count {
        let numeric = sheet.numeric.get(index).copied().unwrap_or(false);
        let header_chars = sheet
            .headers
            .get(index)
            .map(|cell| cell.chars().count())
            .unwrap_or(0);
        let mut cell_chars = 0usize;
        for row in &sheet.rows {
            let count = row.get(index).map(|cell| cell.chars().count()).unwrap_or(0);
            cell_chars = cell_chars.max(count);
        }
        let width = if numeric {
            let number = cell_chars as f64 * digit + NUM_PAD;
            let header = (header_chars as f64 * em + TEXT_PAD).min(TEXT_MAX);
            number.max(header).max(TEXT_MIN)
        } else {
            let chars = cell_chars.max(header_chars);
            (chars as f64 * em + TEXT_PAD).clamp(TEXT_MIN, TEXT_MAX)
        };
        widths.push(width);
    }

    let mut headers = Vec::with_capacity(width_count);
    let mut clipped_rows = vec![vec![String::new(); width_count]; sheet.rows.len()];
    for index in 0..width_count {
        let numeric = sheet.numeric.get(index).copied().unwrap_or(false);
        let header = sheet.headers.get(index).map(String::as_str).unwrap_or("");
        let header_fit = chars_that_fit(widths[index], em, TEXT_PAD);
        headers.push(clip_tail(header, header_fit));
        let cell_fit = if numeric {
            usize::MAX
        } else {
            chars_that_fit(widths[index], em, TEXT_PAD)
        };
        for (row_index, row) in sheet.rows.iter().enumerate() {
            let cell = row.get(index).map(String::as_str).unwrap_or("");
            clipped_rows[row_index][index] = if numeric {
                cell.to_string()
            } else {
                clip_tail(cell, cell_fit)
            };
        }
    }

    let mut stops = Vec::new();
    let mut x = 0.0;
    let mut last = 0.0;
    for (index, column_width) in widths.iter().enumerate() {
        let is_numeric = sheet.numeric.get(index).copied().unwrap_or(false);
        if is_numeric {
            let mut location = x + column_width;
            if location <= last {
                location = last + 0.5;
            }
            stops.push((location, true));
            last = location;
        } else if index > 0 {
            let mut location = x;
            if location <= last {
                location = last + 0.5;
            }
            stops.push((location, false));
            last = location;
        }
        x += column_width + COL_GAP;
    }
    let header = join_tab_row(&headers, &sheet.numeric);
    let body = clipped_rows
        .iter()
        .map(|row| join_tab_row(row, &sheet.numeric))
        .collect::<Vec<_>>()
        .join("\n");
    TableLayout {
        widths,
        stops,
        header,
        body,
    }
}

fn split_grid_cells(line: &str) -> Vec<String> {
    let line = line.trim().strip_prefix('│').unwrap_or(line.trim());
    let line = line.strip_suffix('│').unwrap_or(line);
    line.split('┆')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn is_dash_cell(cell: &str) -> bool {
    cell.chars().any(|c| c == '-') && cell.chars().all(|c| c == '-' || c.is_whitespace())
}

/// Polars dtype names: `i64`, `u32`, `f64`, `decimal[p,s]`. Dates, text and bool stay on the left.
fn dtype_is_numeric(dtype: &str) -> bool {
    let dtype = dtype.trim();
    if dtype.starts_with("decimal") {
        return true;
    }
    let mut chars = dtype.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, 'i' | 'u' | 'f') && chars.all(|c| c.is_ascii_digit()) && dtype.len() > 1
}

fn column_is_numeric(rows: &[Vec<String>], index: usize) -> bool {
    let mut any = false;
    for row in rows {
        let cell = row.get(index).map(String::as_str).unwrap_or("");
        if cell.is_empty() {
            continue;
        }
        if !plain_number(cell) {
            return false;
        }
        any = true;
    }
    any
}

fn plain_number(cell: &str) -> bool {
    let mut chars = cell.chars().peekable();
    if matches!(chars.peek(), Some('+' | '-')) {
        chars.next();
    }
    let mut digits = 0usize;
    let mut separator = false;
    for ch in chars {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        if (ch == '.' || ch == ',') && !separator {
            separator = true;
            continue;
        }
        return false;
    }
    digits > 0
}

fn chars_that_fit(width: f64, unit: f64, pad: f64) -> usize {
    let room = (width - pad) / unit.max(1.0);
    if room.is_sign_negative() {
        0
    } else {
        room.floor() as usize
    }
}

fn clip_tail(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let keep = max_chars - 1;
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

/// A row of cells. A numeric first cell starts with a tab (its right stop); every later cell
/// starts with a tab. No trailing newline.
fn join_tab_row(cells: &[String], numeric: &[bool]) -> String {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        let is_numeric = numeric.get(index).copied().unwrap_or(false);
        if index > 0 || is_numeric {
            line.push('\t');
        }
        line.push_str(cell);
    }
    line
}

