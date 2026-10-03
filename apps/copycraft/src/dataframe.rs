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
}

/// Like [`try_format`], but renders at most `max_rows` rows. Parsing is quick; rendering every
/// row of a large table is what takes time.
pub fn try_format_preview(text: &str, max_rows: usize) -> Option<DataframePreview> {
    let mut df = parse(text)?;
    let dates = read_dates(&mut df, DateOrder::DayFirst);
    let rows = df.height();
    let shown = if rows > max_rows {
        df.head(Some(max_rows))
    } else {
        df
    };
    let shown_rows = shown.height();
    Some(DataframePreview {
        grid: render(shown)?,
        rows,
        shown_rows,
        dates,
    })
}

/// [`try_format_preview`] for a table version worked out already.
pub fn frame_preview(df: &DataFrame, max_rows: usize) -> Option<DataframePreview> {
    let rows = df.height();
    let shown = df.head(Some(max_rows));
    let shown_rows = shown.height();
    Some(DataframePreview {
        grid: render(shown)?,
        rows,
        shown_rows,
        dates: Vec::new(),
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
    let mut df = parse(text)?;
    read_dates(&mut df, DateOrder::DayFirst);
    Some(df)
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
include!("dataframe_header.rs");
include!("dataframe_parse.rs");
