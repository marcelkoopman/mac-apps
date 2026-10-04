/// The delimited table in `text` (see [`table_start`]), read from its header down.
fn try_csv(text: &str) -> Option<(TableStart, DataFrame)> {
    let start = table_start(text)?;
    csv_at(text, start).map(|df| (start, df))
}

/// The delimited table in `text` from the header at `start` down.
fn csv_at(text: &str, start: TableStart) -> Option<DataFrame> {
    let read = |infer: Option<usize>| {
        let mut cursor = Cursor::new(start.body(text).as_bytes());
        let parse = CsvParseOptions::default().with_separator(start.separator);
        CsvReadOptions::default()
            .with_has_header(true)
            .with_parse_options(parse)
            .with_infer_schema_length(infer)
            .into_reader_with_file_handle(&mut cursor)
            .finish()
            .ok()
    };
    let mut df = read(Some(100))?;
    if !(df.width() >= 2 && df.height() >= 1) {
        return None;
    }
    // Numbers with a leading zero (`007`, `0612345678`: codes, phone numbers) stay text: the
    // number columns are read again as text only when there are any.
    if df.columns().iter().any(|column| column.dtype().is_primitive_numeric())
        && let Some(texts) = read(Some(0)).filter(|texts| texts.shape() == df.shape())
    {
        for (index, column) in texts.columns().iter().enumerate() {
            let numeric = df
                .columns()
                .get(index)
                .is_some_and(|read| read.dtype().is_primitive_numeric());
            let zeros = column
                .str()
                .is_ok_and(|text| text.iter().flatten().any(crate::table_ops::has_leading_zero));
            if numeric && zeros {
                df.replace_column(index, column.clone()).ok()?;
            }
        }
    }
    Some(df)
}

fn try_json(text: &str) -> Option<DataFrame> {
    let trimmed = text.trim_start();
    if !(trimmed.starts_with('[') || trimmed.starts_with('{')) {
        return None;
    }
    if trimmed.starts_with('{') && !trimmed.contains("[") {
        return None;
    }
    let mut cursor = Cursor::new(text.as_bytes());
    JsonReader::new(&mut cursor)
        .finish()
        .ok()
        .filter(|df| df.width() >= 1 && df.height() >= 1)
        .or_else(|| try_json_object_of_arrays(text))
}

fn try_json_object_of_arrays(text: &str) -> Option<DataFrame> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let serde_json::Value::Object(map) = value else {
        return None;
    };
    if map.is_empty() {
        return None;
    }
    let mut columns = Vec::new();
    let mut height = None;
    for (key, val) in map {
        let serde_json::Value::Array(items) = val else {
            return None;
        };
        if let Some(expected) = height {
            if items.len() != expected {
                return None;
            }
        } else {
            height = Some(items.len());
        }
        let series = series_from_json_values(&key, &items)?;
        columns.push(series.into_column());
    }
    let height = height?;
    DataFrame::new(height, columns)
        .ok()
        .filter(|df| df.height() >= 1)
}

fn series_from_json_values(name: &str, items: &[serde_json::Value]) -> Option<Series> {
    if items
        .iter()
        .all(|v| v.is_i64() || v.is_u64() || v.is_null())
    {
        let values: Vec<Option<i64>> = items
            .iter()
            .map(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_i64().or_else(|| v.as_u64().map(|n| n as i64))
                }
            })
            .collect();
        return Some(Series::new(name.into(), values));
    }
    if items
        .iter()
        .all(|v| v.is_f64() || v.is_i64() || v.is_u64() || v.is_null())
    {
        let values: Vec<Option<f64>> = items
            .iter()
            .map(|v| {
                if v.is_null() {
                    None
                } else {
                    v.as_f64()
                        .or_else(|| v.as_i64().map(|n| n as f64))
                        .or_else(|| v.as_u64().map(|n| n as f64))
                }
            })
            .collect();
        return Some(Series::new(name.into(), values));
    }
    if items.iter().all(|v| v.is_boolean() || v.is_null()) {
        let values: Vec<Option<bool>> = items.iter().map(|v| v.as_bool()).collect();
        return Some(Series::new(name.into(), values));
    }
    let values: Vec<Option<String>> = items
        .iter()
        .map(|v| {
            if v.is_null() {
                None
            } else if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                Some(v.to_string())
            }
        })
        .collect();
    Some(Series::new(name.into(), values))
}

include!("dataframe_util.rs");
