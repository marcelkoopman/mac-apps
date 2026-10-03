//! The steps of [`crate::table::TableOp`] and the Describe view, on eager polars frames with
//! only the features Copycraft has (no `lazy`, `rows`, `strings` or `dtype-full`): where polars
//! needs one of those, the step is written out here.

use polars::prelude::*;

/// Most rows Transpose takes (each row becomes a column).
pub const TRANSPOSE_MAX_ROWS: usize = 200;

/// Text values trimmed, and empty ones (after trimming) made null.
fn trimmed(df: &DataFrame) -> PolarsResult<DataFrame> {
    let mut columns = Vec::with_capacity(df.width());
    for column in df.columns() {
        match column.str() {
            Ok(text) => {
                let trimmed: StringChunked = text
                    .iter()
                    .map(|value| value.map(str::trim).filter(|value| !value.is_empty()))
                    .collect();
                columns.push(trimmed.with_name(column.name().clone()).into_column());
            }
            Err(_) => columns.push(column.clone()),
        }
    }
    DataFrame::new(df.height(), columns)
}

/// Drop Empty: text trimmed, then the columns and the rows with no value left dropped.
pub fn drop_empty(df: &DataFrame) -> Result<DataFrame, String> {
    let df = trimmed(df).map_err(|e| e.to_string())?;
    let height = df.height();
    let kept: Vec<Column> = df
        .columns()
        .iter()
        .filter(|column| column.null_count() < height)
        .cloned()
        .collect();
    if kept.is_empty() {
        return Err("Every column is empty".to_string());
    }
    let mut any = BooleanChunked::full(PlSmallStr::EMPTY, false, height);
    for column in &kept {
        any = &any | &column.is_not_null();
    }
    DataFrame::new(height, kept)
        .and_then(|df| df.filter(&any))
        .map_err(|e| e.to_string())
}

/// Drop constant columns: the columns with one value in every row (or none) dropped.
pub fn drop_constant(df: &DataFrame) -> Result<DataFrame, String> {
    let mut kept = Vec::new();
    for column in df.columns() {
        if column.n_unique().map_err(|e| e.to_string())? > 1 {
            kept.push(column.clone());
        }
    }
    if kept.is_empty() {
        return Err("Every column has one value".to_string());
    }
    if kept.len() == df.width() {
        return Err("No column has one value".to_string());
    }
    DataFrame::new(df.height(), kept).map_err(|e| e.to_string())
}

/// What a text column holds, when every value (empty ones aside) is the same kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextKind {
    Integers,
    /// Decimals with a point.
    Decimals,
    /// Decimals with a comma (`1,5`), no thousands separators.
    CommaDecimals,
    /// `yyyy-mm-dd`.
    IsoDates,
}

fn is_integer(value: &str) -> bool {
    let digits = value.strip_prefix(['-', '+']).unwrap_or(value);
    !digits.is_empty() && digits.len() <= 18 && digits.bytes().all(|b| b.is_ascii_digit())
}

fn is_decimal(value: &str, mark: char) -> bool {
    let Some((whole, fraction)) = value.split_once(mark) else {
        return false;
    };
    (whole.is_empty() || whole == "-" || is_integer(whole))
        && !fraction.is_empty()
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}

/// The kind every value in `values` has, trying the narrowest first. `None` when one value
/// fits none, or there are no values.
fn text_kind(text: &StringChunked) -> Option<TextKind> {
    let values = || {
        text.iter()
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    values().next()?;
    [
        TextKind::Integers,
        TextKind::Decimals,
        TextKind::CommaDecimals,
        TextKind::IsoDates,
    ]
    .into_iter()
    .find(|kind| {
        values().all(|value| match kind {
            TextKind::Integers => is_integer(value),
            TextKind::Decimals => is_decimal(value, '.') || is_integer(value),
            TextKind::CommaDecimals => is_decimal(value, ',') || is_integer(value),
            TextKind::IsoDates => is_iso_date(value),
        })
    })
}

/// A text column as `kind`; `None` unless every value converts.
fn convert(text: &StringChunked, kind: TextKind) -> Option<Column> {
    let name = text.name().clone();
    fn value(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    let column = match kind {
        TextKind::Integers => {
            let parsed: Int64Chunked = text
                .iter()
                .map(|v| value(v).and_then(|v| v.parse::<i64>().ok()))
                .collect();
            parsed.with_name(name).into_column()
        }
        TextKind::Decimals | TextKind::CommaDecimals => {
            let parsed: Float64Chunked = text
                .iter()
                .map(|v| value(v).and_then(|v| v.replace(',', ".").parse::<f64>().ok()))
                .collect();
            parsed.with_name(name).into_column()
        }
        TextKind::IsoDates => {
            let cleaned: StringChunked = text.iter().map(value).collect();
            let dates = cleaned.as_date(Some("%Y-%m-%d"), false).ok()?;
            dates.into_series().with_name(name).into_column()
        }
    };
    let empty = text.iter().filter(|v| value(*v).is_none()).count();
    (column.null_count() == empty).then_some(column)
}

/// Fix types: a text column becomes numbers or dates when every value (empty ones aside)
/// converts; `a/b/yyyy` dates are read day first ([`crate::dataframe::read_dates`]).
pub fn fix_types(df: &DataFrame) -> Result<DataFrame, String> {
    let mut out = df.clone();
    let mut changed = 0;
    for column in df.columns() {
        let Ok(text) = column.str() else {
            continue;
        };
        let Some(kind) = text_kind(text) else {
            continue;
        };
        if let Some(converted) = convert(text, kind) {
            out.replace(column.name().as_str(), converted)
                .map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    changed += crate::dataframe::read_dates_day_first(&mut out);
    if changed == 0 {
        return Err("No column to change".to_string());
    }
    Ok(out)
}

/// A cell as text for a transposed table; `None` for an empty cell.
fn cell_text(value: AnyValue) -> Option<String> {
    match value {
        AnyValue::Null => None,
        AnyValue::String(text) => Some(text.to_string()),
        AnyValue::StringOwned(text) => Some(text.to_string()),
        other => Some(other.to_string()),
    }
}

/// Transpose: each row becomes a column, named by the row's first value when those are all
/// there and different (else "row 1", "row 2", …); the column names become the first column.
/// Every value becomes text. At most [`TRANSPOSE_MAX_ROWS`] rows.
pub fn transpose(df: &DataFrame) -> Result<DataFrame, String> {
    let height = df.height();
    if height > TRANSPOSE_MAX_ROWS {
        return Err(format!("Transpose takes at most {TRANSPOSE_MAX_ROWS} rows"));
    }
    let columns = df.columns();
    let Some(first) = columns.first() else {
        return Err("The table has no columns".to_string());
    };
    let first_values: Vec<Option<String>> = (0..height)
        .map(|row| first.get(row).ok().and_then(cell_text))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let named_by_first = columns.len() > 1
        && first_values
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| seen.insert(v.clone())));
    let (names, rest) = if named_by_first {
        let names: Vec<String> = first_values.into_iter().flatten().collect();
        (names, &columns[1..])
    } else {
        (
            (1..=height).map(|row| format!("row {row}")).collect(),
            columns,
        )
    };
    let header = if named_by_first {
        first.name().to_string()
    } else {
        "column".to_string()
    };
    let mut out: Vec<Column> = Vec::with_capacity(height + 1);
    let labels: StringChunked = rest.iter().map(|c| Some(c.name().as_str())).collect();
    out.push(labels.with_name(header.as_str().into()).into_column());
    for (row, name) in names.iter().enumerate() {
        if out.iter().any(|c| c.name().as_str() == name.as_str()) {
            return Err(format!("Two columns would be named {name}"));
        }
        let values: StringChunked = rest
            .iter()
            .map(|column| column.get(row).ok().and_then(cell_text))
            .collect();
        out.push(values.with_name(name.as_str().into()).into_column());
    }
    DataFrame::new(rest.len(), out).map_err(|e| e.to_string())
}

/// A value of a reduced column as text ("" for none).
fn scalar_text(scalar: PolarsResult<Scalar>) -> Option<String> {
    let scalar = scalar.ok()?;
    cell_text(scalar.value().clone())
}

/// Describe: one row per column with its type, count of values, empty cells, distinct values,
/// smallest, largest and (numbers) mean. A read-only view of the version shown, not a version.
pub fn describe(df: &DataFrame) -> Result<DataFrame, String> {
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut counts = Vec::new();
    let mut nulls = Vec::new();
    let mut uniques = Vec::new();
    let mut mins = Vec::new();
    let mut maxes = Vec::new();
    let mut means = Vec::new();
    for column in df.columns() {
        names.push(column.name().to_string());
        types.push(column.dtype().to_string());
        let null_count = column.null_count();
        counts.push((column.len() - null_count) as u64);
        nulls.push(null_count as u64);
        uniques.push(column.n_unique().map_err(|e| e.to_string())? as u64);
        mins.push(scalar_text(column.min_reduce()));
        maxes.push(scalar_text(column.max_reduce()));
        let mean = if column.dtype().is_primitive_numeric() {
            column.as_materialized_series().mean()
        } else {
            None
        };
        means.push(mean.map(|m| format!("{m:.4}")));
    }
    let height = names.len();
    let text = |name: &str, values: Vec<Option<String>>| {
        StringChunked::from_iter_options(name.into(), values.into_iter()).into_column()
    };
    let numbers =
        |name: &str, values: Vec<u64>| UInt64Chunked::from_vec(name.into(), values).into_column();
    DataFrame::new(
        height,
        vec![
            text("column", names.into_iter().map(Some).collect()),
            text("type", types.into_iter().map(Some).collect()),
            numbers("count", counts),
            numbers("empty", nulls),
            numbers("distinct", uniques),
            text("min", mins),
            text("max", maxes),
            text("mean", means),
        ],
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{describe, drop_constant, drop_empty, fix_types, transpose};
    use crate::dataframe::parse_table;
    use crate::dataframe::tests::ENERGY_FIXTURE;
    use polars::prelude::*;

    fn names(df: &DataFrame) -> Vec<String> {
        df.get_column_names()
            .into_iter()
            .map(|name| name.to_string())
            .collect()
    }

    fn table(text: &str) -> DataFrame {
        parse_table(text).expect("table")
    }

    #[test]
    fn drop_constant_drops_the_all_zero_columns_of_the_energy_export() {
        let df = table(ENERGY_FIXTURE);
        assert_eq!(df.width(), 20);
        let kept = drop_constant(&df).expect("step");
        assert_eq!(kept.width(), 14);
        assert_eq!(kept.height(), df.height());
        assert!(kept.column("Grid Export (kWh)").is_err());
        assert!(kept.column("Date").is_ok());
        // Nothing left to drop.
        assert_eq!(
            drop_constant(&kept).err().as_deref(),
            Some("No column has one value")
        );
        let one_row = table("a,b\n1,2");
        assert_eq!(
            drop_constant(&one_row).err().as_deref(),
            Some("Every column has one value")
        );
    }

    #[test]
    fn drop_empty_trims_text_and_drops_empty_rows_and_columns() {
        let df = table("name,note,n\n  ann  ,,1\n,,\nbob, ,2");
        let out = drop_empty(&df).expect("step");
        assert_eq!(names(&out), ["name", "n"]);
        assert_eq!(out.height(), 2);
        let names: Vec<Option<&str>> = out.column("name").unwrap().str().unwrap().iter().collect();
        assert_eq!(names, [Some("ann"), Some("bob")]);
    }

    #[test]
    fn fix_types_converts_a_column_only_when_every_value_fits() {
        let df = table(
            "id;price;mixed;day\n\"1\";\"1,5\";\"2\";2026-03-01\n\"2\";\"2,25\";\"x\";2026-03-02",
        );
        // Quoted values stay text in the reader when the text says so; force text to test.
        let text = |name: &str| df.column(name).unwrap().cast(&DataType::String).unwrap();
        let df = DataFrame::new(
            2,
            vec![text("id"), text("price"), text("mixed"), text("day")],
        )
        .unwrap();
        let out = fix_types(&df).expect("step");
        assert_eq!(out.column("id").unwrap().dtype(), &DataType::Int64);
        assert_eq!(out.column("price").unwrap().dtype(), &DataType::Float64);
        assert_eq!(out.column("mixed").unwrap().dtype(), &DataType::String);
        assert_eq!(out.column("day").unwrap().dtype(), &DataType::Date);
        let prices: Vec<Option<f64>> = out.column("price").unwrap().f64().unwrap().iter().collect();
        assert_eq!(prices, [Some(1.5), Some(2.25)]);
        assert_eq!(
            fix_types(&out).err().as_deref(),
            Some("No column to change")
        );
    }

    #[test]
    fn fix_types_reads_day_first_dates() {
        let df = DataFrame::new(
            2,
            vec![
                Column::new("when".into(), ["13/03/2026", "14/03/2026"]),
                Column::new("n".into(), [1i64, 2]),
            ],
        )
        .unwrap();
        let out = fix_types(&df).expect("step");
        assert_eq!(out.column("when").unwrap().dtype(), &DataType::Date);
    }

    #[test]
    fn transpose_names_columns_by_the_first_values_and_stops_over_200_rows() {
        let df = table("name,age,city\nann,30,Utrecht\nbob,41,Delft");
        let out = transpose(&df).expect("step");
        assert_eq!(names(&out), ["name", "ann", "bob"]);
        let labels: Vec<Option<&str>> = out.column("name").unwrap().str().unwrap().iter().collect();
        assert_eq!(labels, [Some("age"), Some("city")]);
        let ann: Vec<Option<&str>> = out.column("ann").unwrap().str().unwrap().iter().collect();
        assert_eq!(ann, [Some("30"), Some("Utrecht")]);
        // Repeated first values: numbered rows, every column kept.
        let repeated = transpose(&table("k,v\na,1\na,2")).expect("step");
        assert_eq!(names(&repeated), ["column", "row 1", "row 2"]);
        assert_eq!(repeated.height(), 2);
        let mut long = String::from("k,v\n");
        for i in 0..201 {
            long.push_str(&format!("r{i},{i}\n"));
        }
        assert_eq!(
            transpose(&table(&long)).err().as_deref(),
            Some("Transpose takes at most 200 rows")
        );
        // The energy export (40 days) turns into one row per measure.
        let energy = transpose(&table(ENERGY_FIXTURE)).expect("step");
        assert_eq!(energy.height(), 19);
        assert_eq!(energy.width(), 41);
    }

    #[test]
    fn describe_has_a_row_per_column() {
        let df = table(ENERGY_FIXTURE);
        let out = describe(&df).expect("describe");
        assert_eq!(out.height(), df.width());
        assert_eq!(
            names(&out),
            [
                "column", "type", "count", "empty", "distinct", "min", "max", "mean"
            ]
        );
        let distinct = out.column("distinct").unwrap().u64().unwrap();
        let names = out.column("column").unwrap().str().unwrap();
        let zero = names
            .iter()
            .position(|n| n == Some("Grid Export (kWh)"))
            .unwrap();
        assert_eq!(distinct.get(zero), Some(1));
        let types = out.column("type").unwrap().str().unwrap();
        assert_eq!(types.get(0), Some("date"));
        let means = out.column("mean").unwrap().str().unwrap();
        assert_eq!(means.get(0), None);
        assert!(means.get(zero).is_some());
    }
}
