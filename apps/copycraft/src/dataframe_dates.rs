/// Which part of a `12/03/2026` date is the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    /// `dd/mm/yyyy`, the default (Dutch and most of Europe).
    DayFirst,
    /// `mm/dd/yyyy`.
    MonthFirst,
}

impl DateOrder {
    fn format(self, separator: char) -> String {
        match self {
            Self::DayFirst => format!("%d{separator}%m{separator}%Y"),
            Self::MonthFirst => format!("%m{separator}%d{separator}%Y"),
        }
    }
}

/// What the text in a column says about its `a/b/yyyy` dates (`/`, `-` or `.` between).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateGuess {
    pub separator: char,
    /// `None` when every day and month is 12 or less, so both orders read every value.
    pub order: Option<DateOrder>,
}

/// The order of the dates in `values` (empty ones skipped), or `None` when one is not an
/// `a/b/yyyy` date, the separators differ, or no order reads them all.
pub fn date_guess<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<DateGuess> {
    let mut separator = None;
    let (mut first_over_12, mut second_over_12, mut any) = (false, false, false);
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let sep = value.chars().find(|ch| matches!(ch, '/' | '-' | '.'))?;
        if *separator.get_or_insert(sep) != sep {
            return None;
        }
        let mut parts = value.split(sep);
        let (a, b, year) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let small = |part: &str| {
            (1..=2).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
        };
        if !small(a) || !small(b) {
            return None;
        }
        let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
        if !(1..=31).contains(&a) || !(1..=31).contains(&b) {
            return None;
        }
        first_over_12 |= a > 12;
        second_over_12 |= b > 12;
        any = true;
    }
    let order = match (first_over_12, second_over_12) {
        (true, true) => return None,
        (true, false) => Some(DateOrder::DayFirst),
        (false, true) => Some(DateOrder::MonthFirst),
        (false, false) => None,
    };
    any.then_some(DateGuess {
        separator: separator?,
        order,
    })
}

/// A text column read as dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateColumn {
    pub order: DateOrder,
    /// Both orders read every value; `order` is the default ([`DateOrder::DayFirst`]) or the
    /// one asked for.
    pub ambiguous: bool,
}

/// Read every text column of `a/b/yyyy` dates in `df` as dates. Where both orders fit,
/// `ambiguous` decides. Returns the columns read.
fn read_dates(df: &mut DataFrame, ambiguous: DateOrder) -> Vec<DateColumn> {
    let mut read = Vec::new();
    let names: Vec<PlSmallStr> = df.get_column_names_owned();
    for name in names {
        let Ok(column) = df.column(name.as_str()) else {
            continue;
        };
        let Ok(text) = column.str() else {
            continue;
        };
        let Some(guess) = date_guess(text.iter().flatten()) else {
            continue;
        };
        let order = guess.order.unwrap_or(ambiguous);
        let Ok(dates) = text.as_date(Some(&order.format(guess.separator)), false) else {
            continue;
        };
        // Only when every date parses (31/02 does not).
        if dates.null_count() != text.null_count() {
            continue;
        }
        if df
            .replace(name.as_str(), dates.into_series().into_column())
            .is_ok()
        {
            read.push(DateColumn {
                order,
                ambiguous: guess.order.is_none(),
            });
        }
    }
    read
}

/// [`read_iso_dates`], then [`read_dates`] day first where both orders fit; the number of
/// columns read.
pub fn read_dates_day_first(df: &mut DataFrame) -> usize {
    read_iso_dates(df) + read_dates(df, DateOrder::DayFirst).len()
}

/// Read every text column of ISO dates (`2026-03-09`) as dates, and of ISO datetimes
/// (`2026-03-09T14:05`, also with a space, seconds and a fraction, as a saved CSV has them) as
/// datetimes. Only when every value (empty ones aside) is one: a column with one other value
/// stays text. Returns the number of columns read.
fn read_iso_dates(df: &mut DataFrame) -> usize {
    let mut read = 0;
    for name in df.get_column_names_owned() {
        let Some(column) = df
            .column(name.as_str())
            .ok()
            .and_then(|column| column.str().ok())
            .and_then(iso_column)
        else {
            continue;
        };
        if df.replace(name.as_str(), column).is_ok() {
            read += 1;
        }
    }
    read
}

/// `text` as a date or datetime column, when every value is an ISO date, or every one an ISO
/// datetime.
fn iso_column(text: &StringChunked) -> Option<Column> {
    let name = text.name().clone();
    let values = || text.iter().flatten().map(str::trim).filter(|v| !v.is_empty());
    values().next()?;
    let empty = |value: Option<&str>| value.is_none_or(|v| v.trim().is_empty());
    if values().all(|value| iso_days(value).is_some()) {
        let days: Int32Chunked = text
            .iter()
            .map(|value| value.and_then(|v| iso_days(v.trim())))
            .collect();
        let column = days.into_date().into_series().with_name(name).into_column();
        let blanks = text.iter().filter(|value| empty(*value)).count();
        return (column.null_count() == blanks).then_some(column);
    }
    if values().all(|value| iso_micros(value).is_some()) {
        let micros: Int64Chunked = text
            .iter()
            .map(|value| value.and_then(|v| iso_micros(v.trim())))
            .collect();
        let column = micros
            .into_datetime(TimeUnit::Microseconds, None)
            .into_series()
            .with_name(name)
            .into_column();
        let blanks = text.iter().filter(|value| empty(*value)).count();
        return (column.null_count() == blanks).then_some(column);
    }
    None
}

/// Days since 1970-01-01 of an ISO date `yyyy-mm-dd` that exists (no 2026-02-30).
fn iso_days(value: &str) -> Option<i32> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i32 = digits(&value[..4])?;
    let month: u32 = digits(&value[5..7])?;
    let day: u32 = digits(&value[8..10])?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Microseconds since 1970-01-01T00:00 of an ISO datetime: a date ([`iso_days`]), `T` or a
/// space, `HH:MM`, then optionally `:SS` and a fraction of 1 to 9 digits. No time zone.
fn iso_micros(value: &str) -> Option<i64> {
    let days = iso_days(value.get(..10)?)?;
    let rest = value.get(10..)?;
    let time = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let (clock, fraction) = match time.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (time, None),
    };
    let mut parts = clock.split(':');
    let hour: i64 = two_digits(parts.next()?)?;
    let minute: i64 = two_digits(parts.next()?)?;
    let second: i64 = match parts.next() {
        Some(second) => two_digits(second)?,
        None if fraction.is_none() => 0,
        None => return None,
    };
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let micros = match fraction {
        None => 0,
        Some(fraction) => {
            if !(1..=9).contains(&fraction.len()) || !fraction.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let padded = format!("{fraction:0<9}");
            padded[..6].parse::<i64>().ok()?
        }
    };
    let seconds = i64::from(days) * 86_400 + hour * 3600 + minute * 60 + second;
    Some(seconds * 1_000_000 + micros)
}

fn digits<T: std::str::FromStr>(part: &str) -> Option<T> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

fn two_digits(part: &str) -> Option<i64> {
    (part.len() == 2).then(|| digits(part)).flatten()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// "Dates read as dd/mm/yyyy" when a column's dates fit both orders, so the card says which one
/// it chose.
pub fn ambiguous_dates_note(dates: &[DateColumn]) -> Option<String> {
    let column = dates.iter().find(|column| column.ambiguous)?;
    Some(match column.order {
        DateOrder::DayFirst => "Dates read as dd/mm/yyyy".to_string(),
        DateOrder::MonthFirst => "Dates read as mm/dd/yyyy".to_string(),
    })
}
