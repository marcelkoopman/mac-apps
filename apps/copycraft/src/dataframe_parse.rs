fn try_csv(text: &str) -> Option<DataFrame> {
    let separator = detect_separator(text)?;
    if !looks_like_delimited_table(text, separator) {
        return None;
    }
    let mut cursor = Cursor::new(text.as_bytes());
    let parse = CsvParseOptions::default().with_separator(separator);
    CsvReadOptions::default()
        .with_has_header(true)
        .with_parse_options(parse)
        .into_reader_with_file_handle(&mut cursor)
        .finish()
        .ok()
        .filter(|df| df.width() >= 2 && df.height() >= 1)
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

fn try_xml(text: &str) -> Option<DataFrame> {
    if !crate::format::looks_like_xml(text) {
        return None;
    }
    let rows = xml_rows(text)?;
    if rows.len() < 2 {
        return None;
    }
    let headers = rows[0].clone();
    if headers.len() < 2 {
        return None;
    }
    let csv = rows_to_csv(&rows);
    try_csv(&csv)
}

include!("dataframe_xml.rs");
