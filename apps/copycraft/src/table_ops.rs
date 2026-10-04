//! The steps of [`crate::table::TableOp`] and the Describe view, on eager polars frames with
//! only the features Copycraft has (no `lazy`, `rows`, `strings` or `dtype-full`): where polars
//! needs one of those, the step is written out here.

use std::collections::HashMap;

use polars::prelude::*;

use crate::dataframe::cell_text;

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
/// converts; `a/b/yyyy` dates are read day first ([`crate::dataframe::read_dates`]). `None`
/// when no column changes: the reader typed them already (numbers, dates), or a text column
/// has values that are not numbers or dates.
pub fn fix_types(df: &DataFrame) -> Result<Option<DataFrame>, String> {
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
    Ok((changed > 0).then_some(out))
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

/// Select columns: `columns`, in that order. At least one.
pub fn select_columns(df: &DataFrame, columns: &[String]) -> Result<DataFrame, String> {
    if columns.is_empty() {
        return Err("Choose at least one column".to_string());
    }
    df.select(columns.iter().map(String::as_str))
        .map_err(|e| e.to_string())
}

/// Drop columns: every column but `columns`. At least one stays.
pub fn drop_columns(df: &DataFrame, columns: &[String]) -> Result<DataFrame, String> {
    for column in columns {
        df.column(column).map_err(|e| e.to_string())?;
    }
    let kept: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| !columns.contains(name))
        .collect();
    if kept.is_empty() {
        return Err("At least one column stays".to_string());
    }
    select_columns(df, &kept)
}

/// Sort by `column`: empty cells last, equal values in table order.
pub fn sort(df: &DataFrame, column: &str, descending: bool) -> Result<DataFrame, String> {
    let options = SortMultipleOptions::default()
        .with_order_descending(descending)
        .with_nulls_last(true)
        .with_maintain_order(true);
    df.sort([column], options).map_err(|e| e.to_string())
}

/// Value counts of `column`: each value once, with the number of rows that have it, the most
/// common first (ties in table order). Empty cells count as one value.
pub fn value_counts(df: &DataFrame, column: &str) -> Result<DataFrame, String> {
    let values = df.column(column).map_err(|e| e.to_string())?;
    let mut order: Vec<(IdxSize, u64)> = Vec::new();
    let mut index: HashMap<Option<String>, usize> = HashMap::new();
    for row in 0..values.len() {
        let key = values.get(row).ok().and_then(cell_text);
        match index.get(&key) {
            Some(&at) => order[at].1 += 1,
            None => {
                index.insert(key, order.len());
                order.push((row as IdxSize, 1));
            }
        }
    }
    order.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let rows = IdxCa::from_vec(
        PlSmallStr::EMPTY,
        order.iter().map(|(row, _)| *row).collect(),
    );
    let picked = values.take(&rows).map_err(|e| e.to_string())?;
    let counts = UInt64Chunked::from_vec("count".into(), order.iter().map(|(_, n)| *n).collect());
    DataFrame::new(order.len(), vec![picked, counts.into_column()]).map_err(|e| e.to_string())
}

/// The rows of `df` grouped by the text of their `keys` cells, in the order each group is
/// first seen: the first row of each group and all its rows. Empty key cells form a group.
fn groups(df: &DataFrame, keys: &[String]) -> Result<Vec<Vec<IdxSize>>, String> {
    let columns = keys
        .iter()
        .map(|key| df.column(key).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut order: Vec<Vec<IdxSize>> = Vec::new();
    let mut index: HashMap<Vec<Option<String>>, usize> = HashMap::new();
    for row in 0..df.height() {
        let key: Vec<Option<String>> = columns
            .iter()
            .map(|column| column.get(row).ok().and_then(cell_text))
            .collect();
        match index.get(&key) {
            Some(&at) => order[at].push(row as IdxSize),
            None => {
                index.insert(key, order.len());
                order.push(vec![row as IdxSize]);
            }
        }
    }
    Ok(order)
}

/// A column [`Agg`] works on: numbers for all, dates and times too for min and max.
///
/// [`Agg`]: crate::table::Agg
pub fn aggregates(agg: crate::table::Agg, dtype: &DataType) -> bool {
    use crate::table::Agg;
    match agg {
        Agg::Count => false,
        Agg::Sum | Agg::Mean => dtype.is_primitive_numeric(),
        Agg::Min | Agg::Max => {
            dtype.is_primitive_numeric()
                || (dtype.is_temporal() && !matches!(dtype, DataType::Duration(_)))
        }
    }
}

/// A name for a new column that no column of `df` has: `want`, else `want (2)`, ….
fn free_name(df: &DataFrame, want: &str) -> String {
    let taken = |name: &str| df.get_column_names().iter().any(|n| n.as_str() == name);
    if !taken(want) {
        return want.to_string();
    }
    (2..)
        .map(|n| format!("{want} ({n})"))
        .find(|name| !taken(name))
        .expect("a free name")
}

/// Group by: one row per value (or combination) of the `keys` columns, in the order first
/// seen, then the number of rows (Count) or, per other number column, its sum, mean, min or
/// max (dates and times too for min and max, keeping their type). Empty cells are left out of
/// a sum, mean, min or max; a group with no value there gets an empty cell.
pub fn group_by(
    df: &DataFrame,
    keys: &[String],
    agg: crate::table::Agg,
) -> Result<DataFrame, String> {
    use crate::table::Agg;
    if keys.is_empty() {
        return Err("Choose a column to group by".to_string());
    }
    let groups = groups(df, keys)?;
    let firsts = IdxCa::from_vec(
        PlSmallStr::EMPTY,
        groups.iter().map(|rows| rows[0]).collect(),
    );
    let mut out: Vec<Column> = Vec::new();
    for key in keys {
        let column = df.column(key).map_err(|e| e.to_string())?;
        out.push(column.take(&firsts).map_err(|e| e.to_string())?);
    }
    if agg == Agg::Count {
        let counts = UInt64Chunked::from_vec(
            free_name(df, "count").into(),
            groups.iter().map(|rows| rows.len() as u64).collect(),
        );
        out.push(counts.into_column());
        return DataFrame::new(groups.len(), out).map_err(|e| e.to_string());
    }
    let values: Vec<&Column> = df
        .columns()
        .iter()
        .filter(|column| {
            !keys
                .iter()
                .any(|key| key.as_str() == column.name().as_str())
        })
        .filter(|column| aggregates(agg, column.dtype()))
        .collect();
    if values.is_empty() {
        return Err(match agg {
            Agg::Sum => "No number column to sum",
            Agg::Mean => "No number column to average",
            _ => "No number or date column",
        }
        .to_string());
    }
    for column in values {
        let name = format!("{} ({})", column.name(), agg.name());
        out.push(aggregate(column, &groups, agg, name.into())?);
    }
    DataFrame::new(groups.len(), out).map_err(|e| e.to_string())
}

/// One aggregate column of [`group_by`], a value per group.
fn aggregate(
    column: &Column,
    groups: &[Vec<IdxSize>],
    agg: crate::table::Agg,
    name: PlSmallStr,
) -> Result<Column, String> {
    use crate::table::Agg;
    let fail = |e: PolarsError| e.to_string();
    let as_f64 = |column: &Column| -> Result<Float64Chunked, String> {
        let physical = column.to_physical_repr();
        let floats = physical.cast(&DataType::Float64).map_err(fail)?;
        Ok(floats.f64().map_err(fail)?.clone())
    };
    match agg {
        Agg::Sum if column.dtype().is_integer() => {
            let ints = column.cast(&DataType::Int64).map_err(fail)?;
            let ints = ints.i64().map_err(fail)?;
            let sums: Int64Chunked = groups
                .iter()
                .map(|rows| {
                    let mut seen = false;
                    let mut sum = 0i64;
                    for &row in rows {
                        if let Some(value) = ints.get(row as usize) {
                            seen = true;
                            sum = sum.saturating_add(value);
                        }
                    }
                    seen.then_some(sum)
                })
                .collect();
            Ok(sums.with_name(name).into_column())
        }
        Agg::Sum | Agg::Mean => {
            let floats = as_f64(column)?;
            let out: Float64Chunked = groups
                .iter()
                .map(|rows| {
                    let values: Vec<f64> = rows
                        .iter()
                        .filter_map(|&row| floats.get(row as usize))
                        .collect();
                    if values.is_empty() {
                        return None;
                    }
                    let sum: f64 = values.iter().sum();
                    Some(if agg == Agg::Mean {
                        sum / values.len() as f64
                    } else {
                        sum
                    })
                })
                .collect();
            Ok(out.with_name(name).into_column())
        }
        Agg::Min | Agg::Max => {
            // The row with the least (or greatest) value, taken so the type stays.
            let floats = as_f64(column)?;
            let picks: Vec<Option<IdxSize>> = groups
                .iter()
                .map(|rows| {
                    let mut best: Option<(IdxSize, f64)> = None;
                    for &row in rows {
                        let Some(value) = floats.get(row as usize) else {
                            continue;
                        };
                        let better = match best {
                            None => true,
                            Some((_, current)) if agg == Agg::Min => value < current,
                            Some((_, current)) => value > current,
                        };
                        if better {
                            best = Some((row, value));
                        }
                    }
                    best.map(|(row, _)| row)
                })
                .collect();
            let picks: IdxCa = picks.into_iter().collect();
            let taken = column.take(&picks).map_err(fail)?;
            Ok(taken.with_name(name))
        }
        Agg::Count => unreachable!("counted in group_by"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        describe, drop_columns, drop_constant, drop_empty, fix_types, group_by, select_columns,
        sort, transpose, value_counts,
    };
    use crate::dataframe::parse_table;
    use crate::dataframe::tests::ENERGY_FIXTURE;
    use crate::table::Agg;
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
        let out = fix_types(&df).expect("step").expect("changed");
        assert_eq!(out.column("id").unwrap().dtype(), &DataType::Int64);
        assert_eq!(out.column("price").unwrap().dtype(), &DataType::Float64);
        assert_eq!(out.column("mixed").unwrap().dtype(), &DataType::String);
        assert_eq!(out.column("day").unwrap().dtype(), &DataType::Date);
        let prices: Vec<Option<f64>> = out.column("price").unwrap().f64().unwrap().iter().collect();
        assert_eq!(prices, [Some(1.5), Some(2.25)]);
        // Typed already: nothing to change, and no error.
        assert!(fix_types(&out).expect("step").is_none());
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
        let out = fix_types(&df).expect("step").expect("changed");
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

    #[test]
    fn sort_puts_empty_cells_last_and_keeps_equal_rows_in_order() {
        let df = table("name,n\na,2\nb,\nc,1\nd,2");
        let up = sort(&df, "n", false).expect("step");
        let names: Vec<Option<&str>> = up.column("name").unwrap().str().unwrap().iter().collect();
        assert_eq!(names, [Some("c"), Some("a"), Some("d"), Some("b")]);
        let down = sort(&df, "n", true).expect("step");
        let names: Vec<Option<&str>> = down.column("name").unwrap().str().unwrap().iter().collect();
        assert_eq!(names, [Some("a"), Some("d"), Some("c"), Some("b")]);
        assert!(sort(&df, "missing", false).is_err());
    }

    #[test]
    fn group_by_counts_sums_averages_and_keeps_the_type_of_min_and_max() {
        let df = table(
            "region,units,price,sold\nNorth,2,1.5,2026-09-02\nSouth,1,4.0,2026-09-01\nNorth,3,2.5,2026-09-05\n,4,1.0,2026-09-03\nNorth,,0.5,",
        );
        let count = group_by(&df, &["region".into()], Agg::Count).expect("count");
        assert_eq!(names(&count), ["region", "count"]);
        let regions: Vec<Option<&str>> = count
            .column("region")
            .unwrap()
            .str()
            .unwrap()
            .iter()
            .collect();
        // The order first seen; the empty region is a group too.
        assert_eq!(regions, [Some("North"), Some("South"), None]);
        let counts: Vec<Option<u64>> = count
            .column("count")
            .unwrap()
            .u64()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(counts, [Some(3), Some(1), Some(1)]);

        let sum = group_by(&df, &["region".into()], Agg::Sum).expect("sum");
        assert_eq!(names(&sum), ["region", "units (sum)", "price (sum)"]);
        let units: Vec<Option<i64>> = sum
            .column("units (sum)")
            .unwrap()
            .i64()
            .unwrap()
            .iter()
            .collect();
        // The empty cell is left out of North's sum.
        assert_eq!(units, [Some(5), Some(1), Some(4)]);
        let price: Vec<Option<f64>> = sum
            .column("price (sum)")
            .unwrap()
            .f64()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(price, [Some(4.5), Some(4.0), Some(1.0)]);

        let mean = group_by(&df, &["region".into()], Agg::Mean).expect("mean");
        let units: Vec<Option<f64>> = mean
            .column("units (mean)")
            .unwrap()
            .f64()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(units, [Some(2.5), Some(1.0), Some(4.0)]);

        let min = group_by(&df, &["region".into()], Agg::Min).expect("min");
        assert_eq!(
            names(&min),
            ["region", "units (min)", "price (min)", "sold (min)"]
        );
        assert_eq!(min.column("sold (min)").unwrap().dtype(), &DataType::Date);
        assert_eq!(min.column("units (min)").unwrap().dtype(), &DataType::Int64);
        let max = group_by(&df, &["region".into()], Agg::Max).expect("max");
        let shown = max.to_string();
        assert!(shown.contains("2026-09-05"), "{shown}");
        let price: Vec<Option<f64>> = max
            .column("price (max)")
            .unwrap()
            .f64()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(price, [Some(2.5), Some(4.0), Some(1.0)]);
    }

    #[test]
    fn group_by_takes_several_keys_and_says_when_there_is_nothing_to_add_up() {
        let df = table("a,b,n\nx,1,10\nx,2,20\nx,1,30\ny,1,40");
        let sum = group_by(&df, &["a".into(), "b".into()], Agg::Sum).expect("sum");
        assert_eq!(sum.shape(), (3, 3));
        let n: Vec<Option<i64>> = sum
            .column("n (sum)")
            .unwrap()
            .i64()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(n, [Some(40), Some(20), Some(40)]);
        let text = table("name,city\nann,Delft\nbob,Delft");
        assert_eq!(
            group_by(&text, &["city".into()], Agg::Sum).err().as_deref(),
            Some("No number column to sum")
        );
        assert_eq!(
            group_by(&text, &["city".into()], Agg::Max).err().as_deref(),
            Some("No number or date column")
        );
        // A count column of its own name when "count" is taken.
        let counted = table("count,x\n1,a\n1,b");
        let out = group_by(&counted, &["count".into()], Agg::Count).expect("count");
        assert_eq!(names(&out), ["count", "count (2)"]);
        assert!(group_by(&counted, &["missing".into()], Agg::Count).is_err());
    }

    #[test]
    fn value_counts_lists_each_value_once_most_common_first() {
        let df = table("city,n\nDelft,1\nUtrecht,2\nDelft,3\n,4\nUtrecht,5\nDelft,6");
        let out = value_counts(&df, "city").expect("step");
        assert_eq!(names(&out), ["city", "count"]);
        let cities: Vec<Option<&str>> = out.column("city").unwrap().str().unwrap().iter().collect();
        assert_eq!(cities, [Some("Delft"), Some("Utrecht"), None]);
        let counts: Vec<Option<u64>> = out.column("count").unwrap().u64().unwrap().iter().collect();
        assert_eq!(counts, [Some(3), Some(2), Some(1)]);
        // The values keep their type.
        let numbers = value_counts(&df, "n").expect("step");
        assert_eq!(numbers.column("n").unwrap().dtype(), &DataType::Int64);
        assert_eq!(numbers.height(), 6);
    }

    #[test]
    fn select_and_drop_columns_keep_at_least_one() {
        let df = table("a,b,c\n1,2,3");
        let picked = select_columns(&df, &["c".into(), "a".into()]).expect("step");
        assert_eq!(names(&picked), ["c", "a"]);
        assert!(select_columns(&df, &[]).is_err());
        assert!(select_columns(&df, &["x".into()]).is_err());
        let dropped = drop_columns(&df, &["b".into()]).expect("step");
        assert_eq!(names(&dropped), ["a", "c"]);
        assert_eq!(
            drop_columns(&df, &["a".into(), "b".into(), "c".into()])
                .err()
                .as_deref(),
            Some("At least one column stays")
        );
        assert!(drop_columns(&df, &["x".into()]).is_err());
    }
}
