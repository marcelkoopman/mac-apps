//! Table ▾ › Filter › a column: keep the rows whose cell fits a rule typed for that column's
//! type. Text: contains (any case). Numbers: between two bounds (either may be left open).
//! Dates (and datetimes, by their day): from a day to a day. Bounds are inclusive. The mask is
//! made here, cell by cell, as polars' `strings` and `lazy` features are not in Copycraft.

use polars::prelude::*;

use crate::dataframe::cell_text;

/// The kind of rule a column takes, from its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Text,
    Number,
    Date,
}

impl FilterKind {
    /// Text columns (and yes/no) take Contains, number columns Between, date and datetime
    /// columns From/To. `None` for the others (times, durations, lists).
    pub fn of(dtype: &DataType) -> Option<Self> {
        match dtype {
            DataType::String | DataType::Boolean => Some(Self::Text),
            dtype if dtype.is_primitive_numeric() => Some(Self::Number),
            DataType::Date | DataType::Datetime(_, _) => Some(Self::Date),
            _ => None,
        }
    }

    /// The question the filter prompt asks.
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Text => "Keep the rows whose value contains this text (any case).",
            Self::Number => {
                "Keep the rows with a number from … to …: 10 to 100, 10-100, >= 10 or <= 100 \
                 (a decimal point or comma)."
            }
            Self::Date => {
                "Keep the rows from one day to another: 2026-09-01 to 2026-09-30, from \
                 2026-09-01, to 2026-09-30, or one day (yyyy-mm-dd or dd/mm/yyyy)."
            }
        }
    }

    /// Said when the answer does not read as this kind of rule.
    pub fn retry(self) -> &'static str {
        match self {
            Self::Text => "Type some text to look for.",
            Self::Number => "That is not a range of numbers. Type 10 to 100, >= 10 or <= 100.",
            Self::Date => {
                "That is not a range of days. Type 2026-09-01 to 2026-09-30, from 2026-09-01 \
                 or to 2026-09-30."
            }
        }
    }
}

/// A rule on one column (bounds kept as text so a step stays `Eq`; read again when applied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterRule {
    /// The cell's text contains this (any case).
    Contains(String),
    /// A number at least `min` and at most `max`.
    Between {
        min: Option<String>,
        max: Option<String>,
    },
    /// A date (or datetime, by its day) on or after `from` and on or before `to`, `yyyy-mm-dd`.
    Dates {
        from: Option<String>,
        to: Option<String>,
    },
}

impl FilterRule {
    /// The rule's kind, for the version label: the bounds and the text are not in it, as they
    /// were typed and may quote the table.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Contains(_) => "text",
            Self::Between { .. } => "number range",
            Self::Dates { .. } => "date range",
        }
    }
}

/// `answer` as a rule of `kind`; `None` when it does not read as one.
pub fn parse(kind: FilterKind, answer: &str) -> Option<FilterRule> {
    let answer = answer.trim();
    if answer.is_empty() {
        return None;
    }
    match kind {
        FilterKind::Text => Some(FilterRule::Contains(answer.to_string())),
        FilterKind::Number => {
            let (min, max) = bounds(answer, true, number)?;
            if let (Some(min), Some(max)) = (min, max)
                && min > max
            {
                return None;
            }
            Some(FilterRule::Between {
                min: min.map(|n| n.to_string()),
                max: max.map(|n| n.to_string()),
            })
        }
        FilterKind::Date => {
            let (from, to) = bounds(answer, false, day)?;
            if let (Some(from), Some(to)) = (&from, &to)
                && from > to
            {
                return None;
            }
            Some(FilterRule::Dates { from, to })
        }
    }
}

/// The two bounds of a range: `a to b`, `a – b`, `a..b`, `a - b` (also `a-b` for numbers),
/// `>= a` / `from a`, `<= b` / `to b` / `until b`, or one value for both. Either bound open,
/// not both.
fn bounds<T: Clone>(
    answer: &str,
    numbers: bool,
    read: impl Fn(&str) -> Option<T>,
) -> Option<(Option<T>, Option<T>)> {
    let lower = answer.to_lowercase();
    for prefix in [">=", "≥", ">", "from ", "after "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return Some((Some(read(rest.trim())?), None));
        }
    }
    for prefix in ["<=", "≤", "<", "to ", "until ", "before "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return Some((None, Some(read(rest.trim())?)));
        }
    }
    let side = |part: &str| -> Option<Option<T>> {
        let part = part.trim();
        if part.is_empty() {
            Some(None)
        } else {
            read(part).map(Some)
        }
    };
    for separator in [" to ", "–", "—", "..", " - "] {
        if let Some((a, b)) = lower.split_once(separator) {
            let (a, b) = (side(a)?, side(b)?);
            return (a.is_some() || b.is_some()).then_some((a, b));
        }
    }
    if let Some(rest) = lower.strip_suffix(" to") {
        return Some((Some(read(rest.trim())?), None));
    }
    // `10-100`: a minus after the first character splits two numbers (`-5-10` too).
    if numbers
        && let Some(at) = lower
            .char_indices()
            .skip(1)
            .find(|&(at, c)| c == '-' && !lower[..at].ends_with(['e', 'E']))
            .map(|(at, _)| at)
    {
        let (a, b) = (side(&lower[..at])?, side(&lower[at + 1..])?);
        return (a.is_some() || b.is_some()).then_some((a, b));
    }
    let one = read(&lower)?;
    Some((Some(one.clone()), Some(one)))
}

/// A number as typed: `12.5`, `12,5` (a decimal comma when there is no point), `1 000`.
fn number(text: &str) -> Option<f64> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let compact = if compact.contains('.') {
        compact.replace(',', "")
    } else {
        compact.replace(',', ".")
    };
    compact.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// A day as typed, `yyyy-mm-dd` or `dd/mm/yyyy` (`d/m/yyyy`, `dd-mm-yyyy`), as `yyyy-mm-dd`.
fn day(text: &str) -> Option<String> {
    if crate::dataframe::iso_days(text).is_some() {
        return Some(text.to_string());
    }
    let parts: Vec<&str> = text.split(['/', '-', '.']).collect();
    let [d, m, y] = parts.as_slice() else {
        return None;
    };
    if y.len() != 4 || d.is_empty() || d.len() > 2 || m.is_empty() || m.len() > 2 {
        return None;
    }
    let iso = format!("{y}-{m:0>2}-{d:0>2}");
    crate::dataframe::iso_days(&iso).map(|_| iso)
}

/// The rows of `df` whose `column` fits `rule`. An error when the column is gone, has the
/// wrong type for the rule, or no row fits; `Ok(None)` when every row does (no version).
pub fn filter(
    df: &DataFrame,
    column: &str,
    rule: &FilterRule,
) -> Result<Option<DataFrame>, String> {
    let values = df.column(column).map_err(|e| e.to_string())?;
    let mask: BooleanChunked = match rule {
        FilterRule::Contains(text) => {
            let needle = text.to_lowercase();
            (0..values.len())
                .map(|row| {
                    values
                        .get(row)
                        .ok()
                        .and_then(cell_text)
                        .is_some_and(|cell| cell.to_lowercase().contains(&needle))
                })
                .collect()
        }
        FilterRule::Between { min, max } => {
            if FilterKind::of(values.dtype()) != Some(FilterKind::Number) {
                return Err(format!("{column} is not a number column"));
            }
            let read = |bound: &Option<String>| -> Result<Option<f64>, String> {
                bound
                    .as_deref()
                    .map(|b| b.parse::<f64>().map_err(|_| format!("Not a number: {b}")))
                    .transpose()
            };
            let (min, max) = (read(min)?, read(max)?);
            let floats = values.cast(&DataType::Float64).map_err(|e| e.to_string())?;
            let floats = floats.f64().map_err(|e| e.to_string())?;
            floats
                .iter()
                .map(|value| {
                    value.is_some_and(|n| {
                        min.is_none_or(|min| n >= min) && max.is_none_or(|max| n <= max)
                    })
                })
                .collect()
        }
        FilterRule::Dates { from, to } => {
            let per_day: i64 = match values.dtype() {
                DataType::Date => 1,
                DataType::Datetime(TimeUnit::Milliseconds, _) => 86_400_000,
                DataType::Datetime(TimeUnit::Microseconds, _) => 86_400_000_000,
                DataType::Datetime(TimeUnit::Nanoseconds, _) => 86_400_000_000_000,
                _ => return Err(format!("{column} is not a date column")),
            };
            let read = |bound: &Option<String>| -> Result<Option<i64>, String> {
                bound
                    .as_deref()
                    .map(|b| {
                        crate::dataframe::iso_days(b)
                            .map(i64::from)
                            .ok_or_else(|| format!("Not a date: {b}"))
                    })
                    .transpose()
            };
            let (from, to) = (read(from)?, read(to)?);
            let physical = values.to_physical_repr();
            let ticks = physical.cast(&DataType::Int64).map_err(|e| e.to_string())?;
            let ticks = ticks.i64().map_err(|e| e.to_string())?;
            ticks
                .iter()
                .map(|value| {
                    value.is_some_and(|tick| {
                        let day = tick.div_euclid(per_day);
                        from.is_none_or(|from| day >= from) && to.is_none_or(|to| day <= to)
                    })
                })
                .collect()
        }
    };
    let kept = mask.sum().unwrap_or(0) as usize;
    if kept == 0 {
        return Err("No row matches".to_string());
    }
    if kept == df.height() {
        return Ok(None);
    }
    df.filter(&mask).map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{FilterKind, FilterRule, filter, parse};
    use crate::dataframe::parse_table;
    use polars::prelude::*;

    fn between(min: Option<&str>, max: Option<&str>) -> Option<FilterRule> {
        Some(FilterRule::Between {
            min: min.map(str::to_string),
            max: max.map(str::to_string),
        })
    }

    fn dates(from: Option<&str>, to: Option<&str>) -> Option<FilterRule> {
        Some(FilterRule::Dates {
            from: from.map(str::to_string),
            to: to.map(str::to_string),
        })
    }

    #[test]
    fn a_number_range_reads_as_typed() {
        let n = |text| parse(FilterKind::Number, text);
        assert_eq!(n("10 to 100"), between(Some("10"), Some("100")));
        assert_eq!(n("10-100"), between(Some("10"), Some("100")));
        assert_eq!(n("10 – 100"), between(Some("10"), Some("100")));
        assert_eq!(n("-5-10"), between(Some("-5"), Some("10")));
        assert_eq!(n("-5 to -1"), between(Some("-5"), Some("-1")));
        assert_eq!(n(">= 2,5"), between(Some("2.5"), None));
        assert_eq!(n("<=1 000.5"), between(None, Some("1000.5")));
        assert_eq!(n("from 3"), between(Some("3"), None));
        assert_eq!(n("3 to"), between(Some("3"), None));
        assert_eq!(n("42"), between(Some("42"), Some("42")));
        assert_eq!(n("100 to 10"), None);
        assert_eq!(n("lots"), None);
        assert_eq!(n("1 to x"), None);
        assert_eq!(n(""), None);
        assert_eq!(n(" to "), None);
    }

    #[test]
    fn a_day_range_reads_iso_and_day_first_dates() {
        let d = |text| parse(FilterKind::Date, text);
        assert_eq!(
            d("2026-09-01 to 2026-09-30"),
            dates(Some("2026-09-01"), Some("2026-09-30"))
        );
        assert_eq!(
            d("1/9/2026 – 30/09/2026"),
            dates(Some("2026-09-01"), Some("2026-09-30"))
        );
        assert_eq!(d("from 2026-09-01"), dates(Some("2026-09-01"), None));
        assert_eq!(d("until 30-09-2026"), dates(None, Some("2026-09-30")));
        assert_eq!(
            d("2026-09-03"),
            dates(Some("2026-09-03"), Some("2026-09-03"))
        );
        assert_eq!(d("2026-02-30"), None);
        assert_eq!(d("2026-09-30 to 2026-09-01"), None);
        assert_eq!(d("09/30/2026"), None);
        assert_eq!(
            parse(FilterKind::Text, "  Lamp "),
            Some(FilterRule::Contains("Lamp".into()))
        );
        assert_eq!(parse(FilterKind::Text, "  "), None);
    }

    #[test]
    fn the_kind_follows_the_column_type() {
        assert_eq!(FilterKind::of(&DataType::String), Some(FilterKind::Text));
        assert_eq!(FilterKind::of(&DataType::Int64), Some(FilterKind::Number));
        assert_eq!(FilterKind::of(&DataType::Float64), Some(FilterKind::Number));
        assert_eq!(FilterKind::of(&DataType::Date), Some(FilterKind::Date));
        assert_eq!(
            FilterKind::of(&DataType::Datetime(TimeUnit::Microseconds, None)),
            Some(FilterKind::Date)
        );
        assert_eq!(FilterKind::of(&DataType::Time), None);
    }

    #[test]
    fn filters_keep_the_rows_that_fit() {
        let df = parse_table(
            "product,units,sold,at\nDesk lamp,2,2026-09-02,2026-09-02 10:00\nOffice chair,1,2026-09-01,2026-09-01 08:30\nlamp shade,5,2026-09-05,2026-09-05 23:59\n,,,",
        )
        .unwrap();
        let rows = |out: Option<DataFrame>| out.map(|df| df.height());
        let lamp = FilterRule::Contains("LAMP".into());
        assert_eq!(rows(filter(&df, "product", &lamp).unwrap()), Some(2));
        let units = between(Some("2"), Some("5")).unwrap();
        assert_eq!(rows(filter(&df, "units", &units).unwrap()), Some(2));
        let open = between(None, Some("1.5")).unwrap();
        assert_eq!(rows(filter(&df, "units", &open).unwrap()), Some(1));
        let days = dates(Some("2026-09-02"), Some("2026-09-05")).unwrap();
        assert_eq!(rows(filter(&df, "sold", &days).unwrap()), Some(2));
        // A datetime by its day: the whole last day is in.
        assert_eq!(rows(filter(&df, "at", &days).unwrap()), Some(2));
        // Nothing fits, everything fits, the wrong type.
        let none = FilterRule::Contains("sofa".into());
        assert_eq!(
            filter(&df, "product", &none).err().as_deref(),
            Some("No row matches")
        );
        let all = parse_table("a,n\nx,1\ny,2").unwrap();
        let every = between(Some("0"), None).unwrap();
        assert_eq!(filter(&all, "n", &every), Ok(None));
        assert_eq!(
            filter(&df, "product", &units).err().as_deref(),
            Some("product is not a number column")
        );
        assert_eq!(
            filter(&df, "units", &days).err().as_deref(),
            Some("units is not a date column")
        );
        // Contains looks at the cell as shown, numbers too.
        let five = FilterRule::Contains("5".into());
        assert_eq!(rows(filter(&df, "units", &five).unwrap()), Some(1));
    }
}
