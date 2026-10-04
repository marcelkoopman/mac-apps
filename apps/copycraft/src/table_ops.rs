//! The steps of [`crate::table::TableOp`], and the typing of text columns when a table is
//! read, on eager polars frames with only the features Copycraft has (no `lazy`, `rows`,
//! `strings` or `dtype-full`): where polars needs one of those, the step is written out here.

use polars::prelude::*;

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

/// `007`, `0612345678`, `-01`: a zero followed by more digits (not `0`, `0.5` or `0,5`). A
/// column with such a value holds codes, not numbers, and stays text.
pub fn has_leading_zero(value: &str) -> bool {
    let value = value.trim();
    let digits = value.strip_prefix(['-', '+']).unwrap_or(value).as_bytes();
    digits.len() > 1 && digits[0] == b'0' && digits[1].is_ascii_digit()
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
    if values().any(has_leading_zero) {
        return None;
    }
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

/// Text columns of numbers or dates typed, as a table is read (what Fix types did as a step):
/// a text column becomes numbers (also with a decimal comma) or `yyyy-mm-dd` dates when every
/// value (empty ones aside) converts; other text stays text. `a/b/yyyy` dates were read
/// already ([`crate::dataframe::read_dates`]). Returns how many columns changed.
pub fn type_text_columns(df: &mut DataFrame) -> usize {
    let mut changed = 0;
    for name in df.get_column_names_owned() {
        let converted = df
            .column(name.as_str())
            .ok()
            .and_then(|column| column.str().ok())
            .and_then(|text| text_kind(text).and_then(|kind| convert(text, kind)));
        if let Some(converted) = converted
            && df.replace(name.as_str(), converted).is_ok()
        {
            changed += 1;
        }
    }
    changed
}

/// Select columns: `columns`, in that order. At least one.
pub fn select_columns(df: &DataFrame, columns: &[String]) -> Result<DataFrame, String> {
    if columns.is_empty() {
        return Err("Choose at least one column".to_string());
    }
    df.select(columns.iter().map(String::as_str))
        .map_err(|e| e.to_string())
}

/// Sort by `column`: empty cells last, equal values in table order.
pub fn sort(df: &DataFrame, column: &str, descending: bool) -> Result<DataFrame, String> {
    let options = SortMultipleOptions::default()
        .with_order_descending(descending)
        .with_nulls_last(true)
        .with_maintain_order(true);
    df.sort([column], options).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{drop_empty, select_columns, sort, type_text_columns};
    use crate::dataframe::parse_table;
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
    fn drop_empty_trims_text_and_drops_empty_rows_and_columns() {
        let df = table("name,note,n\n  ann  ,,1\n,,\nbob, ,2");
        let out = drop_empty(&df).expect("step");
        assert_eq!(names(&out), ["name", "n"]);
        assert_eq!(out.height(), 2);
        let names: Vec<Option<&str>> = out.column("name").unwrap().str().unwrap().iter().collect();
        assert_eq!(names, [Some("ann"), Some("bob")]);
    }

    #[test]
    fn text_columns_are_typed_only_when_every_value_fits() {
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
        let mut out = df;
        assert_eq!(type_text_columns(&mut out), 3);
        assert_eq!(out.column("id").unwrap().dtype(), &DataType::Int64);
        assert_eq!(out.column("price").unwrap().dtype(), &DataType::Float64);
        assert_eq!(out.column("mixed").unwrap().dtype(), &DataType::String);
        assert_eq!(out.column("day").unwrap().dtype(), &DataType::Date);
        let prices: Vec<Option<f64>> = out.column("price").unwrap().f64().unwrap().iter().collect();
        assert_eq!(prices, [Some(1.5), Some(2.25)]);
        // Typed already: nothing to change.
        assert_eq!(type_text_columns(&mut out), 0);
    }

    #[test]
    fn a_column_with_a_leading_zero_stays_text_others_are_typed() {
        use super::has_leading_zero;
        for code in ["007", "0612345678", "-01", " 012 "] {
            assert!(has_leading_zero(code), "{code}");
        }
        for number in ["0", "0.5", "0,5", "10", "-0", "", "2026-03-01"] {
            assert!(!has_leading_zero(number), "{number}");
        }
        // As read: unquoted `007` is no number either.
        let df = table(
            "id,phone,n,price,when\n007,0612345678,1,0.5,2026-03-01\n123,0201234567,0,1.25,2026-03-02",
        );
        assert_eq!(df.column("id").unwrap().dtype(), &DataType::String);
        assert_eq!(df.column("phone").unwrap().dtype(), &DataType::String);
        assert_eq!(df.column("n").unwrap().dtype(), &DataType::Int64);
        assert_eq!(df.column("price").unwrap().dtype(), &DataType::Float64);
        assert_eq!(df.column("when").unwrap().dtype(), &DataType::Date);
        let ids: Vec<Option<&str>> = df.column("id").unwrap().str().unwrap().iter().collect();
        assert_eq!(ids, [Some("007"), Some("123")]);
        // Text columns (a `;` table, decimal commas) the same way.
        let semi = table("code;prijs\n\"0012\";0,5\n\"34\";2,25");
        assert_eq!(semi.column("code").unwrap().dtype(), &DataType::String);
        assert_eq!(semi.column("prijs").unwrap().dtype(), &DataType::Float64);
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
    fn select_columns_keeps_at_least_one() {
        let df = table("a,b,c\n1,2,3");
        let picked = select_columns(&df, &["c".into(), "a".into()]).expect("step");
        assert_eq!(names(&picked), ["c", "a"]);
        assert!(select_columns(&df, &[]).is_err());
        assert!(select_columns(&df, &["x".into()]).is_err());
    }
}
