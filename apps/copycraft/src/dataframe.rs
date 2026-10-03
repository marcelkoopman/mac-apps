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
    /// The column overview of a wide table ([`overview`]).
    pub overview: Option<String>,
    /// Columns in the table.
    pub columns: usize,
}

/// Like [`try_format`], but renders at most `max_rows` rows. Parsing is quick; rendering every
/// row of a large table is what takes time.
/// The card shows it: long shared prefixes of column names are shortened ([`display_names`]).
pub fn try_format_preview(text: &str, max_rows: usize) -> Option<DataframePreview> {
    let mut df = parse(text)?;
    let dates = read_dates(&mut df, DateOrder::DayFirst);
    let mut preview = frame_preview(&df, max_rows)?;
    preview.dates = dates;
    Some(preview)
}

/// [`try_format_preview`] for a table version worked out already.
pub fn frame_preview(df: &DataFrame, max_rows: usize) -> Option<DataframePreview> {
    let rows = df.height();
    let shown = df.head(Some(max_rows));
    let shown_rows = shown.height();
    Some(DataframePreview {
        grid: render(display_frame(shown))?,
        rows,
        shown_rows,
        dates: Vec::new(),
        overview: overview(df),
        columns: df.width(),
    })
}

/// The whole grid of a table version (what Copy takes in the Dataframe view).
pub fn frame_grid(df: &DataFrame) -> Option<String> {
    render(df.clone())
}

/// A table version as CSV, for its sensitivity labels.
pub fn frame_csv(df: &DataFrame) -> Option<String> {
    write_csv(df.clone())
}

/// A table version as Parquet (Save in the Dataframe view).
pub fn frame_parquet(df: &DataFrame) -> Option<Vec<u8>> {
    write_parquet(&mut df.clone())
}

/// The text parses as a non-empty table (what [`try_format`] needs), without rendering it.
pub fn is_table(text: &str) -> bool {
    parse(text).is_some_and(|df| df.width() > 0 && df.height() > 0)
}

pub fn try_csv_text(text: &str) -> Option<String> {
    parse(text).and_then(write_csv)
}

pub fn try_parquet_bytes(text: &str) -> Option<Vec<u8>> {
    let mut df = parse_table(text)?;
    write_parquet(&mut df)
}

pub fn try_json_text(text: &str) -> Option<String> {
    parse(text).and_then(write_json)
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

fn parse(text: &str) -> Option<DataFrame> {
    let trimmed = text.trim();
    if trimmed.is_empty() || crate::format::looks_like_xml(trimmed) {
        return None;
    }
    try_csv(trimmed)
        .map(|(_, df)| df)
        .or_else(|| try_json(trimmed))
}

/// The table as the Dataframe view shows it: [`parse`], with columns of `dd/mm/yyyy` text read
/// as dates ([`read_dates`]; day first where both orders fit). Conversions (CSV → JSON, TSV →
/// CSV) keep the text as copied and use [`parse`].
pub fn parse_table(text: &str) -> Option<DataFrame> {
    parse_table_with(text, ReadOptions::default()).map(|(df, _)| df)
}

/// How to read a copied table when the card was told: the header's line and the order of dates
/// that fit both orders. The default finds the header and reads such dates day first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ReadOptions {
    /// The header's line (0-based in the trimmed text, blank lines counted).
    pub header_line: Option<usize>,
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
    /// Lines the header could be on (the first lines of the trimmed text).
    pub header_lines: usize,
}

impl ReadNotes {
    /// "Header on line N" and "Dates read as dd/mm/yyyy", when either was a choice.
    pub fn meta_notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .start
            .and_then(|start| start.note())
            .into_iter()
            .collect();
        if self.ambiguous_dates {
            notes.push(if self.month_first {
                "Dates read as mm/dd/yyyy".to_string()
            } else {
                "Dates read as dd/mm/yyyy".to_string()
            });
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
    let (start, mut df) = match options.header_line {
        Some(line) => {
            let start = table_start_at(trimmed, line)?;
            (Some(start), csv_at(trimmed, start)?)
        }
        None => match try_csv(trimmed) {
            Some((start, df)) => (Some(start), df),
            None => (None, try_json(trimmed)?),
        },
    };
    let order = if options.month_first {
        DateOrder::MonthFirst
    } else {
        DateOrder::DayFirst
    };
    let dates = read_dates(&mut df, order);
    let notes = ReadNotes {
        start,
        ambiguous_dates: dates.iter().any(|column| column.ambiguous),
        month_first: options.month_first,
        header_lines: trimmed.split('\n').take(MAX_PREAMBLE_LINES + 2).count(),
    };
    Some((df, notes))
}

fn write_parquet(df: &mut DataFrame) -> Option<Vec<u8>> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    let mut buf = Cursor::new(Vec::new());
    ParquetWriter::new(&mut buf).finish(df).ok()?;
    Some(buf.into_inner())
}

fn write_csv(mut df: DataFrame) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf)
        .include_header(true)
        .finish(&mut df)
        .ok()?;
    String::from_utf8(buf).ok()
}

fn write_json(mut df: DataFrame) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    let mut buf = Vec::new();
    JsonWriter::new(&mut buf)
        .with_json_format(JsonFormat::Json)
        .finish(&mut df)
        .ok()?;
    crate::clipboard::try_format_json(&String::from_utf8(buf).ok()?)
}

fn render(df: DataFrame) -> Option<String> {
    if df.width() == 0 || df.height() == 0 {
        return None;
    }
    Some(df.to_string())
}

include!("dataframe_dates.rs");
include!("dataframe_display.rs");
include!("dataframe_header.rs");
include!("dataframe_parse.rs");
