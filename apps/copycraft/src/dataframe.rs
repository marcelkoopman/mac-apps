use std::io::Cursor;

use polars::prelude::*;

pub fn try_format(text: &str) -> Option<String> {
    parse(text).and_then(render)
}

pub fn try_csv_text(text: &str) -> Option<String> {
    parse(text).and_then(write_csv)
}

pub fn try_parquet_bytes(text: &str) -> Option<Vec<u8>> {
    let mut df = parse(text)?;
    write_parquet(&mut df)
}

/// Tabular XML can still become CSV. It is not a dataframe.
pub fn try_xml_csv_text(text: &str) -> Option<String> {
    try_xml(text.trim()).and_then(write_csv)
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
    let separator = detect_separator(trimmed)?;
    try_csv(trimmed)?;
    Some(separator)
}

fn parse(text: &str) -> Option<DataFrame> {
    let trimmed = text.trim();
    if trimmed.is_empty() || crate::format::looks_like_xml(trimmed) {
        return None;
    }
    try_csv(trimmed).or_else(|| try_json(trimmed))
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

include!("dataframe_parse.rs");
