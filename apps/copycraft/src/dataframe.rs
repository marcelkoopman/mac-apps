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
    let dates = read_dates(&mut df, DateOrder::DayFirst);
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

/// The whole grid of a table version (what Copy takes in the Dataframe view).
pub fn frame_grid(df: &DataFrame) -> Option<String> {
    render(df.clone())
}

/// A table version as CSV, for its sensitivity labels.
pub fn frame_csv(df: &DataFrame) -> Option<String> {
    write_csv(df.clone())
}

/// A table version as Parquet (Save in the Dataframe view, when Parquet is chosen).
pub fn frame_parquet(df: &DataFrame) -> Option<Vec<u8>> {
    write_parquet(&mut df.clone())
}

/// The file formats Save offers in the Dataframe view: CSV first (the default), then Parquet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableFile {
    Csv,
    Parquet,
}

impl TableFile {
    /// In the save panel's format popup, in this order; the first is the default.
    pub const ALL: [TableFile; 2] = [TableFile::Csv, TableFile::Parquet];

    pub fn title(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Parquet => "Parquet",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Parquet => "parquet",
        }
    }

    /// `df` as this file: CSV is UTF-8 with a header of the real column names, comma separated,
    /// dates as `yyyy-mm-dd` (also for a TSV or `;` source). `None` for an empty table.
    pub fn bytes(self, df: &DataFrame) -> Option<Vec<u8>> {
        match self {
            Self::Csv => frame_csv(df).map(String::into_bytes),
            Self::Parquet => frame_parquet(df),
        }
    }
}

/// The text parses as a non-empty table (what [`try_format`] needs), without rendering it.
pub fn is_table(text: &str) -> bool {
    parse(text).is_some_and(|df| df.width() > 0 && df.height() > 0)
}

#[cfg(test)]
pub fn try_parquet_bytes(text: &str) -> Option<Vec<u8>> {
    let mut df = parse_table(text)?;
    write_parquet(&mut df)
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
                "Dates read as mm/dd/yyyy".to_string()
            } else {
                "Dates read as dd/mm/yyyy".to_string()
            });
        }
        let forced = "Dates read as mm/dd/yyyy".to_string();
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
        .include_bom(false)
        .with_separator(b',')
        .with_date_format(Some("%Y-%m-%d".into()))
        .finish(&mut df)
        .ok()?;
    String::from_utf8(buf).ok()
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
