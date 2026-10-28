use std::io::Cursor;

use polars::prelude::*;

/// Polars display settings `main` sets before any thread starts (polars reads them from the
/// environment on every render): every row ([`try_format_preview`] limits rows itself), every
/// column, and no table width limit, so a wide table is not cut to 8 columns behind a "…"
/// column or squeezed into 100 characters with wrapped headers. The card scrolls sideways.
pub const DISPLAY_ENV: [(&str, &str); 3] = [
    ("POLARS_FMT_MAX_ROWS", "-1"),
    ("POLARS_FMT_MAX_COLS", "-1"),
    ("POLARS_TABLE_WIDTH", "-1"),
];

/// The whole drawn grid (tests: what the screen draws). Copy and Save take CSV.
#[cfg(test)]
pub fn try_format(text: &str) -> Option<String> {
    parse_table(text).and_then(render)
}

/// A table's first rows as a polars grid, for the card preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataframePreview {
    pub grid: String,
    /// Rows in the whole table.
    pub rows: usize,
    /// Rows in `grid`.
    pub shown_rows: usize,
    /// Text columns read as dates.
    pub dates: Vec<DateColumn>,
    /// The column overview ([`overview`]), when it is the view shown.
    pub overview: Option<String>,
    /// Columns in the table.
    pub columns: usize,
}

/// Like [`try_format`], but renders at most `max_rows` rows. Parsing is quick; rendering every
/// row of a large table is what takes time.
/// The card shows it: long shared prefixes of column names are shortened ([`display_names`]).
/// `overview`: show the column overview ([`shows_overview`]).
pub fn try_format_preview(
    text: &str,
    max_rows: usize,
    overview: Option<bool>,
) -> Option<DataframePreview> {
    let mut df = parse(text)?;
    read_iso_dates(&mut df);
    let order = if crate::settings::load().date_month_first {
        DateOrder::MonthFirst
    } else {
        DateOrder::DayFirst
    };
    let dates = read_dates(&mut df, order);
    crate::table_ops::type_text_columns(&mut df);
    let mut preview = frame_preview(&df, max_rows, overview)?;
    preview.dates = dates;
    Some(preview)
}

/// [`try_format_preview`] for a table version worked out already. The column overview is
/// worked out only when it is shown: `overview` is the entry's choice, `None` before there is
/// one ([`shows_overview`]).
pub fn frame_preview(
    df: &DataFrame,
    max_rows: usize,
    overview: Option<bool>,
) -> Option<DataframePreview> {
    let rows = df.height();
    let shown = df.head(Some(max_rows));
    let shown_rows = shown.height();
    Some(DataframePreview {
        grid: render(display_frame(shown))?,
        rows,
        shown_rows,
        dates: Vec::new(),
        overview: shows_overview(overview, df.width())
            .then(|| self::overview(df))
            .flatten(),
        columns: df.width(),
    })
}

/// A table version as Copy puts it on the clipboard: plain CSV with the separator of `source`
/// (the text it was read from), so the card knows it as a table again. Never the drawn grid.
pub fn frame_copy(df: &DataFrame, source: &str) -> Option<String> {
    write_csv_with(df.clone(), source_separator(source))
}

/// A part of a table version for the grid's right-click menu ([`frame_part`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TablePart {
    /// The cell at row, column.
    Cell(usize, usize),
    /// A row's values.
    Row(usize),
    /// A column's values (without its name).
    Column(usize),
}

/// The values of `part` as text, from the frame itself (not the drawn grid, whose cells may
/// be clipped with "…"): a cell as is, a row tab-separated, a column one value per line.
/// Empty cells are empty text. `None` when the row or column is not there.
pub fn frame_part(df: &DataFrame, part: TablePart) -> Option<String> {
    let value = |column: &Column, row: usize| -> Option<String> {
        column
            .get(row)
            .ok()
            .map(|value| cell_text(value).unwrap_or_default())
    };
    let columns = df.columns();
    match part {
        TablePart::Cell(row, col) => value(columns.get(col)?, row),
        TablePart::Row(row) => {
            if row >= df.height() {
                return None;
            }
            let cells: Option<Vec<String>> = columns.iter().map(|c| value(c, row)).collect();
            Some(cells?.join("\t"))
        }
        TablePart::Column(col) => {
            let column = columns.get(col)?;
            let cells: Option<Vec<String>> = (0..column.len()).map(|r| value(column, r)).collect();
            Some(cells?.join("\n"))
        }
    }
}

/// The separator of the table `source` (`,` `;` or tab); a comma when it has none.
pub fn source_separator(source: &str) -> u8 {
    delimited_separator(source.trim()).unwrap_or(b',')
}

/// A table version as CSV, for its sensitivity labels.
pub fn frame_csv(df: &DataFrame) -> Option<String> {
    write_csv(df.clone())
}

/// A table version as a CSV file (Save in the Dataframe view): UTF-8 with a header of the
/// real column names, `separator` between cells (the source's), dates as `yyyy-mm-dd`. `None`
/// for an empty table.
pub fn table_csv(df: &DataFrame, separator: u8) -> Option<Vec<u8>> {
    write_csv_with(df.clone(), separator).map(String::into_bytes)
}

/// What *Copy as JSON* / *Copy as CSV* make of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableCopy {
    /// A JSON array with one object per row, its keys the column names in order.
    Json,
    /// CSV with commas, quoted where a cell needs it.
    Csv,
}

/// A JSON array of objects (`[{…}, …]`), by its first and last characters only: cheap, for
/// the chips of a dropped file. Whether it reads as a table is up to [`parse`].
pub fn is_json_records(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.len() > 2
        && trimmed.ends_with(']')
        && trimmed
            .strip_prefix('[')
            .is_some_and(|rest| rest.trim_start().starts_with('{'))
}

/// `text`'s table as `copy`, read as it is (no dates or numbers typed: what the file says).
/// `None` when it is no table or does not fit (a nested JSON value has no CSV cell).
pub fn table_copy(text: &str, copy: TableCopy) -> Option<String> {
    frame_as(&parse(text)?, copy)
}

/// A table version as `copy` ([`table_copy`]).
pub fn frame_as(df: &DataFrame, copy: TableCopy) -> Option<String> {
    match copy {
        TableCopy::Json => frame_json(df),
        TableCopy::Csv => write_csv_with(df.clone(), b','),
    }
}

fn frame_json(df: &DataFrame) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    let mut buf = zeroize::Zeroizing::new(Vec::new());
    JsonWriter::new(&mut *buf)
        .with_json_format(JsonFormat::Json)
        .finish(&mut df.clone())
        .ok()?;
    // Pretty, with the keys in column order (serde_json keeps their order).
    let value: serde_json::Value = serde_json::from_slice(&buf).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// The text parses as a non-empty table (what [`try_format`] needs), without rendering it.
pub fn is_table(text: &str) -> bool {
    parse(text).is_some_and(|df| df.width() > 0 && df.height() > 0)
}

pub fn looks_like_csv(text: &str) -> bool {
    matches!(delimited_separator(text), Some(b',') | Some(b';'))
}

pub fn looks_like_tsv(text: &str) -> bool {
    delimited_separator(text) == Some(b'\t')
}

fn delimited_separator(text: &str) -> Option<u8> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    try_csv(trimmed).map(|(start, _)| start.separator)
}

/// Columns of the delimited table in `text`, from its header line alone (quotes respected): cheap,
/// no parse. `None` when the text is no CSV or TSV.
pub fn header_columns(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    if trimmed.is_empty() || crate::format::looks_like_xml(trimmed) {
        return None;
    }
    let start = table_start(trimmed)?;
    let header = start.body(trimmed).lines().next()?;
    Some(count_fields(header, start.separator))
}

fn count_fields(line: &str, separator: u8) -> usize {
    let mut quoted = false;
    let mut fields = 1;
    for byte in line.bytes() {
        if byte == b'"' {
            quoted = !quoted;
        } else if byte == separator && !quoted {
            fields += 1;
        }
    }
    fields
}

fn parse(text: &str) -> Option<DataFrame> {
    let trimmed = text.trim();
    if trimmed.is_empty() || crate::format::looks_like_xml(trimmed) {
        return None;
    }
    try_csv(trimmed)
        .map(|(_, df)| df)
        .or_else(|| try_json(trimmed))
}

/// The table as the Dataframe view shows it: [`parse`], with columns of ISO dates and datetimes
/// read as such ([`read_iso_dates`]) and of `dd/mm/yyyy` text as dates ([`read_dates`]; day
/// first where both orders fit). Conversions (CSV → JSON, TSV →
/// CSV) keep the text as copied and use [`parse`].
pub fn parse_table(text: &str) -> Option<DataFrame> {
    parse_table_with(text, ReadOptions::default()).map(|(df, _)| df)
}

/// How to read a copied table when the card was told: the order of dates that fit both orders.
/// The default reads such dates day first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ReadOptions {
    /// Dates that fit both orders are `mm/dd/yyyy`.
    pub month_first: bool,
}

/// What reading a table found, for the meta line and the Table ▾ menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadNotes {
    /// Where the delimited table starts (`None` for JSON).
    pub start: Option<TableStart>,
    /// A date column fits both orders.
    pub ambiguous_dates: bool,
    /// Those dates were read `mm/dd/yyyy`.
    pub month_first: bool,
    /// A date column had to be read `mm/dd/yyyy`: a value's second part is over 12.
    pub forced_month_first: bool,
}

impl ReadNotes {
    /// "Header on line N", and "Dates read as dd/mm/yyyy" (or mm/dd) when the date order was a
    /// choice, or "Dates read as mm/dd/yyyy" when a value forced that order.
    pub fn meta_notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .start
            .and_then(|start| start.note())
            .into_iter()
            .collect();
        if self.ambiguous_dates {
            notes.push(if self.month_first {
                crate::locale::t("dates_read_mdy").to_string()
            } else {
                crate::locale::t("dates_read_dmy").to_string()
            });
        }
        let forced = crate::locale::t("dates_read_mdy").to_string();
        if self.forced_month_first && !notes.contains(&forced) {
            notes.push(forced);
        }
        notes
    }
}

/// [`parse_table`] read as `options` says, with what reading found.
pub fn parse_table_with(text: &str, options: ReadOptions) -> Option<(DataFrame, ReadNotes)> {
    let trimmed = text.trim();
    if trimmed.is_empty() || crate::format::looks_like_xml(trimmed) {
        return None;
    }
    let (start, mut df) = match try_csv(trimmed) {
        Some((start, df)) => (Some(start), df),
        None => (None, try_json(trimmed)?),
    };
    let order = if options.month_first {
        DateOrder::MonthFirst
    } else {
        DateOrder::DayFirst
    };
    read_iso_dates(&mut df);
    let dates = read_dates(&mut df, order);
    // Numbers and dates copied as text (quoted, or with a decimal comma) typed as they are read:
    // one pass over the text columns, so this stays cheap.
    crate::table_ops::type_text_columns(&mut df);
    let notes = ReadNotes {
        start,
        ambiguous_dates: dates.iter().any(|column| column.ambiguous),
        month_first: options.month_first,
        forced_month_first: dates.iter().any(DateColumn::forced_month_first),
    };
    Some((df, notes))
}

fn write_csv(df: DataFrame) -> Option<String> {
    write_csv_with(df, b',')
}

fn write_csv_with(mut df: DataFrame, separator: u8) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf)
        .include_header(true)
        .include_bom(false)
        .with_separator(separator)
        .with_date_format(Some("%Y-%m-%d".into()))
        .with_datetime_format(Some("%Y-%m-%d %H:%M:%S".into()))
        .finish(&mut df)
        .ok()?;
    String::from_utf8(buf).ok()
}

fn render(df: DataFrame) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    // Drop Polars' leading "shape: (r, c)" line: the card meta already has rows × columns, and
    // with a frozen first column that line peeked past the pin as a stray orange "0)".
    Some(strip_polars_shape(&df.to_string()))
}

/// Remove a leading `shape: (…)` line from a Polars Debug grid.
fn strip_polars_shape(grid: &str) -> String {
    match grid.split_once('\n') {
        Some((first, rest)) if first.starts_with("shape:") => rest.to_string(),
        _ => grid.to_string(),
    }
}

include!("dataframe_dates.rs");
include!("dataframe_display.rs");
include!("dataframe_header.rs");
include!("dataframe_parse.rs");
