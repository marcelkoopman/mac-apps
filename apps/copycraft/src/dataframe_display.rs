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
        DataType::Float32 | DataType::Float64 => "number".to_string(),
        dtype if dtype.is_integer() => "whole number".to_string(),
        dtype if dtype.is_decimal() => "number".to_string(),
        DataType::Date => "date".to_string(),
        DataType::Datetime(_, _) => "datetime".to_string(),
        DataType::String => "text".to_string(),
        DataType::Boolean => "yes/no".to_string(),
        DataType::Null => "empty".to_string(),
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
    let empty_note = (empty > 0).then(|| format!("{empty} empty"));
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
        format!("always {low}")
    } else {
        format!("{low} – {high}")
    })
}

fn bool_summary(column: &Column) -> Option<String> {
    let values = column.bool().ok()?;
    let yes = values.sum().unwrap_or(0) as usize;
    let no = values.len() - values.null_count() - yes;
    Some(match (yes, no) {
        (_, 0) => "always true".to_string(),
        (0, _) => "always false".to_string(),
        _ => format!("true {yes} · false {no}"),
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
            format!("always {}", clipped(value, OVERVIEW_COMMON_CHARS))
        }
        Some((value, count)) if count > 1 => format!(
            "{distinct} distinct · most common: {}",
            clipped(value, OVERVIEW_COMMON_CHARS)
        ),
        _ => format!("{distinct} distinct"),
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
    let head = format!(
        "{} columns · {} {}",
        crate::commands::group_thousands(names.len()),
        crate::commands::group_thousands(rows),
        if rows == 1 { "row" } else { "rows" }
    );
    Some(overview_list(&head, &display_names(&names), &types, &values))
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
    let name_w = width(names, "column");
    let type_w = width(types, "type");
    let line = |name: &str, kind: &str, value: &str| {
        let text = format!("{name:<name_w$}{OVERVIEW_GAP}{kind:<type_w$}{OVERVIEW_GAP}{value}");
        text.trim_end().to_string()
    };
    let mut out = vec![head.to_string(), String::new(), line("column", "type", "values")];
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
/// keep it in view while the grid scrolls sideways. Lines outside the box ("shape: (4, 3)")
/// keep nothing: they scroll like any text. `None` when `grid` has no box with at least two
/// columns (one column has nothing to scroll past).
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

